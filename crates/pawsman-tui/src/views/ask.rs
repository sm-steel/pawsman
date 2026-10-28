//! The yes or no question before a delete.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use unicode_width::UnicodeWidthStr as _;

use super::prompt::{boxed, hint};
use super::response::sanitize;

/// Draws the question centred in `area`.
pub(crate) fn draw(frame: &mut Frame, area: Rect, text: &str) {
    let text = sanitize(text);
    let room = usize::from(area.width.saturating_sub(6)).max(1);
    let wrapped = u16::try_from(text.width().div_ceil(room))
        .unwrap_or(1)
        .max(1);
    let inner = boxed(frame, area, "Delete", text.width(), wrapped + 2);
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(text),
            Line::default(),
            hint("Enter/y yes · Esc/n no"),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}
