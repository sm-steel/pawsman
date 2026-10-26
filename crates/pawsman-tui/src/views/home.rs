//! The main view while no collection is open.

use pawsman_core::keys::Context;
use pawsman_core::notice::{Level, Notice};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::hints;
use super::splash::{BG, LOGO_H, LOGO_W, draw_logo};
use crate::term::Caps;

const DIM: Color = Color::Rgb(150, 150, 175);

/// Draws the home view: wordmark, "no collection open", the notice and the hint bar.
pub(crate) fn draw(frame: &mut Frame, area: Rect, notice: Option<&Notice>, caps: &Caps) {
    let hint = caps.hint();
    frame.render_widget(Block::new().style(Style::new().bg(BG)), area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let body_bottom = area.bottom().saturating_sub(2);
    let mid = area.y + area.height.saturating_sub(2) / 2;
    let logo_fits = area.width >= LOGO_W + 2 && area.height >= LOGO_H + 6;
    let message_y = if logo_fits {
        let logo_y = mid.saturating_sub(LOGO_H / 2 + 1);
        draw_logo(
            frame.buffer_mut(),
            area.x + (area.width - LOGO_W) / 2,
            logo_y,
            caps.color(),
        );
        logo_y + 4
    } else {
        line_at(
            frame,
            area,
            mid.saturating_sub(1),
            Line::from(Span::styled(
                "pawsman",
                Style::new()
                    .fg(Color::Rgb(255, 105, 180))
                    .add_modifier(Modifier::BOLD),
            )),
        );
        mid
    };
    if message_y < body_bottom {
        line_at(
            frame,
            area,
            message_y,
            Line::from(Span::styled("no collection open", Style::new().fg(DIM))),
        );
    }
    if let Some(notice) = notice
        && area.height >= 2
    {
        let style = match notice.level() {
            Level::Warn => Style::new().fg(Color::Yellow),
            Level::Error => Style::new().fg(Color::LightRed),
            _ => Style::new().fg(DIM),
        };
        let row = Rect::new(area.x, area.bottom() - 2, area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(notice.text().to_owned(), style)),
            row,
        );
    }
    let mut bar = hints::line(Context::Home);
    if let Some(hint) = hint {
        bar.push_span(Span::raw(" · "));
        bar.push_span(Span::styled(hint.text(), Style::new().fg(Color::Yellow)));
    }
    let row = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    frame.render_widget(Paragraph::new(bar).style(Style::new().fg(DIM)), row);
}

/// One centred line at row `y`, clipped to `area`.
fn line_at(frame: &mut Frame, area: Rect, y: u16, line: Line<'_>) {
    let row = Rect::new(area.x, y, area.width, 1).intersection(area);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), row);
}
