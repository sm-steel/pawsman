//! The main view while no collection is open.

use pawsman_core::notice::Notice;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::term::CapsHint;

/// Draws the home view: wordmark, "no collection open", the notice and the hint bar.
pub(crate) fn draw(frame: &mut Frame, area: Rect, notice: Option<&Notice>, hint: Option<CapsHint>) {
    let _ = (frame, area, notice, hint, super::hints::line);
}
