//! Startup splash: the pawsman logo coalesces in, fades up and gets a hue sweep, in about 1.2 s.
//! On 16-colour terminals the final frame shows once for the same time instead.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::{CellFilter, Effect, Interpolation, SimpleRng, fx};

use unicode_width::UnicodeWidthStr as _;

use crate::term::ColorDepth;

/// The logo rows.
pub(crate) const LOGO: &[&str] = &[
    " █ █   █▀▀█ █▀▀█ █   █ █▀▀▀ █▀▄▀█ █▀▀█ █▀▀▄",
    "█ ▄ █  █▄▄█ █▄▄█ █ █ █ ▀▀▀█ █ ▀ █ █▄▄█ █  █",
    " ███   █    █  █ ▀▄▀▄▀ ▄▄▄█ █   █ █  █ █  █",
];
/// The line under the logo.
pub(crate) const TAGLINE: &str = "a terminal API client";
/// Logo width in cells.
pub(crate) const LOGO_W: u16 = 44;
/// Three logo rows, a gap, the tagline.
pub(crate) const LOGO_H: u16 = 5;
/// The splash background.
pub(crate) const BG: Color = Color::Rgb(22, 22, 34);
/// How long the static (16-colour) splash stays.
pub(crate) const STATIC_FOR: Duration = Duration::from_millis(1200);

/// One frame step; `sequence` and `delay` move one stage per `process` call.
const TICK: Duration = Duration::from_millis(16);
/// Longer than the whole effect; `skip` advances by this much.
const TOTAL: Duration = Duration::from_millis(2000);

/// The startup splash.
pub(crate) struct Splash {
    effect: Effect,
    last: Duration,
    depth: ColorDepth,
    skipped: bool,
}

impl Splash {
    /// A splash with a seeded coalesce pattern, drawn for `depth`.
    pub(crate) fn new(seed: u32, depth: ColorDepth) -> Self {
        let effect = fx::parallel(&[
            fx::coalesce((500, Interpolation::QuadOut))
                .with_rng(SimpleRng::new(seed))
                .with_filter(CellFilter::Text),
            fx::fade_from_fg(BG, (750, Interpolation::CubicOut)),
            fx::delay(
                550,
                fx::ping_pong(fx::hsl_shift_fg(
                    [40.0, 0.0, 12.0],
                    (300, Interpolation::SineInOut),
                )),
            ),
        ]);
        Self {
            effect,
            last: Duration::ZERO,
            depth,
            skipped: false,
        }
    }

    fn animated(&self) -> bool {
        self.depth != ColorDepth::Ansi16
    }

    /// Draws the frame `elapsed` after the start into `buf`; nothing when `area` is too small.
    pub(crate) fn frame(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect) {
        if !fits(area) {
            return;
        }
        let logo = paint(buf, area);
        if self.animated() {
            let delta = elapsed.saturating_sub(self.last);
            self.last = elapsed;
            self.advance(delta, buf, logo);
        }
        super::colors::downsample(buf, area, self.depth);
    }

    /// Jumps to the end; the next frame is the final one.
    pub(crate) fn skip(&mut self) {
        self.skipped = true;
        let area = Rect::new(0, 0, LOGO_W, LOGO_H);
        let mut scratch = Buffer::empty(area);
        self.advance(TOTAL, &mut scratch, area);
    }

    /// Advances the effect by `delta` and applies it once to `buf`. A long delta is stepped in
    /// ticks, every tick but the last on a scratch copy, because each call blends with what is
    /// already in the buffer.
    fn advance(&mut self, mut delta: Duration, buf: &mut Buffer, area: Rect) {
        let mut scratch = buf.clone();
        while delta > TICK && !self.effect.done() {
            scratch.clone_from(buf);
            self.effect.process(TICK.into(), &mut scratch, area);
            delta -= TICK;
        }
        self.effect.process(delta.into(), buf, area);
    }

    /// The splash has finished.
    pub(crate) fn done(&self, elapsed: Duration) -> bool {
        if self.animated() {
            self.effect.done()
        } else {
            self.skipped || elapsed >= STATIC_FOR
        }
    }

    /// When the next frame is due, or `None` once done.
    pub(crate) fn next_tick(&self, elapsed: Duration) -> Option<Duration> {
        if self.done(elapsed) {
            None
        } else if self.animated() {
            Some(TICK)
        } else {
            Some(STATIC_FOR.saturating_sub(elapsed))
        }
    }
}

/// Whether `area` is big enough for the logo with a margin.
pub(crate) fn fits(area: Rect) -> bool {
    area.width >= LOGO_W + 4 && area.height >= LOGO_H + 2
}

/// Fills the background and paints the logo and tagline centred in `area`; returns the rectangle
/// they cover.
fn paint(buf: &mut Buffer, area: Rect) -> Rect {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
                cell.bg = BG;
            }
        }
    }
    let x0 = area.x + area.width.saturating_sub(LOGO_W) / 2;
    let y0 = area.y + area.height.saturating_sub(LOGO_H) / 2;
    draw_logo(buf, x0, y0);
    let width = u16::try_from(TAGLINE.width()).unwrap_or(u16::MAX);
    let tx = area.x + area.width.saturating_sub(width) / 2;
    buf.set_string(
        tx,
        y0 + LOGO_H - 1,
        TAGLINE,
        Style::new().fg(Color::Rgb(150, 150, 175)),
    );
    Rect::new(x0, y0, LOGO_W, LOGO_H).intersection(area)
}

/// Paints the logo rows with their final gradient at `(x, y)`, leaving the background alone.
pub(crate) fn draw_logo(buf: &mut Buffer, x: u16, y: u16) {
    for (row, line) in (0u16..).zip(LOGO) {
        for (col, ch) in (0u16..).zip(line.chars()) {
            if ch == ' ' {
                continue;
            }
            let t = f32::from(col) / f32::from(LOGO_W);
            if let Some(cell) = buf.cell_mut((x.saturating_add(col), y.saturating_add(row))) {
                cell.set_char(ch)
                    .set_fg(lerp((255, 105, 180), (90, 210, 255), t));
            }
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 before the cast"
)]
fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let mix = |x: u8, y: u8| {
        (f32::from(x) + (f32::from(y) - f32::from(x)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buf: &Buffer) -> String {
        let area = buf.area;
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_at(ms: u64, depth: ColorDepth) -> Buffer {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, depth);
        splash.frame(Duration::from_millis(ms), &mut buf, area);
        buf
    }

    #[test]
    fn same_seed_same_frame_and_logo_drawn() {
        let a = render_at(600, ColorDepth::TrueColor);
        let b = render_at(600, ColorDepth::TrueColor);

        let drawn = a.content.iter().any(|c| c.symbol() == "█");

        assert_eq!(a, b, "deterministic with a seed");
        assert!(drawn, "the logo is on screen at 600 ms");
    }

    #[test]
    fn done_after_the_effect() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        splash.frame(Duration::from_millis(1300), &mut buf, area);

        assert!(
            splash.done(Duration::from_millis(1300)),
            "finished after 1.3 s"
        );
    }

    #[test]
    fn not_done_at_the_start() {
        let splash = Splash::new(42, ColorDepth::TrueColor);

        assert!(!splash.done(Duration::ZERO), "just started");
    }

    #[test]
    fn skip_finishes_at_once() {
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        splash.skip();

        assert!(
            splash.done(Duration::from_millis(10)),
            "skip jumps to the end"
        );
    }

    #[test]
    fn frame_on_a_too_small_area_draws_nothing_and_does_not_panic() {
        let area = Rect::new(0, 0, 80, 3);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        splash.frame(Duration::from_millis(600), &mut buf, area);

        assert!(
            buf.content.iter().all(|c| c.symbol() == " "),
            "no logo below the logo size"
        );
    }

    #[test]
    fn too_small_does_not_fit() {
        assert!(!fits(Rect::new(0, 0, 20, 5)), "smaller than the logo");
        assert!(fits(Rect::new(0, 0, 80, 24)), "a normal terminal");
    }

    #[test]
    fn indexed_mode_leaves_no_rgb() {
        let buf = render_at(600, ColorDepth::Indexed256);

        let drawn = buf.content.iter().any(|c| c.symbol() == "█");
        let rgb = buf
            .content
            .iter()
            .any(|c| matches!(c.fg, Color::Rgb(..)) || matches!(c.bg, Color::Rgb(..)));
        assert!(drawn && !rgb, "drawn with indexed colours only");
    }

    #[test]
    fn ansi16_shows_the_final_frame_once_and_waits() {
        let buf = render_at(0, ColorDepth::Ansi16);
        let splash = Splash::new(42, ColorDepth::Ansi16);

        let named = buf
            .content
            .iter()
            .all(|c| !matches!(c.fg, Color::Rgb(..) | Color::Indexed(_)));
        assert!(
            named && buf.content.iter().any(|c| c.symbol() == "█"),
            "the logo in the 16 named colours"
        );
        assert_eq!(
            splash.next_tick(Duration::ZERO),
            Some(STATIC_FOR),
            "one timer to the end, no frames"
        );
        assert!(splash.done(STATIC_FOR), "done after the static time");
    }

    #[test]
    fn animated_ticks_every_16ms_until_done() {
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        assert_eq!(
            splash.next_tick(Duration::ZERO),
            Some(Duration::from_millis(16)),
            "ticking"
        );
        splash.skip();
        assert_eq!(
            splash.next_tick(Duration::from_millis(20)),
            None,
            "no timer once done"
        );
    }

    #[test]
    fn effects_stay_inside_the_logo() {
        let area = Rect::new(0, 0, 120, 40);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        splash.frame(Duration::from_millis(600), &mut buf, area);

        let corner = &buf[(0, 0)];
        assert_eq!(corner.bg, BG, "the corner keeps the plain background");
        assert_eq!(corner.symbol(), " ", "and no glyph");
    }

    #[test]
    fn tagline_is_centred_by_width() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);
        splash.skip();

        splash.frame(Duration::from_millis(2000), &mut buf, area);

        let row: String = (0..80).map(|x| buf[(x, 13)].symbol().to_owned()).collect();
        let start = row.find(TAGLINE).expect("tagline on row 13");
        assert_eq!(start, (80 - TAGLINE.len()) / 2, "centred: {row:?}");
    }

    #[test]
    fn final_frame_snapshot() {
        let area = Rect::new(0, 0, 60, 9);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);
        splash.skip();

        splash.frame(Duration::from_millis(2000), &mut buf, area);

        insta::assert_snapshot!(text(&buf));
    }
}
