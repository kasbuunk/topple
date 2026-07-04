//! Web shim: a hand-rolled C ABI, no wasm-bindgen. JS owns the canvas, the
//! clock, the keyboard, and localStorage; the wasm module owns everything
//! else. The framebuffer is RGBA — exactly what ImageData wants.

use topple_app::{App, Button, Frame};

struct State {
    app: App,
    fb: Frame,
    save_out: Vec<u8>,
    inbox: Vec<u8>,
}

static mut STATE: Option<State> = None;

#[allow(static_mut_refs)]
fn state() -> &'static mut State {
    unsafe { STATE.as_mut().expect("boot() first") }
}

/// Initialize. Date parts come from JS (`new Date()` is the user's clock).
#[no_mangle]
pub extern "C" fn boot(seed_lo: u32, seed_hi: u32, year: u32, month: u32, day: u32) {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    let date = format!("{year:04}-{month:02}-{day:02}");
    let s = State {
        app: App::new(seed, &date),
        fb: Frame::new(),
        save_out: Vec::new(),
        inbox: Vec::new(),
    };
    unsafe { STATE = Some(s) };
}

/// Pointer to the RGBA framebuffer (WIDTH×HEIGHT×4 bytes).
#[no_mangle]
pub extern "C" fn fb_ptr() -> *const u8 {
    state().fb.px.as_ptr()
}

#[no_mangle]
pub extern "C" fn fb_width() -> u32 {
    topple_app::WIDTH as u32
}

#[no_mangle]
pub extern "C" fn fb_height() -> u32 {
    topple_app::HEIGHT as u32
}

/// Advance and render one frame.
#[no_mangle]
pub extern "C" fn frame(dt_ms: u32) {
    let s = state();
    s.app.tick(dt_ms.min(100));
    s.app.render(&mut s.fb);
}

/// Buttons are indexed: 0 Up, 1 Down, 2 Left, 3 Right, 4 A, 5 B, 6 X, 7 Y,
/// 8 Start, 9 Select.
#[no_mangle]
pub extern "C" fn key(code: u32, down: u32) {
    let b = match code {
        0 => Button::Up,
        1 => Button::Down,
        2 => Button::Left,
        3 => Button::Right,
        4 => Button::A,
        5 => Button::B,
        6 => Button::X,
        7 => Button::Y,
        8 => Button::Start,
        9 => Button::Select,
        _ => return,
    };
    if down != 0 {
        state().app.on_press(b);
    } else {
        state().app.on_release(b);
    }
}

/// Save polling: returns the byte length of a fresh save blob (0 = nothing
/// new); the blob itself is at `save_ptr()` until the next call.
#[no_mangle]
pub extern "C" fn save_poll() -> u32 {
    let s = state();
    match s.app.take_save() {
        Some(blob) => {
            s.save_out = blob;
            s.save_out.len() as u32
        }
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn save_ptr() -> *const u8 {
    state().save_out.as_ptr()
}

/// Loading a save: JS asks for a scratch buffer, writes into it, commits.
#[no_mangle]
pub extern "C" fn inbox_alloc(len: u32) -> *mut u8 {
    let s = state();
    s.inbox = vec![0; len as usize];
    s.inbox.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn inbox_load_save() {
    let s = state();
    let blob = std::mem::take(&mut s.inbox);
    s.app.load_save(Some(&blob));
}
