use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::text::{self, Font};

const LOGO: &[u8] = include_bytes!("../../../assets/logo.png");
const MIN_SHOWN: Duration = Duration::from_millis(1200);
const FRAME: Duration = Duration::from_millis(16);
const TOP: u32 = 0x161033;
const BOTTOM: u32 = 0x0b0819;
const TRACK: u32 = 0x2b2350;
const ACCENT_A: u32 = 0x7c3aed;
const ACCENT_B: u32 = 0xc4b5fd;

pub struct Splash {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    w: u32,
    h: u32,
    s: f32,
    base: Vec<u32>,
    started: Instant,
    ready: bool,
    next_frame: Instant,
}

impl Splash {
    pub fn new(el: &ActiveEventLoop, area: (i32, i32, u32, u32), scale: f64, font: &Font) -> anyhow::Result<Splash> {
        let s = scale.clamp(1.0, 3.0) as f32;
        let w = (360.0 * s) as u32;
        let h = (280.0 * s) as u32;
        let x = area.0 + (area.2 as i32 - w as i32) / 2;
        let y = area.1 + (area.3 as i32 - h as i32) / 2;
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title("MarkdownShot")
            .with_decorations(false)
            .with_resizable(false)
            .with_active(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(x, y))
            .with_inner_size(PhysicalSize::new(w, h));
        #[cfg(windows)]
        {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            attrs = attrs.with_skip_taskbar(true).with_undecorated_shadow(true).with_corner_preference(CornerPreference::Round);
        }
        let window = Rc::new(el.create_window(attrs)?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let base = background(w, h, s, font);
        let mut sp = Splash {
            window,
            surface,
            _context: context,
            w,
            h,
            s,
            base,
            started: Instant::now(),
            ready: false,
            next_frame: Instant::now(),
        };
        sp.redraw();
        Ok(sp)
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn set_ready(&mut self) {
        self.ready = true;
    }

    pub fn done(&self) -> bool {
        self.ready && self.started.elapsed() >= MIN_SHOWN
    }

    pub fn wake_at(&self) -> Instant {
        self.next_frame
    }

    pub fn tick(&mut self) {
        if Instant::now() >= self.next_frame {
            self.window.request_redraw();
        }
    }

    pub fn handle(&mut self, ev: &WindowEvent) {
        if matches!(ev, WindowEvent::RedrawRequested) {
            self.redraw();
        }
    }

    fn redraw(&mut self) {
        self.next_frame = Instant::now() + FRAME;
        let (Some(nw), Some(nh)) = (NonZeroU32::new(self.w), NonZeroU32::new(self.h)) else { return };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        let (w, h) = (self.w as usize, self.h as usize);
        if buf.len() != self.base.len() {
            return;
        }
        buf.copy_from_slice(&self.base);
        let s = self.s;
        let track_w = 200.0 * s;
        let x0 = (w as f32 - track_w) / 2.0;
        let y0 = 246.0 * s;
        let bh = (4.0 * s).round().max(2.0);
        round_bar(&mut buf, w, h, x0, y0, track_w, bh, TRACK, TRACK);
        let t = self.started.elapsed().as_secs_f32();
        let phase = (t / 1.4).fract();
        let ease = 0.5 - 0.5 * (phase * std::f32::consts::PI * 2.0).cos();
        let seg = track_w * 0.32;
        let pos = if (t / 1.4) as u32 % 2 == 0 { ease } else { 1.0 - ease };
        let sx = x0 + (track_w - seg) * pos;
        round_bar(&mut buf, w, h, sx, y0, seg, bh, ACCENT_A, ACCENT_B);
        let _ = buf.present();
    }
}

fn background(w: u32, h: u32, s: f32, font: &Font) -> Vec<u32> {
    let (wu, hu) = (w as usize, h as usize);
    let mut buf = vec![0u32; wu * hu];
    for y in 0..hu {
        let c = lerp(TOP, BOTTOM, y as f32 / hu.max(1) as f32);
        buf[y * wu..(y + 1) * wu].fill(c);
    }
    text::frame(&mut buf, wu, hu, 1, 1, w as i32 - 1, h as i32 - 1, 1, 0x2e2752);
    let size = (124.0 * s) as u32;
    if let Ok(img) = image::load_from_memory(LOGO) {
        let logo = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3).to_rgba8();
        let lx = (w - size) / 2;
        let ly = (30.0 * s) as u32;
        for (x, y, p) in logo.enumerate_pixels() {
            let (px, py) = ((lx + x) as usize, (ly + y) as usize);
            if px >= wu || py >= hu || p[3] == 0 {
                continue;
            }
            let i = py * wu + px;
            let a = p[3] as u32;
            let d = buf[i];
            let mix = |src: u8, dst: u32| (src as u32 * a + dst * (255 - a)) / 255;
            buf[i] = (mix(p[0], (d >> 16) & 255) << 16) | (mix(p[1], (d >> 8) & 255) << 8) | mix(p[2], d & 255);
        }
    }
    let title = "MarkdownShot";
    let ts = 26.0 * s;
    font.draw(&mut buf, wu, hu, (w as f32 - font.measure(title, ts)) / 2.0, 196.0 * s, ts, 0xf5f3ff, title);
    let status = t!("Chargement des modèles…", "Loading the models…");
    let ss = 13.0 * s;
    font.draw(&mut buf, wu, hu, (w as f32 - font.measure(status, ss)) / 2.0, 224.0 * s, ss, 0xa39cc4, status);
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    let vs = 11.0 * s;
    font.draw(&mut buf, wu, hu, w as f32 - font.measure(version, vs) - 12.0 * s, h as f32 - 10.0 * s, vs, 0x5f5785, version);
    buf
}

#[allow(clippy::too_many_arguments)]
fn round_bar(buf: &mut [u32], bw: usize, bh: usize, x: f32, y: f32, len: f32, thick: f32, a: u32, b: u32) {
    let r = thick / 2.0;
    let (x0, x1) = (x.floor() as i32, (x + len).ceil() as i32);
    let (y0, y1) = (y.floor() as i32, (y + thick).ceil() as i32);
    for py in y0.max(0)..y1.min(bh as i32) {
        for px in x0.max(0)..x1.min(bw as i32) {
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            let cx = fx.clamp(x + r, x + len - r);
            let cy = y + r;
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let c = lerp(a, b, ((fx - x) / len.max(1.0)).clamp(0.0, 1.0));
            let alpha = (cov * 255.0) as u32;
            let i = py as usize * bw + px as usize;
            let d = buf[i];
            let mix = |s: u32, t: u32| (s * alpha + t * (255 - alpha)) / 255;
            buf[i] = (mix((c >> 16) & 255, (d >> 16) & 255) << 16) | (mix((c >> 8) & 255, (d >> 8) & 255) << 8) | mix(c & 255, d & 255);
        }
    }
}

fn lerp(a: u32, b: u32, t: f32) -> u32 {
    let ch = |sh: u32| {
        let (x, y) = (((a >> sh) & 255) as f32, ((b >> sh) & 255) as f32);
        ((x + (y - x) * t).round() as u32) << sh
    };
    ch(16) | ch(8) | ch(0)
}
