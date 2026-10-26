//! Colour downsampling for terminals without truecolour: RGB to the xterm 256-colour palette or to
//! the 16 ANSI colours, after effects have run.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::term::ColorDepth;

/// The xterm defaults for the 16 ANSI colours, in index order.
const ANSI: [(Color, (u8, u8, u8)); 16] = [
    (Color::Black, (0, 0, 0)),
    (Color::Red, (205, 0, 0)),
    (Color::Green, (0, 205, 0)),
    (Color::Yellow, (205, 205, 0)),
    (Color::Blue, (0, 0, 238)),
    (Color::Magenta, (205, 0, 205)),
    (Color::Cyan, (0, 205, 205)),
    (Color::Gray, (229, 229, 229)),
    (Color::DarkGray, (127, 127, 127)),
    (Color::LightRed, (255, 0, 0)),
    (Color::LightGreen, (0, 255, 0)),
    (Color::LightYellow, (255, 255, 0)),
    (Color::LightBlue, (92, 92, 255)),
    (Color::LightMagenta, (255, 0, 255)),
    (Color::LightCyan, (0, 255, 255)),
    (Color::White, (255, 255, 255)),
];
/// The levels of the 6×6×6 cube.
const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Maps the colours of every cell in `area` to `depth`.
pub(crate) fn downsample(buf: &mut Buffer, area: Rect, depth: ColorDepth) {
    let map: fn(Color) -> Color = match depth {
        ColorDepth::TrueColor => return,
        ColorDepth::Indexed256 => to_indexed,
        ColorDepth::Ansi16 => to_ansi16,
    };
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.fg = map(cell.fg);
                cell.bg = map(cell.bg);
            }
        }
    }
}

/// The nearest xterm-256 cube or grey-ramp colour; non-RGB colours pass through.
pub(crate) fn to_indexed(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let level = |v: u8| {
        STEPS
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| (i32::from(**s) - i32::from(v)).abs())
            .map_or(0, |(i, _)| i)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let step = |i: usize| STEPS.get(i).copied().unwrap_or(0);
    let cube = (step(ri), step(gi), step(bi));
    let grey_i = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3)
        .saturating_sub(8)
        .div_euclid(10)
        .min(23);
    let grey_i = u8::try_from(grey_i).unwrap_or(23);
    let grey = 8 + 10 * grey_i;
    if distance((grey, grey, grey), (r, g, b)) < distance(cube, (r, g, b)) {
        Color::Indexed(232 + grey_i)
    } else {
        let index = 16 + 36 * ri + 6 * gi + bi;
        Color::Indexed(u8::try_from(index).unwrap_or(231))
    }
}

/// The nearest of the 16 ANSI colours; named colours pass through.
pub(crate) fn to_ansi16(color: Color) -> Color {
    let Some(rgb) = rgb(color) else {
        return color;
    };
    ANSI.iter()
        .min_by_key(|(_, c)| distance(*c, rgb))
        .map_or(color, |(named, _)| *named)
}

/// The RGB value of an RGB or indexed colour.
fn rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(i @ 0..=15) => ANSI.get(usize::from(i)).map(|(_, c)| *c),
        Color::Indexed(i @ 16..=231) => {
            let i = usize::from(i - 16);
            let step = |n: usize| STEPS.get(n).copied().unwrap_or(0);
            Some((step(i / 36), step(i / 6 % 6), step(i % 6)))
        }
        Color::Indexed(i) => {
            let v = 8 + 10 * (i - 232);
            Some((v, v, v))
        }
        _ => None,
    }
}

/// The RGB value of any colour with a known value, named colours included (xterm defaults).
#[cfg(test)]
pub(crate) fn to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    ANSI.iter()
        .find(|(named, _)| *named == color)
        .map(|(_, c)| *c)
        .or_else(|| rgb(color))
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
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
    #[case(Color::Rgb(255, 105, 180), Color::Magenta)]
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
