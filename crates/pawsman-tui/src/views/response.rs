//! The response to the selected request: a status line and the raw body.

use pawsman_core::state::RequestStatus;
use pawsman_error::ErrorInfo as _;
use pawsman_net::Body;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::main::MainView;

const DIM: Color = Color::Rgb(110, 110, 135);

/// Text safe to put in cells: CRLF becomes LF, tabs become spaces, other control characters
/// become `�`, so a body can never move the cursor or change modes.
pub(crate) fn sanitize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .map(|c| match c {
            '\n' => "\n".to_owned(),
            '\t' => "    ".to_owned(),
            c if c.is_control() => "\u{fffd}".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// `1.2 KB`, `18.3 MB`.
pub(crate) fn size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    let tenths = |unit: u64| (bytes * 10 + unit / 2) / unit;
    match bytes {
        b if b < KB => format!("{b} B"),
        b if b < MB => {
            let t = tenths(KB);
            format!("{}.{} KB", t / 10, t % 10)
        }
        _ => {
            let t = tenths(MB);
            format!("{}.{} MB", t / 10, t % 10)
        }
    }
}

fn status_color(status: u16) -> Color {
    match status {
        200..=299 => Color::Rgb(120, 220, 140),
        300..=399 => Color::Rgb(90, 210, 255),
        400..=499 => Color::Rgb(240, 200, 90),
        _ => Color::Rgb(255, 110, 110),
    }
}

/// Draws the status line and the body, scrolled by `view.scroll`.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &MainView<'_>) {
    let Some(id) = view.selected else {
        return;
    };
    let mut lines: Vec<Line<'_>> = Vec::new();
    match view.snapshot.status(id) {
        RequestStatus::Idle => lines.push(Line::styled("F5 sends", Style::new().fg(DIM))),
        RequestStatus::Sending { since } => {
            let secs = view.now.saturating_duration_since(*since).as_secs_f64();
            lines.push(Line::styled(
                format!("sending… {secs:.1} s"),
                Style::new().fg(Color::Rgb(90, 210, 255)),
            ));
        }
        RequestStatus::Done(r) => {
            let ms = r.elapsed().as_millis();
            lines.push(Line::styled(
                format!(
                    "{} {} · {ms} ms · {}",
                    r.status(),
                    r.reason(),
                    size(r.body().len())
                ),
                Style::new()
                    .fg(status_color(r.status()))
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::default());
            let text = sanitize(&String::from_utf8_lossy(r.body().preview()));
            lines.extend(text.lines().map(|l| Line::raw(l.to_owned())));
            if let Body::Spilled { len, head, .. } = r.body() {
                let rest = len.saturating_sub(u64::try_from(head.len()).unwrap_or(0));
                lines.push(Line::styled(
                    format!("… {} more in a temp file", size(rest)),
                    Style::new().fg(DIM),
                ));
            }
        }
        RequestStatus::Failed(err) => {
            lines.push(Line::styled(
                format!("{}: {err}", err.code()),
                Style::new().fg(Color::LightRed),
            ));
            if let Some(help) = err.help() {
                lines.push(Line::styled(help.into_owned(), Style::new().fg(DIM)));
            }
        }
        RequestStatus::Cancelled => {
            lines.push(Line::styled("cancelled", Style::new().fg(Color::Yellow)));
        }
        _ => {}
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((view.scroll, 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_neutralises_escapes_and_keeps_newlines() {
        assert_eq!(
            sanitize("ok\x1b[2J\x07\x00end"),
            "ok\u{fffd}[2J\u{fffd}\u{fffd}end",
            "escapes"
        );
        assert_eq!(sanitize("a\r\nb\tc"), "a\nb    c", "crlf and tabs");
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(size(0), "0 B", "bytes");
        assert_eq!(size(1023), "1023 B", "bytes");
        assert_eq!(size(1229), "1.2 KB", "kilobytes");
        assert_eq!(size(19_188_940), "18.3 MB", "megabytes");
    }
}
