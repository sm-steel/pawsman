//! The selected request, read-only in this piece.

use pawsman_core::state::Snapshot;
use pawsman_error::ErrorInfo as _;
use pawsman_model::{EntryKind, RequestId};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::response::sanitize;
use super::sidebar::method_color;

const DIM: Color = Color::Rgb(110, 110, 135);

/// Draws the method and URL, headers and body of the selected request, or its error.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    selected: Option<RequestId>,
) {
    let entry = snapshot
        .collection()
        .zip(selected)
        .and_then(|(c, id)| c.entries().iter().find(|e| e.id() == id));
    let Some(entry) = entry else {
        frame.render_widget(
            Paragraph::new(Span::styled("no request selected", Style::new().fg(DIM))),
            area,
        );
        return;
    };
    let mut lines: Vec<Line<'_>> = Vec::new();
    match entry.kind() {
        EntryKind::Request(r) => {
            if r.has_template() {
                lines.push(Line::styled(
                    "variables are not resolved yet",
                    Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                ));
            }
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", r.method()),
                    Style::new()
                        .fg(method_color(r.method()))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(sanitize(r.url())),
            ]));
            for (name, value) in r.headers() {
                lines.push(Line::from(vec![
                    Span::styled(format!("{name}: "), Style::new().fg(DIM)),
                    Span::raw(sanitize(value)),
                ]));
            }
            if let Some(body) = r.body_text() {
                lines.push(Line::default());
                lines.extend(sanitize(body).lines().map(|l| Line::raw(l.to_owned())));
            }
        }
        EntryKind::Invalid(err) => {
            lines.push(Line::styled(
                format!("{}: {err}", err.code()),
                Style::new().fg(Color::LightRed),
            ));
            if let Some(help) = err.help() {
                lines.push(Line::styled(help.into_owned(), Style::new().fg(DIM)));
            }
        }
        _ => {}
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
