//! Miyoo Mini (Plus) shim: direct /dev/fb0 + evdev, no SDL, one static
//! musl binary. The panel is 640×480 — the app's native resolution — and
//! the stock framebuffer is mounted upside-down, so we rotate 180° by
//! default (set TOPPLE_ROT=0 to disable).

use std::fs::File;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use topple_app::{App, Button, Frame, HEIGHT, WIDTH};

const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOGET_FSCREENINFO: u32 = 0x4602;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FbVarScreeninfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: FbBitfield,
    green: FbBitfield,
    blue: FbBitfield,
    transp: FbBitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FbFixScreeninfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

impl Default for FbFixScreeninfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

struct Fb {
    mem: *mut u8,
    len: usize,
    line_length: usize,
    xoffset: usize,
    yoffset: usize,
    _file: File,
}

impl Fb {
    fn open() -> Fb {
        let file = File::options()
            .read(true)
            .write(true)
            .open("/dev/fb0")
            .expect("open /dev/fb0 — run on the device");
        let fd = file.as_raw_fd();
        let mut var = FbVarScreeninfo::default();
        let mut fix = FbFixScreeninfo::default();
        unsafe {
            if libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var) != 0 {
                panic!("FBIOGET_VSCREENINFO failed");
            }
            if libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut fix) != 0 {
                panic!("FBIOGET_FSCREENINFO failed");
            }
        }
        assert_eq!(var.bits_per_pixel, 32, "expected a 32bpp framebuffer");
        let len = fix.smem_len as usize;
        let mem = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if mem == libc::MAP_FAILED {
            panic!("mmap /dev/fb0 failed");
        }
        Fb {
            mem: mem as *mut u8,
            len,
            line_length: fix.line_length as usize,
            xoffset: var.xoffset as usize,
            yoffset: var.yoffset as usize,
            _file: file,
        }
    }

    /// Blit RGBA → BGRA with optional 180° rotation.
    fn blit(&mut self, fb: &Frame, rotate: bool) {
        let src = &fb.px;
        for y in 0..HEIGHT {
            let row_off = (self.yoffset + y) * self.line_length + self.xoffset * 4;
            if row_off + WIDTH * 4 > self.len {
                break;
            }
            for x in 0..WIDTH {
                let (sx, sy) = if rotate {
                    (WIDTH - 1 - x, HEIGHT - 1 - y)
                } else {
                    (x, y)
                };
                let i = (sy * WIDTH + sx) * 4;
                unsafe {
                    let d = self.mem.add(row_off + x * 4);
                    *d = src[i + 2]; // B
                    *d.add(1) = src[i + 1]; // G
                    *d.add(2) = src[i]; // R
                    *d.add(3) = 255; // A/X
                }
            }
        }
    }
}

/// evdev input_event on 32-bit ARM: two 32-bit time words + type/code/value.
#[repr(C)]
struct InputEvent {
    tv_sec: libc::c_long,
    tv_usec: libc::c_long,
    type_: u16,
    code: u16,
    value: i32,
}

fn map_key(code: u16) -> Option<Button> {
    Some(match code {
        103 => Button::Up,    // KEY_UP
        108 => Button::Down,  // KEY_DOWN
        105 => Button::Left,  // KEY_LEFT
        106 => Button::Right, // KEY_RIGHT
        57 => Button::A,      // KEY_SPACE  — A
        29 => Button::B,      // KEY_LEFTCTRL — B
        42 => Button::X,      // KEY_LEFTSHIFT — X
        56 => Button::Y,      // KEY_LEFTALT — Y
        28 => Button::Start,  // KEY_ENTER — START
        97 => Button::Select, // KEY_RIGHTCTRL — SELECT
        1 => Button::Start,   // KEY_ESC — MENU acts as START
        _ => return None,
    })
}

fn main() {
    let rotate = std::env::var("TOPPLE_ROT")
        .map(|v| v != "0")
        .unwrap_or(true);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x70991E);

    let mut app = App::new(seed, &today_iso());
    let save_file = save_path();
    if let Ok(blob) = std::fs::read(&save_file) {
        app.load_save(Some(&blob));
    }

    let mut fb = Fb::open();
    let mut frame = Frame::new();
    let mut input = File::options()
        .read(true)
        .open("/dev/input/event0")
        .expect("open /dev/input/event0");
    set_nonblocking(&input);

    let mut buf = [0u8; std::mem::size_of::<InputEvent>() * 32];
    let frame_ns: u64 = 1_000_000_000 / 60;
    loop {
        let t0 = now_ns();
        // Drain input.
        loop {
            match input.read(&mut buf) {
                Ok(n) if n >= std::mem::size_of::<InputEvent>() => {
                    let count = n / std::mem::size_of::<InputEvent>();
                    for k in 0..count {
                        let ev: InputEvent = unsafe {
                            std::ptr::read_unaligned(
                                buf.as_ptr().add(k * std::mem::size_of::<InputEvent>())
                                    as *const InputEvent,
                            )
                        };
                        if ev.type_ != 1 {
                            continue; // EV_KEY only
                        }
                        if let Some(b) = map_key(ev.code) {
                            match ev.value {
                                1 => app.on_press(b),
                                0 => app.on_release(b),
                                _ => {} // kernel auto-repeat: we do our own
                            }
                        }
                    }
                }
                _ => break,
            }
        }

        app.tick(16);
        if let Some(blob) = app.take_save() {
            let _ = std::fs::write(&save_file, blob);
        }
        if app.wants_exit() {
            break;
        }
        app.render(&mut frame);
        fb.blit(&frame, rotate);

        let spent = now_ns().saturating_sub(t0);
        if spent < frame_ns {
            let rest = frame_ns - spent;
            let ts = libc::timespec {
                tv_sec: 0,
                tv_nsec: rest as libc::c_long,
            };
            unsafe { libc::nanosleep(&ts, std::ptr::null_mut()) };
        }
    }
}

fn set_nonblocking(f: &File) {
    unsafe {
        let fd = f.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// Save next to the binary (the SD card), like every Miyoo port does.
fn save_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("topple-save.bin")))
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "topple-save.bin".into())
}

fn today_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
