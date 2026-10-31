pub mod review;
pub mod settings;
pub mod toolbar;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(37, 99, 235);
pub const ACCENT_SOFT: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
pub const ZONE_IMAGE: egui::Color32 = egui::Color32::from_rgb(245, 158, 11);
pub const ZONE_TEXT: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);
pub const ZONE_IGNORE: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);

pub struct EguiWindow {
    pub window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    pub ctx: egui::Context,
    state: egui_winit::State,
    renderer: EguiSoftwareRender,
    pub next_repaint: Option<Instant>,
    pub close_requested: bool,
    pub clear: [u8; 3],
}

impl EguiWindow {
    pub fn new(el: &ActiveEventLoop, attrs: WindowAttributes) -> anyhow::Result<EguiWindow> {
        let window = Rc::new(el.create_window(attrs)?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let ctx = egui::Context::default();
        apply_style(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(8192),
        );
        Ok(EguiWindow {
            window,
            surface,
            _context: context,
            ctx,
            state,
            renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
            next_repaint: Some(Instant::now()),
            close_requested: false,
            clear: [24, 24, 27],
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn on_event(&mut self, ev: &WindowEvent) -> bool {
        if let WindowEvent::CloseRequested = ev {
            self.close_requested = true;
            return false;
        }
        if let WindowEvent::RedrawRequested = ev {
            return true;
        }
        let r = self.state.on_window_event(&self.window, ev);
        if r.repaint {
            self.next_repaint = Some(Instant::now());
        }
        r.repaint
    }

    pub fn due(&self) -> bool {
        self.next_repaint.is_some_and(|t| Instant::now() >= t)
    }

    pub fn paint(&mut self, mut ui_fn: impl FnMut(&mut egui::Ui)) {
        self.next_repaint = None;
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        let input = self.state.take_egui_input(&self.window);
        let out = self.ctx.run_ui(input, |ui| ui_fn(ui));
        self.state.handle_platform_output(&self.window, out.platform_output);
        if let Some(vo) = out.viewport_output.get(&egui::ViewportId::ROOT) {
            if vo.repaint_delay < Duration::from_secs(3600) {
                self.next_repaint = Some(Instant::now() + vo.repaint_delay);
            }
            for cmd in &vo.commands {
                if let egui::ViewportCommand::Close = cmd {
                    self.close_requested = true;
                }
            }
        }
        let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        if self.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        let [r, g, b] = self.clear;
        buf.fill(((r as u32) << 16) | ((g as u32) << 8) | b as u32);
        {
            let px: &mut [[u8; 4]] = bytemuck::cast_slice_mut(&mut buf[..]);
            let mut target = BufferMutRef::new(px, w.get() as usize, h.get() as usize);
            self.renderer.render(&mut target, &prims, &out.textures_delta, out.pixels_per_point);
        }
        for p in buf.iter_mut() {
            *p &= 0x00ff_ffff;
        }
        let _ = buf.present();
    }
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = egui::vec2(10.0, 8.0);
            s.spacing.button_padding = egui::vec2(12.0, 6.0);
            s.spacing.interact_size.y = 28.0;
            s.spacing.combo_width = 220.0;
            let v = &mut s.visuals;
            let r = egui::CornerRadius::same(4);
            v.widgets.noninteractive.corner_radius = r;
            v.widgets.inactive.corner_radius = r;
            v.widgets.hovered.corner_radius = r;
            v.widgets.active.corner_radius = r;
            v.widgets.open.corner_radius = r;
            v.window_corner_radius = egui::CornerRadius::same(10);
            v.selection.bg_fill = ACCENT;
            v.selection.stroke.color = egui::Color32::WHITE;
            v.hyperlink_color = ACCENT_SOFT;
            if theme == egui::Theme::Dark {
                v.panel_fill = egui::Color32::from_rgb(24, 24, 27);
                v.window_fill = egui::Color32::from_rgb(30, 30, 34);
                v.extreme_bg_color = egui::Color32::from_rgb(17, 17, 19);
                v.faint_bg_color = egui::Color32::from_rgb(32, 32, 37);
            } else {
                v.panel_fill = egui::Color32::from_rgb(250, 250, 250);
                v.window_fill = egui::Color32::WHITE;
                v.extreme_bg_color = egui::Color32::WHITE;
                v.faint_bg_color = egui::Color32::from_rgb(244, 244, 245);
            }
            s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
            s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
            s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        });
    }
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(egui::Color32::WHITE).strong())
            .fill(ACCENT)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
}

pub fn window_icon() -> Option<winit::window::Icon> {
    winit::window::Icon::from_rgba(crate::icon::rgba(64), 64, 64).ok()
}

pub fn panel_rgb(ctx: &egui::Context) -> [u8; 3] {
    let c = ctx.global_style().visuals.panel_fill;
    [c.r(), c.g(), c.b()]
}
