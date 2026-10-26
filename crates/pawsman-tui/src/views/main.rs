//! The collection view: sidebar, request pane and response pane, laid out by width.

use std::time::Instant;

use pawsman_core::keys::Context;
use pawsman_core::notice::Notice;
use pawsman_core::state::Snapshot;
use pawsman_model::RequestId;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::splash::BG;
use super::{hints, request, response, sidebar};

/// The focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    /// The request list.
    Sidebar,
    /// The selected request.
    Request,
    /// Its response.
    Response,
}

impl Focus {
    /// The next pane, wrapping.
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Sidebar => Self::Request,
            Self::Request => Self::Response,
            Self::Response => Self::Sidebar,
        }
    }

    /// The previous pane, wrapping.
    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Sidebar => Self::Response,
            Self::Request => Self::Sidebar,
            Self::Response => Self::Request,
        }
    }
}

/// Everything the main view needs.
pub(crate) struct MainView<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) selected: Option<RequestId>,
    pub(crate) focus: Focus,
    pub(crate) scroll: u16,
    pub(crate) now: Instant,
    pub(crate) notice: Option<&'a Notice>,
}

/// Sidebar width in the wide layouts.
const SIDEBAR: u16 = 28;
/// The accent for the focused pane.
pub(crate) const ACCENT: Color = Color::Rgb(90, 210, 255);
const DIM: Color = Color::Rgb(110, 110, 135);

/// Draws the main view.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &MainView<'_>) {
    frame.render_widget(Block::new().style(Style::new().bg(BG)), area);
    if area.width < 3 || area.height < 3 {
        return;
    }
    let notice_rows = u16::from(view.notice.is_some());
    let [body, notice, bar] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(notice_rows),
        Constraint::Length(1),
    ])
    .areas(area);
    if let Some(n) = view.notice {
        frame.render_widget(super::home::notice_line(n), notice);
    }
    frame.render_widget(
        Paragraph::new(hints::line(Context::Main)).style(Style::new().fg(DIM)),
        bar,
    );
    if body.width >= 160 {
        let [side, req, res] = Layout::horizontal([
            Constraint::Length(SIDEBAR),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .areas(body);
        panes(frame, view, Some(side), Some(req), Some(res));
    } else if body.width >= 80 {
        let [side, right] =
            Layout::horizontal([Constraint::Length(SIDEBAR), Constraint::Fill(1)]).areas(body);
        let [req, res] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(right);
        panes(frame, view, Some(side), Some(req), Some(res));
    } else {
        let [switcher, pane] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(body);
        frame.render_widget(switcher_line(view.focus), switcher);
        match view.focus {
            Focus::Sidebar => panes(frame, view, Some(pane), None, None),
            Focus::Request => panes(frame, view, None, Some(pane), None),
            Focus::Response => panes(frame, view, None, None, Some(pane)),
        }
    }
}

fn panes(
    frame: &mut Frame,
    view: &MainView<'_>,
    side: Option<Rect>,
    req: Option<Rect>,
    res: Option<Rect>,
) {
    if let Some(area) = side {
        let inner = framed(frame, area, " Requests ", view.focus == Focus::Sidebar);
        sidebar::draw(frame, inner, view.snapshot, view.selected);
    }
    if let Some(area) = req {
        let inner = framed(frame, area, " Request ", view.focus == Focus::Request);
        request::draw(frame, inner, view.snapshot, view.selected);
    }
    if let Some(area) = res {
        let inner = framed(frame, area, " Response ", view.focus == Focus::Response);
        response::draw(frame, inner, view);
    }
}

/// Draws a bordered pane, accented when focused; returns its inside.
fn framed(frame: &mut Frame, area: Rect, title: &str, focused: bool) -> Rect {
    let colour = if focused { ACCENT } else { DIM };
    let block = Block::bordered()
        .title(title.to_owned())
        .border_style(Style::new().fg(colour));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn switcher_line(focus: Focus) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (pane, name)) in [
        (Focus::Sidebar, "Requests"),
        (Focus::Request, "Request"),
        (Focus::Response, "Response"),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(DIM)));
        }
        let style = if pane == focus {
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(DIM)
        };
        spans.push(Span::styled(name, style));
    }
    Line::from(spans)
}
