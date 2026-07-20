//! Settings dashboard: the switcher's TUI vocabulary applied to the plugin's flat
//! `config.toml`.
//!
//! This was an fzf list, which made a fixed form behave like a search: a fuzzy prompt
//! and a match counter. You do not *find* `sort` in this list, you walk
//! to it — so it is a form now, in the picker's colours and command-bar pills.
//!
//! It draws no border of its own. This runs in a popup pane, which herdr already frames
//! and titles from the manifest; a second box would double the title and cost two rows
//! and two columns of a window sized to fit exactly. The picker can afford its boxes
//! because its overlay title is minimised to an icon.
//!
//! Values are written the moment they change, as the fzf version did: the picker reads
//! `config.toml` at startup, so no server reload is involved.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::data::{Config, Theme};
use crate::tui::{self, Flow, Pill, SimpleMode};

/// How Enter changes a setting.
enum Cycle {
    /// Step through a fixed ring. An unrecognised current value lands on the first
    /// entry, matching the `*)` fallback each `cycle()` case in settings.sh had.
    Ring(&'static [&'static str]),
    /// Free text, typed in place. Only `split_ratio` wants this.
    Prompt,
}

struct Setting {
    /// The section this setting sits under; a new value starts a new heading,
    /// so the array's order is the display order (like the `?` cheatsheet).
    group: &'static str,
    key: &'static str,
    default: &'static str,
    hint: &'static str,
    cycle: Cycle,
}

const BOOL: &[&str] = &["true", "false"];

/// The settings, in display order, grouped into sections. `write_setting` is
/// keyed by `key`, so the order here is free to read well.
const SETTINGS: &[Setting] = &[
    Setting {
        group: "Open",
        key: "default_target",
        default: "workspace",
        hint: "where Enter opens a repo",
        cycle: Cycle::Ring(&["workspace", "tab", "split", "pane"]),
    },
    Setting {
        group: "Open",
        key: "split_direction",
        default: "right",
        hint: "split growth direction",
        cycle: Cycle::Ring(&["right", "down"]),
    },
    Setting {
        group: "Open",
        key: "split_ratio",
        default: "0.5",
        hint: "split size (0.1-0.9)",
        cycle: Cycle::Prompt,
    },
    Setting {
        group: "Open",
        key: "label",
        default: "repo",
        hint: "workspace/tab label style",
        cycle: Cycle::Ring(&["repo", "owner-repo", "path"]),
    },
    Setting {
        group: "Sources",
        key: "include_agents",
        default: "true",
        hint: "list running agents in the switcher",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Sources",
        key: "include_workspaces",
        default: "true",
        hint: "list open workspaces in the switcher",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Sources",
        key: "sort",
        default: "recent",
        hint: "resting list order (recent/name/kind)",
        cycle: Cycle::Ring(&["recent", "name", "kind"]),
    },
    Setting {
        group: "Keys",
        key: "keymode",
        default: "insert",
        hint: "start mode: insert (type-to-filter) or normal (Vim)",
        cycle: Cycle::Ring(&["insert", "normal"]),
    },
    Setting {
        group: "Preview",
        key: "preview",
        default: "enabled",
        hint: "show the preview pane",
        cycle: Cycle::Ring(&["enabled", "disabled"]),
    },
    Setting {
        group: "Preview",
        key: "preview_position",
        default: "down",
        hint: "which side the preview sits on",
        cycle: Cycle::Ring(&["right", "down", "up", "left"]),
    },
    Setting {
        group: "Preview",
        key: "preview_size",
        default: "60%",
        hint: "preview share of the body",
        cycle: Cycle::Ring(&["40%", "50%", "60%", "70%", "80%"]),
    },
    Setting {
        group: "Preview",
        key: "preview_readme",
        default: "true",
        hint: "include README in the preview",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Appearance",
        key: "title_color",
        default: "peach",
        hint: "box title colour (theme slot or #hex)",
        cycle: Cycle::Ring(&["peach", "mauve", "teal", "blue", "accent"]),
    },
    Setting {
        group: "Appearance",
        key: "transparency",
        default: "auto",
        hint: "popup background transparency",
        cycle: Cycle::Ring(&["auto", "enabled", "disabled"]),
    },
    Setting {
        group: "Clone",
        key: "clone_source",
        default: "clipboard",
        hint: "seed clone input from clipboard",
        cycle: Cycle::Ring(&["clipboard", "prompt"]),
    },
    Setting {
        group: "Clone",
        key: "open_after_clone",
        default: "true",
        hint: "open a repo right after cloning",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Updates",
        key: "update_check",
        default: "true",
        hint: "check GitHub daily for a newer version",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Notifications",
        key: "notifications",
        default: "true",
        hint: "show herdr notifications",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        group: "Notifications",
        key: "notification_position",
        default: "top-right",
        hint: "notification corner",
        cycle: Cycle::Ring(&["top-right", "top-left", "bottom-left", "bottom-right"]),
    },
];

/// The next value in a ring. An unknown current value restarts at the first.
fn next_in(ring: &[&str], current: &str) -> String {
    let i = ring.iter().position(|v| *v == current);
    match i {
        Some(i) => ring[(i + 1) % ring.len()].to_string(),
        None => ring[0].to_string(),
    }
}

/// Replace `key`'s line in the flat config, or append one. Comments, unknown keys,
/// and ordering survive — this file is hand-edited too. Mirrors settings.sh's
/// `config_set`, including its `key = "value"` output shape.
fn write_setting(path: &PathBuf, key: &str, value: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();

    let mut out = String::with_capacity(existing.len() + 32);
    let mut replaced = false;
    for line in existing.lines() {
        let is_key = line
            .trim_start()
            .strip_prefix(key)
            .map(|rest| rest.trim_start().starts_with('='))
            .unwrap_or(false);
        if is_key && !replaced {
            out.push_str(&format!("{key} = \"{value}\"\n"));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        out.push_str(&format!("{key} = \"{value}\"\n"));
    }

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, out)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn config_path() -> PathBuf {
    let dir = std::env::var("HERDR_PLUGIN_CONFIG_DIR").unwrap_or_default();
    if dir.is_empty() {
        // Same fallback settings.sh uses when herdr does not hand us a config dir.
        let root = std::env::var("HERDR_PLUGIN_ROOT").unwrap_or_else(|_| ".".into());
        PathBuf::from(root).join(".config").join("config.toml")
    } else {
        PathBuf::from(dir).join("config.toml")
    }
}

pub struct App {
    theme: Theme,
    title_color: Color,
    values: Vec<String>,
    sel: usize,
    /// `Some` while typing a `Cycle::Prompt` value.
    editing: Option<String>,
    path: PathBuf,
    /// Shown in the command bar when a write fails; the form stays usable.
    error: Option<String>,
}

impl App {
    fn new(cfg: &Config, theme: Theme, path: PathBuf) -> Self {
        let title_color = theme
            .resolve(&cfg.get("title_color", "peach"))
            .unwrap_or(Color::Yellow);
        let values = SETTINGS.iter().map(|s| cfg.get(s.key, s.default)).collect();
        App {
            theme,
            title_color,
            values,
            sel: 0,
            editing: None,
            path,
            error: None,
        }
    }

    fn commit(&mut self, value: String) {
        let key = SETTINGS[self.sel].key;
        match write_setting(&self.path, key, &value) {
            Ok(()) => {
                self.values[self.sel] = value;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("could not save {key}: {e}")),
        }
    }
}

impl SimpleMode for App {
    fn draw(&mut self, f: &mut Frame) {
        draw(f, self);
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        if let Some(buf) = self.editing.as_mut() {
            match k.code {
                KeyCode::Esc => self.editing = None,
                KeyCode::Enter => {
                    let v = buf.trim().to_string();
                    self.editing = None;
                    if !v.is_empty() {
                        self.commit(v);
                    }
                }
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) => buf.push(c),
                _ => {}
            }
            return Flow::Continue;
        }

        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Flow::Quit,
            KeyCode::Char('c') if ctrl => return Flow::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                self.sel = (self.sel + 1) % SETTINGS.len();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.sel = (self.sel + SETTINGS.len() - 1) % SETTINGS.len();
            }
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = SETTINGS.len() - 1,
            KeyCode::Enter => match &SETTINGS[self.sel].cycle {
                Cycle::Ring(ring) => {
                    let next = next_in(ring, &self.values[self.sel]);
                    self.commit(next);
                }
                Cycle::Prompt => self.editing = Some(self.values[self.sel].clone()),
            },
            _ => {}
        }
        Flow::Continue
    }
}

fn draw(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let ink = t.or("panel_bg", Color::Rgb(16, 18, 20));
    let text = t.or("text", Color::Reset);
    let sub = t.or("subtext0", Color::Gray);
    let accent = t.or("accent", Color::Cyan);
    let title = app.title_color;

    // No box of our own: a popup pane already has herdr's frame and its manifest title.
    // Drawing a second bordered box inside it doubles the title. Instead it borrows the
    // `?` cheatsheet's language — section headings in the title colour, values as filled
    // pills, and a `▌` marker on the selected row — so the two surfaces feel like one.
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());
    let area = rows[0];
    let width = area.width as usize;

    // Build the whole grouped card: a heading when the group changes, then a row per
    // setting. Remember where the selected setting landed so the window scrolls to it.
    let name_w = 22usize; // widest key ("notification_position")
    let pill_w = 12usize; // widest value ("bottom-right")
    let mut lines: Vec<Line> = Vec::new();
    let mut sel_row = 0usize;
    let mut last_group = "";
    for (i, s) in SETTINGS.iter().enumerate() {
        if s.group != last_group {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!(" {}", s.group),
                Style::default().fg(title).add_modifier(Modifier::BOLD),
            )));
            last_group = s.group;
        }

        let selected = i == app.sel;
        if selected {
            sel_row = lines.len();
        }
        let editing = selected && app.editing.is_some();
        let value = match &app.editing {
            Some(buf) if selected => format!("{buf}▏"),
            _ => app.values[i].clone(),
        };
        // The selected (or editing) row's value pill uses the title colour to pop;
        // the rest sit in a calm accent, the way the cheatsheet's key caps do.
        let pill_bg = if selected || editing { title } else { accent };
        let used = 2 + name_w + 1 + (pill_w + 2) + 2;
        let hint_w = width.saturating_sub(used);
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▌ " } else { "  " }.to_string(),
                Style::default().fg(accent),
            ),
            Span::styled(
                format!("{:<name_w$}", s.key),
                Style::default().fg(if selected { text } else { sub }),
            ),
            Span::raw(" "),
            Span::styled(
                format!(" {value:<pill_w$} "),
                Style::default()
                    .bg(pill_bg)
                    .fg(ink)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("{:<hint_w$}", s.hint), Style::default().fg(sub)),
        ]));
    }

    // Scroll to keep the selected row in view with a little context above it (its
    // heading, usually), never past the last screenful.
    let visible = (area.height as usize).max(1);
    let max_off = lines.len().saturating_sub(visible);
    let offset = sel_row.saturating_sub(2).min(max_off);
    let shown: Vec<Line> = lines.into_iter().skip(offset).take(visible).collect();

    f.render_widget(Paragraph::new(shown), area);
    draw_bar(f, app, rows[1]);
}

/// The picker's coloured-pill command bar, with this form's verbs.
fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let ink = t.or("panel_bg", Color::Rgb(16, 18, 20));

    if let Some(err) = &app.error {
        let red = t.or("red", Color::Red);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {err} "),
                Style::default().fg(red),
            ))),
            area,
        );
        return;
    }

    let pills: Vec<Pill> = if app.editing.is_some() {
        vec![
            Pill::new("↵", "save", t.or("accent", Color::Cyan)),
            Pill::new("esc", "cancel", t.or("red", Color::Red)),
        ]
    } else {
        vec![
            Pill::new("↵", "change", t.or("accent", Color::Cyan)),
            Pill::new("↑ ↓", "move", t.or("blue", Color::Blue)),
            Pill::new("esc", "done", t.or("red", Color::Red)),
        ]
    };

    let (spans, _) = tui::pill_row(&pills, ink, area.x);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Entry point for `herdr-ghq-switcher --settings`.
pub fn main() -> Result<()> {
    let cfg = Config::load();
    let theme = Theme::load();
    let mut app = App::new(&cfg, theme, config_path());

    tui::run_simple(&mut app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_cycles_and_wraps() {
        let ring = &["workspace", "tab", "split", "pane"];
        assert_eq!(next_in(ring, "workspace"), "tab");
        assert_eq!(next_in(ring, "pane"), "workspace");
    }

    #[test]
    fn draw_renders_grouped_rows_with_heading_value_and_hint() {
        let app = App::new(
            &Config::default(),
            Theme::default(),
            PathBuf::from("/tmp/x"),
        );
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let screen: String = (0..24)
            .map(|y| {
                (0..90)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        // A section heading, the first setting's value, and a hint all render.
        assert!(screen.contains("Open"), "{screen}");
        assert!(screen.contains("default_target"), "{screen}");
        assert!(screen.contains("workspace"), "{screen}");
        assert!(screen.contains("where Enter opens a repo"), "{screen}");
        // The selected row (row 0) carries the ▌ marker.
        assert!(screen.contains('▌'), "{screen}");
    }

    #[test]
    fn unknown_value_restarts_the_ring() {
        // settings.sh's `*)` fallback: a hand-edited or empty value lands on the first.
        assert_eq!(next_in(&["true", "false"], ""), "true");
        assert_eq!(next_in(&["true", "false"], "yes"), "true");
    }

    #[test]
    fn write_replaces_in_place_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("ghq-set-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &path,
            "# a comment\nsort = \"name\"\nunknown_key = \"keep\"\n",
        )
        .unwrap();

        write_setting(&path, "sort", "recent").unwrap();
        let text = fs::read_to_string(&path).unwrap();

        assert_eq!(
            text,
            "# a comment\nsort = \"recent\"\nunknown_key = \"keep\"\n"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_appends_a_missing_key() {
        let dir = std::env::temp_dir().join(format!("ghq-app-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "sort = \"name\"\n").unwrap();

        write_setting(&path, "label", "path").unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "sort = \"name\"\nlabel = \"path\"\n"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_setting_has_a_default_its_ring_accepts() {
        // A default outside its own ring would make the first Enter appear to do nothing.
        for s in SETTINGS {
            if let Cycle::Ring(ring) = &s.cycle {
                assert!(
                    ring.contains(&s.default),
                    "{} default {:?} is not in its ring",
                    s.key,
                    s.default
                );
            }
        }
    }
}
