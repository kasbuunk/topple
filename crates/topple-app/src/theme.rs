//! One palette for every platform. Dark, warm ⊤, cool ⊥ — readable at arm's
//! length on a 3.5" panel.

use crate::fb::Color;
use topple_core::Side;

pub const BG: Color = Color::rgb(0x0E, 0x11, 0x16);
pub const PANEL: Color = Color::rgb(0x16, 0x1B, 0x22);
pub const PANEL_EDGE: Color = Color::rgb(0x2A, 0x33, 0x40);
pub const TEXT: Color = Color::rgb(0xE6, 0xED, 0xF3);
pub const DIM: Color = Color::rgb(0x8B, 0x94, 0x9E);
pub const FAINT: Color = Color::rgb(0x4A, 0x53, 0x5E);

pub const TOP: Color = Color::rgb(0xFF, 0xC5, 0x3D); // amber — ⊤
pub const BOT: Color = Color::rgb(0x4D, 0xC4, 0xFF); // cyan — ⊥

pub const GLOW_BG: Color = Color::rgba(0x3A, 0x46, 0x63, 0xB4);
pub const CURSOR: Color = Color::rgb(0xF0, 0xF6, 0xFC);
pub const REDEX_BG: Color = Color::rgba(0x5A, 0x46, 0x14, 0xC8);
pub const REDEX_EDGE: Color = Color::rgb(0xC8, 0x9B, 0x2D);
pub const REWRITE_BG: Color = Color::rgba(0x17, 0x46, 0x26, 0xC8); // green — what a rewrite left behind

pub const GOOD: Color = Color::rgb(0x3F, 0xB9, 0x50);
pub const BAD: Color = Color::rgb(0xF8, 0x51, 0x49);

pub fn side_color(s: Side) -> Color {
    match s {
        Side::Top => TOP,
        Side::Bot => BOT,
    }
}
