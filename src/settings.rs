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
    key: &'static str,
    default: &'static str,
    hint: &'static str,
    cycle: Cycle,
}

const BOOL: &[&str] = &["true", "false"];

/// Mirrors the `SETTINGS` array and `cycle()` cases of the fzf dashboard, in order.
const SETTINGS: &[Setting] = &[
    Setting {
        key: "default_target",
        default: "workspace",
        hint: "where Enter opens a repo",
        cycle: Cycle::Ring(&["workspace", "tab", "split", "pane"]),
    },
    Setting {
        key: "split_direction",
        default: "right",
        hint: "split growth direction",
        cycle: Cycle::Ring(&["right", "down"]),
    },
    Setting {
        key: "split_ratio",
        default: "0.5",
        hint: "split size (0.1-0.9)",
        cycle: Cycle::Prompt,
    },
    Setting {
        key: "label",
        default: "repo",
        hint: "workspace/tab label style",
        cycle: Cycle::Ring(&["repo", "owner-repo", "path"]),
    },
    Setting {
        key: "include_agents",
        default: "true",
        hint: "list running agents in the switcher",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        key: "include_workspaces",
        default: "true",
        hint: "list open workspaces in the switcher",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        key: "sort",
        default: "recent",
        hint: "resting list order (recent/name/kind)",
        cycle: Cycle::Ring(&["recent", "name", "kind"]),
    },
    Setting {
        key: "keymode",
        default: "insert",
        hint: "start mode: insert (type-to-filter) or normal (Vim)",
        cycle: Cycle::Ring(&["insert", "normal"]),
    },
    Setting {
        key: "title_color",
        default: "peach",
        hint: "box title colour (theme slot or #hex)",
        cycle: Cycle::Ring(&["peach", "mauve", "teal", "blue", "accent"]),
    },
    Setting {
        key: "preview",
        default: "enabled",
        hint: "show the preview pane",
        cycle: Cycle::Ring(&["enabled", "disabled"]),
    },
    Setting {
        key: "preview_position",
        default: "down",
        hint: "which side the preview sits on",
        cycle: Cycle::Ring(&["right", "down", "up", "left"]),
    },
    Setting {
        key: "preview_size",
        default: "60%",
        hint: "preview share of the body",
        cycle: Cycle::Ring(&["40%", "50%", "60%", "70%", "80%"]),
    },
    Setting {
        key: "preview_readme",
        default: "true",
        hint: "include README in the preview",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        key: "clone_source",
        default: "clipboard",
        hint: "seed clone input from clipboard",
        cycle: Cycle::Ring(&["clipboard", "prompt"]),
    },
    Setting {
        key: "open_after_clone",
        default: "true",
        hint: "open a repo right after cloning",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        key: "transparency",
        default: "auto",
        hint: "popup background transparency",
        cycle: Cycle::Ring(&["auto", "enabled", "disabled"]),
    },
    Setting {
        key: "update_check",
        default: "true",
        hint: "check GitHub daily for a newer version",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
        key: "notifications",
        default: "true",
        hint: "show herdr notifications",
        cycle: Cycle::Ring(BOOL),
    },
    Setting {
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
    let text = t.or("text", Color::Reset);
    let sub = t.or("subtext0", Color::Gray);
    let accent = t.or("accent", Color::Cyan);
    let surface = t.or("surface1", Color::DarkGray);

    // No box of our own: a popup pane already has herdr's frame and its manifest title.
    // Drawing a second bordered box inside it doubles the title and spends two rows and
    // two columns we do not have — the picker gets away with its boxes only because its
    // overlay title is minimised to an icon.
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());
    let area = rows[0];

    // herdr clamps the popup to the terminal, so a short window must scroll rather than
    // silently drop the last settings — the exact trap the fzf version fell into. The
    // offset is derived from the selection, so there is no scroll state to keep in sync.
    let visible = (area.height as usize).max(1);
    let offset = (app.sel + 1).saturating_sub(visible);

    let mut lines = Vec::with_capacity(visible);
    for (i, s) in SETTINGS.iter().enumerate().skip(offset).take(visible) {
        let selected = i == app.sel;
        let row_bg = if selected { surface } else { Color::Reset };
        let editing = selected && app.editing.is_some();
        let value = match &app.editing {
            Some(buf) if selected => format!("{buf}▏"),
            _ => app.values[i].clone(),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<22}", s.key),
                Style::default()
                    .bg(row_bg)
                    .fg(if selected { text } else { sub }),
            ),
            Span::styled(
                format!("{value:<14}"),
                Style::default()
                    .bg(row_bg)
                    .fg(if editing { app.title_color } else { accent })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<w$}", s.hint, w = area.width.saturating_sub(37) as usize),
                Style::default().bg(row_bg).fg(sub),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), area);
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
