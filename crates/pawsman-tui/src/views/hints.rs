//! The hint bar, generated from the key table.

use pawsman_core::keys::{Context, hints};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// `Ctrl-Q quit · F1 help` for `context`.
pub(crate) fn line(context: Context) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, hint)) in hints(context).into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(key, Style::new().add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(format!(" {hint}")));
    }
    Line::from(spans)
}
