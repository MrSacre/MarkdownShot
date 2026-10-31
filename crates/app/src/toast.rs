use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId, WindowLevel};

use crate::text::{self, Font};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Success,
    Busy,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Click {
    None,
    Left,
    Right,
}

pub struct Toast {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    w: u32,
    h: u32,
    pub kind: Kind,
    title: String,
    body: String,
    pub until: Option<Instant>,
    pub open: Option<PathBuf>,
}

impl Toast {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        el: &ActiveEventLoop,
        area: (i32, i32, u32, u32),
        scale: f64,
        kind: Kind,
        title: &str,
        body: &str,
        secs: Option<u64>,
        open: Option<PathBuf>,
        font: &Font,
    ) -> anyhow::Result<Toast> {
        let s = scale.clamp(1.0, 3.0);
        let w = (420.0 * s) as u32;
        let h = ((if body.contains('\n') { 98.0 } else { 78.0 }) * s) as u32;
        let x = area.0 + area.2 as i32 - w as i32 - (20.0 * s) as i32;
        let y = area.1 + area.3 as i32 - h as i32 - (64.0 * s) as i32;
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
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true);
        }
        let window = Rc::new(el.create_window(attrs)?);
        if open.is_some() {
            window.set_cursor(CursorIcon::Pointer);
        }
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut t = Toast {
            window,
            surface,
            _context: context,
            w,
            h,
            kind,
            title: title.into(),
            body: body.into(),
            until: secs.map(|s| Instant::now() + Duration::from_secs(s)),
            open,
        };
        t.redraw(font);
        Ok(t)
    }

    pub fn set_clickable(&self) {
        self.window.set_cursor(CursorIcon::Pointer);
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn expired(&self) -> bool {
        self.until.is_some_and(|u| Instant::now() >= u)
    }

    pub fn handle(&mut self, ev: &WindowEvent, font: &Font) -> Click {
        match ev {
            WindowEvent::RedrawRequested => {
                self.redraw(font);
                Click::None
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => Click::Left,
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => Click::Right,
            _ => Click::None,
        }
    }

    fn redraw(&mut self, font: &Font) {
        let (Some(nw), Some(nh)) = (NonZeroU32::new(self.w), NonZeroU32::new(self.h)) else { return };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        let (w, h) = (self.w as usize, self.h as usize);
        let s = w as f32 / 420.0;
        buf.fill(0x111827);
        let accent = match self.kind {
            Kind::Info => 0x3b82f6,
            Kind::Success => 0x22c55e,
            Kind::Busy => 0xf59e0b,
            Kind::Error => 0xef4444,
        };
        text::fill(&mut buf, w, h, 0, 0, (5.0 * s) as i32, h as i32, accent, 255);
        text::frame(&mut buf, w, h, 1, 1, w as i32 - 1, h as i32 - 1, 1, 0x374151);
        font.draw(
            &mut buf,
            w,
            h,
            20.0 * s,
            31.0 * s,
            16.0 * s,
            0xf9fafb,
            &ellipsize(font, &self.title, 16.0 * s, w as f32 - 34.0 * s),
        );
        for (i, line) in self.body.lines().take(2).enumerate() {
            let (y, color) = if i == 0 { (57.0, 0x9ca3af) } else { (79.0, 0x6b7280) };
            font.draw(
                &mut buf,
                w,
                h,
                20.0 * s,
                y * s,
                13.0 * s,
                color,
                &ellipsize(font, line, 13.0 * s, w as f32 - 34.0 * s),
            );
        }
        let _ = buf.present();
    }
}

fn ellipsize(font: &Font, s: &str, size: f32, max: f32) -> String {
    if font.measure(s, size) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if font.measure(&format!("{out}{c}…"), size) > max {
            break;
        }
        out.push(c);
    }
    format!("{out}…")
}
