//! iOS shim: a C ABI in the style of the web shim. Swift owns the screen,
//! the clock, the touch stream, UserDefaults, and Game Center; this library
//! owns everything else. The framebuffer is RGBA — one `CGImage` away from
//! a `CALayer`.
//!
//! Threading: every function here must be called from the same thread (the
//! main thread, driven by `CADisplayLink`).

use topple_app::{App, Button, Frame, OnlineStatus};

struct State {
    app: App,
    fb: Frame,
    save_out: Vec<u8>,
    online_out: Vec<u8>,
    inbox: Vec<u8>,
}

static mut STATE: Option<State> = None;

#[allow(static_mut_refs)]
fn state() -> &'static mut State {
    unsafe { STATE.as_mut().expect("topple_boot() first") }
}

/// Initialize. Date parts come from Swift (`Date()` is the user's clock).
/// `online` enables the "online duel" title item once Game Center is in.
#[no_mangle]
pub extern "C" fn topple_boot(
    seed_lo: u32,
    seed_hi: u32,
    year: u32,
    month: u32,
    day: u32,
    online: u32,
) {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    let date = format!("{year:04}-{month:02}-{day:02}");
    let mut app = App::new(seed, &date);
    // iOS: no self-quit, online per the platform's Game Center state.
    app.configure_platform(online != 0, false);
    let s = State {
        app,
        fb: Frame::new(),
        save_out: Vec::new(),
        online_out: Vec::new(),
        inbox: Vec::new(),
    };
    unsafe { STATE = Some(s) };
}

/// Game Center signed in (or out) after boot.
#[no_mangle]
pub extern "C" fn topple_set_online_available(online: u32) {
    state().app.configure_platform(online != 0, false);
}

/// Pointer to the RGBA framebuffer (WIDTH×HEIGHT×4 bytes).
#[no_mangle]
pub extern "C" fn topple_fb_ptr() -> *const u8 {
    state().fb.px.as_ptr()
}

#[no_mangle]
pub extern "C" fn topple_fb_width() -> u32 {
    topple_app::WIDTH as u32
}

#[no_mangle]
pub extern "C" fn topple_fb_height() -> u32 {
    topple_app::HEIGHT as u32
}

/// Advance and render one frame.
#[no_mangle]
pub extern "C" fn topple_frame(dt_ms: u32) {
    let s = state();
    s.app.tick(dt_ms.min(100));
    s.app.render(&mut s.fb);
}

/// Buttons are indexed: 0 Up, 1 Down, 2 Left, 3 Right, 4 A, 5 B, 6 X, 7 Y,
/// 8 Start, 9 Select.
#[no_mangle]
pub extern "C" fn topple_key(code: u32, down: u32) {
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

/// A tap in framebuffer coordinates (Swift maps view points to the
/// letterboxed 640×480 canvas).
#[no_mangle]
pub extern "C" fn topple_tap(x: f32, y: f32) {
    state().app.on_tap(x, y);
}

/// Save polling: returns the byte length of a fresh save blob (0 = nothing
/// new); the blob itself is at `topple_save_ptr()` until the next call.
#[no_mangle]
pub extern "C" fn topple_save_poll() -> u32 {
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
pub extern "C" fn topple_save_ptr() -> *const u8 {
    state().save_out.as_ptr()
}

/// Loading bytes in: Swift asks for a scratch buffer, writes into it, then
/// commits it as a save or as online match data.
#[no_mangle]
pub extern "C" fn topple_inbox_alloc(len: u32) -> *mut u8 {
    let s = state();
    s.inbox = vec![0; len as usize];
    s.inbox.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn topple_inbox_load_save() {
    let s = state();
    let blob = std::mem::take(&mut s.inbox);
    s.app.load_save(Some(&blob));
}

// ---------------------------------------------------------------- online --

/// The player asked for an online duel: returns the chosen difficulty
/// (1–5), or 0 if not. One-shot; Swift then presents the matchmaker.
#[no_mangle]
pub extern "C" fn topple_online_request_poll() -> u32 {
    state().app.take_online_request().map_or(0, u32::from)
}

/// The player resigned from the pause menu. One-shot; Swift then quits the
/// Game Center match.
#[no_mangle]
pub extern "C" fn topple_online_resign_poll() -> u32 {
    state().app.take_online_resign() as u32
}

/// Start a brand-new match as its creator (P1). Take the outbox right after
/// and store it as the match data.
#[no_mangle]
pub extern "C" fn topple_online_create(seed_lo: u32, seed_hi: u32, level: u32, local_p1: u32) {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    state()
        .app
        .online_create(seed, level.clamp(1, 5) as u8, local_p1 != 0);
}

/// Commit the inbox buffer as match data from the wire. Returns 1 if the
/// blob was accepted, 0 if it was corrupt.
#[no_mangle]
pub extern "C" fn topple_online_load(local_p1: u32) -> u32 {
    let s = state();
    let blob = std::mem::take(&mut s.inbox);
    s.app.online_load(&blob, local_p1 != 0) as u32
}

/// Fresh match data to ship, if any: returns its byte length (0 = nothing
/// new); the bytes are at `topple_online_outbox_ptr()` until the next call.
#[no_mangle]
pub extern "C" fn topple_online_outbox_poll() -> u32 {
    let s = state();
    match s.app.take_online_outbox() {
        Some(blob) => {
            s.online_out = blob;
            s.online_out.len() as u32
        }
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn topple_online_outbox_ptr() -> *const u8 {
    state().online_out.as_ptr()
}

/// 0 no match · 1 local turn · 2 remote turn · 3 local won · 4 remote won.
#[no_mangle]
pub extern "C" fn topple_online_status() -> u32 {
    match state().app.online_status() {
        OnlineStatus::Inactive => 0,
        OnlineStatus::LocalTurn => 1,
        OnlineStatus::RemoteTurn => 2,
        OnlineStatus::WonLocal => 3,
        OnlineStatus::WonRemote => 4,
    }
}

/// The opponent quit (or the match was ended by the service).
#[no_mangle]
pub extern "C" fn topple_online_opponent_quit() {
    state().app.online_opponent_quit();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full shim round-trip a Swift host would drive.
    #[test]
    fn boots_renders_and_plays_online_through_the_c_abi() {
        topple_boot(0xBEEF, 0, 2026, 7, 3, 1);
        topple_frame(16);
        assert_eq!(topple_fb_width(), 640);
        assert_eq!(topple_fb_height(), 480);
        assert!(!topple_fb_ptr().is_null());

        // Ask for an online duel from the title: down to "online duel", A,
        // then A on the difficulty list.
        topple_key(1, 1);
        topple_key(1, 0);
        topple_key(4, 1);
        topple_key(4, 0);
        topple_frame(16);
        topple_key(4, 1);
        topple_key(4, 0);
        topple_frame(16);
        assert_eq!(topple_online_request_poll(), 2);
        assert_eq!(topple_online_request_poll(), 0, "request is one-shot");

        // Create a match, pick ⊤; the outbox must carry both events.
        topple_online_create(42, 0, 2, 1);
        assert_eq!(topple_online_status(), 1);
        let n = topple_online_outbox_poll();
        assert!(n > 0, "creator ships the header");
        topple_key(6, 1); // X: take ⊤
        topple_key(6, 0);
        topple_frame(16);
        assert_eq!(topple_online_status(), 2);
        let n = topple_online_outbox_poll();
        assert!(n > 0, "the pick goes on the wire");

        // Round-trip that blob back through the inbox as the opponent.
        let blob =
            unsafe { std::slice::from_raw_parts(topple_online_outbox_ptr(), n as usize) }.to_vec();
        let ptr = topple_inbox_alloc(blob.len() as u32);
        unsafe { std::ptr::copy_nonoverlapping(blob.as_ptr(), ptr, blob.len()) };
        assert_eq!(topple_online_load(0), 1);
        assert_eq!(topple_online_status(), 1, "as P2 it is now the local turn");
    }
}
