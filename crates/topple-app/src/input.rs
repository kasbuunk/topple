//! Abstract pad. The Miyoo's face layout is the mnemonic: X sits on top of
//! the diamond and sets ⊤; B sits at the bottom and sets ⊥.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    /// Zoom / confirm.
    A,
    /// Set ⊥ / back.
    B,
    /// Set ⊤.
    X,
    /// Ghost preview.
    Y,
    Start,
    Select,
}

pub const ALL_BUTTONS: [Button; 10] = [
    Button::Up,
    Button::Down,
    Button::Left,
    Button::Right,
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::Start,
    Button::Select,
];

/// What tapping a zone does. Menus encode navigation as a button sequence,
/// so a tap behaves exactly like keys — one input model everywhere.
#[derive(Clone, Debug)]
pub enum TapAction {
    /// Synthesize these presses in order.
    Press(Vec<Button>),
    /// Point the play cursor at this atom occurrence.
    Cursor(usize),
}

/// A tappable rectangle in framebuffer coordinates, rebuilt every render.
#[derive(Clone, Debug)]
pub struct TapZone {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub act: TapAction,
}

impl TapZone {
    pub fn hit(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// The button sequence that walks a menu from `sel` to `target`, then
/// confirms.
pub fn menu_taps(sel: usize, target: usize) -> Vec<Button> {
    let mut seq = Vec::new();
    if target >= sel {
        seq.extend(std::iter::repeat_n(Button::Down, target - sel));
    } else {
        seq.extend(std::iter::repeat_n(Button::Up, sel - target));
    }
    seq.push(Button::A);
    seq
}

/// Auto-repeat for held d-pad directions, synthesized app-side so every
/// frontend behaves identically.
pub struct Repeater {
    held: Option<(Button, u32)>, // button + ms since (re)fire
    fired_once: bool,
}

impl Default for Repeater {
    fn default() -> Self {
        Self::new()
    }
}

const REPEAT_DELAY: u32 = 380;
const REPEAT_RATE: u32 = 120;

impl Repeater {
    pub fn new() -> Repeater {
        Repeater {
            held: None,
            fired_once: false,
        }
    }

    pub fn press(&mut self, b: Button) {
        if matches!(b, Button::Up | Button::Down | Button::Left | Button::Right) {
            self.held = Some((b, 0));
            self.fired_once = false;
        }
    }

    pub fn release(&mut self, b: Button) {
        if let Some((h, _)) = self.held {
            if h == b {
                self.held = None;
            }
        }
    }

    /// Advance time; returns synthesized repeat presses.
    pub fn tick(&mut self, dt_ms: u32) -> Vec<Button> {
        let mut out = Vec::new();
        if let Some((b, t)) = &mut self.held {
            *t += dt_ms;
            loop {
                let threshold = if self.fired_once {
                    REPEAT_RATE
                } else {
                    REPEAT_DELAY
                };
                if *t < threshold {
                    break;
                }
                *t -= threshold;
                self.fired_once = true;
                out.push(*b);
            }
        }
        out
    }
}
