//! Desktop shim (macOS / Linux / Windows): winit window + softbuffer, pure
//! software presentation of the app's 640×480 framebuffer with integer
//! scaling and letterboxing.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;
use topple_app::{App, Button, Frame, HEIGHT, WIDTH};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut shell = Shell::new();
    event_loop.run_app(&mut shell).expect("run");
    shell.persist_save();
}

struct Shell {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    app: App,
    fb: Frame,
    last: Instant,
}

impl Shell {
    fn new() -> Shell {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x70991E);
        let mut app = App::new(seed, &today_iso());
        if let Ok(blob) = std::fs::read(save_path()) {
            app.load_save(Some(&blob));
        }
        Shell {
            window: None,
            surface: None,
            app,
            fb: Frame::new(),
            last: Instant::now(),
        }
    }

    fn persist_save(&mut self) {
        if let Some(blob) = self.app.take_save() {
            let path = save_path();
            if let Some(dir) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, blob);
        }
    }
}

fn save_path() -> String {
    if let Ok(home) = std::env::var("HOME") {
        format!("{home}/.local/share/topple/save.bin")
    } else {
        "topple-save.bin".to_string()
    }
}

/// Civil date from the system clock (Howard Hinnant's algorithm).
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

fn map_key(code: KeyCode) -> Option<Button> {
    Some(match code {
        KeyCode::ArrowUp | KeyCode::KeyW => Button::Up,
        KeyCode::ArrowDown | KeyCode::KeyS => Button::Down,
        KeyCode::ArrowLeft => Button::Left,
        KeyCode::ArrowRight => Button::Right,
        KeyCode::KeyX | KeyCode::KeyT => Button::X,
        KeyCode::KeyB | KeyCode::KeyF => Button::B,
        KeyCode::KeyA | KeyCode::KeyZ => Button::A,
        KeyCode::KeyY | KeyCode::KeyP => Button::Y,
        KeyCode::Enter | KeyCode::Escape => Button::Start,
        KeyCode::Tab | KeyCode::Backspace => Button::Select,
        _ => return None,
    })
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let window = Rc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("⊤OPP⊥E")
                    .with_inner_size(LogicalSize::new(WIDTH as f64, HEIGHT as f64))
                    .with_min_inner_size(LogicalSize::new(WIDTH as f64, HEIGHT as f64)),
            )
            .expect("window"),
        );
        let context = softbuffer::Context::new(window.clone()).expect("context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
        self.window = Some(window);
        self.surface = Some(surface);
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.persist_save();
                el.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return; // the app synthesizes its own d-pad repeats
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(b) = map_key(code) {
                        match event.state {
                            ElementState::Pressed => self.app.on_press(b),
                            ElementState::Released => self.app.on_release(b),
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let dt = self.last.elapsed().as_millis().min(100) as u32;
                self.last = Instant::now();
                self.app.tick(dt);
                self.persist_save();
                if self.app.wants_exit() {
                    el.exit();
                    return;
                }
                self.app.render(&mut self.fb);
                self.present();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl Shell {
    fn present(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (pw, ph) = (size.width as usize, size.height as usize);
        if pw == 0 || ph == 0 {
            return;
        }
        surface
            .resize(
                NonZeroU32::new(pw as u32).unwrap(),
                NonZeroU32::new(ph as u32).unwrap(),
            )
            .expect("resize");
        let mut buf = surface.buffer_mut().expect("buffer");
        // Integer scale, centered, black borders.
        let scale = (pw / WIDTH).min(ph / HEIGHT).max(1);
        let (ow, oh) = (WIDTH * scale, HEIGHT * scale);
        let x0 = pw.saturating_sub(ow) / 2;
        let y0 = ph.saturating_sub(oh) / 2;
        buf.fill(0);
        let px = &self.fb.px;
        for y in 0..oh.min(ph) {
            let sy = y / scale;
            let row = &px[sy * WIDTH * 4..(sy + 1) * WIDTH * 4];
            let dst_y = y0 + y;
            if dst_y >= ph {
                break;
            }
            let dst = &mut buf[dst_y * pw..];
            for x in 0..ow.min(pw) {
                let sx = x / scale;
                let i = sx * 4;
                let (r, g, b) = (row[i] as u32, row[i + 1] as u32, row[i + 2] as u32);
                if x0 + x < pw {
                    dst[x0 + x] = (r << 16) | (g << 8) | b;
                }
            }
        }
        buf.present().expect("present");
    }
}
