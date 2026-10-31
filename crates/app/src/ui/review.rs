use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use markdownshot_core::{Analysis, Rect, Zone, ZoneKind};
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;

use super::{EguiWindow, ZONE_IGNORE, ZONE_IMAGE, hint, primary_button};

const TILE: u32 = 1024;
const MAP_W: f32 = 120.0;
const SNIPE_MD_W: f32 = 520.0;
const CUT: egui::Color32 = egui::Color32::from_rgb(100, 116, 139);

pub enum Action {
    Save(Vec<Zone>),
    Cancel,
    Preview(u64, Vec<Zone>),
    Copy { zones: Vec<Zone>, close: bool },
    ChooseFolder,
}

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

enum Drag {
    New(egui::Pos2, ZoneKind),
    Cut,
    Edit(usize, Handle, Rect, egui::Pos2),
}

pub struct ReviewWindow {
    pub win: EguiWindow,
    pub st: ReviewState,
}

pub struct ReviewState {
    analysis: Arc<Analysis>,
    pub zones: Vec<Zone>,
    selected: Option<usize>,
    cut: Option<f32>,
    placing_cut: bool,
    zoom: Option<f32>,
    pub save_label: String,
    pub snipe: bool,
    copied: Option<Instant>,
    pub dest: std::path::PathBuf,
    eff_zoom: f32,
    tiles: HashMap<u32, egui::TextureHandle>,
    thumb: Option<(egui::TextureHandle, f32)>,
    drag: Option<Drag>,
    show_preview: bool,
    preview: String,
    preview_gen: u64,
    preview_wanted: bool,
    scroll_to: Option<f32>,
    title: String,
    opened: Instant,
    viewport: Option<(f32, f32, f32)>,
}

impl ReviewWindow {
    pub fn new(
        el: &ActiveEventLoop,
        analysis: Arc<Analysis>,
        zones: Vec<Zone>,
        title: &str,
        snipe: bool,
    ) -> anyhow::Result<ReviewWindow> {
        let size = if snipe {
            let (iw, ih) = analysis.image.dimensions();
            let page_w = (iw as f64).clamp(420.0, 900.0);
            LogicalSize::new(page_w + SNIPE_MD_W as f64 + MAP_W as f64 + 40.0, (ih as f64 + 120.0).clamp(520.0, 900.0))
        } else {
            LogicalSize::new(1360.0, 900.0)
        };
        let attrs = WindowAttributes::default()
            .with_title(format!("MarkdownShot - {title}"))
            .with_inner_size(size)
            .with_min_inner_size(LogicalSize::new(820.0, 520.0))
            .with_maximized(!snipe)
            .with_window_icon(super::window_icon());
        let mut win = EguiWindow::new(el, attrs)?;
        win.window.focus_window();
        win.clear = super::panel_rgb(&win.ctx);
        let st = ReviewState {
            analysis,
            zones,
            selected: None,
            cut: None,
            placing_cut: false,
            zoom: None,
            save_label: t!("Enregistrer la note", "Save note").into(),
            snipe,
            copied: None,
            dest: std::path::PathBuf::new(),
            eff_zoom: 0.5,
            tiles: HashMap::new(),
            thumb: None,
            drag: None,
            show_preview: true,
            preview: String::new(),
            preview_gen: 0,
            preview_wanted: true,
            scroll_to: None,
            title: title.to_string(),
            opened: Instant::now(),
            viewport: None,
        };
        Ok(ReviewWindow { win, st })
    }

    pub fn paint(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let st = &mut self.st;
        self.win.paint(|ui| st.ui(ui, &mut actions));
        actions
    }

    pub fn copied(&mut self) {
        self.st.copied = Some(Instant::now());
        self.win.next_repaint = Some(Instant::now());
    }

    pub fn set_preview(&mut self, gen_: u64, md: String) {
        if gen_ == self.st.preview_gen {
            self.st.preview = md;
            self.win.next_repaint = Some(Instant::now());
        }
    }
}

impl ReviewState {
    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.keyboard(ui);
        if !ui.ctx().egui_wants_keyboard_input() && self.drag.is_none() {
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter && self.snipe {
                actions.push(Action::Copy { zones: self.zones_out(), close: true });
            } else if enter {
                actions.push(Action::Save(self.zones_out()));
            } else if esc && self.placing_cut {
                self.placing_cut = false;
            } else if esc {
                actions.push(Action::Cancel);
            }
        }
        let (iw, ih) = self.analysis.image.dimensions();

        egui::Panel::top("toolbar").exact_size(56.0).resizable(false).show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let heading = if self.snipe { "✂  Snipe" } else { t!("Zones d'images", "Image areas") };
                ui.label(egui::RichText::new(heading).size(17.0).strong());
                ui.add_space(12.0);
                for (k, label, color, key) in [
                    (ZoneKind::Image, "▣  Image", ZONE_IMAGE, "I"),
                    (ZoneKind::Ignore, t!("✖  Ignorer", "✖  Ignore"), ZONE_IGNORE, "X"),
                ] {
                    let sel = self.selected.and_then(|i| self.zones.get(i)).is_some_and(|z| z.kind == k);
                    let text = egui::RichText::new(label).color(if sel { egui::Color32::WHITE } else { color });
                    let b = egui::Button::new(text).fill(if sel { color } else { egui::Color32::TRANSPARENT });
                    let tip = if k == ZoneKind::Image {
                        t!(
                            "Zone image : tracer avec le clic gauche. Sur une zone choisie : la changer en image  (I)",
                            "Image area: draw with the left button. On a selected area: make it an image  (I)"
                        )
                    } else {
                        t!(
                            "Zone à ignorer (publicité, bouton, bandeau…) : tracer avec le clic droit. Sur une zone choisie : l'ignorer  (X)",
                            "Area to ignore (ad, button, banner…): draw with the right button. On a selected area: ignore it  (X)"
                        )
                    };
                    if ui.add(b).on_hover_text(format!("{tip}")).clicked() {
                        if let Some(i) = self.selected.filter(|i| *i < self.zones.len()) {
                            self.zones[i].kind = k;
                            self.preview_wanted = true;
                        }
                    }
                    let _ = key;
                }
                let cut_label = if self.cut.is_some() {
                    t!("✂  Fin de page ✖", "✂  Page end ✖")
                } else {
                    t!("✂  Fin de page", "✂  Page end")
                };
                let b = egui::Button::new(egui::RichText::new(cut_label).color(if self.placing_cut { egui::Color32::WHITE } else { CUT }))
                    .fill(if self.placing_cut { CUT } else { egui::Color32::TRANSPARENT });
                let tip = if self.cut.is_some() {
                    t!("Retirer la fin de page (la ligne se déplace en la glissant)  (Fin)", "Remove the page end (drag the line to move it)  (End)")
                } else {
                    t!(
                        "Cliquer sur la page pour y placer la fin : ce qui est en dessous n'entre pas dans la note  (Fin)",
                        "Click on the page to set its end: what is below is left out of the note  (End)"
                    )
                };
                if ui.add(b).on_hover_text(tip).clicked() {
                    self.toggle_cut();
                }
                let n = self.missing_suggestions().len();
                if ui
                    .add_enabled(n > 0, egui::Button::new(tf!("Proposer les images ({n})", "Suggest images ({n})")))
                    .on_hover_text(t!("Ajoute les images détectées automatiquement (modifiables ensuite)", "Adds the automatically detected images (editable afterwards)"))
                    .clicked()
                {
                    let add = self.missing_suggestions();
                    self.zones.extend(add.into_iter().map(Zone::image));
                    self.preview_wanted = true;
                }
                ui.add_space(12.0);
                ui.separator();
                if ui.button("−").on_hover_text(t!("Zoom arrière  (Ctrl + molette)", "Zoom out  (Ctrl + wheel)")).clicked() {
                    self.zoom = Some((self.eff_zoom / 1.25).max(0.1));
                }
                if ui.button(t!("Ajuster", "Fit")).on_hover_text(t!("Ajuster à la largeur", "Fit to width")).clicked() {
                    self.zoom = None;
                }
                if ui.button("+").on_hover_text(t!("Zoom avant  (Ctrl + molette)", "Zoom in  (Ctrl + wheel)")).clicked() {
                    self.zoom = Some((self.eff_zoom * 1.25).min(4.0));
                }
                ui.separator();
                ui.toggle_value(&mut self.show_preview, t!("👁 Aperçu", "👁 Preview"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.snipe {
                        let recent = self.copied.is_some_and(|t| t.elapsed().as_secs_f32() < 1.6);
                        let label = if recent { t!("✔  Copié", "✔  Copied") } else { t!("📋  Copier", "📋  Copy") };
                        if primary_button(ui, label)
                            .on_hover_text(t!("Copier tout le Markdown  (Entrée : copier et fermer)", "Copy all the Markdown  (Enter: copy and close)"))
                            .clicked()
                        {
                            actions.push(Action::Copy { zones: self.zones_out(), close: false });
                        }
                        if recent {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                        }
                        if ui.button(t!("Enregistrer la note", "Save note")).on_hover_text(t!("Écrire la note et ses images, comme en mode Auto", "Write the note and its images, as in Auto mode")).clicked() {
                            actions.push(Action::Save(self.zones_out()));
                        }
                        if ui.button(t!("Fermer", "Close")).on_hover_text(t!("Fermer sans rien enregistrer  (Échap)", "Close without saving anything  (Esc)")).clicked() {
                            actions.push(Action::Cancel);
                        }
                    } else {
                        if primary_button(ui, &self.save_label).on_hover_text(t!("Entrée", "Enter")).clicked() {
                            actions.push(Action::Save(self.zones_out()));
                        }
                        if ui.button(t!("Annuler", "Cancel")).on_hover_text(t!("Échap", "Esc")).clicked() {
                            actions.push(Action::Cancel);
                        }
                    }
                    let name = self.dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| self.dest.display().to_string());
                    if ui
                        .button(format!("📁 {name}"))
                        .on_hover_text(tf!(
                            "Enregistrer dans : {}\nCliquer pour choisir un autre dossier (cette note seulement)",
                            "Save to: {}\nClick to pick another folder (this note only)",
                            self.dest.display()
                        ))
                        .clicked()
                    {
                        actions.push(Action::ChooseFolder);
                    }
                });
            });
        });

        egui::Panel::bottom("pied").exact_size(30.0).resizable(false).show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let n_img = self.zones.iter().filter(|z| z.kind == ZoneKind::Image).count();
                let n_ign = self.zones.iter().filter(|z| z.kind == ZoneKind::Ignore).count();
                hint(
                    ui,
                    &tf!(
                        "{} · page de {iw} × {ih} px · {n_img} image{} · {n_ign} zone{} ignorée{}",
                        "{} · {iw} × {ih} px page · {n_img} image{} · {n_ign} ignored area{}{}",
                        self.title,
                        if n_img > 1 { "s" } else { "" },
                        if n_ign > 1 { "s" } else { "" },
                        if n_ign > 1 && !crate::i18n::is_en() { "s" } else { "" }
                    ),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.snipe {
                        hint(ui, t!("Entrée : copier et fermer · Échap : fermer sans enregistrer · sélection + Ctrl+C : copier une partie", "Enter: copy and close · Esc: close without saving · select + Ctrl+C: copy a part"));
                    } else {
                        hint(
                            ui,
                            t!(
                                "Clic gauche glissé : image · clic droit glissé : à ignorer · glisser une zone : déplacer · bords : redimensionner · Suppr : retirer · Fin : fin de page",
                                "Left drag: image · right drag: ignore · drag an area: move · edges: resize · Del: remove · End: page end"
                            ),
                        );
                    }
                });
            });
        });

        if self.show_preview {
            let width = if self.snipe { SNIPE_MD_W } else { 420.0 };
            egui::Panel::right("apercu").default_size(width).size_range(280.0..=900.0).show_inside(ui, |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(t!("Aperçu du Markdown", "Markdown preview")).strong());
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut text = self.preview.as_str();
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE),
                    );
                });
            });
        }

        egui::Panel::right("minimap").exact_size(MAP_W + 16.0).resizable(false).show_inside(ui, |ui| {
            self.minimap(ui);
        });

        egui::CentralPanel::no_frame().show_inside(ui, |ui| {
            let avail = ui.available_width() - 16.0;
            let zoom = self.zoom.unwrap_or((avail / iw as f32).min(1.0 / ui.ctx().pixels_per_point().max(0.5) * 1.5));
            self.eff_zoom = zoom;
            let (ctrl, factor) = ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.zoom_delta()));
            if factor != 1.0 && ui.rect_contains_pointer(ui.max_rect()) {
                self.zoom = Some((zoom * factor).clamp(0.1, 4.0));
            }
            let source = egui::scroll_area::ScrollSource { scroll_bar: true, drag: false, mouse_wheel: !ctrl };
            let mut area = egui::ScrollArea::both().auto_shrink([false, false]).scroll_source(source);
            if let Some(y) = self.scroll_to.take() {
                area = area.vertical_scroll_offset(y);
            }
            let out = area.show_viewport(ui, |ui, viewport| {
                let size = egui::vec2(iw as f32 * zoom, ih as f32 * zoom);
                let margin = ((ui.available_width() - size.x) / 2.0).max(8.0);
                let (resp, painter) =
                    ui.allocate_painter(egui::vec2(size.x + 2.0 * margin, size.y + 16.0), egui::Sense::click_and_drag());
                let origin = resp.rect.min + egui::vec2(margin, 8.0);
                let page = egui::Rect::from_min_size(origin, size);
                painter.rect_filled(page.expand(1.0), 0, egui::Color32::from_gray(90));
                self.paint_tiles(ui.ctx(), &painter, page, zoom, viewport.translate(resp.rect.min.to_vec2()));
                self.interact(ui, &resp, &painter, page, zoom);
            });
            self.viewport = Some((out.state.offset.y, out.inner_rect.height(), ih as f32 * zoom));
        });

        if self.preview_wanted && self.drag.is_none() {
            self.preview_wanted = false;
            self.preview_gen += 1;
            actions.push(Action::Preview(self.preview_gen, self.zones_out()));
        }
        let _ = self.opened;
    }

    fn paint_tiles(&mut self, ctx: &egui::Context, painter: &egui::Painter, page: egui::Rect, zoom: f32, visible: egui::Rect) {
        let (iw, ih) = self.analysis.image.dimensions();
        let n = ih.div_ceil(TILE);
        for t in 0..n {
            let y0 = t * TILE;
            let y1 = (y0 + TILE).min(ih);
            let r = egui::Rect::from_min_max(
                egui::pos2(page.min.x, page.min.y + y0 as f32 * zoom),
                egui::pos2(page.max.x, page.min.y + y1 as f32 * zoom),
            );
            if !r.intersects(visible.expand(200.0)) {
                continue;
            }
            let img = &self.analysis.image;
            let tex = self.tiles.entry(t).or_insert_with(|| {
                let crop = image::imageops::crop_imm(&**img, 0, y0, iw, y1 - y0).to_image();
                let ci = egui::ColorImage::from_rgb([crop.width() as usize, crop.height() as usize], crop.as_raw());
                ctx.load_texture(format!("page{t}"), ci, egui::TextureOptions::LINEAR)
            });
            painter.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    fn minimap(&mut self, ui: &mut egui::Ui) {
        let (iw, ih) = self.analysis.image.dimensions();
        let img = &self.analysis.image;
        let (tex, scale) = self
            .thumb
            .get_or_insert_with(|| {
                let w = 200u32.min(iw);
                let h = ((ih as f32 * w as f32 / iw as f32).round() as u32).clamp(1, 4096);
                let small = image::imageops::thumbnail(&**img, w, h);
                let ci = egui::ColorImage::from_rgb([small.width() as usize, small.height() as usize], small.as_raw());
                let tex = ui.ctx().load_texture("minimap", ci, egui::TextureOptions::LINEAR);
                (tex, MAP_W / iw as f32)
            })
            .clone();
        let avail_h = ui.available_height() - 16.0;
        let scale = scale.min(avail_h / ih as f32);
        let size = egui::vec2(iw as f32 * scale, ih as f32 * scale);
        ui.add_space(8.0);
        let (resp, painter) = ui.allocate_painter(egui::vec2(MAP_W, size.y), egui::Sense::click_and_drag());
        let r = egui::Rect::from_min_size(resp.rect.min + egui::vec2((MAP_W - size.x) / 2.0, 0.0), size);
        painter.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        for z in &self.zones {
            let zr = egui::Rect::from_min_max(
                r.min + egui::vec2(z.rect.x0 * scale, z.rect.y0 * scale),
                r.min + egui::vec2(z.rect.x1 * scale, z.rect.y1 * scale),
            );
            painter.rect_filled(zr, 0, kind_color(z.kind).gamma_multiply(0.45));
        }
        if let Some(c) = self.cut {
            let y = r.min.y + c * scale;
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(r.min.x, y), r.max), 0, egui::Color32::from_rgba_unmultiplied(40, 44, 52, 170));
            painter.line_segment([egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)], egui::Stroke::new(1.5_f32, CUT));
        }
        if let Some((off, view_h, content_h)) = self.viewport {
            if content_h > 0.0 {
                let y0 = r.min.y + off / content_h * size.y;
                let y1 = r.min.y + (off + view_h).min(content_h) / content_h * size.y;
                let vr = egui::Rect::from_min_max(egui::pos2(r.min.x - 3.0, y0), egui::pos2(r.max.x + 3.0, y1));
                painter.rect_stroke(vr, 3, egui::Stroke::new(2.0_f32, super::ACCENT_SOFT), egui::StrokeKind::Outside);
                if (resp.clicked() || resp.dragged()) && let Some(p) = resp.interact_pointer_pos() {
                    let frac = ((p.y - r.min.y) / size.y).clamp(0.0, 1.0);
                    self.scroll_to = Some((frac * content_h - view_h / 2.0).max(0.0));
                }
            }
        }
    }

    fn zones_out(&self) -> Vec<Zone> {
        let mut z = self.zones.clone();
        if let Some(c) = self.cut {
            let (iw, ih) = self.analysis.image.dimensions();
            if c < ih as f32 - 1.0 {
                z.push(Zone { rect: Rect::new(0.0, c, iw as f32, ih as f32), kind: ZoneKind::Ignore });
            }
        }
        z
    }

    fn toggle_cut(&mut self) {
        if self.cut.take().is_some() {
            self.placing_cut = false;
            self.preview_wanted = true;
        } else {
            self.placing_cut = !self.placing_cut;
        }
    }

    fn missing_suggestions(&self) -> Vec<Rect> {
        self.analysis
            .suggestions
            .iter()
            .map(|f| f.rect)
            .filter(|r| self.zones.iter().all(|z| r.iou(&z.rect) < 0.2 && r.covered_by(&z.rect) < 0.5))
            .collect()
    }

    fn interact(&mut self, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, page: egui::Rect, zoom: f32) {
        let to_screen = |r: &Rect| {
            egui::Rect::from_min_max(
                page.min + egui::vec2(r.x0 * zoom, r.y0 * zoom),
                page.min + egui::vec2(r.x1 * zoom, r.y1 * zoom),
            )
        };
        let to_page = |p: egui::Pos2| egui::pos2((p.x - page.min.x) / zoom, (p.y - page.min.y) / zoom);
        let (iw, ih) = self.analysis.image.dimensions();
        let clamp = |r: Rect| r.clamp(iw as f32, ih as f32);
        let hover = ui.input(|i| i.pointer.hover_pos());

        let hit = |p: egui::Pos2, zones: &[Zone], selected: Option<usize>| -> Option<(usize, Handle)> {
            let order: Vec<usize> = selected.into_iter().chain((0..zones.len()).rev()).collect();
            for i in order {
                if i >= zones.len() {
                    continue;
                }
                let r = to_screen(&zones[i].rect);
                let m = 6.0;
                if !r.expand(m).contains(p) {
                    continue;
                }
                let (l, rt, t, b) =
                    ((p.x - r.min.x).abs() < m, (p.x - r.max.x).abs() < m, (p.y - r.min.y).abs() < m, (p.y - r.max.y).abs() < m);
                let h = match (l, rt, t, b) {
                    (true, _, true, _) => Handle::NW,
                    (_, true, true, _) => Handle::NE,
                    (true, _, _, true) => Handle::SW,
                    (_, true, _, true) => Handle::SE,
                    (true, ..) => Handle::W,
                    (_, true, ..) => Handle::E,
                    (_, _, true, _) => Handle::N,
                    (_, _, _, true) => Handle::S,
                    _ => Handle::Move,
                };
                return Some((i, h));
            }
            None
        };

        if let Some(p) = hover.filter(|p| page.contains(*p) || self.drag.is_some()) {
            let near_cut = |p: egui::Pos2| self.cut.is_some_and(|c| (p.y - (page.min.y + c * zoom)).abs() < 6.0);
            let icon = match &self.drag {
                Some(Drag::Edit(_, h, ..)) => cursor_for(*h),
                Some(Drag::Cut) => egui::CursorIcon::ResizeVertical,
                Some(Drag::New(..)) => egui::CursorIcon::Crosshair,
                None if self.placing_cut => egui::CursorIcon::ResizeVertical,
                None if near_cut(p) => egui::CursorIcon::ResizeVertical,
                None => hit(p, &self.zones, self.selected).map(|(_, h)| cursor_for(h)).unwrap_or(egui::CursorIcon::Crosshair),
            };
            ui.ctx().set_cursor_icon(icon);
        }

        if self.placing_cut && resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.cut = Some(to_page(p).y.clamp(1.0, ih as f32));
                self.placing_cut = false;
                self.preview_wanted = true;
            }
        }
        let on_cut = |p: egui::Pos2| self.cut.is_some_and(|c| (p.y - (page.min.y + c * zoom)).abs() < 6.0);
        let right = resp.drag_started_by(egui::PointerButton::Secondary);
        if resp.drag_started_by(egui::PointerButton::Primary) || right {
            let origin = ui.input(|i| i.pointer.press_origin());
            if let Some(p) = origin.or(resp.interact_pointer_pos()) {
                if right {
                    self.selected = None;
                    self.drag = Some(Drag::New(to_page(p), ZoneKind::Ignore));
                } else if on_cut(p) {
                    self.drag = Some(Drag::Cut);
                } else {
                    match hit(p, &self.zones, self.selected) {
                        Some((i, h)) => {
                            self.selected = Some(i);
                            self.drag = Some(Drag::Edit(i, h, self.zones[i].rect, to_page(p)));
                        }
                        None => {
                            self.selected = None;
                            self.drag = Some(Drag::New(to_page(p), ZoneKind::Image));
                        }
                    }
                }
            }
        }
        let cur = resp.interact_pointer_pos().map(to_page);
        let mut live: Option<Rect> = None;
        if let (Some(d), Some(c)) = (&self.drag, cur) {
            match d {
                Drag::New(s, _) => live = Some(clamp(Rect::new(s.x.min(c.x), s.y.min(c.y), s.x.max(c.x), s.y.max(c.y)))),
                Drag::Cut => self.cut = Some(c.y.clamp(1.0, ih as f32)),
                Drag::Edit(i, h, orig, s) => {
                    let (dx, dy) = (c.x - s.x, c.y - s.y);
                    let mut r = *orig;
                    match h {
                        Handle::Move => r = r.translate(dx, dy),
                        Handle::N => r.y0 += dy,
                        Handle::S => r.y1 += dy,
                        Handle::E => r.x1 += dx,
                        Handle::W => r.x0 += dx,
                        Handle::NE => (r.y0, r.x1) = (r.y0 + dy, r.x1 + dx),
                        Handle::NW => (r.y0, r.x0) = (r.y0 + dy, r.x0 + dx),
                        Handle::SE => (r.y1, r.x1) = (r.y1 + dy, r.x1 + dx),
                        Handle::SW => (r.y1, r.x0) = (r.y1 + dy, r.x0 + dx),
                    }
                    let r = Rect::new(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
                    if let Some(z) = self.zones.get_mut(*i) {
                        z.rect = clamp(r);
                    }
                }
            }
        }
        if resp.drag_stopped() {
            if let (Some(Drag::New(_, kind)), Some(r)) = (&self.drag, live) {
                if r.w() * zoom >= 6.0 && r.h() * zoom >= 6.0 {
                    self.zones.push(Zone { rect: r, kind: *kind });
                    self.selected = Some(self.zones.len() - 1);
                }
            }
            self.drag = None;
            self.preview_wanted = true;
        }
        if resp.clicked() && !self.placing_cut {
            if let Some(p) = resp.interact_pointer_pos() {
                self.selected = hit(p, &self.zones, None).map(|(i, _)| i);
            }
        }
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.selected = hit(p, &self.zones, None).map(|(i, _)| i);
            }
        }
        let mut changed = false;
        resp.context_menu(|ui| {
            if let Some(i) = self.selected.filter(|i| *i < self.zones.len()) {
                if ui.button("▣  Image").clicked() {
                    self.zones[i].kind = ZoneKind::Image;
                    changed = true;
                    ui.close();
                }
                if ui.button(t!("✖  Ignorer", "✖  Ignore")).clicked() {
                    self.zones[i].kind = ZoneKind::Ignore;
                    changed = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(t!("🗑  Supprimer", "🗑  Delete")).clicked() {
                    self.zones.remove(i);
                    self.selected = None;
                    changed = true;
                    ui.close();
                }
            } else {
                ui.label(t!("Tracez une zone pour la créer.", "Draw an area to create it."));
            }
        });
        if changed {
            self.preview_wanted = true;
        }

        let mut n_img = 0;
        for (i, z) in self.zones.iter().enumerate() {
            let r = to_screen(&z.rect);
            let c = kind_color(z.kind);
            let sel = self.selected == Some(i);
            painter.rect_filled(r, 2, c.gamma_multiply(if z.kind == ZoneKind::Ignore { 0.28 } else { 0.16 }));
            painter.rect_stroke(r, 2, egui::Stroke::new(if sel { 3.0_f32 } else { 2.0 }, c), egui::StrokeKind::Outside);
            let label = match z.kind {
                ZoneKind::Image => {
                    n_img += 1;
                    format!("image {n_img}")
                }
                ZoneKind::Ignore => t!("ignoré", "ignored").into(),
                ZoneKind::Text => t!("texte", "text").into(),
            };
            let g = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), egui::Color32::WHITE);
            let chip = egui::Rect::from_min_size(r.min + egui::vec2(0.0, -g.size().y - 6.0), g.size() + egui::vec2(10.0, 6.0));
            let chip = if chip.min.y < page.min.y { chip.translate(egui::vec2(0.0, g.size().y + 8.0)) } else { chip };
            painter.rect_filled(chip, 4, c);
            painter.galley(chip.min + egui::vec2(5.0, 3.0), g, egui::Color32::WHITE);
            if sel {
                for p in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                    painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(9.0, 9.0)), 2, egui::Color32::WHITE);
                    painter.rect_stroke(egui::Rect::from_center_size(p, egui::vec2(9.0, 9.0)), 2, egui::Stroke::new(1.5_f32, c), egui::StrokeKind::Inside);
                }
            }
        }
        if let (Some(Drag::New(_, kind)), Some(r)) = (&self.drag, live) {
            let c = kind_color(*kind);
            let r = to_screen(&r);
            painter.rect_filled(r, 2, c.gamma_multiply(0.16));
            painter.rect_stroke(r, 2, egui::Stroke::new(2.0_f32, c), egui::StrokeKind::Outside);
        }
        let shown_cut = self.cut.or_else(|| {
            (self.placing_cut).then(|| hover.filter(|p| page.contains(*p)).map(|p| to_page(p).y)).flatten()
        });
        if let Some(c) = shown_cut {
            let y = page.min.y + c * zoom;
            let below = egui::Rect::from_min_max(egui::pos2(page.min.x, y), page.max);
            if self.cut.is_some() {
                painter.rect_filled(below, 0, egui::Color32::from_rgba_unmultiplied(40, 44, 52, 150));
            }
            painter.line_segment([egui::pos2(page.min.x, y), egui::pos2(page.max.x, y)], egui::Stroke::new(2.5_f32, CUT));
            let g = painter.layout_no_wrap(
                t!("✂ fin de la page : la suite n'entre pas dans la note", "✂ end of page: what follows is left out").to_string(),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
            let chip = egui::Rect::from_min_size(egui::pos2(page.min.x + 8.0, y + 4.0), g.size() + egui::vec2(10.0, 6.0));
            painter.rect_filled(chip, 4, CUT);
            painter.galley(chip.min + egui::vec2(5.0, 3.0), g, egui::Color32::WHITE);
        }
    }

    fn keyboard(&mut self, ui: &egui::Ui) {
        let (del, img, ign) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.key_pressed(egui::Key::I),
                i.key_pressed(egui::Key::X),
            )
        });
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::End)) {
            self.toggle_cut();
        }
        if del {
            if let Some(i) = self.selected.take().filter(|i| *i < self.zones.len()) {
                self.zones.remove(i);
                self.preview_wanted = true;
            }
        }
        for (pressed, k) in [(img, ZoneKind::Image), (ign, ZoneKind::Ignore)] {
            if pressed {
                if let Some(i) = self.selected.filter(|i| *i < self.zones.len()) {
                    self.zones[i].kind = k;
                    self.preview_wanted = true;
                }
            }
        }
    }
}

fn kind_color(k: ZoneKind) -> egui::Color32 {
    match k {
        ZoneKind::Image => ZONE_IMAGE,
        ZoneKind::Ignore => ZONE_IGNORE,
        ZoneKind::Text => super::ZONE_TEXT,
    }
}

fn cursor_for(h: Handle) -> egui::CursorIcon {
    match h {
        Handle::Move => egui::CursorIcon::Grab,
        Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
        Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
        Handle::NE | Handle::SW => egui::CursorIcon::ResizeNeSw,
        Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
    }
}
