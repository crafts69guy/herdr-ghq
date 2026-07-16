//! herdr-ghq-switcher — a unified herdr switcher TUI (agents, workspaces, ghq
//! repos) with fuzzy search, a live preview, and a full-width command bar.

mod action;
mod data;
mod history;
mod preview;
mod ui;

use std::cmp::Reverse;
use std::collections::HashMap;
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
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
    pub root: String,
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
    pub show_help: bool,
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
    fn new(
        entries: Vec<Entry>,
        theme: Theme,
        cfg: Config,
        root: String,
        script_dir: String,
    ) -> Self {
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
        let recent = history::load();
        let present_kinds = [Kind::Agent, Kind::Workspace, Kind::Repo]
            .into_iter()
            .filter(|&k| entries.iter().any(|e| e.kind == k))
            .collect();
        let preview_worker = preview::Worker::spawn(script_dir.clone(), root.clone(), cfg.clone());
        let mut app = App {
            entries,
            filtered: Vec::new(),
            selected: 0,
            query: String::new(),
            matcher: Matcher::new(NucleoConfig::DEFAULT),
            theme,
            title_color,
            cfg,
            root,
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
            show_help: false,
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
        self.preview_pending = self.preview_worker.request(self.preview_seq, entry);
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
            self.preview = done.text;
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
        KeyCode::Char('s') if alt => {
            app.sort = app.sort.next();
            app.recompute();
            Flow::Continue
        }
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
        app.request_preview();
        terminal.draw(|f| ui::draw(f, app))?;
        wait_for_work(app)?;
        if !event::poll(Duration::ZERO)? {
            continue; // woke for a finished preview, not a key: redraw it
        }
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match handle_key(app, k) {
                Flow::Continue => {}
                Flow::Quit => return Ok(None),
                Flow::Accept(a) => return Ok(Some((app.selected_entry().cloned(), a))),
            }
        }
    }
}

fn main() -> Result<()> {
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

    let entries = data::load(&cfg, &theme, &root);
    if entries.is_empty() {
        // Nothing to switch to yet — hand off to the clone flow.
        let err = Command::new("bash")
            .arg(format!("{script_dir}/get.sh"))
            .exec();
        return Err(err.into());
    }

    let mut app = App::new(entries, theme, cfg, root, script_dir.clone());
    let mut terminal = ratatui::init();
    let outcome = run(&mut terminal, &mut app);
    ratatui::restore();

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
                Accept::Update | Accept::Clone => {}
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
