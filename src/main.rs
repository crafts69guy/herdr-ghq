//! herdr-ghq-switcher — a unified herdr switcher TUI (agents, workspaces, ghq
//! repos) with fuzzy search, a live preview, and a full-width command bar.

mod action;
mod changelog;
mod data;
mod history;
mod preview;
mod settings;
mod ui;
mod update;

use std::cmp::Reverse;
use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::layout::{Position, Rect};
use ratatui::text::Text;

use action::Accept;
use data::{Config, Entry, GroupFilter, Kind, SortMode, Theme};

pub struct App {
    pub entries: Vec<Entry>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub query: String,
    matcher: Matcher,
    pub theme: Theme,
    pub title_color: ratatui::style::Color,
    pub cfg: Config,
    pub script_dir: String,
    pub preview: Text<'static>,
    preview_id: String,
    preview_worker: preview::Worker,
    /// Seq of the newest render requested; results tagged older are stale.
    preview_seq: u64,
    /// A render is queued or running, so the shown preview is one entry behind.
    preview_pending: bool,
    /// When the in-flight render started, for the placeholder's grace + phase.
    preview_since: Option<Instant>,
    /// Name of the entry being rendered, shown under the placeholder spinner.
    pub preview_label: String,
    pub preview_enabled: bool,
    pub preview_position: String,
    pub preview_pct: u16,
    pub preview_scroll: u16,
    /// Where the preview pane sat at the last draw, `None` before the first one.
    /// One rect answers three questions — how wide to build the card, how many
    /// rows can show it, and whether the pointer is over it — so they cannot
    /// disagree. Only the layout knows it, which is why `run` draws before it
    /// calls `request_preview`.
    pub preview_area: Option<Rect>,
    /// Rows the current card occupies. Because the card clips rather than wraps,
    /// one card line is one screen row, so this and [`App::preview_rows`] bound
    /// the scroll exactly.
    pub preview_len: u16,
    pub show_help: bool,
    /// A newer version the cache knows about; shown, never acted on.
    pub update: Option<String>,
    pub show_changelog: bool,
    /// Parsed on first open, not at startup: most sessions never press ⌥c.
    pub changelog: Vec<changelog::Block>,
    pub changelog_scroll: u16,
    /// Rendered rows and visible rows at the last draw, so scrolling can stop.
    pub changelog_len: u16,
    pub changelog_rows: u16,
    pub group: GroupFilter,
    pub sort: SortMode,
    /// id → last-opened epoch, for the Recent sort.
    recent: HashMap<String, u64>,
    /// Kinds actually present, in tab order — drives group cycling + the strip.
    pub present_kinds: Vec<Kind>,
}

enum Flow {
    Continue,
    Quit,
    Accept(Accept),
}

impl App {
    fn new(entries: Vec<Entry>, theme: Theme, cfg: Config, script_dir: String) -> Self {
        let preview_enabled = cfg.get("preview", "enabled") != "disabled";
        let preview_position = cfg.get("preview_position", "right");
        let preview_pct = cfg
            .get("preview_size", "60%")
            .trim_end_matches('%')
            .parse::<u16>()
            .unwrap_or(52)
            .clamp(20, 80);
        let title_color = theme
            .resolve(&cfg.get("title_color", "peach"))
            .unwrap_or_else(|| theme.or("accent", ratatui::style::Color::Cyan));
        let sort = SortMode::parse(&cfg.get("sort", "recent"));
        // Read before `cfg` moves into the struct.
        let update = update::available(&cfg);
        let recent = history::load();
        let present_kinds = [Kind::Agent, Kind::Workspace, Kind::Repo]
            .into_iter()
            .filter(|&k| entries.iter().any(|e| e.kind == k))
            .collect();
        let preview_worker = preview::Worker::spawn(script_dir.clone(), cfg.clone(), theme.clone());
        let mut app = App {
            entries,
            filtered: Vec::new(),
            selected: 0,
            query: String::new(),
            matcher: Matcher::new(NucleoConfig::DEFAULT),
            theme,
            title_color,
            cfg,
            script_dir,
            preview: Text::default(),
            preview_id: String::new(),
            preview_worker,
            preview_seq: 0,
            preview_pending: false,
            preview_since: None,
            preview_label: String::new(),
            preview_enabled,
            preview_position,
            preview_pct,
            preview_scroll: 0,
            // Filled by the first draw, which always precedes the first request.
            preview_area: None,
            preview_len: 0,
            show_help: false,
            update,
            show_changelog: false,
            changelog: Vec::new(),
            changelog_scroll: 0,
            changelog_len: 0,
            changelog_rows: 1,
            group: GroupFilter::All,
            sort,
            recent,
            present_kinds,
        };
        // Apply the initial sort (Recent by default) to the resting list.
        app.recompute();
        app
    }

    fn recompute(&mut self) {
        let group = self.group;
        if self.query.is_empty() {
            // Browse mode: filter by group, then order by the active sort.
            self.filtered = browse_order(&self.entries, &self.recent, group, self.sort);
        } else {
            // Search mode: fuzzy score wins; group still narrows the candidates.
            let pat = Pattern::parse(&self.query, CaseMatching::Smart, Normalization::Smart);
            let mut buf = Vec::new();
            let mut scored: Vec<(u32, usize)> = Vec::new();
            for (i, e) in self.entries.iter().enumerate() {
                if !group.matches(e.kind) {
                    continue;
                }
                buf.clear();
                if let Some(score) =
                    pat.score(Utf32Str::new(&e.search, &mut buf), &mut self.matcher)
                {
                    scored.push((score, i));
                }
            }
            scored.sort_by_key(|&(score, _)| Reverse(score));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        self.selected = 0;
    }

    /// The tab strip in order: All, then each present kind.
    pub fn tabs(&self) -> Vec<GroupFilter> {
        let mut v = vec![GroupFilter::All];
        v.extend(self.present_kinds.iter().map(|&k| GroupFilter::Only(k)));
        v
    }

    /// Move to the next/previous non-empty group (wraps).
    fn cycle_group(&mut self, dir: i32) {
        let tabs = self.tabs();
        if tabs.len() < 2 {
            return;
        }
        let cur = tabs.iter().position(|&g| g == self.group).unwrap_or(0) as i32;
        let next = (cur + dir).rem_euclid(tabs.len() as i32);
        self.group = tabs[next as usize];
        self.recompute();
    }

    /// Parse the changelog the first time it is asked for. A failure leaves the popup
    /// open with a single line saying so, rather than a blank box.
    fn open_changelog(&mut self) {
        if self.changelog.is_empty() {
            self.changelog = match changelog::changelog_text() {
                Ok(text) => changelog::parse(&text),
                Err(e) => changelog::parse(&format!("## [unavailable]\n\n- {e}\n")),
            };
        }
        self.changelog_scroll = 0;
        self.show_changelog = true;
    }

    /// Width the card is built to: the preview pane's interior, less its border.
    /// Before the first draw there is no pane to measure, so guess a common one —
    /// the next draw publishes the real width and the card is rebuilt to it.
    fn preview_width(&self) -> u16 {
        self.preview_area.map_or(60, |a| a.width.saturating_sub(2))
    }

    /// Rows of card the pane can show at once.
    pub fn preview_rows(&self) -> u16 {
        self.preview_area.map_or(1, |a| a.height.saturating_sub(2))
    }

    /// Scroll the preview, stopping at both ends. The list keeps `^j`/`^k`, so
    /// the preview takes the `⌥` pair: the same fingers, the other pane.
    fn scroll_preview(&mut self, delta: i32) {
        let max = self.preview_len.saturating_sub(self.preview_rows()) as i32;
        self.preview_scroll = (self.preview_scroll as i32 + delta).clamp(0, max) as u16;
    }

    /// A wheel turn moves the pane under the pointer: the card when it is over
    /// the preview, the selection anywhere else. Reports whether anything moved,
    /// so the caller can skip a redraw for a wheel over dead space.
    fn on_wheel(&mut self, at: Position, delta: i32) -> bool {
        let over_preview =
            self.preview_enabled && self.preview_area.is_some_and(|a| a.contains(at));
        if over_preview {
            let before = self.preview_scroll;
            // Three rows a notch: the conventional feel for text, and the card
            // is long enough that one row at a time would be a chore.
            self.scroll_preview(delta * 3);
            self.preview_scroll != before
        } else {
            // One entry a notch: the list is a menu, and overshooting it costs
            // a preview render.
            self.move_sel(delta.signum());
            true
        }
    }

    fn toggle_preview(&mut self) {
        self.preview_enabled = !self.preview_enabled;
        if self.preview_enabled {
            // Force request_preview to re-queue for the current selection.
            self.preview_id.clear();
        }
    }

    fn move_sel(&mut self, delta: i32) {
        let n = self.filtered.len();
        if n == 0 {
            return;
        }
        let cur = self.selected as i32;
        let next = (cur + delta).rem_euclid(n as i32);
        self.selected = next as usize;
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.filtered.get(self.selected).map(|&i| &self.entries[i])
    }

    /// Queues a preview render for the current selection if it changed. Never
    /// blocks: the worker renders while the UI keeps taking keys.
    fn request_preview(&mut self) {
        let idx = match self.filtered.get(self.selected) {
            Some(&i) => i,
            None => return,
        };
        if self.entries[idx].id == self.preview_id {
            return;
        }
        self.preview_id = self.entries[idx].id.clone();
        if !self.preview_enabled {
            return;
        }
        self.preview_seq += 1;
        let entry = self.entries[idx].clone();
        self.preview_label = entry.primary.clone();
        self.preview_pending =
            self.preview_worker
                .request(self.preview_seq, entry, self.preview_width());
        self.preview_since = Some(Instant::now());
    }

    fn preview_pending(&self) -> bool {
        self.preview_pending
    }

    /// Frame index for the pending placeholder, or `None` when the shown
    /// preview is current. Renders that finish inside `PLACEHOLDER_GRACE` —
    /// agents, small repos — never reach frame 0, so the pane doesn't flash.
    pub fn placeholder_frame(&self) -> Option<usize> {
        if !self.preview_pending {
            return None;
        }
        let waited = self
            .preview_since?
            .elapsed()
            .checked_sub(PLACEHOLDER_GRACE)?;
        Some((waited.as_millis() / PLACEHOLDER_FRAME.as_millis()) as usize)
    }

    /// Installs a finished preview, reporting whether the UI needs a redraw.
    /// Results for entries already scrolled past are dropped.
    fn absorb_preview(&mut self) -> bool {
        let mut installed = false;
        while let Some(done) = self.preview_worker.poll() {
            if done.seq != self.preview_seq {
                continue; // stale: the selection moved on
            }
            self.preview_len = done.text.lines.len() as u16;
            self.preview = done.text;
            // A new card starts at the top: the offset belonged to the old one.
            self.preview_scroll = 0;
            self.preview_pending = false;
            installed = true;
        }
        installed
    }
}

/// The no-query browse order: entries passing `group`, ordered by `sort`.
/// Ties break on original load order so the list is stable.
fn browse_order(
    entries: &[Entry],
    recent: &HashMap<String, u64>,
    group: GroupFilter,
    sort: SortMode,
) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..entries.len())
        .filter(|&i| group.matches(entries[i].kind))
        .collect();
    match sort {
        SortMode::Recent => idx.sort_by(|&a, &b| {
            let ra = recent.get(&entries[a].id).copied().unwrap_or(0);
            let rb = recent.get(&entries[b].id).copied().unwrap_or(0);
            rb.cmp(&ra).then(a.cmp(&b))
        }),
        SortMode::Name => idx.sort_by(|&a, &b| {
            entries[a]
                .primary
                .to_lowercase()
                .cmp(&entries[b].primary.to_lowercase())
                .then(a.cmp(&b))
        }),
        SortMode::Kind => idx.sort_by(|&a, &b| {
            entries[a]
                .kind
                .order()
                .cmp(&entries[b].kind.order())
                .then(a.cmp(&b))
        }),
    }
    idx
}

fn handle_key(app: &mut App, k: crossterm::event::KeyEvent) -> Flow {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);

    // The changelog popup scrolls, so it cannot dismiss on any key the way the help
    // cheatsheet does; esc/q closes it and the movement keys drive it.
    if app.show_changelog {
        let page = app.changelog_rows.saturating_sub(2).max(1);
        let max = app.changelog_len.saturating_sub(app.changelog_rows);
        match k.code {
            KeyCode::Char('c') if ctrl => return Flow::Quit,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') => app.show_changelog = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.changelog_scroll = (app.changelog_scroll + 1).min(max)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.changelog_scroll = app.changelog_scroll.saturating_sub(1)
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                app.changelog_scroll = (app.changelog_scroll + page).min(max)
            }
            KeyCode::PageUp => app.changelog_scroll = app.changelog_scroll.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => app.changelog_scroll = 0,
            KeyCode::End | KeyCode::Char('G') => app.changelog_scroll = max,
            _ => {}
        }
        return Flow::Continue;
    }

    // While the help popup is open, swallow every key: the first press just
    // dismisses it (^c still quits, so you're never trapped).
    if app.show_help {
        if ctrl && matches!(k.code, KeyCode::Char('c')) {
            return Flow::Quit;
        }
        app.show_help = false;
        return Flow::Continue;
    }

    match k.code {
        KeyCode::Esc => Flow::Quit,
        // `?` (no modifiers) opens the keybindings cheatsheet.
        KeyCode::Char('?') if !ctrl && !alt => {
            app.show_help = true;
            Flow::Continue
        }
        // Tab / Shift-Tab cycle the group filter (skipping empty groups).
        KeyCode::Tab => {
            app.cycle_group(1);
            Flow::Continue
        }
        KeyCode::BackTab => {
            app.cycle_group(-1);
            Flow::Continue
        }
        // Alt-p toggles the preview pane; Alt-s cycles the sort order.
        KeyCode::Char('p') if alt => {
            app.toggle_preview();
            Flow::Continue
        }
        // Alt-j/k scroll the preview without moving the selection, so a long
        // README or an agent's backlog can be read from the list.
        KeyCode::Char('j') if alt => {
            app.scroll_preview(1);
            Flow::Continue
        }
        KeyCode::Char('k') if alt => {
            app.scroll_preview(-1);
            Flow::Continue
        }
        KeyCode::Char('s') if alt => {
            app.sort = app.sort.next();
            app.recompute();
            Flow::Continue
        }
        // ⌥c reads the changelog without leaving the list; ⌥u updates the plugin
        // itself, next to ^u which updates the highlighted repo.
        KeyCode::Char('c') if alt => {
            app.open_changelog();
            Flow::Continue
        }
        KeyCode::Char('u') if alt => Flow::Accept(Accept::UpdatePlugin),
        KeyCode::Enter if alt => Flow::Accept(Accept::Clone),
        KeyCode::Enter => Flow::Accept(Accept::Default),
        KeyCode::Up => {
            app.move_sel(-1);
            Flow::Continue
        }
        KeyCode::Down => {
            app.move_sel(1);
            Flow::Continue
        }
        KeyCode::PageUp => {
            app.move_sel(-10);
            Flow::Continue
        }
        KeyCode::PageDown => {
            app.move_sel(10);
            Flow::Continue
        }
        KeyCode::Backspace => {
            app.query.pop();
            app.recompute();
            Flow::Continue
        }
        KeyCode::Char(c) if ctrl => match c {
            'c' => Flow::Quit,
            'j' | 'n' => {
                app.move_sel(1);
                Flow::Continue
            }
            'k' | 'p' => {
                app.move_sel(-1);
                Flow::Continue
            }
            'w' => Flow::Accept(Accept::Workspace),
            't' => Flow::Accept(Accept::Tab),
            's' => Flow::Accept(Accept::Split),
            'o' => Flow::Accept(Accept::Pane),
            'g' => Flow::Accept(Accept::Git),
            'u' => Flow::Accept(Accept::Update),
            'x' => Flow::Accept(Accept::Remove),
            _ => Flow::Continue,
        },
        KeyCode::Char(c) if !alt => {
            app.query.push(c);
            app.recompute();
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

/// Wake cadence while a preview render is in flight — short enough that the
/// result appears promptly, long enough to cost nothing.
const PREVIEW_TICK: Duration = Duration::from_millis(16);
/// Wake cadence with nothing in flight; the loop is just parked on the keyboard.
const IDLE_TICK: Duration = Duration::from_secs(1);
/// How long a render may take before the placeholder replaces the stale preview.
const PLACEHOLDER_GRACE: Duration = Duration::from_millis(90);
/// Placeholder animation frame length.
const PLACEHOLDER_FRAME: Duration = Duration::from_millis(80);

/// Blocks until there is something to draw: a key, a finished preview, or the
/// next placeholder frame.
fn wait_for_work(app: &mut App) -> Result<()> {
    let entered_on = app.placeholder_frame();
    loop {
        let tick = if app.preview_pending() {
            PREVIEW_TICK
        } else {
            IDLE_TICK
        };
        if event::poll(tick)? {
            return Ok(()); // a key is waiting: it takes priority
        }
        if app.absorb_preview() {
            return Ok(());
        }
        // Redraw on a frame change only — polling at 16ms must not drag the
        // 80ms animation up to a 60fps repaint.
        if app.placeholder_frame() != entered_on {
            return Ok(());
        }
    }
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> Result<Option<(Option<Entry>, Accept)>> {
    loop {
        // Draw first: it publishes the preview pane's width, which the request
        // below needs to clip the card to. The first pass draws an empty pane
        // for one frame, which is what the placeholder is for anyway.
        terminal.draw(|f| ui::draw(f, app))?;
        app.request_preview();
        wait_for_work(app)?;
        if !event::poll(Duration::ZERO)? {
            continue; // woke for a finished preview, not a key: redraw it
        }
        match event::read()? {
            Event::Mouse(m) => {
                let at = Position::new(m.column, m.row);
                let delta = match m.kind {
                    MouseEventKind::ScrollDown => 1,
                    MouseEventKind::ScrollUp => -1,
                    // A click or a drag: herdr owns those, and this pane has
                    // nothing to say about them.
                    _ => continue,
                };
                app.on_wheel(at, delta);
            }
            Event::Key(k) => {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                match handle_key(app, k) {
                    Flow::Continue => {}
                    Flow::Quit => return Ok(None),
                    Flow::Accept(a) => return Ok(Some((app.selected_entry().cloned(), a))),
                }
            }
            _ => {}
        }
    }
}

/// Wheel reporting, on and off.
///
/// Not crossterm's `EnableMouseCapture`: that also turns on any-event tracking
/// (`?1003h`), which reports every pointer *move*. The loop would wake and
/// redraw hundreds of times a second for events it discards. `?1000h` reports
/// buttons only — the wheel among them — and `?1006h` asks for SGR encoding, so
/// coordinates past column 223 survive.
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1006l\x1b[?1000l";

/// Claim the terminal, then the wheel. Also chains the mouse teardown ahead of
/// the panic hook `ratatui::init` installs — that hook restores the screen but
/// knows nothing about the wheel, and a panic must not leave the terminal
/// reporting clicks at a shell.
fn init_terminal() -> ratatui::DefaultTerminal {
    let terminal = ratatui::init();
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        print!("{MOUSE_OFF}");
        let _ = io::stdout().flush();
        restore(info);
    }));
    print!("{MOUSE_ON}");
    let _ = io::stdout().flush();
    terminal
}

fn restore_terminal() {
    print!("{MOUSE_OFF}");
    let _ = io::stdout().flush();
    ratatui::restore();
}

fn main() -> Result<()> {
    // One binary, one mode per entrypoint: bin/settings.sh execs us with --settings so
    // both dashboards share this build, the theme, and the flat config reader.
    let mode = env::args().skip(1).find(|a| a.starts_with("--"));
    match mode.as_deref() {
        Some("--settings") => return settings::main(),
        Some("--changelog") => return changelog::main(),
        Some("--update-check") => return update::main(),
        _ => {}
    }

    let cfg = Config::load();
    let theme = Theme::load();
    let root = data::ghq_root();
    let script_dir = env::var("HERDR_PLUGIN_ROOT")
        .map(|r| format!("{r}/bin"))
        .unwrap_or_else(|_| ".".into());
    let origin = env::var("GHQ_ORIGIN_PANE_ID").unwrap_or_default();
    // Resolve where Enter lands a repo once, before `cfg` moves into the App.
    let default_target = action::resolve_default_target(
        action::forced_target().as_deref(),
        &cfg.get("default_target", "workspace"),
    );

    // Hands the network to a detached child and returns immediately; the badge it
    // enables shows up on a later launch. Nothing below waits on it.
    update::spawn_refresh_if_stale(&cfg);

    let entries = data::load(&cfg, &theme, &root);
    if entries.is_empty() {
        // Nothing to switch to yet — hand off to the clone flow.
        let err = Command::new("bash")
            .arg(format!("{script_dir}/get.sh"))
            .exec();
        return Err(err.into());
    }

    // `root` is not carried into the App: repo entries already hold their absolute
    // `dir`, which is the only thing the preview and the actions ever needed it for.
    let mut app = App::new(entries, theme, cfg, script_dir.clone());
    let mut terminal = init_terminal();
    let outcome = run(&mut terminal, &mut app);
    restore_terminal();

    if let Some((entry, accept)) = outcome? {
        let id = entry.as_ref().map(|e| e.id.clone());
        action::dispatch(
            entry,
            accept,
            &origin,
            &app.cfg,
            &script_dir,
            &default_target,
        )?;
        // Record recency only for successful opens (dispatch returned Ok above).
        if let Some(id) = id {
            match accept {
                Accept::Default
                | Accept::Workspace
                | Accept::Tab
                | Accept::Split
                | Accept::Pane
                | Accept::Git => history::touch(&id),
                Accept::Remove => history::forget(&id),
                // Clone and UpdatePlugin exec away; neither touches an entry's recency.
                Accept::Update | Accept::Clone | Accept::UpdatePlugin => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use data::Kind;
    use ratatui::style::Color;

    fn entry(kind: Kind, id: &str, primary: &str) -> Entry {
        Entry {
            kind,
            id: id.into(),
            dir: None,
            label: primary.into(),
            icon: String::new(),
            icon_color: Color::Reset,
            primary: primary.into(),
            secondary: String::new(),
            search: primary.into(),
        }
    }

    fn sample() -> Vec<Entry> {
        vec![
            entry(Kind::Repo, "gh/zeta", "zeta"),
            entry(Kind::Agent, "term-1", "alpha"),
            entry(Kind::Repo, "gh/mid", "mid"),
            entry(Kind::Workspace, "ws-1", "work"),
        ]
    }

    #[test]
    fn recent_sort_puts_latest_opened_first() {
        let e = sample();
        let mut recent = HashMap::new();
        recent.insert("gh/mid".to_string(), 100u64);
        recent.insert("term-1".to_string(), 200u64);
        let order = browse_order(&e, &recent, GroupFilter::All, SortMode::Recent);
        // term-1 (200) then gh/mid (100), then the untouched two in load order.
        assert_eq!(order, vec![1, 2, 0, 3]);
    }

    #[test]
    fn name_sort_is_alphabetical() {
        let e = sample();
        let order = browse_order(&e, &HashMap::new(), GroupFilter::All, SortMode::Name);
        // alpha, mid, work, zeta
        assert_eq!(order, vec![1, 2, 3, 0]);
    }

    #[test]
    fn kind_sort_groups_agents_then_workspaces_then_repos() {
        let e = sample();
        let order = browse_order(&e, &HashMap::new(), GroupFilter::All, SortMode::Kind);
        // agent(1), workspace(3), repos in load order(0,2)
        assert_eq!(order, vec![1, 3, 0, 2]);
    }

    /// An app whose preview pane sits at 0,0 and shows `rows` of a `len`-row card.
    /// The pane is two rows and two columns taller/wider than its interior: the border.
    fn app_with_preview(len: u16, rows: u16) -> App {
        let mut app = App::new(sample(), Theme::default(), Config::default(), ".".into());
        app.preview_area = Some(Rect::new(0, 0, 40, rows + 2));
        app.preview_len = len;
        app
    }

    #[test]
    fn preview_scroll_stops_at_the_last_screenful() {
        let mut app = app_with_preview(60, 20);
        app.scroll_preview(1000);
        // The end of the scroll is the last full screen, not the last line:
        // scrolling past it would leave the pane showing blanks.
        assert_eq!(app.preview_scroll, 40);
    }

    #[test]
    fn preview_scroll_stops_at_the_top() {
        let mut app = app_with_preview(60, 20);
        app.scroll_preview(-5);
        assert_eq!(app.preview_scroll, 0);
    }

    #[test]
    fn preview_that_fits_does_not_scroll() {
        let mut app = app_with_preview(5, 20);
        app.scroll_preview(3);
        assert_eq!(app.preview_scroll, 0);
    }

    #[test]
    fn wheel_over_the_preview_scrolls_the_card() {
        let mut app = app_with_preview(60, 20);
        app.on_wheel(Position::new(5, 5), 1);
        // Three rows a notch.
        assert_eq!(app.preview_scroll, 3);
        assert_eq!(app.selected, 0, "the selection must not move with the card");
    }

    #[test]
    fn wheel_outside_the_preview_walks_the_list() {
        let mut app = app_with_preview(60, 20);
        // The pane is 40 wide; this is past its right edge.
        app.on_wheel(Position::new(80, 5), 1);
        assert_eq!(app.selected, 1);
        assert_eq!(
            app.preview_scroll, 0,
            "the card must not move with the list"
        );
    }

    #[test]
    fn wheel_over_a_hidden_preview_walks_the_list() {
        let mut app = app_with_preview(60, 20);
        // ⌥p hides the pane; its rect is stale, so the pointer being "inside" it
        // means nothing and the wheel belongs to the list.
        app.preview_enabled = false;
        app.on_wheel(Position::new(5, 5), 1);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn group_filter_narrows_to_one_kind() {
        let e = sample();
        let order = browse_order(
            &e,
            &HashMap::new(),
            GroupFilter::Only(Kind::Repo),
            SortMode::Name,
        );
        // only the two repos, alphabetical: mid(2), zeta(0)
        assert_eq!(order, vec![2, 0]);
    }
}
