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
