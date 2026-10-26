//! The hint bar, generated from the key table.

use pawsman_core::keys::Context;
use ratatui::text::Line;

/// `Ctrl-Q quit · F1 help` for `context`.
pub(crate) fn line(context: Context) -> Line<'static> {
    let _ = context;
    Line::default()
}
