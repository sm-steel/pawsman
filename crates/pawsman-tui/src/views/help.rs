//! The F1 overlay: every row of the key table, the hint bar's first, in two columns when they
//! fit, scrolled when even that is too tall.

use pawsman_core::keys::{Context, Keymap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr as _;

use super::hints;
use super::splash::BG;

/// Cells between the two columns.
const GAP: usize = 3;

/// Draws the help box centred in `area`, its rows scrolled by `scroll`; returns the scroll used,
/// clamped to the rows that exist.
pub(crate) fn draw(frame: &mut Frame, area: Rect, keys: &Keymap, scroll: u16) -> u16 {
    if area.width < 3 || area.height < 3 {
        return 0;
    }
    let mut bindings: Vec<_> = keys
        .bindings()
        .iter()
        .filter(|b| !b.chords().is_empty())
        .collect();
    // The hint bar's rows first, so quit, help and close are on the first page.
    bindings.sort_by_key(|b| !b.in_bar());
    let rows: Vec<(String, &str)> = bindings
        .into_iter()
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
    let row_w = rows
        .iter()
        .map(|(_, h)| key_w + 2 + h.width())
        .max()
        .unwrap_or(0);
    let cell = |(keys, hint): &(String, &'static str)| {
        vec![
            Span::styled(
                format!("{keys:<key_w$}  "),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{hint:<0$}", row_w - key_w - 2)),
        ]
    };
    let inner_h = usize::from(area.height.saturating_sub(4));
    let two = rows.len() > inner_h && 2 * row_w + GAP + 4 <= usize::from(area.width);
    let mut lines: Vec<Line<'_>> = if two {
        let half = rows.len().div_ceil(2);
        (0..half)
            .map(|i| {
                let mut spans = rows.get(i).map(cell).unwrap_or_default();
                if let Some(right) = rows.get(i + half) {
                    spans.push(Span::raw(" ".repeat(GAP)));
                    spans.extend(cell(right));
                }
                Line::from(spans)
            })
            .collect()
    } else {
        rows.iter().map(|r| Line::from(cell(r))).collect()
    };
    let body = lines.len();
    let max_scroll = u16::try_from(body.saturating_sub(inner_h)).unwrap_or(u16::MAX);
    let scroll = scroll.min(max_scroll);
    lines.push(Line::default());
    lines.push(hints::line(keys, Context::Help));
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
    let title = if max_scroll > 0 {
        " keys · ↑↓ scroll "
    } else {
        " keys "
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)).block(
            Block::bordered()
                .title(title)
                .border_style(Style::new().fg(Color::Rgb(90, 210, 255)))
                .style(Style::new().bg(BG).fg(Color::Rgb(220, 220, 235)))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        ),
        rect,
    );
    scroll
}
