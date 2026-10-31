use std::sync::Arc;

use image::RgbaImage;
use crate::config::Mode;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowLevel};

use super::{ACCENT, EguiWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    Zone,
    Window,
    Full,
    Record,
}

pub enum Action {
    Kind(CaptureKind),
    Mode(Mode),
    AutoScroll(bool),
    Settings,
    Cancel,
}

pub struct ToolbarWindow {
    pub win: EguiWindow,
    pub st: ToolbarState,
}

pub struct ToolbarState {
    pub kind: CaptureKind,
    pub mode: Mode,
    pub auto_scroll: bool,
    pub hint: String,
    backdrop: Arc<egui::ColorImage>,
    backdrop_tex: Option<egui::TextureHandle>,
}

const W: f32 = 990.0;
const H: f32 = 92.0;

impl ToolbarWindow {
    pub fn new(
        el: &ActiveEventLoop,
        shot_dim: &RgbaImage,
        origin: (i32, i32),
        scale: f64,
        kind: CaptureKind,
        mode: Mode,
        auto_scroll: bool,
    ) -> anyhow::Result<ToolbarWindow> {
        let w = (W as f64 * scale).round() as u32;
        let h = (H as f64 * scale).round() as u32;
        let x = ((shot_dim.width().saturating_sub(w)) / 2) as i32;
        let y = (14.0 * scale) as i32;
        #[allow(unused_mut)]
        let mut attrs = WindowAttributes::default()
            .with_title("MarkdownShot")
            .with_decorations(false)
            .with_resizable(false)
            .with_active(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(origin.0 + x, origin.1 + y))
            .with_inner_size(PhysicalSize::new(w, h));
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true).with_undecorated_shadow(false);
        }
        let win = EguiWindow::new(el, attrs)?;
        let (bw, bh) = (w.min(shot_dim.width()), h.min(shot_dim.height().saturating_sub(y as u32)));
        let crop = image::imageops::crop_imm(shot_dim, x as u32, y as u32, bw.max(1), bh.max(1)).to_image();
        let backdrop = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            [crop.width() as usize, crop.height() as usize],
            crop.as_raw(),
        ));
        let st = ToolbarState { kind, mode, auto_scroll, hint: String::new(), backdrop, backdrop_tex: None };
        Ok(ToolbarWindow { win, st })
    }

    pub fn paint(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let st = &mut self.st;
        self.win.paint(|ui| st.ui(ui, &mut actions));
        actions
    }

    pub fn set(&mut self, kind: CaptureKind, mode: Mode, auto_scroll: bool, hint: String) {
        if self.st.kind != kind || self.st.mode != mode || self.st.auto_scroll != auto_scroll || self.st.hint != hint {
            self.st.kind = kind;
            self.st.mode = mode;
            self.st.auto_scroll = auto_scroll;
            self.st.hint = hint;
            self.win.next_repaint = Some(std::time::Instant::now());
        }
    }
}

impl ToolbarState {
    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        ui.style_mut().visuals = egui::Visuals::dark();
        let tex = self
            .backdrop_tex
            .get_or_insert_with(|| ctx.load_texture("backdrop", self.backdrop.clone(), egui::TextureOptions::LINEAR))
            .clone();
        let full = ui.max_rect();
        ui.painter().image(tex.id(), full, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);

        let bar = egui::Rect::from_min_size(full.min + egui::vec2(4.0, 2.0), egui::vec2(full.width() - 8.0, 52.0));
        ui.painter().rect_filled(bar, 12, egui::Color32::from_rgb(28, 28, 32));
        ui.painter().rect_stroke(bar, 12, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 60, 68)), egui::StrokeKind::Inside);

        let mut row = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(10.0, 8.0))));
        row.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (k, icon, label, key) in [
                (CaptureKind::Zone, "⛶", "Zone", "Z"),
                (CaptureKind::Window, "🗖", t!("Fenêtre", "Window"), t!("F", "W")),
                (CaptureKind::Full, "🖵", t!("Plein écran", "Full screen"), t!("P", "F")),
                (CaptureKind::Record, "⏺", t!("Enregistrer", "Record"), "R"),
            ] {
                if tool_button(ui, self.kind == k, icon, label, k == CaptureKind::Record)
                    .on_hover_text(format!("{label}  ({key})"))
                    .clicked()
                {
                    actions.push(Action::Kind(k));
                }
            }
            let rec = self.kind == CaptureKind::Record;
            let tip = t!(
                "Défilement automatique : MarkdownShot fait défiler la page jusqu'en bas  (D)",
                "Auto-scroll: MarkdownShot scrolls the page down to the bottom  (D)"
            );
            if ui
                .add_enabled_ui(rec, |ui| tool_button(ui, rec && self.auto_scroll, "⬇", t!("Défil. auto", "Auto-scroll"), false))
                .inner
                .on_hover_text(tip)
                .clicked()
            {
                actions.push(Action::AutoScroll(!self.auto_scroll));
            }
            separator(ui);
            for (m, icon, label, tip) in [
                (Mode::Auto, "⚡", "Auto", t!("MarkdownShot découpe les images et crée la note  (M)", "MarkdownShot crops the images and creates the note  (M)")),
                (Mode::Snipe, "✂", "Snipe", t!("Aperçu du Markdown à copier-coller, sans créer de note  (S)", "Markdown preview to copy and paste, no note created  (S)")),
                (Mode::Manual, "✋", t!("Manuel", "Manual"), t!("Vous marquez les zones d'images  (M)", "You mark the image areas  (M)")),
            ] {
                if tool_button(ui, self.mode == m, icon, label, false).on_hover_text(tip).clicked() {
                    actions.push(Action::Mode(m));
                }
            }
            separator(ui);
            if icon_button(ui, "⚙").on_hover_text(t!("Paramètres", "Settings")).clicked() {
                actions.push(Action::Settings);
            }
            if icon_button(ui, "✖").on_hover_text(t!("Annuler  (Échap)", "Cancel  (Esc)")).clicked() {
                actions.push(Action::Cancel);
            }
        });

        if !self.hint.is_empty() {
            let font = egui::FontId::proportional(13.0);
            let galley = ui.painter().layout_no_wrap(self.hint.clone(), font, egui::Color32::from_rgb(226, 232, 240));
            let size = galley.size() + egui::vec2(20.0, 8.0);
            let pill = egui::Rect::from_center_size(egui::pos2(full.center().x, bar.max.y + 6.0 + size.y / 2.0), size);
            ui.painter().rect_filled(pill, 8, egui::Color32::from_rgba_unmultiplied(15, 23, 42, 235));
            ui.painter().galley(pill.min + egui::vec2(10.0, 4.0), galley, egui::Color32::WHITE);
        }
    }
}

fn tool_button(ui: &mut egui::Ui, active: bool, icon: &str, label: &str, record: bool) -> egui::Response {
    let fg = if active { egui::Color32::WHITE } else { egui::Color32::from_rgb(203, 213, 225) };
    let icon_color = if record { egui::Color32::from_rgb(248, 113, 113) } else { fg };
    let mut job = egui::text::LayoutJob::default();
    job.append(icon, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(16.0), color: icon_color, ..Default::default() });
    job.append(label, 7.0, egui::TextFormat { font_id: egui::FontId::proportional(14.0), color: fg, ..Default::default() });
    let fill = if active { ACCENT } else { egui::Color32::TRANSPARENT };
    ui.add(egui::Button::new(job).fill(fill).corner_radius(8).min_size(egui::vec2(0.0, 34.0)).stroke(egui::Stroke::NONE))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_button(ui: &mut egui::Ui, icon: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).size(16.0).color(egui::Color32::from_rgb(203, 213, 225)))
            .fill(egui::Color32::TRANSPARENT)
            .corner_radius(8)
            .min_size(egui::vec2(34.0, 34.0))
            .stroke(egui::Stroke::NONE),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn separator(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(egui::vec2(1.0, 26.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0, egui::Color32::from_rgb(63, 63, 70));
    ui.add_space(6.0);
}
