//! Headless harness: drive the app with a scripted input string and dump
//! PNG frames. Used to eyeball every screen without a display.
//!
//! Script grammar (comma-separated):
//!   U D L R A B X Y S E   — press a button (Start = S, Select = E)
//!   w500                  — tick 500 ms
//!   shot:name             — write name.png
//!
//! Usage: topple-shot <outdir> <script> [seed] [date]

use std::fs::File;
use std::io::BufWriter;
use topple_app::{App, Button, Frame, HEIGHT, WIDTH};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: topple-shot <outdir> <script> [seed] [date]");
        std::process::exit(2);
    }
    let outdir = &args[1];
    let script = &args[2];
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0xBEEF);
    let date = args.get(4).map(String::as_str).unwrap_or("2026-07-03");
    std::fs::create_dir_all(outdir).expect("outdir");

    let mut app = App::new(seed, date);
    let mut fb = Frame::new();

    for tok in script.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        if let Some(name) = tok.strip_prefix("shot:") {
            app.render(&mut fb);
            write_png(&format!("{outdir}/{name}.png"), &fb);
            continue;
        }
        if let Some(name) = tok.strip_prefix("icon:") {
            app.render(&mut fb);
            write_icon(&format!("{outdir}/{name}.png"), &fb);
            continue;
        }
        if let Some(name) = tok.strip_prefix("appicon:") {
            write_app_icon(&format!("{outdir}/{name}.png"));
            continue;
        }
        if tok == "online" {
            // Pretend to be the iOS shell: online duels, no quit item.
            app.configure_platform(true, false);
            continue;
        }
        if let Some(ms) = tok.strip_prefix('w') {
            let ms: u32 = ms.parse().expect("wait ms");
            let mut left = ms;
            while left > 0 {
                let step = left.min(40);
                app.tick(step);
                left -= step;
            }
            continue;
        }
        let b = match tok {
            "U" => Button::Up,
            "D" => Button::Down,
            "L" => Button::Left,
            "R" => Button::Right,
            "A" => Button::A,
            "B" => Button::B,
            "X" => Button::X,
            "Y" => Button::Y,
            "S" => Button::Start,
            "E" => Button::Select,
            _ => panic!("unknown token {tok:?}"),
        };
        app.on_press(b);
        app.on_release(b);
        app.tick(16);
    }
}

/// 128×128 launcher icon: crop the logo band of the title screen (256×128
/// around the wordmark), downsample 2×, and pad to a square on the bg color.
fn write_icon(path: &str, fb: &Frame) {
    const OUT: usize = 128;
    let (cx0, cy0, cw, ch) = (192usize, 52usize, 256usize, 64usize);
    let (ow, oh) = (cw / 2, ch / 2); // 128×64 after downsample
    let y_pad = (OUT - oh) / 2;
    let mut px = vec![0u8; OUT * OUT * 4];
    // Background.
    for p in px.chunks_exact_mut(4) {
        p[0] = 0x0E;
        p[1] = 0x11;
        p[2] = 0x16;
        p[3] = 255;
    }
    for oy in 0..oh {
        for ox in 0..ow {
            let sx = cx0 + ox * 2;
            let sy = cy0 + oy * 2;
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            let mut n = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (x, y) = (sx + dx, sy + dy);
                    if x < WIDTH && y < HEIGHT {
                        let i = (y * WIDTH + x) * 4;
                        r += fb.px[i] as u32;
                        g += fb.px[i + 1] as u32;
                        b += fb.px[i + 2] as u32;
                        n += 1;
                    }
                }
            }
            let o = ((oy + y_pad) * OUT + ox) * 4;
            px[o] = (r / n.max(1)) as u8;
            px[o + 1] = (g / n.max(1)) as u8;
            px[o + 2] = (b / n.max(1)) as u8;
            px[o + 3] = 255;
        }
    }
    let file = File::create(path).expect("create icon");
    let mut enc = png::Encoder::new(BufWriter::new(file), OUT as u32, OUT as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&px).expect("png data");
    println!("wrote {path}");
}

/// 1024×1024 App Store icon, rasterized at full resolution: the two win
/// conditions side by side on the game's background, in their side colors.
fn write_app_icon(path: &str) {
    const OUT: usize = 1024;
    const BOLD: &[u8] = include_bytes!("../../../assets/DejaVuSansMono-Bold.ttf");
    let font = fontdue::Font::from_bytes(BOLD, fontdue::FontSettings::default())
        .expect("embedded bold font");
    let mut px = vec![0u8; OUT * OUT * 4];
    for p in px.chunks_exact_mut(4) {
        p.copy_from_slice(&[0x0E, 0x11, 0x16, 0xFF]); // theme::BG
    }
    let size = 560.0f32;
    let glyphs: [(char, [u8; 3]); 2] = [
        ('⊤', [0xFF, 0xC5, 0x3D]), // theme::TOP
        ('⊥', [0x4D, 0xC4, 0xFF]), // theme::BOT
    ];
    let advance = font.metrics('⊤', size).advance_width;
    let total = advance * 2.0;
    let mut pen_x = (OUT as f32 - total) / 2.0;
    let baseline = OUT as f32 / 2.0 + size * 0.36;
    for (ch, color) in glyphs {
        let (m, cov) = font.rasterize(ch, size);
        let x0 = (pen_x + m.xmin as f32) as i32;
        let y0 = (baseline - m.height as f32 - m.ymin as f32) as i32;
        for gy in 0..m.height {
            for gx in 0..m.width {
                let a = cov[gy * m.width + gx] as u32;
                if a == 0 {
                    continue;
                }
                let (x, y) = (x0 + gx as i32, y0 + gy as i32);
                if x < 0 || y < 0 || x >= OUT as i32 || y >= OUT as i32 {
                    continue;
                }
                let i = (y as usize * OUT + x as usize) * 4;
                for c in 0..3 {
                    let bg = px[i + c] as u32;
                    px[i + c] = ((color[c] as u32 * a + bg * (255 - a)) / 255) as u8;
                }
            }
        }
        pen_x += advance;
    }
    let file = File::create(path).expect("create app icon");
    let mut enc = png::Encoder::new(BufWriter::new(file), OUT as u32, OUT as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&px).expect("png data");
    println!("wrote {path}");
}

fn write_png(path: &str, fb: &Frame) {
    let file = File::create(path).expect("create png");
    let mut enc = png::Encoder::new(BufWriter::new(file), WIDTH as u32, HEIGHT as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&fb.px).expect("png data");
    println!("wrote {path}");
}
