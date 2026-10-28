//! The request list: the active environment, then the folder tree.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pawsman_core::state::Snapshot;
use pawsman_core::tree::NodeRef;
use pawsman_model::{Collection, EntryKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::main::ACCENT;
use super::response::sanitize;
use crate::tree::{self, Row};

const DIM: Color = Color::Rgb(110, 110, 135);

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

/// Draws the environment row and the visible tree rows, the selected one reversed and kept in
/// view.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    rows: &[Row],
    selected: Option<&NodeRef>,
    collapsed: &BTreeSet<PathBuf>,
) {
    let Some(collection) = snapshot.collection() else {
        return;
    };
    let env = snapshot.active_env().unwrap_or("none");
    let mut lines = vec![Line::from(vec![
        Span::styled("env: ", Style::new().fg(DIM)),
        Span::styled(sanitize(env), Style::new().fg(ACCENT)),
    ])];
    let room = usize::from(area.height.saturating_sub(1)).max(1);
    let at = rows
        .iter()
        .position(|r| Some(&r.node) == selected)
        .unwrap_or(0);
    let first = at.saturating_sub(room - 1);
    for row in rows.iter().skip(first).take(room) {
        let mut line = row_line(collection, row, collapsed);
        if Some(&row.node) == selected {
            line = line.style(Style::new().add_modifier(Modifier::REVERSED));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(collection: &Collection, row: &Row, collapsed: &BTreeSet<PathBuf>) -> Line<'static> {
    let indent = "  ".repeat(row.depth);
    match &row.node {
        NodeRef::Folder(path) => {
            let Some(folder) = tree::folder(collection, path) else {
                return Line::default();
            };
            let (mark, colour) = if folder.error().is_some() {
                ("! ", Color::LightRed)
            } else if collapsed.contains(path) {
                ("▸ ", DIM)
            } else {
                ("▾ ", DIM)
            };
            Line::styled(
                format!("{indent}{mark}{}", sanitize(folder.name())),
                Style::new().fg(colour),
            )
        }
        NodeRef::Request(id) => {
            let Some(entry) = collection.entry(*id) else {
                return Line::default();
            };
            if let EntryKind::Request(r) = entry.kind() {
                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!("{:<7}", r.method()),
                        Style::new().fg(method_color(r.method())),
                    ),
                    Span::raw(sanitize(r.name())),
                ])
            } else {
                let file = entry
                    .path()
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                Line::from(vec![
                    Span::raw(indent),
                    Span::styled("!      ", Style::new().fg(Color::LightRed)),
                    Span::styled(sanitize(&file), Style::new().fg(Color::LightRed)),
                ])
            }
        }
    }
}
