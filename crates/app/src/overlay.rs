use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use image::RgbaImage;
use markdownshot_core::Rect;

use crate::config::Mode;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId, WindowLevel};

use crate::text::{self, Font};
use crate::ui::toolbar::CaptureKind;

pub enum Outcome {
    Continue,
    Cancel,
    Done { sel: Rect, zones: Vec<Rect>, kind: CaptureKind, mode: Mode },
}

enum Drag {
    Selection { x: f64, y: f64 },
    Zone { x: f64, y: f64 },
}

const ACCENT: u32 = 0x3b82f6;
const ZONE: u32 = 0xf59e0b;
const REC: u32 = 0xef4444;

pub struct Overlay {
    pub window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    pub kind: CaptureKind,
    pub mode: Mode,
    pub shot: RgbaImage,
    pub origin: (i32, i32),
    windows: Vec<Rect>,
    w: usize,
    h: usize,
    dim: Vec<u32>,
    bright: Vec<u32>,
    sel: Option<Rect>,
    zones: Vec<Rect>,
    drag: Option<Drag>,
    cursor: (f64, f64),
    hover_window: Option<Rect>,
    last_click: Option<Instant>,
    shown: bool,
    pub auto_scroll: bool,
}

impl Overlay {
    pub fn new(
        el: &ActiveEventLoop,
        shot: RgbaImage,
        origin: (i32, i32),
        windows: Vec<Rect>,
        kind: CaptureKind,
        mode: Mode,
    ) -> anyhow::Result<Overlay> {
        let (w, h) = (shot.width() as usize, shot.height() as usize);
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title("MarkdownShot")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(origin.0, origin.1))
            .with_inner_size(PhysicalSize::new(w as u32, h as u32));
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true).with_undecorated_shadow(false);
        }
        let window = Rc::new(el.create_window(attrs)?);
        window.set_cursor(CursorIcon::Crosshair);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut dim = Vec::with_capacity(w * h);
        let mut bright = Vec::with_capacity(w * h);
        for p in shot.pixels() {
            let (r, g, b) = (p[0] as u32, p[1] as u32, p[2] as u32);
            bright.push((r << 16) | (g << 8) | b);
            dim.push(((r * 45 / 100) << 16) | ((g * 45 / 100) << 8) | (b * 45 / 100));
        }
        let o = Overlay {
            window,
            surface,
            _context: context,
            kind,
            mode,
            shot,
            origin,
            windows,
            w,
            h,
            dim,
            bright,
            sel: None,
            zones: Vec::new(),
            drag: None,
            cursor: (0.0, 0.0),
            hover_window: None,
            last_click: None,
            shown: false,
            auto_scroll: false,
        };
        o.update_cursor_icon();
        Ok(o)
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn dim_rgba(&self) -> RgbaImage {
        let mut out = RgbaImage::new(self.w as u32, self.h as u32);
        for (p, &v) in out.pixels_mut().zip(self.dim.iter()) {
            *p = image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
        }
        out
    }

    pub fn hint(&self) -> String {
        let manual = self.mode == Mode::Manual;
        match (self.kind, self.sel.is_some() && manual) {
            (CaptureKind::Record, _) if self.auto_scroll => t!(
                "Tracez la zone à enregistrer : MarkdownShot la fait défiler jusqu'en bas · D : défilement à la main",
                "Draw the area to record: MarkdownShot scrolls it to the bottom · D: scroll by hand"
            )
            .into(),
            (CaptureKind::Record, _) => t!(
                "Tracez la zone à enregistrer, puis faites défiler la page · Échap pour terminer · D : défilement auto",
                "Draw the area to record, then scroll the page · Esc to finish · D: auto-scroll"
            )
            .into(),
            (_, true) => {
                let n = self.zones.len();
                let zones = match n {
                    0 => t!("aucune image marquée", "no image marked").to_string(),
                    1 => t!("1 image marquée", "1 image marked").to_string(),
                    n => tf!("{n} images marquées", "{n} images marked"),
                };
                tf!(
                    "Tracez les zones d'images dans la sélection ({zones}) · Entrée : valider · clic droit : retirer",
                    "Draw the image areas in the selection ({zones}) · Enter: confirm · right-click: remove"
                )
            }
            (CaptureKind::Zone, _) if manual => t!("Tracez la zone à capturer, puis marquez les images", "Draw the area to capture, then mark the images").into(),
            (CaptureKind::Zone, _) if self.mode == Mode::Snipe => {
                t!("Tracez la zone à capturer : son Markdown s'affiche, prêt à copier", "Draw the area to capture: its Markdown is shown, ready to copy").into()
            }
            (CaptureKind::Zone, _) => t!("Tracez la zone à capturer : la note est créée dès que vous relâchez", "Draw the area to capture: the note is created as soon as you release").into(),
            (CaptureKind::Window, _) if manual => t!("Cliquez sur une fenêtre, puis marquez les images", "Click a window, then mark the images").into(),
            (CaptureKind::Window, _) if self.mode == Mode::Snipe => {
                t!("Cliquez sur une fenêtre : son Markdown s'affiche, prêt à copier", "Click a window: its Markdown is shown, ready to copy").into()
            }
            (CaptureKind::Window, _) => t!("Cliquez sur une fenêtre pour la capturer", "Click a window to capture it").into(),
            (CaptureKind::Full, _) => t!("Plein écran", "Full screen").into(),
        }
    }

    fn rect_from(a: (f64, f64), b: (f64, f64)) -> Rect {
        Rect::new(a.0.min(b.0) as f32, a.1.min(b.1) as f32, a.0.max(b.0) as f32, a.1.max(b.1) as f32)
    }

    fn screen(&self) -> Rect {
        Rect::new(0.0, 0.0, self.w as f32, self.h as f32)
    }

    fn done(&self) -> Outcome {
        match self.sel {
            Some(s) if s.w() >= 4.0 && s.h() >= 4.0 => {
                let zones =
                    self.zones.iter().map(|z| z.intersect(&s)).filter(|z| z.w() >= 4.0 && z.h() >= 4.0).collect();
                Outcome::Done { sel: s, zones, kind: self.kind, mode: self.mode }
            }
            _ => Outcome::Cancel,
        }
    }

    fn selected(&mut self, r: Rect) -> Outcome {
        self.sel = Some(r.intersect(&self.screen()));
        self.zones.clear();
        self.window.request_redraw();
        if self.mode != Mode::Manual || self.kind == CaptureKind::Record { self.done() } else { Outcome::Continue }
    }

    pub fn set_kind(&mut self, kind: CaptureKind) -> Outcome {
        self.kind = kind;
        self.sel = None;
        self.zones.clear();
        self.drag = None;
        self.update_cursor_icon();
        self.window.request_redraw();
        if kind == CaptureKind::Full {
            return self.selected(self.screen());
        }
        Outcome::Continue
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.window.request_redraw();
    }

    fn update_cursor_icon(&self) {
        self.window.set_cursor(match self.kind {
            CaptureKind::Window => CursorIcon::Pointer,
            _ => CursorIcon::Crosshair,
        });
    }

    fn window_at(&self, x: f64, y: f64) -> Option<Rect> {
        self.windows.iter().find(|r| r.contains_point(x as f32, y as f32)).map(|r| r.intersect(&self.screen()))
    }

    pub fn handle(&mut self, ev: &WindowEvent, font: &Font) -> Outcome {
        match ev {
            WindowEvent::CloseRequested => return Outcome::Cancel,
            WindowEvent::RedrawRequested => self.redraw(font),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if self.drag.is_some() {
                    self.window.request_redraw();
                } else if self.kind == CaptureKind::Window && self.sel.is_none() {
                    let h = self.window_at(position.x, position.y);
                    if h != self.hover_window {
                        self.hover_window = h;
                        self.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    let (x, y) = self.cursor;
                    let now = Instant::now();
                    let double = self.last_click.is_some_and(|t| now.duration_since(t).as_millis() < 350);
                    self.last_click = Some(now);
                    let inside = self.sel.is_some_and(|s| s.contains_point(x as f32, y as f32));
                    let tagging = self.mode == Mode::Manual && self.kind != CaptureKind::Record;
                    if double && inside && tagging {
                        return self.done();
                    }
                    if inside && tagging {
                        self.drag = Some(Drag::Zone { x, y });
                    } else if self.kind == CaptureKind::Window && !(inside && tagging) {
                        if let Some(r) = self.window_at(x, y) {
                            self.hover_window = None;
                            return self.selected(r);
                        }
                    } else if self.kind != CaptureKind::Full || self.sel.is_none() {
                        self.drag = Some(Drag::Selection { x, y });
                        self.zones.clear();
                    }
                    self.window.request_redraw();
                }
                (MouseButton::Left, ElementState::Released) => {
                    if let Some(d) = self.drag.take() {
                        match d {
                            Drag::Selection { x, y } => {
                                let r = Self::rect_from((x, y), self.cursor);
                                if r.w() >= 6.0 && r.h() >= 6.0 {
                                    return self.selected(r);
                                }
                            }
                            Drag::Zone { x, y } => {
                                let r = Self::rect_from((x, y), self.cursor);
                                if r.w() >= 8.0 && r.h() >= 8.0 {
                                    self.zones.push(r);
                                }
                            }
                        }
                        self.window.request_redraw();
                    }
                }
                (MouseButton::Right, ElementState::Pressed) => {
                    let (x, y) = self.cursor;
                    if let Some(i) = self.zones.iter().rposition(|z| z.contains_point(x as f32, y as f32)) {
                        self.zones.remove(i);
                        self.window.request_redraw();
                    } else {
                        return Outcome::Cancel;
                    }
                }
                _ => {}
            },
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                return self.key(&event.logical_key);
            }
            _ => {}
        }
        Outcome::Continue
    }

    pub fn key(&mut self, key: &Key) -> Outcome {
        match key {
            Key::Named(NamedKey::Escape) => return Outcome::Cancel,
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => return self.done(),
            Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete) => {
                self.zones.pop();
                self.window.request_redraw();
            }
            Key::Character(c) => match c.to_lowercase().as_str() {
                "z" => return self.set_kind(CaptureKind::Zone),
                "w" => return self.set_kind(CaptureKind::Window),
                "f" if crate::i18n::is_en() => return self.set_kind(CaptureKind::Full),
                "f" => return self.set_kind(CaptureKind::Window),
                "p" | "a" => return self.set_kind(CaptureKind::Full),
                "r" => return self.set_kind(CaptureKind::Record),
                "d" if self.kind == CaptureKind::Record => {
                    self.auto_scroll = !self.auto_scroll;
                    self.window.request_redraw();
                }
                "m" => self.set_mode(self.mode.next()),
                "s" => self.set_mode(if self.mode == Mode::Snipe { Mode::Auto } else { Mode::Snipe }),
                _ => {}
            },
            _ => {}
        }
        Outcome::Continue
    }

    fn redraw(&mut self, font: &Font) {
        let (w, h) = (self.w, self.h);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w as u32), NonZeroU32::new(h as u32)) else { return };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let screen = self.screen();
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        buf.copy_from_slice(&self.dim);

        let accent = if self.kind == CaptureKind::Record { REC } else { ACCENT };
        let live = match &self.drag {
            Some(Drag::Selection { x, y }) => Some(Self::rect_from((*x, *y), self.cursor)),
            _ => self.sel.or(self.hover_window),
        };
        if let Some(s) = live {
            let s = s.intersect(&screen);
            let (x0, y0, x1, y1) = (s.x0 as usize, s.y0 as usize, (s.x1 as usize).min(w), (s.y1 as usize).min(h));
            for y in y0..y1 {
                buf[y * w + x0..y * w + x1].copy_from_slice(&self.bright[y * w + x0..y * w + x1]);
            }
            text::frame(&mut buf, w, h, s.x0 as i32, s.y0 as i32, s.x1 as i32, s.y1 as i32, 2, accent);
            let label = format!("{} × {}", s.w() as i32, s.h() as i32);
            let lw = font.measure(&label, 14.0) as i32 + 12;
            let ly = if s.y0 > 26.0 { s.y0 as i32 - 26 } else { s.y0 as i32 + 4 };
            text::fill(&mut buf, w, h, s.x0 as i32, ly, s.x0 as i32 + lw, ly + 22, 0x111827, 230);
            font.draw(&mut buf, w, h, s.x0 + 6.0, ly as f32 + 16.0, 14.0, 0xffffff, &label);
        }
        let mut zones = self.zones.clone();
        if let Some(Drag::Zone { x, y }) = &self.drag {
            zones.push(Self::rect_from((*x, *y), self.cursor));
        }
        for z in &zones {
            text::fill(&mut buf, w, h, z.x0 as i32, z.y0 as i32, z.x1 as i32, z.y1 as i32, ZONE, 50);
            text::frame(&mut buf, w, h, z.x0 as i32, z.y0 as i32, z.x1 as i32, z.y1 as i32, 2, ZONE);
            font.draw(&mut buf, w, h, z.x0 + 6.0, z.y0 + 18.0, 13.0, ZONE, "image");
        }
        let _ = buf.present();
    }

    pub fn show(&mut self, font: &Font) {
        if !self.shown {
            self.shown = true;
            self.window.set_visible(true);
            self.window.focus_window();
        }
        self.redraw(font);
    }
}
