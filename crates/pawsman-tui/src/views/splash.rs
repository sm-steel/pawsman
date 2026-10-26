//! Startup splash: the pawsman logo coalesces in, fades up and gets a hue sweep, in about 1.2 s.
//! On 16-colour terminals the final frame shows once for the same time instead.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tachyonfx::{CellFilter, Effect, Interpolation, SimpleRng, fx};

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
        let _ = (
            seed,
            CellFilter::Text,
            Interpolation::QuadOut,
            SimpleRng::new(0),
        );
        Self {
            effect: fx::sleep(0),
            last: Duration::ZERO,
            depth,
            skipped: false,
        }
    }

    /// Draws the frame `elapsed` after the start into `buf`.
    pub(crate) fn frame(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect) {
        let _ = (elapsed, buf, area, self.last, self.depth);
    }

    /// Jumps to the end; the next frame is the final one.
    pub(crate) fn skip(&mut self) {
        self.skipped = true;
    }

    /// The splash has finished.
    pub(crate) fn done(&self, elapsed: Duration) -> bool {
        let _ = elapsed;
        false
    }

    /// When the next frame is due, or `None` once done.
    pub(crate) fn next_tick(&self, elapsed: Duration) -> Option<Duration> {
        let _ = elapsed;
        None
    }
}

/// Whether `area` is big enough for the logo with a margin.
pub(crate) fn fits(area: Rect) -> bool {
    let _ = area;
    true
}

/// Paints the logo rows with their final gradient at `(x, y)`, leaving the background alone.
pub(crate) fn draw_logo(buf: &mut Buffer, x: u16, y: u16) {
    let _ = (buf, x, y);
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
