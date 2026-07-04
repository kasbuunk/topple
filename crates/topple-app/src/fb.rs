//! A 640×480 RGBA framebuffer and the handful of primitives the game needs.
//! Pure software rendering: the same pixels on a Miyoo panel, a canvas, and
//! a Retina window.

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }
    pub fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }
    /// Linear blend toward `other` by t/255.
    pub fn mix(self, other: Color, t: u8) -> Color {
        let lerp = |a: u8, b: u8| -> u8 {
            (a as i32 + (b as i32 - a as i32) * t as i32 / 255).clamp(0, 255) as u8
        };
        Color {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: 255,
        }
    }
}

pub struct Frame {
    /// RGBA, row-major, WIDTH×HEIGHT.
    pub px: Vec<u8>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub fn new() -> Frame {
        Frame {
            px: vec![0; WIDTH * HEIGHT * 4],
        }
    }

    pub fn clear(&mut self, c: Color) {
        for p in self.px.chunks_exact_mut(4) {
            p[0] = c.r;
            p[1] = c.g;
            p[2] = c.b;
            p[3] = 255;
        }
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Color) {
        if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 || c.a == 0 {
            return;
        }
        let i = (y as usize * WIDTH + x as usize) * 4;
        if c.a == 255 {
            self.px[i] = c.r;
            self.px[i + 1] = c.g;
            self.px[i + 2] = c.b;
            self.px[i + 3] = 255;
        } else {
            let a = c.a as u32;
            let na = 255 - a;
            self.px[i] = ((c.r as u32 * a + self.px[i] as u32 * na) / 255) as u8;
            self.px[i + 1] = ((c.g as u32 * a + self.px[i + 1] as u32 * na) / 255) as u8;
            self.px[i + 2] = ((c.b as u32 * a + self.px[i + 2] as u32 * na) / 255) as u8;
            self.px[i + 3] = 255;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for yy in y.max(0)..(y + h).min(HEIGHT as i32) {
            for xx in x.max(0)..(x + w).min(WIDTH as i32) {
                self.put(xx, yy, c);
            }
        }
    }

    /// Filled rectangle with clipped corners — the game's "rounded" look.
    pub fn fill_rrect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: Color) {
        self.fill_rect(x + r, y, w - 2 * r, h, c);
        self.fill_rect(x, y + r, w, h - 2 * r, c);
        for (cx, cy) in [
            (x + r, y + r),
            (x + w - r - 1, y + r),
            (x + r, y + h - r - 1),
            (x + w - r - 1, y + h - r - 1),
        ] {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy <= r * r {
                        self.put(cx + dx, cy + dy, c);
                    }
                }
            }
        }
    }

    pub fn rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, c: Color) {
        self.fill_rect(x, y, w, t, c);
        self.fill_rect(x, y + h - t, w, t, c);
        self.fill_rect(x, y, t, h, c);
        self.fill_rect(x + w - t, y, t, h, c);
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Color) {
        self.fill_rect(x, y, w, 1, c);
    }

    /// Blit an 8-bit coverage mask (from the rasterizer) in `c`.
    pub fn blit_mask(&mut self, x: i32, y: i32, w: usize, h: usize, mask: &[u8], c: Color) {
        for my in 0..h {
            for mx in 0..w {
                let cov = mask[my * w + mx];
                if cov > 0 {
                    let a = (cov as u32 * c.a as u32 / 255) as u8;
                    self.put(x + mx as i32, y + my as i32, c.with_alpha(a));
                }
            }
        }
    }
}
