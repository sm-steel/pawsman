//! Colour downsampling for terminals without truecolour: RGB to the xterm 256-colour palette or to
//! the 16 ANSI colours, after effects have run.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::term::ColorDepth;

/// Maps the colours of every cell in `area` to `depth`.
pub(crate) fn downsample(buf: &mut Buffer, area: Rect, depth: ColorDepth) {
    let _ = (buf, area, depth);
}

/// The nearest xterm-256 cube or grey-ramp colour; non-RGB colours pass through.
pub(crate) fn to_indexed(color: Color) -> Color {
    color
}

/// The nearest of the 16 ANSI colours; named colours pass through.
pub(crate) fn to_ansi16(color: Color) -> Color {
    color
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Color::Rgb(0, 0, 0), Color::Indexed(16))]
    #[case(Color::Rgb(255, 255, 255), Color::Indexed(231))]
    #[case(Color::Rgb(128, 128, 128), Color::Indexed(244))]
    #[case(Color::Rgb(255, 0, 0), Color::Indexed(196))]
    #[case(Color::Red, Color::Red)]
    fn indexed_nearest(#[case] from: Color, #[case] expected: Color) {
        assert_eq!(to_indexed(from), expected, "{from:?}");
    }

    #[rstest]
    #[case(Color::Rgb(255, 105, 180), Color::LightMagenta)]
    #[case(Color::Rgb(90, 210, 255), Color::LightCyan)]
    #[case(Color::Rgb(22, 22, 34), Color::Black)]
    #[case(Color::Indexed(196), Color::LightRed)]
    #[case(Color::Indexed(244), Color::DarkGray)]
    #[case(Color::Yellow, Color::Yellow)]
    fn ansi16_nearest(#[case] from: Color, #[case] expected: Color) {
        assert_eq!(to_ansi16(from), expected, "{from:?}");
    }

    #[test]
    fn downsample_touches_only_the_area() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        for x in 0..4 {
            buf[(x, 0)].set_fg(Color::Rgb(255, 0, 0));
        }

        downsample(&mut buf, Rect::new(1, 0, 2, 1), ColorDepth::Indexed256);

        assert_eq!(buf[(0, 0)].fg, Color::Rgb(255, 0, 0), "outside kept");
        assert_eq!(buf[(1, 0)].fg, Color::Indexed(196), "inside mapped");
        assert_eq!(buf[(3, 0)].fg, Color::Rgb(255, 0, 0), "outside kept");
    }

    #[test]
    fn truecolor_is_left_alone() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        buf[(0, 0)].set_fg(Color::Rgb(1, 2, 3));

        downsample(&mut buf, Rect::new(0, 0, 1, 1), ColorDepth::TrueColor);

        assert_eq!(buf[(0, 0)].fg, Color::Rgb(1, 2, 3), "unchanged");
    }
}
