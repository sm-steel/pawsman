//! The hint bar, generated from the key table.

use pawsman_core::keys::{Context, Keymap};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// `Ctrl-Q quit · F1 help` for `context`.
pub(crate) fn line(keys: &Keymap, context: Context) -> Line<'static> {
    build(&keys.hints(context))
}

/// The hint bar for `context` in `width` cells: hints are dropped from the right until it fits,
/// but quit and help always stay.
pub(crate) fn fit(keys: &Keymap, context: Context, width: u16) -> Line<'static> {
    let mut kept = keys.hints(context);
    loop {
        let bar = build(&kept);
        if bar.width() <= usize::from(width) {
            return bar;
        }
        let droppable = (0..kept.len())
            .rev()
            .filter(|i| kept.get(*i).is_some_and(|(_, h)| keep_rank(h).is_some()))
            .min_by_key(|i| kept.get(*i).and_then(|(_, h)| keep_rank(h)));
        match droppable {
            Some(i) if kept.len() > 1 => {
                kept.remove(i);
            }
            _ => return bar,
        }
    }
}

/// How long a hint stays as the bar narrows, lowest dropped first; `None` never drops.
fn keep_rank(hint: &str) -> Option<u8> {
    match hint {
        "quit" | "help" => None,
        "trace" => Some(0),
        "env" => Some(1),
        "palette" => Some(2),
        "pane" => Some(3),
        _ => Some(4),
    }
}

fn build(hints: &[(String, &'static str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, hint)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(
            key.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {hint}")));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_narrow_bar_keeps_quit_and_help() {
        let bar = fit(&Keymap::default(), Context::Main, 60);

        assert!(bar.width() <= 60, "fits: {}", text(&bar));
        assert!(text(&bar).contains("Ctrl-Q quit"), "{}", text(&bar));
        assert!(text(&bar).ends_with("F1 help"), "{}", text(&bar));
        assert!(
            text(&bar).starts_with("F5 send"),
            "send stays first: {}",
            text(&bar)
        );
        assert!(
            text(&bar).contains("Esc cancel"),
            "cancel outlives env and trace: {}",
            text(&bar)
        );
    }

    #[test]
    fn a_wide_bar_has_every_hint() {
        let keys = Keymap::default();

        assert_eq!(
            text(&fit(&keys, Context::Main, 200)),
            text(&line(&keys, Context::Main))
        );
    }
}
