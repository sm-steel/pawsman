//! The request list: the active environment, then the folder tree.

use pawsman_core::state::Snapshot;
use pawsman_model::{Collection, EntryKind, Folder, Node, RequestId};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::main::ACCENT;
use super::response::sanitize;

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

/// Draws the environment row and one row per folder and request, the selected one reversed.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    selected: Option<RequestId>,
) {
    let Some(collection) = snapshot.collection() else {
        return;
    };
    let env = snapshot.active_env().unwrap_or("none");
    let mut lines = vec![Line::from(vec![
        Span::styled("env: ", Style::new().fg(DIM)),
        Span::styled(sanitize(env), Style::new().fg(ACCENT)),
    ])];
    rows(
        collection,
        collection.root_folder(),
        0,
        selected,
        &mut lines,
    );
    frame.render_widget(Paragraph::new(lines), area);
}

fn rows(
    collection: &Collection,
    folder: &Folder,
    depth: usize,
    selected: Option<RequestId>,
    lines: &mut Vec<Line<'static>>,
) {
    let indent = "  ".repeat(depth);
    for node in folder.children() {
        match node {
            Node::Folder(child) => {
                let (mark, colour) = if child.error().is_some() {
                    ("! ", Color::LightRed)
                } else {
                    ("▾ ", DIM)
                };
                lines.push(Line::styled(
                    format!("{indent}{mark}{}", sanitize(child.name())),
                    Style::new().fg(colour),
                ));
                rows(collection, child, depth + 1, selected, lines);
            }
            Node::Entry(id) => {
                let Some(entry) = collection.entry(*id) else {
                    continue;
                };
                let mut line = if let EntryKind::Request(r) = entry.kind() {
                    Line::from(vec![
                        Span::raw(indent.clone()),
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
                        Span::raw(indent.clone()),
                        Span::styled("!      ", Style::new().fg(Color::LightRed)),
                        Span::styled(sanitize(&file), Style::new().fg(Color::LightRed)),
                    ])
                };
                if Some(*id) == selected {
                    line = line.style(Style::new().add_modifier(Modifier::REVERSED));
                }
                lines.push(line);
            }
        }
    }
}
