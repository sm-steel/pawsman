//! The one-line name prompt: a title, the text with a cursor, and its keys.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use super::main::ACCENT;
use super::response::sanitize;
use super::splash::BG;

const DIM: Color = Color::Rgb(110, 110, 135);
/// The narrowest box, in cells.
const MIN_WIDTH: u16 = 40;

/// Draws the prompt centred in `area`.
pub(crate) fn draw(frame: &mut Frame, area: Rect, title: &str, text: &str) {
    let inner = boxed(frame, area, title, title.width(), 3);
    if inner.width < 2 {
        return;
    }
    let shown = tail(&sanitize(text), usize::from(inner.width) - 1);
    let line = Line::from(vec![
        Span::raw(shown),
        Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED)),
    ]);
    frame.render_widget(
        Paragraph::new(vec![line, Line::default(), hint("Enter ok · Esc cancel")]),
        inner,
    );
}

/// The end of `text` that fits in `width` cells, `…` first when cut.
fn tail(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut kept = Vec::new();
    let mut used = 1;
    for g in text.graphemes(true).rev() {
        used += g.width();
        if used > width {
            break;
        }
        kept.push(g);
    }
    kept.reverse();
    format!("…{}", kept.concat())
}

/// A dim line of key hints.
pub(crate) fn hint(text: &str) -> Line<'static> {
    Line::styled(text.to_owned(), Style::new().fg(DIM))
}

/// Draws a titled box centred in `area`, wide enough for `content` cells (at least 40) and
/// `rows` lines; returns its inside.
pub(crate) fn boxed(frame: &mut Frame, area: Rect, title: &str, content: usize, rows: u16) -> Rect {
    let width = u16::try_from(content.max(title.width()) + 6)
        .unwrap_or(u16::MAX)
        .max(MIN_WIDTH)
        .min(area.width);
    let height = (rows + 2).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    );
    let block = Block::bordered()
        .title(format!(" {} ", sanitize(title)))
        .border_style(Style::new().fg(ACCENT))
        .style(Style::new().bg(BG))
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_shows_its_end() {
        assert_eq!(tail("abcdef", 4), "…def");
        assert_eq!(tail("abc", 4), "abc");
        assert_eq!(tail("日本語", 4), "…語", "wide characters never split");
    }
}
