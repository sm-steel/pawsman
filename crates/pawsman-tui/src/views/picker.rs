//! The environment picker: `none`, then every environment; broken ones show but cannot be chosen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::main::ACCENT;
use super::response::sanitize;
use super::splash::BG;

/// One row of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The environment, `None` for "none".
    pub(crate) env: Option<String>,
    /// Whether it can be chosen.
    pub(crate) valid: bool,
}

/// Draws the items in a box at the top left of `area`, `highlighted` reversed.
pub(crate) fn draw(frame: &mut Frame, area: Rect, items: &[Item], highlighted: usize) {
    let lines: Vec<Line<'_>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let (text, colour) = match (&item.env, item.valid) {
                (None, _) => ("none".to_owned(), Color::Reset),
                (Some(name), true) => (sanitize(name), Color::Reset),
                (Some(name), false) => (format!("! {} (invalid)", sanitize(name)), Color::LightRed),
            };
            let mut style = Style::new().fg(colour);
            if i == highlighted {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Line::styled(text, style)
        })
        .collect();
    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(" Environment ".len())
        .saturating_add(4);
    let height = lines.len().saturating_add(2);
    let rect = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(2),
        width: u16::try_from(width).unwrap_or(u16::MAX),
        height: u16::try_from(height).unwrap_or(u16::MAX),
    }
    .intersection(area);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" Environment ")
                .border_style(Style::new().fg(ACCENT))
                .style(Style::new().bg(BG)),
        ),
        rect,
    );
}
