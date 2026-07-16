//! Rendering: Search input (top), Switcher list (middle), Preview (below), and
//! a full-width colourful command bar pinned to the very bottom.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Frame;

use crate::App;

pub fn draw(f: &mut Frame, app: &mut App) {
    let accent = app.theme.or("accent", Color::Cyan);
    let text = app.theme.or("text", Color::Reset);
    let sub = app.theme.or("subtext0", Color::DarkGray);
    let overlay = app.theme.or("overlay0", Color::DarkGray);
    let surface = app.theme.or("surface1", Color::Indexed(236));

    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(f.area());

    // Body: list + preview. The footer (root[2]) is always a separate full-width
    // row, so the preview can sit on any side without shrinking the command bar.
    let body = root[1];
    let (list_area, preview_area) = if app.preview_enabled {
        let pct = app.preview_pct;
        let rest = 100u16.saturating_sub(pct);
        match app.preview_position.as_str() {
            "right" => {
                let c = Layout::horizontal([Constraint::Percentage(rest), Constraint::Percentage(pct)])
                    .split(body);
                (c[0], Some(c[1]))
            }
            "left" => {
                let c = Layout::horizontal([Constraint::Percentage(pct), Constraint::Percentage(rest)])
                    .split(body);
                (c[1], Some(c[0]))
            }
            "up" => {
                let c = Layout::vertical([Constraint::Percentage(pct), Constraint::Percentage(rest)])
                    .split(body);
                (c[1], Some(c[0]))
            }
            _ => {
                let c = Layout::vertical([Constraint::Percentage(rest), Constraint::Percentage(pct)])
                    .split(body);
                (c[0], Some(c[1]))
            }
        }
    } else {
        (body, None)
    };

    let title = app.title_color;
    draw_input(f, app, root[0], title, accent, sub, overlay);
    draw_list(f, app, list_area, title, accent, text, overlay, surface);
    if let Some(area) = preview_area {
        draw_preview(f, app, area, title, overlay);
    }
    draw_footer(f, app, root[2]);

    if app.show_help {
        draw_help(f, app, f.area());
    }
}

fn boxed(title: &str, accent: Color, border: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
}

fn draw_input(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: Color,
    accent: Color,
    sub: Color,
    border: Color,
) {
    let count = format!(" {}/{} ", app.filtered.len(), app.entries.len());
    let block = boxed("Search", title, border)
        .title(Line::from(Span::styled(count, Style::default().fg(sub))).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let line = Line::from(vec![
        Span::styled("  ", Style::default().fg(accent)),
        Span::raw(&app.query),
    ]);
    f.render_widget(Paragraph::new(line), inner);
    // Cursor after the prompt + query.
    let cx = inner.x + 2 + app.query.chars().count() as u16;
    f.set_cursor_position(Position::new(cx.min(inner.x + inner.width.saturating_sub(1)), inner.y));
}

#[allow(clippy::too_many_arguments)]
fn draw_list(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    title: Color,
    accent: Color,
    text: Color,
    border: Color,
    surface: Color,
) {
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let e = &app.entries[i];
            let mut primary = e.primary.clone();
            let width = 38usize;
            let plen = primary.chars().count();
            if plen < width {
                primary.push_str(&" ".repeat(width - plen));
            }
            // The secondary column carries the entry's own colour (host tint
            // for repos, live state for agents, accent for workspaces) so the
            // list reads as colourful at a glance instead of a wall of grey.
            ListItem::new(Line::from(vec![
                Span::styled(e.icon.clone(), Style::default().fg(e.icon_color)),
                Span::raw(" "),
                Span::styled(primary, Style::default().fg(text)),
                Span::raw(" "),
                Span::styled(
                    e.secondary.clone(),
                    Style::default()
                        .fg(e.icon_color)
                        .add_modifier(Modifier::DIM),
                ),
            ]))
        })
        .collect();

    // Title row = a group tab strip (All + each present kind) with the active
    // tab highlighted, plus a right-aligned sort indicator.
    let ink = app.theme.or("panel_bg", Color::Rgb(16, 18, 20));
    let mut tab_spans: Vec<Span> = Vec::new();
    for g in app.tabs() {
        let style = if g == app.group {
            Style::default().fg(ink).bg(title).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(border)
        };
        tab_spans.push(Span::styled(format!(" {} ", g.label()), style));
        tab_spans.push(Span::raw(" "));
    }
    let sort_hint = Span::styled(
        format!(" sort: {} ", app.sort.label()),
        Style::default().fg(border),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Line::from(tab_spans))
        .title(Line::from(sort_hint).right_aligned());

    let list = List::new(items)
        .block(block)
        .highlight_symbol("▌ ")
        .highlight_style(
            Style::default()
                .fg(accent)
                .bg(surface)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect, title: Color, border: Color) {
    let block = boxed("󰈈 Preview", title, border);
    let para = Paragraph::new(app.preview.clone())
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.preview_scroll, 0));
    f.render_widget(para, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    // Dark ink for text sitting on the coloured pills.
    let ink = t.or("panel_bg", Color::Rgb(16, 18, 20));
    let keys: [(&str, &str, Color); 10] = [
        ("↵", "open", t.or("accent", Color::Cyan)),
        ("^t", "tab", t.or("green", Color::Green)),
        ("^s", "split", t.or("yellow", Color::Yellow)),
        ("^o", "cd", t.or("blue", Color::Blue)),
        ("^w", "workspace", t.or("mauve", Color::Magenta)),
        ("^g", "git", t.or("peach", Color::Yellow)),
        ("^u", "update", t.or("teal", Color::Cyan)),
        ("^x", "remove", t.or("red", Color::Red)),
        ("⌥↵", "clone", t.or("blue", Color::Magenta)),
        ("?", "help", t.or("lavender", Color::White)),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (key, label, color) in keys.iter() {
        // Each command is a coloured pill: bold key + full label, dark ink.
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
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// A centred, colourful keybindings cheatsheet drawn on top of everything.
fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let ink = t.or("panel_bg", Color::Rgb(16, 18, 20));
    let text = t.or("text", Color::Reset);
    let sub = t.or("subtext0", Color::Gray);
    let title = app.title_color;
    let border = t.or("accent", Color::Cyan);

    // A row: a colour-filled key pill followed by its description.
    let row = |key: &str, color: Color, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!(" {key:<8}"),
                Style::default().bg(color).fg(ink).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(desc.to_string(), Style::default().fg(text)),
        ])
    };
    let head = |s: &str| -> Line<'static> {
        Line::from(Span::styled(
            s.to_string(),
            Style::default().fg(title).add_modifier(Modifier::BOLD),
        ))
    };
    let blank = || Line::from("");

    let green = t.or("green", Color::Green);
    let yellow = t.or("yellow", Color::Yellow);
    let blue = t.or("blue", Color::Blue);
    let mauve = t.or("mauve", Color::Magenta);
    let peach = t.or("peach", Color::Yellow);
    let teal = t.or("teal", Color::Cyan);
    let red = t.or("red", Color::Red);

    let left = vec![
        head(" Navigate"),
        row("↑ / ↓", border, "Move selection"),
        row("^j / ^k", border, "Down / up (vim)"),
        row("^n / ^p", border, "Down / up (emacs)"),
        row("PgUp/Dn", border, "Jump by 10"),
        row("Tab", teal, "Next group"),
        row("⇧Tab", teal, "Prev group"),
        row("type…", green, "Fuzzy filter"),
        row("⌫", sub, "Delete a character"),
        blank(),
        head(" General"),
        row("?", title, "Toggle this help"),
        row("Esc", red, "Close / quit"),
        row("^c", red, "Quit"),
    ];
    let right = vec![
        head(" Open"),
        row("↵", border, "Open (default)"),
        row("⌥↵", blue, "Clone repo"),
        row("^t", green, "Open in new tab"),
        row("^s", yellow, "Open in split"),
        row("^o", blue, "cd pane here"),
        blank(),
        head(" Manage"),
        row("^w", mauve, "Send to workspace"),
        row("^g", peach, "Git actions"),
        row("^u", teal, "Update repo"),
        row("^x", red, "Remove"),
        blank(),
        head(" View"),
        row("⌥s", blue, "Cycle sort order"),
        row("⌥p", mauve, "Toggle preview"),
    ];

    // Centre a comfortably sized popup within the screen.
    let w = area.width.saturating_sub(6).clamp(40, 64);
    let want_h = left.len().max(right.len()) as u16 + 4;
    let h = want_h.min(area.height.saturating_sub(2)).max(8);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(ink))
        .title(Span::styled(
            "  Keybindings ",
            Style::default().fg(title).add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(Span::styled(
                " any key to close ",
                Style::default().fg(sub),
            ))
            .right_aligned(),
        );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .horizontal_margin(1)
        .vertical_margin(1)
        .split(inner);
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(right), cols[1]);
}
