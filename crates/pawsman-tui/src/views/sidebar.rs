//! The request list.

use pawsman_core::state::Snapshot;
use pawsman_model::{EntryKind, RequestId};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// The colour of an HTTP method.
pub(crate) fn method_color(method: &str) -> Color {
    match method {
        "GET" => Color::Rgb(120, 220, 140),
        "POST" => Color::Rgb(240, 200, 90),
        "PUT" => Color::Rgb(110, 160, 255),
        "PATCH" => Color::Rgb(200, 140, 255),
        "DELETE" => Color::Rgb(255, 110, 110),
        _ => Color::Rgb(170, 170, 190),
    }
}

/// Draws one row per entry, the selected one reversed.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    selected: Option<RequestId>,
) {
    let Some(collection) = snapshot.collection() else {
        return;
    };
    let lines: Vec<Line<'_>> = collection
        .entries()
        .iter()
        .map(|entry| {
            let mut line = match entry.kind() {
                EntryKind::Request(r) => Line::from(vec![
                    Span::styled(
                        format!("{:<7}", r.method()),
                        Style::new().fg(method_color(r.method())),
                    ),
                    Span::raw(r.name().to_owned()),
                ]),
                _ => {
                    let file = entry
                        .path()
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    Line::from(vec![
                        Span::styled("!      ", Style::new().fg(Color::LightRed)),
                        Span::styled(file, Style::new().fg(Color::LightRed)),
                    ])
                }
            };
            if Some(entry.id()) == selected {
                line = line.style(Style::new().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}
