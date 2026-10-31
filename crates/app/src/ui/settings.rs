use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::HotKey;
use crate::config::Mode;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;

use super::{EguiWindow, hint};
use crate::config::Config;
use crate::platform;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Capture,
    Shortcuts,
    Engine,
    Advanced,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HotkeySlot {
    Capture,
    Record,
}

pub enum Action {
    Apply(Box<Config>),
    SuspendHotkeys(bool),
    Open(PathBuf),
    CheckUpdate,
}

pub struct SettingsWindow {
    pub win: EguiWindow,
    pub st: SettingsState,
}

pub struct SettingsState {
    tab: Tab,
    draft: Config,
    tags_text: String,
    recording: Option<HotkeySlot>,
    pub hotkey_errors: Vec<(HotkeySlot, String)>,
    saved_at: Option<Instant>,
    confirm_reset: bool,
    logo: Option<egui::TextureHandle>,
    vaults: Vec<crate::obsidian::Vault>,
    shown_en: bool,
}

const MODEL_FILES: &[(&str, &str, &str)] = &[
    ("det_tiny.onnx", "Détection du texte (rapide)", "Text detection (fast)"),
    ("det.onnx", "Détection du texte (précise)", "Text detection (accurate)"),
    ("rec_tiny.onnx", "Lecture du texte (rapide)", "Text recognition (fast)"),
    ("rec.onnx", "Lecture du texte (précise)", "Text recognition (accurate)"),
    ("layout.onnx", "Analyse de mise en page", "Layout analysis"),
];

fn title() -> &'static str {
    t!("MarkdownShot - Paramètres", "MarkdownShot - Settings")
}

impl SettingsWindow {
    pub fn new(el: &ActiveEventLoop, cfg: &Config) -> anyhow::Result<SettingsWindow> {
        let attrs = WindowAttributes::default()
            .with_title(title())
            .with_inner_size(LogicalSize::new(820.0, 600.0))
            .with_min_inner_size(LogicalSize::new(640.0, 460.0))
            .with_window_icon(super::window_icon());
        let mut win = EguiWindow::new(el, attrs)?;
        win.window.focus_window();
        win.clear = super::panel_rgb(&win.ctx);
        let st = SettingsState {
            tab: Tab::General,
            tags_text: cfg.output.tags.join(", "),
            vaults: crate::obsidian::vaults(),
            draft: cfg.clone(),
            recording: None,
            hotkey_errors: Vec::new(),
            saved_at: None,
            confirm_reset: false,
            logo: None,
            shown_en: crate::i18n::is_en(),
        };
        Ok(SettingsWindow { win, st })
    }

    pub fn show_tab(&mut self, tab: Tab) {
        self.st.tab = tab;
        self.win.window.focus_window();
        self.win.next_repaint = Some(Instant::now());
    }

    pub fn sync(&mut self, cfg: &Config) {
        self.st.draft = cfg.clone();
        self.st.tags_text = cfg.output.tags.join(", ");
        self.win.next_repaint = Some(Instant::now());
    }

    pub fn is_recording(&self) -> bool {
        self.st.recording.is_some()
    }

    pub fn paint(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let st = &mut self.st;
        self.win.paint(|ui| st.ui(ui, &mut actions));
        if self.st.shown_en != crate::i18n::is_en() {
            self.st.shown_en = crate::i18n::is_en();
            self.win.window.set_title(title());
        }
        if actions.iter().any(|a| matches!(a, Action::Apply(_))) {
            self.win.next_repaint = Some(Instant::now() + Duration::from_millis(2100));
        }
        actions
    }
}

impl SettingsState {
    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let before = self.draft.clone();
        self.capture_hotkey(ui, actions);

        egui::Panel::left("nav").exact_size(190.0).resizable(false).show_inside(ui, |ui| {
            ui.add_space(14.0);
            let logo = self.logo(ui.ctx());
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::vec2(26.0, 26.0)));
                ui.label(egui::RichText::new("MarkdownShot").size(18.0).strong());
            });
            ui.add_space(14.0);
            for (tab, label) in [
                (Tab::General, t!("📁  Général", "📁  General")),
                (Tab::Capture, "✂  Capture"),
                (Tab::Shortcuts, t!("⌨  Raccourcis", "⌨  Shortcuts")),
                (Tab::Engine, t!("⚙  Moteur", "⚙  Engine")),
                (Tab::Advanced, t!("🛠  Avancé", "🛠  Advanced")),
                (Tab::About, t!("ℹ  À propos", "ℹ  About")),
            ] {
                let selected = self.tab == tab;
                let text = egui::RichText::new(label).size(15.0);
                let b = egui::Button::selectable(selected, text).min_size(egui::vec2(170.0, 34.0));
                if ui.add(b).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(10.0);
                if self.saved_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                    ui.label(egui::RichText::new(t!("✔ Enregistré", "✔ Saved")).color(super::ZONE_TEXT));
                } else {
                    hint(ui, t!("Les changements sont appliqués immédiatement.", "Changes apply immediately."));
                }
            });
        });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                match self.tab {
                    Tab::General => self.tab_general(ui, actions),
                    Tab::Capture => self.tab_capture(ui),
                    Tab::Shortcuts => self.tab_shortcuts(ui, actions),
                    Tab::Engine => self.tab_engine(ui),
                    Tab::Advanced => self.tab_advanced(ui, actions),
                    Tab::About => self.tab_about(ui),
                }
            });
        });

        if !same(&before, &self.draft) {
            self.saved_at = Some(Instant::now());
            actions.push(Action::Apply(Box::new(self.draft.clone())));
        }
    }

    fn tab_general(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        section(ui, t!("Langue", "Language"));
        let lang = self.draft.interface.language.clone();
        let auto_label = if crate::i18n::system() == "en" {
            t!("Automatique (anglais, comme Windows)", "Automatic (English, like Windows)")
        } else {
            t!("Automatique (français, comme Windows)", "Automatic (French, like Windows)")
        };
        let label = match lang.as_str() {
            "fr" => "Français",
            "en" => "English",
            _ => auto_label,
        };
        egui::ComboBox::from_id_salt("language").selected_text(label).width(300.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut self.draft.interface.language, "auto".into(), auto_label);
            ui.selectable_value(&mut self.draft.interface.language, "fr".into(), "Français");
            ui.selectable_value(&mut self.draft.interface.language, "en".into(), "English");
        });

        section(ui, t!("Coffre Obsidian", "Obsidian vault"));
        let current = crate::obsidian::vault_of(&self.draft.output.notes_dir);
        if self.vaults.is_empty() {
            hint(ui, t!("Aucun coffre Obsidian détecté (Obsidian n'est pas installé, ou aucun coffre n'a encore été ouvert).", "No Obsidian vault found (Obsidian is not installed, or no vault has been opened yet)."));
        } else {
            let label = match &current {
                Some(v) => self
                    .vaults
                    .iter()
                    .find(|x| &x.path == v)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| v.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                None => t!("Aucun (dossier libre)", "None (any folder)").into(),
            };
            egui::ComboBox::from_id_salt("vault").selected_text(label).width(380.0).show_ui(ui, |ui| {
                for v in self.vaults.clone() {
                    let sel = current.as_ref() == Some(&v.path);
                    if ui.selectable_label(sel, &v.name).on_hover_text(v.path.display().to_string()).clicked() && !sel {
                        let sub = match &current {
                            Some(c) => self.draft.output.notes_dir.strip_prefix(c).map(|p| p.to_path_buf()).unwrap_or_default(),
                            None => self.draft.output.notes_dir.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("MarkdownShot")),
                        };
                        self.draft.output.notes_dir = v.path.join(sub);
                    }
                }
            });
            hint(ui, t!("Les notes vont dans ce coffre (dossier modifiable ci-dessous, et pour chaque note à l'enregistrement).", "Notes go to this vault (folder can be changed below, and for each note when saving)."));
        }
        ui.checkbox(
            &mut self.draft.output.follow_vault,
            t!(
                "Suivre les réglages du coffre : dossier des pièces jointes et format des liens",
                "Follow the vault settings: attachments folder and link format"
            ),
        );
        let rules = if self.draft.output.follow_vault { crate::obsidian::rules_for(&self.draft.output.notes_dir) } else { None };
        if let Some(r) = &rules {
            hint(
                ui,
                &tf!(
                    "Pièces jointes : {}  ·  liens {}",
                    "Attachments: {}  ·  {} links",
                    r.attachments.display(),
                    if r.wikilinks { "wiki  ![[image.png]]" } else { "Markdown  ![](image.png)" }
                ),
            );
        }

        section(ui, t!("Dossier des notes", "Notes folder"));
        ui.horizontal(|ui| {
            let mut s = self.draft.output.notes_dir.to_string_lossy().to_string();
            if ui.add(egui::TextEdit::singleline(&mut s).desired_width(380.0)).changed() {
                self.draft.output.notes_dir = PathBuf::from(s);
            }
            if ui.button(t!("Parcourir…", "Browse…")).clicked() {
                if let Some(d) = rfd::FileDialog::new()
                    .set_title(t!("Dossier des notes (par exemple dans votre coffre Obsidian)", "Notes folder (for example in your Obsidian vault)"))
                    .set_directory(&self.draft.output.notes_dir)
                    .pick_folder()
                {
                    self.draft.output.notes_dir = d;
                }
            }
            if ui.button(t!("Ouvrir", "Open")).clicked() {
                let _ = std::fs::create_dir_all(&self.draft.output.notes_dir);
                actions.push(Action::Open(self.draft.output.notes_dir.clone()));
            }
        });
        hint(ui, t!("Choisissez un dossier de votre coffre Obsidian pour retrouver vos captures dans Obsidian.", "Pick a folder in your Obsidian vault to find your captures in Obsidian."));

        section(ui, t!("Dossier des images", "Images folder"));
        let follow = rules.is_some();
        ui.add_enabled_ui(!follow, |ui| ui.horizontal(|ui| {
            let mut s = self.draft.output.images_dir.to_string_lossy().to_string();
            if ui.add(egui::TextEdit::singleline(&mut s).desired_width(380.0)).changed() {
                self.draft.output.images_dir = PathBuf::from(s);
            }
            if ui.button(t!("Parcourir…", "Browse…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&self.draft.output.notes_dir).pick_folder() {
                    self.draft.output.images_dir = d
                        .strip_prefix(&self.draft.output.notes_dir)
                        .map(|p| p.to_path_buf())
                        .unwrap_or(d);
                }
            }
        }));
        if follow {
            hint(ui, t!("Réglé par le coffre Obsidian (voir plus haut).", "Set by the Obsidian vault (see above)."));
        }
        let images = if self.draft.output.images_dir.is_absolute() {
            self.draft.output.images_dir.clone()
        } else {
            self.draft.output.notes_dir.join(&self.draft.output.images_dir)
        };
        if !follow {
            hint(ui, &tf!("Relatif au dossier des notes, ou chemin absolu. Actuellement : {}", "Relative to the notes folder, or an absolute path. Currently: {}", images.display()));
        }

        section(ui, t!("Format des images", "Image format"));
        let png = t!("PNG (sans perte)", "PNG (lossless)");
        let webp = t!("WebP sans perte (2,5 × plus léger, conseillé)", "Lossless WebP (2.5× smaller, recommended)");
        let jpeg = t!("JPEG (le plus léger, flou sur le texte)", "JPEG (smallest, blurs text)");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("image_format")
                .selected_text(match self.draft.output.image_format.as_str() {
                    "webp" => webp,
                    "jpeg" => jpeg,
                    _ => png,
                })
                .width(260.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.draft.output.image_format, "png".into(), png);
                    ui.selectable_value(&mut self.draft.output.image_format, "webp".into(), webp);
                    ui.selectable_value(&mut self.draft.output.image_format, "jpeg".into(), jpeg);
                });
            if self.draft.output.image_format == "jpeg" {
                ui.label(t!("qualité", "quality"));
                ui.add(egui::Slider::new(&mut self.draft.output.jpeg_quality, 40..=100));
            }
        });
        ui.horizontal(|ui| {
            let mut limit = self.draft.output.max_image_width > 0;
            if ui.checkbox(&mut limit, t!("Réduire les images plus larges que", "Shrink images wider than")).changed() {
                self.draft.output.max_image_width = if limit { 1600 } else { 0 };
            }
            if limit {
                ui.add(egui::DragValue::new(&mut self.draft.output.max_image_width).range(200..=8000).speed(10).suffix(" px"));
            } else {
                let mut placeholder = 1600u32;
                ui.add_enabled(false, egui::DragValue::new(&mut placeholder).suffix(" px"));
            }
        });
        ui.checkbox(
            &mut self.draft.output.dedupe_images,
            t!(
                "Une seule copie des images identiques (dans la note et dans le dossier des images)",
                "Keep one copy of identical images (within the note and in the images folder)"
            ),
        );
        hint(ui, t!("Sur un écran à 125 % ou 150 %, la réduction garde des notes légères.", "On a 125% or 150% display, shrinking keeps notes light."));

        section(ui, t!("Nom des notes", "Note names"));
        ui.add(egui::TextEdit::singleline(&mut self.draft.output.filename).desired_width(380.0));
        let (d, t, _) = crate::config::now_strings();
        let example = self
            .draft
            .output
            .filename
            .replace("{date}", &d)
            .replace("{time}", &t)
            .replace("{title}", t!("Titre de la page", "Page title"));
        hint(ui, &tf!("{{date}}, {{time}} et {{title}} sont remplacés. Exemple : {example}.md", "{{date}}, {{time}} and {{title}} are replaced. Example: {example}.md"));

        section(ui, t!("Contenu de la note", "Note content"));
        ui.add_enabled(
            !follow,
            egui::Checkbox::new(&mut self.draft.output.wikilinks, t!("Liens d'images au format Obsidian  ![[image.png]]", "Obsidian-style image links  ![[image.png]]")),
        );
        ui.checkbox(&mut self.draft.output.front_matter, t!("En-tête YAML (date, source, tags)", "YAML front matter (date, source, tags)"));
        ui.horizontal(|ui| {
            ui.label("Tags");
            if ui.add(egui::TextEdit::singleline(&mut self.tags_text).desired_width(300.0)).changed() {
                self.draft.output.tags =
                    self.tags_text.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            }
        });
        ui.checkbox(&mut self.draft.output.copy_markdown, t!("Copier aussi le Markdown dans le presse-papier", "Also copy the Markdown to the clipboard"));

        section(ui, t!("Après la capture", "After a capture"));
        let nothing = t!("Ne rien ouvrir (notification seulement)", "Open nothing (notification only)");
        let note = t!("Ouvrir la note", "Open the note");
        let obsidian = t!("Ouvrir la note dans Obsidian", "Open the note in Obsidian");
        egui::ComboBox::from_id_salt("open_after")
            .selected_text(match self.draft.output.open_after.as_str() {
                "note" => note,
                "obsidian" => obsidian,
                _ => nothing,
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.draft.output.open_after, "none".into(), nothing);
                ui.selectable_value(&mut self.draft.output.open_after, "note".into(), note);
                ui.selectable_value(&mut self.draft.output.open_after, "obsidian".into(), obsidian);
            });

        if cfg!(windows) {
            section(ui, t!("Démarrage", "Startup"));
            let mut auto = platform::autostart_enabled();
            if ui.checkbox(&mut auto, t!("Lancer MarkdownShot au démarrage de Windows", "Start MarkdownShot with Windows")).changed() {
                platform::set_autostart(auto);
            }
        }

        section(ui, t!("Mises à jour", "Updates"));
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.draft.updates.check,
                t!("Chercher une nouvelle version au démarrage (une fois par jour)", "Check for a new version at startup (once a day)"),
            );
            if ui.button(t!("Rechercher maintenant", "Check now")).clicked() {
                actions.push(Action::CheckUpdate);
            }
        });
        hint(
            ui,
            t!(
                "Seule la page des versions de GitHub est consultée ; l'installation se fait d'un clic sur la notification.",
                "Only the GitHub releases page is queried; installing takes one click on the notification."
            ),
        );
    }

    fn tab_capture(&mut self, ui: &mut egui::Ui) {
        section(ui, t!("Mode par défaut", "Default mode"));
        hint(ui, t!("Modifiable à chaque capture dans la barre d'outils (touche M, ou S pour Snipe).", "Can be changed on each capture in the toolbar (M key, or S for Snipe)."));
        ui.add_space(4.0);
        let mode = self.draft.mode();
        ui.horizontal_top(|ui| {
            let w = ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0 - 44.0).clamp(150.0, 240.0);
            if choice_card(
                ui,
                w,
                mode == Mode::Auto,
                t!("⚡  Automatique", "⚡  Automatic"),
                t!(
                    "MarkdownShot détecte et découpe les images tout seul. La note est écrite aussitôt.",
                    "MarkdownShot finds and crops the images on its own. The note is written right away."
                ),
            ) {
                self.draft.set_mode(Mode::Auto);
            }
            if choice_card(
                ui,
                w,
                mode == Mode::Snipe,
                "✂  Snipe",
                t!(
                    "Le Markdown s'affiche à côté de la capture, prêt à copier-coller. La note n'est écrite que si vous l'enregistrez.",
                    "The Markdown is shown next to the capture, ready to copy and paste. The note is written only if you save it."
                ),
            ) {
                self.draft.set_mode(Mode::Snipe);
            }
            if choice_card(
                ui,
                w,
                mode == Mode::Manual,
                t!("✋  Manuel", "✋  Manual"),
                t!(
                    "Vous marquez vous-même les zones d’images. Après un enregistrement, la page s'ouvre pour les marquer.",
                    "You mark the image areas yourself. After a recording, the page opens so you can mark them."
                ),
            ) {
                self.draft.set_mode(Mode::Manual);
            }
        });
        ui.add_space(6.0);
        ui.add_enabled(
            mode == Mode::Manual,
            egui::Checkbox::new(
                &mut self.draft.capture.manual_suggestions,
                t!(
                    "Mode manuel : proposer les images détectées (à accepter d'un clic)",
                    "Manual mode: suggest the detected images (accept with one click)"
                ),
            ),
        );

        section(ui, t!("Enregistrement d'un défilement", "Scroll recording"));
        ui.checkbox(
            &mut self.draft.capture.auto_scroll,
            t!(
                "Défilement automatique : MarkdownShot fait défiler la page jusqu'en bas (touche D pour changer)",
                "Auto-scroll: MarkdownShot scrolls the page down to the bottom (D key to switch)"
            ),
        );
        hint(
            ui,
            t!(
                "Ne touchez pas la souris pendant le défilement ; Échap l'arrête. Si la page ne défile pas toute seule, faites-la défiler à la main.",
                "Don't touch the mouse while it scrolls; Esc stops it. If the page doesn't scroll by itself, scroll it by hand."
            ),
        );
        ui.horizontal(|ui| {
            ui.label(t!("Images par seconde (défilement à la main)", "Frames per second (scrolling by hand)"));
            ui.add(egui::Slider::new(&mut self.draft.capture.frames_per_second, 2..=15));
        });
        hint(ui, t!("Plus élevé : tolère un défilement plus rapide, mais consomme davantage.", "Higher: handles faster scrolling, but uses more resources."));
        ui.checkbox(&mut self.draft.capture.keep_long_capture, t!("Enregistrer aussi la page assemblée complète (PNG)", "Also save the full stitched page (PNG)"));
    }

    fn tab_shortcuts(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        section(ui, t!("Raccourcis clavier", "Keyboard shortcuts"));
        hint(ui, t!("Raccourcis globaux : ils fonctionnent depuis n'importe quelle application.", "Global shortcuts: they work from any application."));
        ui.add_space(6.0);
        egui::Grid::new("hotkeys").num_columns(3).spacing([18.0, 12.0]).show(ui, |ui| {
            for (slot, label) in [
                (HotkeySlot::Capture, t!("Ouvrir la barre de capture", "Open the capture toolbar")),
                (HotkeySlot::Record, t!("Enregistrer un défilement", "Record a scroll")),
            ] {
                ui.label(label);
                let current = match slot {
                    HotkeySlot::Capture => self.draft.hotkeys.capture.clone(),
                    HotkeySlot::Record => self.draft.hotkeys.record.clone(),
                };
                let recording = self.recording == Some(slot);
                let text = if recording { t!("Appuyez sur la combinaison…", "Press the key combination…").to_string() } else { pretty_hotkey(&current) };
                let chip = egui::Button::new(egui::RichText::new(text).monospace().size(14.0))
                    .min_size(egui::vec2(230.0, 30.0))
                    .selected(recording);
                if ui.add(chip).clicked() {
                    self.start_recording(slot, actions);
                }
                if recording {
                    if ui.button(t!("Annuler", "Cancel")).clicked() {
                        self.stop_recording(actions);
                    }
                } else if ui.button(t!("Modifier", "Change")).clicked() {
                    self.start_recording(slot, actions);
                }
                ui.end_row();
            }
        });
        for (slot, err) in &self.hotkey_errors {
            let which = if *slot == HotkeySlot::Capture { t!("Barre de capture", "Capture toolbar") } else { t!("Enregistrement", "Recording") };
            ui.colored_label(super::ZONE_IGNORE, format!("⚠ {which} : {err}"));
        }

        section(ui, t!("Touche Impr. écran", "Print Screen key"));
        ui.label(t!(
            "Comme l'Outil Capture de Windows, MarkdownShot peut s'ouvrir avec la touche Impr. écran.",
            "Like the Windows Snipping Tool, MarkdownShot can open with the Print Screen key."
        ));
        ui.horizontal(|ui| {
            if ui.button(t!("Utiliser Impr. écran pour la barre de capture", "Use Print Screen for the capture toolbar")).clicked() {
                self.draft.hotkeys.capture = "PrintScreen".into();
            }
        });
        hint(
            ui,
            t!(
                "Windows 11 réserve cette touche à l'Outil Capture : désactivez « Utiliser le bouton Impr. écran \
                 pour ouvrir la capture d'écran » dans Paramètres » Accessibilité » Clavier.",
                "Windows 11 reserves this key for the Snipping Tool: turn off \"Use the Print screen key to open \
                 screen capture\" in Settings > Accessibility > Keyboard."
            ),
        );
        if cfg!(windows) && ui.link(t!("Ouvrir ce réglage de Windows", "Open this Windows setting")).clicked() {
            actions.push(Action::Open(PathBuf::from("ms-settings:easeofaccess-keyboard")));
        }

        section(ui, t!("Dans la barre de capture", "In the capture toolbar"));
        egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
            for (k, v) in [
                (t!("Z / F / P", "Z / W / F"), t!("Zone, Fenêtre, Plein écran", "Area, Window, Full screen")),
                ("R", t!("Enregistrer un défilement", "Record a scroll")),
                ("D", t!("Enregistrement : défilement automatique ou à la main", "Recording: auto-scroll or by hand")),
                ("M", t!("Mode suivant : Automatique, Snipe, Manuel", "Next mode: Automatic, Snipe, Manual")),
                ("S", t!("Snipe (aperçu à copier) ou retour en Automatique", "Snipe (preview to copy) or back to Automatic")),
                (t!("Entrée", "Enter"), t!("Valider (mode manuel)", "Confirm (manual mode)")),
                (t!("Échap", "Esc"), t!("Annuler", "Cancel")),
            ] {
                ui.label(egui::RichText::new(k).monospace().strong());
                ui.label(v);
                ui.end_row();
            }
        });
    }

    fn tab_engine(&mut self, ui: &mut egui::Ui) {
        section(ui, t!("Qualité de lecture", "Reading quality"));
        let q = self.draft.engine.quality.clone();
        for (val, title, desc) in [
            (
                "fast",
                t!("Rapide", "Fast"),
                t!("Environ 4 fois plus rapide ; quelques erreurs de lecture possibles.", "About 4 times faster; a few reading errors possible."),
            ),
            (
                "balanced",
                t!("Équilibrée (recommandé)", "Balanced (recommended)"),
                t!("Lecture précise, détection rapide : le meilleur compromis.", "Accurate reading, fast detection: the best trade-off."),
            ),
            ("accurate", t!("Précise", "Accurate"), t!("Pour le petit texte ; plus lent.", "For small text; slower.")),
        ] {
            if ui.radio(q == val, egui::RichText::new(title).strong()).clicked() {
                self.draft.engine.quality = val.into();
            }
            ui.indent(val, |ui| hint(ui, desc));
        }

        section(ui, t!("Carte graphique", "Graphics card"));
        ui.add_enabled(
            cfg!(windows),
            egui::Checkbox::new(
                &mut self.draft.engine.gpu,
                t!("Lire sur la carte graphique (DirectML, expérimental)", "Read on the graphics card (DirectML, experimental)"),
            ),
        );
        let state = if markdownshot_core::runtime::gpu_active() {
            t!("En service : les modèles tournent sur la carte graphique.", "Active: the models run on the graphics card.")
        } else if self.draft.engine.gpu {
            t!(
                "Pas encore en service (appliqué en enregistrant), ou carte non compatible DirectX 12 : le processeur prend le relais.",
                "Not active yet (applied on save), or no DirectX 12 card: the processor takes over."
            )
        } else {
            t!("Plus rapide sur la plupart des cartes récentes ; le processeur reste utilisé sinon.", "Faster on most recent cards; the processor is used otherwise.")
        };
        hint(ui, state);

        section(ui, "Images");
        ui.checkbox(
            &mut self.draft.engine.full_width_images,
            t!(
                "Intégrer à l'image le texte posé à sa gauche ou à sa droite (légende, annotation)",
                "Include in the image the text placed to its left or right (caption, annotation)"
            ),
        );
        ui.checkbox(
            &mut self.draft.engine.layout,
            t!(
                "Analyse de mise en page (meilleure détection des graphiques et schémas)",
                "Layout analysis (better detection of charts and diagrams)"
            ),
        );

        section(ui, t!("Modèles", "Models"));
        match markdownshot_core::runtime::models_dir() {
            Ok(dir) => {
                hint(ui, &tf!("Dossier : {}", "Folder: {}", dir.display()));
                egui::Grid::new("models").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (f, fr, en) in MODEL_FILES {
                        let p = dir.join(f);
                        let size = std::fs::metadata(&p).map(|m| m.len()).ok();
                        ui.label(if size.is_some() { "✔" } else { "✖" });
                        ui.label(t!(*fr, *en));
                        ui.label(egui::RichText::new(match size {
                            Some(s) => tf!("{:.1} Mo", "{:.1} MB", s as f64 / 1e6),
                            None => t!("absent", "missing").into(),
                        }).weak());
                        ui.end_row();
                    }
                });
            }
            Err(e) => {
                ui.colored_label(super::ZONE_IGNORE, format!("⚠ {e}"));
            }
        }
        hint(ui, t!("Tout est traité sur votre ordinateur : aucune image ni aucun texte n'est envoyé en ligne.", "Everything is processed on your computer: no image or text is sent online."));
    }

    fn tab_advanced(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let cfg_path = Config::path();
        section(ui, t!("Fichier de configuration", "Configuration file"));
        hint(ui, &cfg_path.display().to_string());
        ui.horizontal(|ui| {
            if ui.button(t!("Ouvrir le fichier", "Open the file")).clicked() {
                actions.push(Action::Open(cfg_path.clone()));
            }
            if let Some(d) = cfg_path.parent() {
                if ui.button(t!("Ouvrir le dossier", "Open the folder")).clicked() {
                    actions.push(Action::Open(d.to_path_buf()));
                }
            }
        });
        hint(ui, t!("Placez un fichier markdownshot.toml à côté de l'exécutable pour une utilisation portable.", "Put a markdownshot.toml file next to the executable for portable use."));

        section(ui, t!("Journal", "Log"));
        if let Some(d) = cfg_path.parent() {
            let log = d.join("markdownshot.log");
            hint(ui, &log.display().to_string());
            if ui.button(t!("Ouvrir le journal", "Open the log")).clicked() {
                actions.push(Action::Open(log));
            }
        }

        section(ui, t!("Réinitialiser", "Reset"));
        if self.confirm_reset {
            ui.label(t!("Revenir aux réglages par défaut ? Le dossier des notes est conservé.", "Go back to the default settings? The notes folder is kept."));
            ui.horizontal(|ui| {
                if ui.button(t!("Oui, réinitialiser", "Yes, reset")).clicked() {
                    let keep = self.draft.output.notes_dir.clone();
                    let lang = self.draft.interface.language.clone();
                    self.draft = Config::default();
                    self.draft.output.notes_dir = keep;
                    self.draft.interface.language = lang;
                    self.tags_text = self.draft.output.tags.join(", ");
                    self.confirm_reset = false;
                }
                if ui.button(t!("Annuler", "Cancel")).clicked() {
                    self.confirm_reset = false;
                }
            });
        } else if ui.button(t!("Réinitialiser les paramètres…", "Reset settings…")).clicked() {
            self.confirm_reset = true;
        }
    }

    fn logo(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        self.logo
            .get_or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([128, 128], &crate::icon::rgba(128));
                ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
            })
            .clone()
    }

    fn tab_about(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let logo = self.logo(ui.ctx());
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::vec2(88.0, 88.0)));
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("MarkdownShot").size(28.0).strong());
                ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).weak());
            });
        });
        ui.add_space(8.0);
        ui.label(t!("De la capture d’écran à la note Markdown (texte, mise en forme et images), 100 % local.", "From screenshot to Markdown note (text, formatting and images), 100% local."));
        ui.add_space(14.0);
        section(ui, t!("Confidentialité", "Privacy"));
        ui.label(t!(
            "Tout le traitement se fait sur cet ordinateur. Aucune donnée n'est envoyée, aucune télémétrie. \
             Seule la recherche de mises à jour (désactivable) consulte GitHub.",
            "All processing happens on this computer. No data is sent, no telemetry. \
             Only the update check (can be turned off) contacts GitHub."
        ));
        section(ui, t!("Licences", "Licenses"));
        ui.label(t!("MarkdownShot : licence MIT.", "MarkdownShot: MIT license."));
        hint(
            ui,
            t!(
                "Modèles PP-OCRv6 et PP-Layout (PaddleOCR, Apache-2.0), ONNX Runtime (MIT), egui (MIT/Apache-2.0), \
                 police Liberation Sans (SIL OFL 1.1). Détails dans le dossier « licenses » de l'application.",
                "PP-OCRv6 and PP-Layout models (PaddleOCR, Apache-2.0), ONNX Runtime (MIT), egui (MIT/Apache-2.0), \
                 Liberation Sans font (SIL OFL 1.1). Details in the application's \"licenses\" folder."
            ),
        );
        ui.add_space(8.0);
    }

    fn start_recording(&mut self, slot: HotkeySlot, actions: &mut Vec<Action>) {
        if self.recording.is_none() {
            actions.push(Action::SuspendHotkeys(true));
        }
        self.recording = Some(slot);
    }

    fn stop_recording(&mut self, actions: &mut Vec<Action>) {
        if self.recording.take().is_some() {
            actions.push(Action::SuspendHotkeys(false));
        }
    }

    fn capture_hotkey(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(slot) = self.recording else { return };
        let (events, mods) = ui.input(|i| (i.events.clone(), i.modifiers));
        for ev in events {
            let (key, modifiers) = match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } => (key, modifiers),
                egui::Event::Cut => (egui::Key::X, mods),
                egui::Event::Copy => (egui::Key::C, mods),
                egui::Event::Paste(_) => (egui::Key::V, mods),
                _ => continue,
            };
            if key == egui::Key::Escape && !modifiers.any() {
                self.stop_recording(actions);
                return;
            }
            let mut parts: Vec<&str> = Vec::new();
            if modifiers.ctrl || (modifiers.command && !cfg!(target_os = "macos")) {
                parts.push("Ctrl");
            }
            if modifiers.alt {
                parts.push("Alt");
            }
            if modifiers.shift {
                parts.push("Shift");
            }
            let name = key.name();
            let is_fn = name.starts_with('F') && name.len() > 1 && name[1..].chars().all(|c| c.is_ascii_digit());
            if parts.is_empty() && !is_fn {
                self.hotkey_errors.retain(|(s, _)| *s != slot);
                self.hotkey_errors.push((slot, t!("ajoutez Ctrl, Alt ou Maj (les touches F1 à F24 sont acceptées seules)", "add Ctrl, Alt or Shift (F1 to F24 are accepted alone)").into()));
                continue;
            }
            parts.push(name);
            let combo = parts.join("+");
            match HotKey::from_str(&combo) {
                Ok(_) => {
                    match slot {
                        HotkeySlot::Capture => self.draft.hotkeys.capture = combo,
                        HotkeySlot::Record => self.draft.hotkeys.record = combo,
                    }
                    self.hotkey_errors.retain(|(s, _)| *s != slot);
                    self.stop_recording(actions);
                    return;
                }
                Err(_) => {
                    self.hotkey_errors.retain(|(s, _)| *s != slot);
                    self.hotkey_errors.push((slot, tf!("« {combo} » n'est pas utilisable", "\"{combo}\" cannot be used")));
                }
            }
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(14.0);
    ui.label(egui::RichText::new(title).size(15.5).strong());
    ui.add_space(2.0);
}

fn choice_card(ui: &mut egui::Ui, width: f32, selected: bool, title: &str, desc: &str) -> bool {
    let stroke = if selected {
        egui::Stroke::new(2.0_f32, super::ACCENT)
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    let fill = if selected { super::ACCENT.gamma_multiply(0.12) } else { ui.visuals().faint_bg_color };
    let r = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.set_min_height(76.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).size(16.0).strong());
                ui.add_space(2.0);
                ui.label(egui::RichText::new(desc).small());
            });
        });
    r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

pub fn pretty_hotkey(s: &str) -> String {
    s.split('+')
        .map(|p| match p.trim().to_uppercase().as_str() {
            "PRINTSCREEN" => t!("Impr. écran", "Print Screen").to_string(),
            "SHIFT" => t!("Maj", "Shift").to_string(),
            "CONTROL" | "CTRL" => "Ctrl".to_string(),
            "ALT" => "Alt".to_string(),
            "SPACE" => t!("Espace", "Space").to_string(),
            _ => p.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn same(a: &Config, b: &Config) -> bool {
    toml::to_string(a).ok() == toml::to_string(b).ok()
}

