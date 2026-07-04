//! Text engine: DejaVu Sans Mono (embedded) rasterized through fontdue with
//! a glyph cache. Monospaced on purpose — the board is a grid of glyphs and
//! the design doc's legibility budget is counted in them.

use crate::fb::{Color, Frame};
use std::collections::HashMap;

const REGULAR: &[u8] = include_bytes!("../../../assets/DejaVuSansMono.ttf");
const BOLD: &[u8] = include_bytes!("../../../assets/DejaVuSansMono-Bold.ttf");

struct Glyph {
    w: usize,
    h: usize,
    xmin: i32,
    ymin: i32,
    advance: f32,
    mask: Vec<u8>,
}

pub struct FontEngine {
    regular: fontdue::Font,
    bold: fontdue::Font,
    cache: HashMap<(char, u32, bool), Glyph>,
}

impl Default for FontEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FontEngine {
    pub fn new() -> FontEngine {
        let settings = fontdue::FontSettings::default();
        FontEngine {
            regular: fontdue::Font::from_bytes(REGULAR, settings).expect("embedded font"),
            bold: fontdue::Font::from_bytes(BOLD, settings).expect("embedded bold font"),
            cache: HashMap::new(),
        }
    }

    /// Fixed cell advance for a size — the mono grid step.
    pub fn advance(&self, size: f32) -> f32 {
        self.regular.metrics('M', size).advance_width
    }

    pub fn ascent(&self, size: f32) -> f32 {
        self.regular
            .horizontal_line_metrics(size)
            .map(|m| m.ascent)
            .unwrap_or(size * 0.8)
    }

    pub fn line_height(&self, size: f32) -> f32 {
        self.regular
            .horizontal_line_metrics(size)
            .map(|m| m.new_line_size)
            .unwrap_or(size * 1.2)
    }

    fn glyph(&mut self, ch: char, size: u32, bold: bool) -> &Glyph {
        let font = if bold { &self.bold } else { &self.regular };
        self.cache.entry((ch, size, bold)).or_insert_with(|| {
            let (m, mask) = font.rasterize(ch, size as f32);
            Glyph {
                w: m.width,
                h: m.height,
                xmin: m.xmin,
                ymin: m.ymin,
                advance: m.advance_width,
                mask,
            }
        })
    }

    /// Draw `text` with its left edge at `x` and *baseline* at `baseline_y`.
    /// Returns the x position after the last glyph.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        fb: &mut Frame,
        x: f32,
        baseline_y: f32,
        size: f32,
        color: Color,
        bold: bool,
        text: &str,
    ) -> f32 {
        let mut cx = x;
        for ch in text.chars() {
            self.draw_char(fb, cx, baseline_y, size, color, bold, ch);
            cx += self.char_advance(size, ch);
        }
        cx
    }

    /// Advance for one char: the mono cell, except a few wide math glyphs
    /// that DejaVu ships double-width; we honour their real advance.
    pub fn char_advance(&mut self, size: f32, ch: char) -> f32 {
        let cell = self.advance(size);
        let adv = self.glyph(ch, size as u32, false).advance;
        if adv > cell * 1.2 {
            adv
        } else {
            cell
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_char(
        &mut self,
        fb: &mut Frame,
        x: f32,
        baseline_y: f32,
        size: f32,
        color: Color,
        bold: bool,
        ch: char,
    ) {
        if ch == ' ' {
            return;
        }
        let g = self.glyph(ch, size as u32, bold);
        let gx = x as i32 + g.xmin;
        let gy = baseline_y as i32 - g.h as i32 - g.ymin;
        fb.blit_mask(gx, gy, g.w, g.h, &g.mask, color);
    }

    /// Width of `text` at `size`.
    pub fn measure(&mut self, size: f32, text: &str) -> f32 {
        text.chars().map(|c| self.char_advance(size, c)).sum()
    }

    /// Draw centered horizontally around `cx`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_centered(
        &mut self,
        fb: &mut Frame,
        cx: f32,
        baseline_y: f32,
        size: f32,
        color: Color,
        bold: bool,
        text: &str,
    ) {
        let w = self.measure(size, text);
        self.draw(fb, cx - w / 2.0, baseline_y, size, color, bold, text);
    }
}
