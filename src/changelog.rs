//! Changelog viewer: `CHANGELOG.md`, rendered in the picker's colours.
//!
//! No network. An installed plugin is a git checkout of this repo, so the changelog
//! ships next to the code it describes, and `bin/release.sh` feeds the same section
//! verbatim to `gh release create` — the local file and the GitHub release notes are
//! the same text by construction.
//!
//! Like the settings dashboard, this draws no border of its own: herdr frames and
//! titles the popup pane already.

use std::fs;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::data::{Config, Theme};

/// The version this binary was built as. Deliberately not `herdr plugin list`, which
/// caches a plugin's manifest at link/install time and goes stale: it reported 0.3.3
/// for a 0.5.0 checkout because `herdr server reload-config` does not re-read manifests.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One parsed piece of the changelog. Bullets arrive re-joined, so they can be
/// re-wrapped to the popup's width instead of keeping the file's 88-column breaks.
enum Block {
    Version { version: String, date: String },
    Section(String),
    Bullet(String),
    Blank,
}

/// Parse Keep a Changelog markdown. Everything before the first `## [` is the preamble,
/// and the `[x.y.z]: https://…` compare links at the bottom are markdown plumbing — both
/// are noise in a viewer.
fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut bullet: Option<String> = None;
    let mut started = false;

    // A bullet spans its continuation lines; flush it once the next block begins.
    fn flush(bullet: &mut Option<String>, blocks: &mut Vec<Block>) {
        if let Some(b) = bullet.take() {
            blocks.push(Block::Bullet(b));
        }
    }

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(rest) = line.strip_prefix("## ") {
            started = true;
            flush(&mut bullet, &mut blocks);
            // `## [0.5.0] - 2026-07-16` or `## [Unreleased]`
            let (version, date) = match rest.split_once("] - ") {
                Some((v, d)) => (v.trim_start_matches('['), d.trim()),
                None => (
                    rest.trim().trim_start_matches('[').trim_end_matches(']'),
                    "",
                ),
            };
            if !blocks.is_empty() {
                blocks.push(Block::Blank);
            }
            blocks.push(Block::Version {
                version: version.trim_end_matches(']').to_string(),
                date: date.to_string(),
            });
            continue;
        }
        if !started {
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            flush(&mut bullet, &mut blocks);
            blocks.push(Block::Blank);
            blocks.push(Block::Section(rest.trim().to_string()));
            continue;
        }
        if trimmed.starts_with("[") && trimmed.contains("]: http") {
            continue; // compare-link definitions
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            flush(&mut bullet, &mut blocks);
            bullet = Some(flatten_links(rest));
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut bullet, &mut blocks);
            continue;
        }
        // A continuation of the current bullet.
        if let Some(b) = bullet.as_mut() {
            b.push(' ');
            b.push_str(&flatten_links(trimmed));
        }
    }
    flush(&mut bullet, &mut blocks);
    blocks
}

/// `[text](url)` → `text`. A URL is unclickable here and swamps the line it sits on;
/// the reader wants the word, and the file is one `git show` away.
fn flatten_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open..];
        // `](` then a closing paren makes it a link; anything else is literal.
        match after.find("](").and_then(|mid| {
            let mid = open + mid;
            rest[mid + 2..].find(')').map(|end| (mid, mid + 2 + end))
        }) {
            Some((mid, end)) => {
                out.push_str(&rest[..open]);
                out.push_str(&rest[open + 1..mid]);
                rest = &rest[end + 1..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Columns a fragment occupies once drawn. Backticks and asterisks delimit inline styles
/// and are consumed by `spans`, so counting them would wrap early and leave a ragged
/// right edge.
fn display_width(s: &str) -> usize {
    s.chars().filter(|c| *c != '`' && *c != '*').count()
}

/// Greedy word wrap with a hanging indent, so a bullet's second line lines up under its
/// first word rather than under the marker.
fn wrap(text: &str, width: usize, first: &str, rest: &str) -> Vec<(String, String)> {
    let width = width.max(20);
    let mut out = Vec::new();
    let mut line = String::new();
    let mut prefix = first;

    for word in text.split_whitespace() {
        let room = width.saturating_sub(prefix.chars().count());
        if line.is_empty() {
            line.push_str(word);
        } else if display_width(&line) + 1 + display_width(word) <= room {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push((prefix.to_string(), std::mem::take(&mut line)));
            prefix = rest;
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push((prefix.to_string(), line));
    }
    out
}

/// The inline markdown this changelog actually uses: `` `code` ``, `**bold**`, `*italic*`.
/// Delimiters are balanced within a bullet and `wrap` never splits a word, so a run
/// cannot straddle a line break.
fn spans(text: &str, base: Style, code: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let (mut in_code, mut bold, mut italic) = (false, false, false);

    let style = |in_code: bool, bold: bool, italic: bool| {
        let mut s = if in_code { code } else { base };
        if bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        if italic {
            s = s.add_modifier(Modifier::ITALIC);
        }
        s
    };
    // Emit what is buffered under the style in force *before* the delimiter flips it.
    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                out.push(Span::styled(
                    std::mem::take(&mut buf),
                    style(in_code, bold, italic),
                ));
            }
        };
    }

    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                flush!();
                in_code = !in_code;
            }
            // Inside a code span, markdown punctuation is literal.
            '*' if !in_code => {
                flush!();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    bold = !bold;
                } else {
                    italic = !italic;
                }
            }
            _ => buf.push(c),
        }
    }
    flush!();
    out
}

fn render(blocks: &[Block], width: usize, theme: &Theme, title: Color) -> Vec<Line<'static>> {
    let text = theme.or("text", Color::Reset);
    let sub = theme.or("subtext0", Color::Gray);
    let accent = theme.or("accent", Color::Cyan);
    let ink = theme.or("panel_bg", Color::Rgb(16, 18, 20));

    let base = Style::default().fg(text);
    let code = Style::default().fg(accent);
    let mut lines = Vec::new();

    for b in blocks {
        match b {
            Block::Blank => lines.push(Line::from("")),
            Block::Version { version, date } => {
                let installed = version == VERSION;
                let mut row = vec![Span::styled(
                    format!(" {version} "),
                    Style::default()
                        .bg(if installed { accent } else { title })
                        .fg(ink)
                        .add_modifier(Modifier::BOLD),
                )];
                if !date.is_empty() {
                    row.push(Span::styled(format!("  {date}"), Style::default().fg(sub)));
                }
                if installed {
                    row.push(Span::styled(
                        "  ← installed",
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    ));
                }
                lines.push(Line::from(row));
            }
            Block::Section(name) => {
                // Keep a Changelog's own vocabulary, coloured by what it means for you.
                let colour = match name.as_str() {
                    "Added" => theme.or("green", Color::Green),
                    "Changed" => theme.or("blue", Color::Blue),
                    "Fixed" => theme.or("yellow", Color::Yellow),
                    "Removed" => theme.or("red", Color::Red),
                    _ => sub,
                };
                lines.push(Line::from(Span::styled(
                    format!(" {name}"),
                    Style::default().fg(colour).add_modifier(Modifier::BOLD),
                )));
            }
            Block::Bullet(t) => {
                for (prefix, chunk) in wrap(t, width, "   • ", "     ") {
                    let mut row = vec![Span::styled(prefix, Style::default().fg(sub))];
                    row.extend(spans(&chunk, base, code));
                    lines.push(Line::from(row));
                }
            }
        }
    }
    lines
}

pub struct App {
    theme: Theme,
    title_color: Color,
    blocks: Vec<Block>,
    scroll: u16,
    /// Total rendered rows at the last draw, so scrolling can stop at the end.
    height: u16,
    rows: u16,
}

enum Flow {
    Continue,
    Quit,
}

fn handle_key(app: &mut App, k: event::KeyEvent) -> Flow {
    let page = app.rows.saturating_sub(2).max(1);
    let max = app.height.saturating_sub(app.rows);
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Esc | KeyCode::Char('q') => return Flow::Quit,
        KeyCode::Char('c') if ctrl => return Flow::Quit,
        KeyCode::Down | KeyCode::Char('j') => app.scroll = (app.scroll + 1).min(max),
        KeyCode::Up | KeyCode::Char('k') => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::PageDown | KeyCode::Char(' ') => app.scroll = (app.scroll + page).min(max),
        KeyCode::PageUp => app.scroll = app.scroll.saturating_sub(page),
        KeyCode::Home | KeyCode::Char('g') => app.scroll = 0,
        KeyCode::End | KeyCode::Char('G') => app.scroll = max,
        _ => {}
    }
    Flow::Continue
}

fn draw(f: &mut Frame, app: &mut App) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());
    let area = rows[0];

    let lines = render(
        &app.blocks,
        area.width.saturating_sub(2) as usize,
        &app.theme,
        app.title_color,
    );
    app.height = lines.len() as u16;
    app.rows = area.height;
    app.scroll = app.scroll.min(app.height.saturating_sub(app.rows));

    f.render_widget(Paragraph::new(lines).scroll((app.scroll, 0)), area);
    draw_bar(f, app, rows[1]);
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let ink = t.or("panel_bg", Color::Rgb(16, 18, 20));
    let sub = t.or("subtext0", Color::Gray);

    let keys: [(&str, &str, Color); 3] = [
        ("↑ ↓", "scroll", t.or("accent", Color::Cyan)),
        ("g G", "top / end", t.or("blue", Color::Blue)),
        ("esc", "close", t.or("red", Color::Red)),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (key, label, color) in keys.iter() {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .bg(*color)
                .fg(ink)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().bg(*color).fg(ink),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("v{VERSION}"),
        Style::default().fg(sub),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `$HERDR_PLUGIN_ROOT/CHANGELOG.md` — the installed plugin is a checkout of this repo.
fn changelog_text() -> Result<String> {
    let root = std::env::var("HERDR_PLUGIN_ROOT").unwrap_or_else(|_| ".".into());
    let path = std::path::Path::new(&root).join("CHANGELOG.md");
    fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("could not read {}: {e}", path.display()))
}

/// Entry point for `herdr-ghq-switcher --changelog`.
pub fn main() -> Result<()> {
    let cfg = Config::load();
    let theme = Theme::load();
    let title_color = theme
        .resolve(&cfg.get("title_color", "peach"))
        .unwrap_or(Color::Yellow);

    let blocks = parse(&changelog_text()?);
    let mut app = App {
        theme,
        title_color,
        blocks,
        scroll: 0,
        height: 0,
        rows: 1,
    };

    let mut terminal = ratatui::init();
    let outcome = loop {
        if let Err(e) = terminal.draw(|f| draw(f, &mut app)) {
            break Err(e.into());
        }
        match event::poll(Duration::from_millis(200)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => break Err(e.into()),
        }
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                if let Flow::Quit = handle_key(&mut app, k) {
                    break Ok(());
                }
            }
            Ok(_) => {}
            Err(e) => break Err(e.into()),
        }
    };
    ratatui::restore();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Changelog

Preamble prose that is not part of any release.

## [Unreleased]

### Changed

- A change that spans
  two source lines.

## [0.4.0] - 2026-07-16

### Added

- `alt-p` toggles the preview.

[Unreleased]: https://github.com/o/r/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/o/r/releases/tag/v0.4.0
";

    #[test]
    fn parse_skips_preamble_and_link_definitions() {
        let blocks = parse(SAMPLE);
        let versions: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Version { version, .. } => Some(version.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(versions, ["Unreleased", "0.4.0"]);
        // The preamble prose and the `[x]: http…` lines must not become bullets.
        let bullets: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Bullet(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            bullets,
            [
                "A change that spans two source lines.",
                "`alt-p` toggles the preview."
            ]
        );
    }

    #[test]
    fn parse_reads_the_date_only_when_present() {
        let blocks = parse(SAMPLE);
        let dates: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Version { date, .. } => Some(date.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(dates, ["", "2026-07-16"]);
    }

    #[test]
    fn wrap_hangs_the_indent_under_the_first_word() {
        let out = wrap("one two three four five", 14, "   • ", "     ");
        assert_eq!(out[0].0, "   • ");
        assert!(out.len() > 1, "expected a wrap");
        for (prefix, _) in &out[1..] {
            assert_eq!(prefix, "     ");
        }
    }

    #[test]
    fn wrap_measures_what_is_drawn_not_the_backticks() {
        // "`a` `b` `c`" draws as "a b c" (5 columns), so it must not wrap at width 8.
        let out = wrap("`a` `b` `c`", 8, "", "");
        assert_eq!(out.len(), 1, "backticks were counted as width: {out:?}");
    }

    #[test]
    fn wrap_never_splits_a_word() {
        let out = wrap("short supercalifragilistic end", 20, "", "");
        let joined: Vec<&str> = out.iter().map(|(_, l)| l.as_str()).collect();
        assert!(joined.iter().any(|l| l.contains("supercalifragilistic")));
    }

    #[test]
    fn flatten_links_keeps_the_text_and_drops_the_url() {
        assert_eq!(
            flatten_links("needs [jq](https://jqlang.github.io/jq/) installed"),
            "needs jq installed"
        );
        // Not a link: a bare bracket must survive untouched.
        assert_eq!(flatten_links("an [aside] here"), "an [aside] here");
    }

    #[test]
    fn spans_handle_bold_italic_and_code() {
        let base = Style::default().fg(Color::White);
        let code = Style::default().fg(Color::Cyan);
        let out = spans("a **b** *c* `d`", base, code);
        let text: Vec<&str> = out.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, ["a ", "b", " ", "c", " ", "d"]);
        assert!(out[1].style.add_modifier.contains(Modifier::BOLD));
        assert!(out[3].style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(out[5].style.fg, code.fg);
    }

    #[test]
    fn spans_leave_punctuation_inside_code_alone() {
        let base = Style::default().fg(Color::White);
        let code = Style::default().fg(Color::Cyan);
        let out = spans("`a*b`", base, code);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "a*b");
    }

    #[test]
    fn spans_style_code_between_backticks() {
        let base = Style::default().fg(Color::White);
        let code = Style::default().fg(Color::Cyan);
        let out = spans("press `alt-p` now", base, code);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1].content, "alt-p");
        assert_eq!(out[1].style, code);
        assert_eq!(out[0].style, base);
    }
}
