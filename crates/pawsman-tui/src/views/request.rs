//! The selected request as it will be sent: variables resolved, secrets masked, and on demand
//! the trace of where each value came from.

use pawsman_core::resolve::Preview;
use pawsman_core::tree::NodeRef;
use pawsman_error::ErrorInfo as _;
use pawsman_model::trace::{Field, TraceValue};
use pawsman_model::{EntryKind, Folder, Request};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::main::{MainView, TraceView};
use super::response::sanitize;
use super::sidebar::method_color;
use crate::tree;

const DIM: Color = Color::Rgb(110, 110, 135);
/// How a secret is drawn.
const MASK: &str = "••••••";

/// Draws the resolved request, or the template and why it does not resolve, or a file's error.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &MainView<'_>) {
    let entry = view
        .snapshot
        .collection()
        .zip(view.selected)
        .and_then(|(c, id)| c.entry(id));
    let Some(entry) = entry else {
        let folder = view
            .snapshot
            .collection()
            .zip(view.node)
            .and_then(|(c, node)| match node {
                NodeRef::Folder(path) => tree::folder(c, path),
                NodeRef::Request(_) => None,
            });
        let lines = folder.map_or_else(
            || vec![Line::styled("no request selected", Style::new().fg(DIM))],
            folder_summary,
        );
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    };
    let mut lines: Vec<Line<'_>> = Vec::new();
    match (entry.kind(), view.preview) {
        (EntryKind::Request(_), Some(Ok(preview))) => resolved(&mut lines, preview, view.trace),
        (EntryKind::Request(request), Some(Err(notice))) => {
            template(&mut lines, request);
            lines.push(Line::default());
            lines.push(Line::styled(
                sanitize(notice.text()),
                Style::new().fg(Color::LightRed),
            ));
        }
        (EntryKind::Request(request), None) => template(&mut lines, request),
        (EntryKind::Invalid(err), _) => {
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

fn method_span(method: &str) -> Span<'static> {
    Span::styled(
        format!("{method} "),
        Style::new()
            .fg(method_color(method))
            .add_modifier(Modifier::BOLD),
    )
}

fn header_line(name: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{}: ", sanitize(name)), Style::new().fg(DIM)),
        Span::raw(sanitize(value)),
    ])
}

fn resolved(lines: &mut Vec<Line<'_>>, preview: &Preview, trace: TraceView) {
    lines.push(Line::from(vec![
        method_span(preview.method()),
        Span::raw(sanitize(preview.url())),
    ]));
    for (name, value) in preview.headers() {
        lines.push(header_line(name, value));
    }
    if let Some(body) = preview.body_text() {
        lines.push(Line::default());
        lines.extend(sanitize(body).lines().map(|l| Line::raw(l.to_owned())));
    }
    if preview.has_m2_parts() {
        lines.push(Line::default());
        lines.push(Line::styled(
            "captures, asserts and scripts run from M2",
            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
        ));
    }
    if trace == TraceView::Hidden {
        return;
    }
    lines.push(Line::default());
    let title = match preview.env() {
        Some(env) => format!("Trace · env {}", sanitize(env)),
        None => "Trace · no environment".to_owned(),
    };
    lines.push(Line::styled(
        title,
        Style::new().fg(DIM).add_modifier(Modifier::BOLD),
    ));
    if preview.trace().entries().is_empty() {
        lines.push(Line::styled("no variables", Style::new().fg(DIM)));
    }
    for entry in preview.trace().entries() {
        let indent = if matches!(entry.field(), Field::Var(_)) {
            "  "
        } else {
            ""
        };
        let value = match entry.value() {
            TraceValue::Plain(value) => sanitize(value),
            TraceValue::Secret => MASK.to_owned(),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{indent}{}  ", entry.field()), Style::new().fg(DIM)),
            Span::raw(format!("{{{{{}}}}}", sanitize(entry.name()))),
            Span::styled(
                format!(" ← {} ", entry.source()),
                Style::new().fg(super::main::ACCENT),
            ),
            Span::raw(format!("= {value}")),
        ]));
    }
}

/// The request as written, before resolution.
fn template(lines: &mut Vec<Line<'_>>, request: &Request) {
    lines.push(Line::from(vec![
        method_span(request.method()),
        Span::raw(sanitize(request.url())),
    ]));
    for (name, value) in request.headers() {
        lines.push(header_line(name, value));
    }
    if let Some(body) = request.body_text() {
        lines.push(Line::default());
        lines.extend(sanitize(body).lines().map(|l| Line::raw(l.to_owned())));
    }
}

/// A folder: its name, path, how many requests it holds, and the defaults it gives them.
fn folder_summary(folder: &Folder) -> Vec<Line<'static>> {
    let n = tree::count(folder);
    let noun = if n == 1 { "request" } else { "requests" };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("folder ", Style::new().fg(DIM)),
            Span::styled(
                sanitize(folder.name()),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            format!(
                "{}/ · {n} {noun}",
                sanitize(&folder.path().display().to_string())
            ),
            Style::new().fg(DIM),
        ),
    ];
    if let Some(err) = folder.error() {
        lines.push(Line::default());
        lines.push(Line::styled(
            sanitize(&format!("{}: {err}", err.code())),
            Style::new().fg(Color::LightRed),
        ));
    }
    for (title, pairs) in [("Headers", folder.headers()), ("Vars", folder.vars())] {
        if pairs.is_empty() {
            continue;
        }
        lines.push(Line::default());
        lines.push(Line::styled(title, Style::new().fg(DIM)));
        lines.extend(
            pairs
                .iter()
                .map(|(k, v)| Line::raw(format!("{}: {}", sanitize(k), sanitize(v)))),
        );
    }
    lines
}
