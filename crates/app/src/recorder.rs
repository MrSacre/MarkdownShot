use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use image::RgbImage;
use markdownshot_core::stitch::Stitcher;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::scrolling::{self, End, Options, Status};
use crate::text::{self, Font};

struct Deco {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    w: u32,
    h: u32,
}

impl Deco {
    fn new(el: &ActiveEventLoop, x: i32, y: i32, w: u32, h: u32) -> anyhow::Result<Deco> {
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title("MarkdownShot")
            .with_decorations(false)
            .with_resizable(false)
            .with_active(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(x, y))
            .with_inner_size(PhysicalSize::new(w.max(1), h.max(1)));
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true).with_undecorated_shadow(false);
        }
        let window = Rc::new(el.create_window(attrs)?);
        let _ = window.set_cursor_hittest(false);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Deco { window, surface, _context: context, w: w.max(1), h: h.max(1) })
    }

    fn paint(&mut self, f: impl FnOnce(&mut [u32], usize, usize)) {
        let (Some(nw), Some(nh)) = (NonZeroU32::new(self.w), NonZeroU32::new(self.h)) else { return };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            f(&mut buf, self.w as usize, self.h as usize);
            let _ = buf.present();
        }
    }
}

pub struct Recording {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<(Stitcher, End)>>,
    pub status: Arc<Mutex<Status>>,
    borders: Vec<Deco>,
    label: Option<Deco>,
    pub source: Option<String>,
    started: Instant,
}

const RED: u32 = 0xef4444;

impl Recording {
    pub fn start(
        el: &ActiveEventLoop,
        origin: (i32, i32),
        screen: (u32, u32),
        sel: (u32, u32, u32, u32),
        opts: Options,
        source: Option<String>,
        font: &Font,
    ) -> anyhow::Result<Recording> {
        let (sx, sy, sw, sh) = sel;
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status { height: sh, auto: opts.auto_step.is_some(), ..Default::default() }));
        let handle = scrolling::spawn(origin, sel, opts, stop.clone(), status.clone())?;

        let mut borders = Vec::new();
        let t = 3u32;
        let (gx, gy) = (origin.0 + sx as i32, origin.1 + sy as i32);
        let (scr_x1, scr_y1) = (origin.0 + screen.0 as i32, origin.1 + screen.1 as i32);
        let sides = [
            (gx - t as i32 - 1, gy - t as i32 - 1, sw + 2 * t + 2, t, gy - t as i32 - 1 >= origin.1),
            (gx - t as i32 - 1, gy + sh as i32 + 1, sw + 2 * t + 2, t, gy + sh as i32 + 1 + t as i32 <= scr_y1),
            (gx - t as i32 - 1, gy - 1, t, sh + 2, gx - t as i32 - 1 >= origin.0),
            (gx + sw as i32 + 1, gy - 1, t, sh + 2, gx + sw as i32 + 1 + t as i32 <= scr_x1),
        ];
        for (x, y, w, h, ok) in sides {
            if ok {
                if let Ok(mut d) = Deco::new(el, x, y, w, h) {
                    d.paint(|b, _, _| b.fill(RED));
                    borders.push(d);
                }
            }
        }
        let (lw, lh) = (460u32, 30u32);
        let above = gy - lh as i32 - 8 >= origin.1;
        let below = gy + sh as i32 + lh as i32 + 8 <= scr_y1;
        let label = if above || below {
            let ly = if above { gy - lh as i32 - 8 } else { gy + sh as i32 + 8 };
            Deco::new(el, gx.max(origin.0), ly, lw, lh).ok()
        } else {
            None
        };
        let mut rec = Recording { stop, handle: Some(handle), status, borders, label, source, started: Instant::now() };
        rec.refresh(font);
        Ok(rec)
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.borders.iter().any(|b| b.window.id() == id) || self.label.as_ref().is_some_and(|l| l.window.id() == id)
    }

    pub fn refresh(&mut self, font: &Font) {
        let s = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        let (frames, height, gaps) = (s.frames, s.height, s.gaps);
        let secs = self.started.elapsed().as_secs();
        let txt = if s.auto {
            tf!(
                "● AUTO {secs}s · {height} px · ne touchez pas la souris · Échap pour arrêter",
                "● AUTO {secs}s · {height} px · don't touch the mouse · Esc to stop"
            )
        } else if s.auto_failed {
            tf!(
                "● REC {secs}s · {height} px · la page ne défile pas seule : faites-la défiler · Échap pour finir",
                "● REC {secs}s · {height} px · the page won't scroll by itself: scroll it · Esc to finish"
            )
        } else if s.auto_interrupted {
            tf!(
                "● REC {secs}s · {height} px · défilement auto arrêté (souris) · Échap pour finir",
                "● REC {secs}s · {height} px · auto-scroll stopped (mouse) · Esc to finish"
            )
        } else if gaps > 0 {
            tf!("● REC {secs}s · {height} px · défilez moins vite ! · Échap pour finir", "● REC {secs}s · {height} px · scroll slower! · Esc to finish")
        } else {
            tf!("● REC {secs}s · {height} px · {frames} vues · Échap pour finir", "● REC {secs}s · {height} px · {frames} frames · Esc to finish")
        };
        if let Some(l) = self.label.as_mut() {
            l.paint(|b, w, h| {
                b.fill(0x0f172a);
                text::fill(b, w, h, 0, 0, 4, h as i32, RED, 255);
                font.draw(b, w, h, 14.0, 20.0, 14.0, 0xf8fafc, &txt);
            });
        }
        for d in self.borders.iter_mut() {
            d.paint(|b, _, _| b.fill(RED));
        }
    }

    pub fn finished(&self) -> Option<End> {
        self.status.lock().ok().and_then(|s| s.finished)
    }

    pub fn stop(mut self) -> Option<RgbImage> {
        self.stop.store(true, Ordering::Relaxed);
        self.borders.clear();
        self.label = None;
        let (st, _) = self.handle.take()?.join().ok()?;
        if st.frames_used == 0 {
            return None;
        }
        log::info!("recording: {} of {} frames used, {} gap(s)", st.frames_used, st.frames_seen, st.gaps);
        Some(st.finish())
    }
}
