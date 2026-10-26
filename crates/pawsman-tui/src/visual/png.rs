//! Renders a ratatui `Buffer` to a PNG file, cell by cell, for reading UI changes as pictures.
//! Block elements and light box drawing are drawn exactly; other glyphs come from a system font.

use std::collections::BTreeMap;
use std::path::Path;

use image::{Rgb, RgbImage};
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::Modifier;

use crate::views::colors::to_rgb;

const CELL_W: u32 = 10;
const CELL_H: u32 = 20;
const FONT_PX: f32 = 16.0;
const DEFAULT_FG: (u8, u8, u8) = (220, 220, 235);
const DEFAULT_BG: (u8, u8, u8) = (22, 22, 34);
const FONTS: &[&str] = &[
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
];

/// A font and its rasterised glyphs.
pub(crate) struct Renderer {
    font: fontdue::Font,
    cache: BTreeMap<char, (fontdue::Metrics, Vec<u8>)>,
}

impl Renderer {
    /// Loads the first font found: `PAWSMAN_VISUAL_FONT`, then common system paths.
    #[expect(
        clippy::disallowed_methods,
        reason = "picks a font for local PNG files"
    )]
    pub(crate) fn new() -> Self {
        let chosen = std::env::var("PAWSMAN_VISUAL_FONT")
            .ok()
            .into_iter()
            .chain(FONTS.iter().map(|p| (*p).to_owned()))
            .find(|p| Path::new(p).is_file())
            .unwrap_or_else(|| {
                panic!("no font: set PAWSMAN_VISUAL_FONT or install one of {FONTS:?}")
            });
        let bytes = std::fs::read(&chosen).unwrap();
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).unwrap();
        Self {
            font,
            cache: BTreeMap::new(),
        }
    }

    /// Writes `buf` to `path`.
    pub(crate) fn render(&mut self, buf: &Buffer, path: &Path) {
        let area = buf.area;
        let mut img = RgbImage::new(
            u32::from(area.width) * CELL_W,
            u32::from(area.height) * CELL_H,
        );
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &buf[(x, y)];
                let ox = u32::from(x - area.x) * CELL_W;
                let oy = u32::from(y - area.y) * CELL_H;
                self.cell(&mut img, cell, ox, oy);
            }
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        img.save(path).unwrap();
    }

    fn cell(&mut self, img: &mut RgbImage, cell: &Cell, ox: u32, oy: u32) {
        let (mut fg, mut bg) = (
            to_rgb(cell.fg).unwrap_or(DEFAULT_FG),
            to_rgb(cell.bg).unwrap_or(DEFAULT_BG),
        );
        if cell.modifier.contains(Modifier::REVERSED) {
            std::mem::swap(&mut fg, &mut bg);
        }
        fill(img, ox, oy, CELL_W, CELL_H, bg);
        let Some(ch) = cell.symbol().chars().next() else {
            return;
        };
        if ch == ' ' || shape(img, ch, ox, oy, fg) {
            return;
        }
        let bold = cell.modifier.contains(Modifier::BOLD);
        let font = &self.font;
        let (metrics, bitmap) = self
            .cache
            .entry(ch)
            .or_insert_with(|| font.rasterize(ch, FONT_PX));
        let baseline = i64::from(oy) + 15;
        let left = i64::from(ox) + i64::from(metrics.xmin);
        let top = baseline - i64::try_from(metrics.height).unwrap() - i64::from(metrics.ymin);
        for (i, &coverage) in bitmap.iter().enumerate() {
            if coverage == 0 {
                continue;
            }
            let gx = left + i64::try_from(i % metrics.width).unwrap();
            let gy = top + i64::try_from(i / metrics.width).unwrap();
            for dx in 0..=i64::from(bold) {
                blend(img, gx + dx, gy, fg, coverage);
            }
        }
    }
}

/// Draws block elements and light box drawing exactly; false for anything else.
fn shape(img: &mut RgbImage, ch: char, ox: u32, oy: u32, fg: (u8, u8, u8)) -> bool {
    let (w, h) = (CELL_W, CELL_H);
    match ch {
        '█' => fill(img, ox, oy, w, h, fg),
        '▀' => fill(img, ox, oy, w, h / 2, fg),
        '▄' => fill(img, ox, oy + h / 2, w, h / 2, fg),
        '▌' => fill(img, ox, oy, w / 2, h, fg),
        '▐' => fill(img, ox + w / 2, oy, w / 2, h, fg),
        _ => {
            let Some((l, r, u, d)) = box_arms(ch) else {
                return false;
            };
            let (cx, cy) = (ox + w / 2, oy + h / 2);
            if l {
                fill(img, ox, cy, w / 2 + 1, 1, fg);
            }
            if r {
                fill(img, cx, cy, w - w / 2, 1, fg);
            }
            if u {
                fill(img, cx, oy, 1, h / 2 + 1, fg);
            }
            if d {
                fill(img, cx, cy, 1, h - h / 2, fg);
            }
        }
    }
    true
}

/// Which arms (left, right, up, down) a light box-drawing character has.
fn box_arms(ch: char) -> Option<(bool, bool, bool, bool)> {
    Some(match ch {
        '─' => (true, true, false, false),
        '│' => (false, false, true, true),
        '┌' | '╭' => (false, true, false, true),
        '┐' | '╮' => (true, false, false, true),
        '└' | '╰' => (false, true, true, false),
        '┘' | '╯' => (true, false, true, false),
        '├' => (false, true, true, true),
        '┤' => (true, false, true, true),
        '┬' => (true, true, false, true),
        '┴' => (true, true, true, false),
        '┼' => (true, true, true, true),
        _ => return None,
    })
}

fn fill(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, c: (u8, u8, u8)) {
    for py in y..(y + h).min(img.height()) {
        for px in x..(x + w).min(img.width()) {
            img.put_pixel(px, py, Rgb([c.0, c.1, c.2]));
        }
    }
}

fn blend(img: &mut RgbImage, x: i64, y: i64, c: (u8, u8, u8), coverage: u8) {
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return;
    };
    if x >= img.width() || y >= img.height() {
        return;
    }
    let Rgb(old) = *img.get_pixel(x, y);
    let a = u16::from(coverage);
    let mix = |o: u8, n: u8| {
        u8::try_from((u16::from(o) * (255 - a) + u16::from(n) * a) / 255).unwrap_or(255)
    };
    img.put_pixel(
        x,
        y,
        Rgb([mix(old[0], c.0), mix(old[1], c.1), mix(old[2], c.2)]),
    );
}
