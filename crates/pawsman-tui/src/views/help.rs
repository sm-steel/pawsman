//! The F1 overlay: every row of the key table.

use pawsman_core::keys::{Context, table};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr as _;

use super::hints;
use super::splash::BG;

/// Draws the help box centred in `area`.
pub(crate) fn draw(frame: &mut Frame, area: Rect) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let rows: Vec<(String, &str)> = table()
        .iter()
        .map(|b| {
            let keys = b
                .chords()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" / ");
            (keys, b.hint())
        })
        .collect();
    let key_w = rows.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let mut lines: Vec<Line<'_>> = rows
        .iter()
        .map(|(keys, hint)| {
            Line::from(vec![
                Span::styled(
                    format!("{keys:<key_w$}  "),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(*hint),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines.push(hints::line(Context::Help));
    let text_w = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(text_w + 4)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" keys ")
                .border_style(Style::new().fg(Color::Rgb(90, 210, 255)))
                .style(Style::new().bg(BG).fg(Color::Rgb(220, 220, 235)))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        ),
        rect,
    );
}
