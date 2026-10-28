//! The command palette: the query, then the ranked items with the matched characters marked.

use pawsman_core::fuzzy::Match;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr as _;

use super::main::ACCENT;
use super::response::sanitize;
use super::sidebar::method_color;
use super::splash::BG;
use crate::palette::Item;

const DIM: Color = Color::Rgb(110, 110, 135);
/// The narrowest box, in cells.
const MIN_WIDTH: u16 = 40;

/// Draws the palette in the top third of `area`; `at` is the highlighted row.
pub(crate) fn draw(frame: &mut Frame, area: Rect, query: &str, items: &[(Item, Match)], at: usize) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let width = (area.width * 3 / 5).max(MIN_WIDTH).min(area.width);
    let top = area.height / 6;
    let room = usize::from(area.height.saturating_sub(top + 4));
    let shown = items.len().min(room).max(1);
    let height = u16::try_from(shown + 4)
        .unwrap_or(u16::MAX)
        .min(area.height - top);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + top,
        width,
        height,
    );
    let inner_w = usize::from(width.saturating_sub(4));
    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", Style::new().fg(ACCENT)),
            Span::raw(sanitize(query)),
            Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED)),
        ]),
        Line::default(),
    ];
    if items.is_empty() {
        lines.push(Line::styled("no match", Style::new().fg(DIM)));
    }
    let first = at.saturating_sub(room.saturating_sub(1));
    for (i, (item, m)) in items.iter().enumerate().skip(first).take(room) {
        let mut line = row(item, m, inner_w);
        if i == at {
            line = line.style(Style::new().add_modifier(Modifier::REVERSED));
        }
        lines.push(line);
    }
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" Go to ")
                .border_style(Style::new().fg(ACCENT))
                .style(Style::new().bg(BG))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        ),
        rect,
    );
}

/// A tag, the label with its matched characters in bold accent, and an action's key at the right.
fn row(item: &Item, m: &Match, width: usize) -> Line<'static> {
    let (tag, colour, key) = match item {
        Item::Request { method, .. } => {
            let method = method.clone().unwrap_or_else(|| "!".to_owned());
            let colour = method_color(&method);
            (method, colour, None)
        }
        Item::Action { key, .. } => ("act".to_owned(), DIM, key.clone()),
    };
    let mut spans = vec![Span::styled(format!("{tag:<7}"), Style::new().fg(colour))];
    for (i, c) in sanitize(item.label()).chars().enumerate() {
        let style = if m.positions().contains(&i) {
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        spans.push(Span::styled(c.to_string(), style));
    }
    if let Some(key) = key {
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        let pad = width.saturating_sub(used + key.width());
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(key, Style::new().fg(DIM)));
    }
    Line::from(spans)
}
