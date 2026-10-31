use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use image::RgbImage;
use markdownshot_core::{Analysis, Rect, Zone};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::WindowId;

use crate::config::{Config, Mode};
use crate::overlay::{Outcome, Overlay};
use crate::recorder::Recording;
use crate::text::Font;
use crate::toast::{Click, Kind, Toast};
use crate::ui::review::{self, ReviewWindow};
use crate::ui::settings::{self, HotkeySlot, SettingsWindow};
use crate::ui::toolbar::{self, CaptureKind, ToolbarWindow};
use crate::worker::{Job, JobResult, LastCapture};
use crate::{icon, platform, update, worker};

fn image_link(wikilinks: bool, i: usize) -> String {
    if wikilinks { format!("![[image-{}.png]]", i + 1) } else { format!("![{n}](<image-{n}.png>)", n = i + 1) }
}

pub enum UserEvent {
    Hotkey(GlobalHotKeyEvent),
    Menu(MenuEvent),
    Tray(TrayIconEvent),
    Worker(JobResult),
    Preview(u64, String),
    UpdateChecked { result: Result<update::Release, String>, manual: bool },
    UpdateDownloaded(Result<std::path::PathBuf, String>),
}

struct Items {
    capture: MenuItem,
    record: MenuItem,
    mode_auto: CheckMenuItem,
    mode_snipe: CheckMenuItem,
    mode_manual: CheckMenuItem,
    review_last: MenuItem,
    open_image: MenuItem,
    open_folder: MenuItem,
    check_update: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

struct ReviewCtx {
    analysis: Arc<Analysis>,
    source: Option<String>,
    long: bool,
    replace: Option<Arc<LastCapture>>,
    snipe: bool,
}

pub struct App {
    cfg: Config,
    first_run: bool,
    proxy: EventLoopProxy<UserEvent>,
    worker: Sender<Job>,
    font: Font,
    tray: Option<TrayIcon>,
    items: Option<Items>,
    hotkeys: Option<GlobalHotKeyManager>,
    hk_snip: Option<HotKey>,
    hk_rec: Option<HotKey>,
    hk_esc: Option<HotKey>,
    hotkeys_suspended: bool,
    overlay: Option<Overlay>,
    toolbar: Option<ToolbarWindow>,
    overlay_source: Option<String>,
    recording: Option<Recording>,
    rec_mode: Mode,
    next_refresh: Instant,
    toast: Option<Toast>,
    settings: Option<SettingsWindow>,
    review: Option<(ReviewWindow, ReviewCtx)>,
    last: Option<Arc<LastCapture>>,
    pending: Option<(Instant, CaptureKind)>,
    area: (i32, i32, u32, u32),
    scale: f64,
    pub open_on_start: Option<(std::path::PathBuf, bool)>,
    clipboard: Option<arboard::Clipboard>,
    update: Option<update::Release>,
    toast_update: bool,
    update_busy: bool,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>, cfg: Config, first_run: bool) -> App {
        let worker = worker::spawn(proxy.clone(), cfg.clone());
        App {
            rec_mode: cfg.mode(),
            cfg,
            first_run,
            proxy,
            worker,
            font: Font::new(),
            tray: None,
            items: None,
            hotkeys: None,
            hk_snip: None,
            hk_rec: None,
            hk_esc: None,
            hotkeys_suspended: false,
            overlay: None,
            toolbar: None,
            overlay_source: None,
            recording: None,
            next_refresh: Instant::now(),
            toast: None,
            settings: None,
            review: None,
            last: None,
            pending: None,
            area: (0, 0, 1920, 1080),
            scale: 1.0,
            open_on_start: None,
            clipboard: None,
            update: None,
            toast_update: false,
            update_busy: false,
        }
    }

    fn init(&mut self, el: &ActiveEventLoop) {
        if let Some(m) = el.primary_monitor() {
            let p = m.position();
            let s = m.size();
            self.area = (p.x, p.y, s.width, s.height);
            self.scale = m.scale_factor();
        }
        match GlobalHotKeyManager::new() {
            Ok(m) => {
                self.hotkeys = Some(m);
                self.register_hotkeys(el);
            }
            Err(e) => self.notify(el, Kind::Error, t!("Raccourcis indisponibles", "Shortcuts unavailable"), &e.to_string(), Some(10), None),
        }
        let p = self.proxy.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Hotkey(e));
        }));
        let p = self.proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Menu(e));
        }));
        let p = self.proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Tray(e));
        }));
        self.build_tray();
        if self.cfg.update_check_due(&today()) && !self.first_run {
            self.check_update(false);
        }
        if let Some((p, snipe)) = self.open_on_start.take() {
            let snipe = snipe || self.cfg.mode() == Mode::Snipe;
            self.open_image(el, &p, snipe);
        } else if self.first_run {
            self.open_settings(el, settings::Tab::General);
        }
    }

    fn register_hotkeys(&mut self, el: &ActiveEventLoop) {
        self.unregister_hotkeys();
        let Some(m) = &self.hotkeys else { return };
        let mut errors: Vec<(HotkeySlot, String)> = Vec::new();
        for (txt, slot) in [
            (self.cfg.hotkeys.capture.clone(), HotkeySlot::Capture),
            (self.cfg.hotkeys.record.clone(), HotkeySlot::Record),
        ] {
            match HotKey::from_str(&txt) {
                Ok(hk) => match m.register(hk) {
                    Ok(()) => match slot {
                        HotkeySlot::Capture => self.hk_snip = Some(hk),
                        HotkeySlot::Record => self.hk_rec = Some(hk),
                    },
                    Err(_) => errors.push((slot, tf!("{} est déjà utilisé par une autre application", "{} is already used by another application", settings::pretty_hotkey(&txt)))),
                },
                Err(_) => errors.push((slot, tf!("« {txt} » n'est pas un raccourci valide", "\"{txt}\" is not a valid shortcut"))),
            }
        }
        if let Some(s) = self.settings.as_mut() {
            s.st.hotkey_errors = errors;
            s.win.next_repaint = Some(Instant::now());
        } else if !errors.is_empty() {
            let body = errors.iter().map(|e| e.1.clone()).collect::<Vec<_>>().join(" · ");
            self.notify(el, Kind::Error, t!("Raccourci clavier indisponible", "Keyboard shortcut unavailable"), &body, Some(12), None);
        }
    }

    fn unregister_hotkeys(&mut self) {
        let Some(m) = &self.hotkeys else { return };
        for hk in [self.hk_snip.take(), self.hk_rec.take()].into_iter().flatten() {
            let _ = m.unregister(hk);
        }
    }

    fn build_tray(&mut self) {
        if cfg!(all(unix, not(target_os = "macos"))) {
            log::warn!("tray icon not available on this system (hotkeys only)");
            return;
        }
        let mode = self.cfg.mode();
        let items = Items {
            capture: MenuItem::new("", true, None),
            record: MenuItem::new("", true, None),
            mode_auto: CheckMenuItem::new("", true, mode == Mode::Auto, None),
            mode_snipe: CheckMenuItem::new("", true, mode == Mode::Snipe, None),
            mode_manual: CheckMenuItem::new("", true, mode == Mode::Manual, None),
            review_last: MenuItem::new("", self.last.is_some(), None),
            open_image: MenuItem::new("", true, None),
            open_folder: MenuItem::new("", true, None),
            check_update: MenuItem::new("", true, None),
            settings: MenuItem::new("", true, None),
            quit: MenuItem::new("", true, None),
        };
        let menu = Menu::new();
        let sep = || PredefinedMenuItem::separator();
        let _ = menu.append(&items.capture);
        let _ = menu.append(&items.record);
        let _ = menu.append(&sep());
        let _ = menu.append(&items.mode_auto);
        let _ = menu.append(&items.mode_snipe);
        let _ = menu.append(&items.mode_manual);
        let _ = menu.append(&sep());
        let _ = menu.append(&items.review_last);
        let _ = menu.append(&items.open_image);
        let _ = menu.append(&items.open_folder);
        let _ = menu.append(&sep());
        let _ = menu.append(&items.check_update);
        let _ = menu.append(&items.settings);
        let _ = menu.append(&items.quit);

        let icon = tray_icon::Icon::from_rgba(icon::rgba(32), 32, 32).ok();
        let mut b = TrayIconBuilder::new().with_tooltip(self.tooltip()).with_menu(Box::new(menu)).with_menu_on_left_click(false);
        if let Some(i) = icon {
            b = b.with_icon(i);
        }
        match b.build() {
            Ok(t) => self.tray = Some(t),
            Err(e) => log::error!("tray icon: {e}"),
        }
        self.items = Some(items);
        self.sync_tray();
    }

    fn sync_tray(&self) {
        if let Some(it) = &self.items {
            let m = self.cfg.mode();
            let snip = settings::pretty_hotkey(&self.cfg.hotkeys.capture);
            let rec = settings::pretty_hotkey(&self.cfg.hotkeys.record);
            it.mode_auto.set_checked(m == Mode::Auto);
            it.mode_snipe.set_checked(m == Mode::Snipe);
            it.mode_manual.set_checked(m == Mode::Manual);
            it.review_last.set_enabled(self.last.is_some());
            it.capture.set_text(tf!("Capturer\t{snip}", "Capture\t{snip}"));
            it.record.set_text(tf!("Enregistrer un défilement\t{rec}", "Record a scroll\t{rec}"));
            it.mode_auto.set_text(t!("Mode automatique", "Automatic mode"));
            it.mode_snipe.set_text(t!("Mode snipe (aperçu à copier)", "Snipe mode (preview to copy)"));
            it.mode_manual.set_text(t!("Mode manuel", "Manual mode"));
            it.review_last.set_text(t!("Revoir la dernière capture…", "Review last capture…"));
            it.open_image.set_text(t!("Ouvrir une image…", "Open an image…"));
            it.open_folder.set_text(t!("Ouvrir le dossier des notes", "Open notes folder"));
            it.check_update.set_text(t!("Rechercher une mise à jour", "Check for updates"));
            it.settings.set_text(t!("Paramètres…", "Settings…"));
            it.quit.set_text(t!("Quitter", "Quit"));
        }
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(self.tooltip()));
        }
    }

    fn tooltip(&self) -> String {
        let snip = settings::pretty_hotkey(&self.cfg.hotkeys.capture);
        let rec = settings::pretty_hotkey(&self.cfg.hotkeys.record);
        tf!("MarkdownShot - {snip} : capture · {rec} : enregistrement", "MarkdownShot - {snip}: capture · {rec}: record")
    }

    fn apply_config(&mut self, el: &ActiveEventLoop, cfg: Config) {
        let hotkeys_changed = cfg.hotkeys.capture != self.cfg.hotkeys.capture
            || cfg.hotkeys.record != self.cfg.hotkeys.record;
        self.cfg = cfg;
        crate::i18n::set(&self.cfg.interface.language);
        if let Err(e) = self.cfg.save() {
            self.notify(el, Kind::Error, t!("Configuration non enregistrée", "Settings not saved"), &format!("{e:#}"), Some(8), None);
        }
        let _ = self.worker.send(Job::Config(Box::new(self.cfg.clone())));
        if hotkeys_changed && !self.hotkeys_suspended {
            self.register_hotkeys(el);
        }
        self.sync_tray();
    }

    fn set_mode(&mut self, el: &ActiveEventLoop, m: Mode) {
        let mut c = self.cfg.clone();
        c.set_mode(m);
        self.apply_config(el, c);
        if let Some(s) = self.settings.as_mut() {
            s.sync(&self.cfg);
        }
    }

    fn open_settings(&mut self, el: &ActiveEventLoop, tab: settings::Tab) {
        if let Some(s) = self.settings.as_mut() {
            s.show_tab(tab);
            s.win.window.set_minimized(false);
            s.win.window.focus_window();
            return;
        }
        match SettingsWindow::new(el, &self.cfg) {
            Ok(mut s) => {
                s.show_tab(tab);
                self.settings = Some(s);
            }
            Err(e) => log::error!("settings window: {e}"),
        }
    }

    fn notify(
        &mut self,
        el: &ActiveEventLoop,
        kind: Kind,
        title: &str,
        body: &str,
        secs: Option<u64>,
        open: Option<std::path::PathBuf>,
    ) {
        self.toast = None;
        self.toast_update = false;
        log::debug!("notification: {title} | {body}");
        match Toast::new(el, self.area, self.scale, kind, title, body, secs, open, &self.font) {
            Ok(t) => self.toast = Some(t),
            Err(e) => log::warn!("notification: {e}"),
        }
    }

    fn check_update(&mut self, manual: bool) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = update::latest().map_err(|e| format!("{e:#}"));
            let _ = proxy.send_event(UserEvent::UpdateChecked { result, manual });
        });
    }

    fn on_update_checked(&mut self, el: &ActiveEventLoop, result: Result<update::Release, String>, manual: bool) {
        self.update_busy = false;
        let rel = match result {
            Ok(r) => r,
            Err(e) => {
                log::warn!("update check: {e}");
                if manual {
                    self.notify(el, Kind::Error, t!("Vérification impossible", "Update check failed"), &e, Some(8), None);
                }
                return;
            }
        };
        let mut c = self.cfg.clone();
        c.updates.last_check = today();
        self.apply_config(el, c);
        let current = env!("CARGO_PKG_VERSION");
        if !update::is_newer(current, &rel.version) {
            log::info!("up to date ({current}, latest release {})", rel.version);
            if manual {
                let body = tf!("Version {current} : c'est la dernière.", "Version {current} is the latest.");
                self.notify(el, Kind::Success, t!("MarkdownShot est à jour", "MarkdownShot is up to date"), &body, Some(5), None);
            }
            return;
        }
        if !manual && self.cfg.updates.skip_version == rel.version {
            return;
        }
        log::info!("new version {} (current {current})", rel.version);
        let one_click = update::installed_with_setup() && rel.setup_url.is_some();
        let body = if one_click {
            tf!(
                "Vous avez la {current}.\nClic : installer et redémarrer · clic droit : ignorer",
                "You have {current}.\nClick: install and restart · right-click: skip"
            )
        } else {
            tf!(
                "Vous avez la {current}.\nClic : page de téléchargement · clic droit : ignorer",
                "You have {current}.\nClick: download page · right-click: skip"
            )
        };
        let title = tf!("Nouvelle version : MarkdownShot {}", "New version: MarkdownShot {}", rel.version);
        self.update = Some(rel);
        self.notify(el, Kind::Info, &title, &body, Some(30), None);
        if let Some(t) = &self.toast {
            t.set_clickable();
        }
        self.toast_update = true;
    }

    fn install_update(&mut self, el: &ActiveEventLoop) {
        let Some(rel) = self.update.clone() else { return };
        if !update::installed_with_setup() || rel.setup_url.is_none() {
            platform::open(&rel.page);
            return;
        }
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        let body = t!("MarkdownShot redémarrera tout seul une fois installé.", "MarkdownShot will restart once installed.");
        let title = tf!("Téléchargement de la version {}…", "Downloading version {}…", rel.version);
        self.notify(el, Kind::Busy, &title, body, None, None);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let r = update::fetch_installer(&rel).map_err(|e| format!("{e:#}"));
            let _ = proxy.send_event(UserEvent::UpdateDownloaded(r));
        });
    }

    fn on_update_downloaded(&mut self, el: &ActiveEventLoop, r: Result<std::path::PathBuf, String>) {
        self.update_busy = false;
        match r.and_then(|p| update::run_installer(&p).map_err(|e| format!("{e:#}"))) {
            Ok(()) => {
                log::info!("installer started, exiting for the update");
                el.exit();
            }
            Err(e) => {
                log::error!("update: {e}");
                self.notify(el, Kind::Error, t!("Mise à jour impossible", "Update failed"), &e, Some(12), None);
            }
        }
    }

    fn skip_update(&mut self, el: &ActiveEventLoop) {
        if let Some(rel) = self.update.take() {
            let mut c = self.cfg.clone();
            c.updates.skip_version = rel.version;
            self.apply_config(el, c);
        }
    }

    fn start_overlay(&mut self, el: &ActiveEventLoop, kind: CaptureKind) {
        if self.overlay.is_some() || self.recording.is_some() {
            return;
        }
        log::info!("capture toolbar ({kind:?})");
        self.overlay_source = platform::foreground_title();
        let monitor = platform::cursor_pos().and_then(|(x, y)| xcap::Monitor::from_point(x, y).ok()).or_else(|| {
            xcap::Monitor::all().ok().and_then(|v| {
                v.into_iter()
                    .find(|m| m.is_primary().unwrap_or(false))
                    .or_else(|| xcap::Monitor::all().ok()?.into_iter().next())
            })
        });
        let Some(monitor) = monitor else {
            self.notify(el, Kind::Error, t!("Capture impossible", "Capture failed"), t!("Aucun écran trouvé.", "No screen found."), Some(6), None);
            return;
        };
        let origin = (monitor.x().unwrap_or(0), monitor.y().unwrap_or(0));
        self.toast = None;
        let shot = match monitor.capture_image() {
            Ok(s) => s,
            Err(e) => {
                log::error!("screenshot: {e}");
                self.notify(el, Kind::Error, t!("Capture impossible", "Capture failed"), &e.to_string(), Some(8), None);
                return;
            }
        };
        self.area = (origin.0, origin.1, shot.width(), shot.height());
        self.scale = monitor.scale_factor().unwrap_or(1.0) as f64;
        let windows = visible_windows(origin, shot.width(), shot.height());
        let mode = self.cfg.mode();
        match Overlay::new(el, shot, origin, windows, kind, mode) {
            Ok(mut o) => {
                o.auto_scroll = self.cfg.capture.auto_scroll;
                o.show(&self.font);
                match ToolbarWindow::new(el, &o.dim_rgba(), origin, self.scale, kind, mode, o.auto_scroll) {
                    Ok(mut t) => {
                        t.st.hint = o.hint();
                        self.toolbar = Some(t);
                    }
                    Err(e) => log::warn!("capture toolbar: {e}"),
                }
                self.overlay = Some(o);
                if kind == CaptureKind::Full {
                    if let Some(out) = self.overlay.as_mut().map(|o| o.set_kind(CaptureKind::Full)) {
                        self.on_outcome(el, out);
                    }
                }
            }
            Err(e) => {
                log::error!("selection window: {e}");
                self.notify(el, Kind::Error, t!("Capture impossible", "Capture failed"), &e.to_string(), Some(8), None)
            }
        }
    }

    fn close_overlay(&mut self) {
        self.overlay = None;
        self.toolbar = None;
    }

    fn on_outcome(&mut self, el: &ActiveEventLoop, out: Outcome) {
        match out {
            Outcome::Continue => {
                if let (Some(o), Some(t)) = (self.overlay.as_ref(), self.toolbar.as_mut()) {
                    t.set(o.kind, o.mode, o.auto_scroll, o.hint());
                }
                if let Some(a) = self.overlay.as_ref().map(|o| o.auto_scroll) {
                    if a != self.cfg.capture.auto_scroll {
                        let mut c = self.cfg.clone();
                        c.capture.auto_scroll = a;
                        self.apply_config(el, c);
                    }
                }
            }
            Outcome::Cancel => self.close_overlay(),
            Outcome::Done { sel, zones, kind, mode } => self.overlay_done(el, sel, zones, kind, mode),
        }
    }

    fn overlay_done(&mut self, el: &ActiveEventLoop, sel: Rect, zones: Vec<Rect>, kind: CaptureKind, mode: Mode) {
        self.toolbar = None;
        let Some(o) = self.overlay.take() else { return };
        let source = self.overlay_source.take();
        let (iw, ih) = o.shot.dimensions();
        let (x, y, w, h) = sel.to_px(iw, ih);
        if kind == CaptureKind::Record {
            let origin = o.origin;
            let auto = o.auto_scroll;
            o.window.set_visible(false);
            drop(o);
            let opts = crate::scrolling::Options {
                fps: self.cfg.capture.frames_per_second,
                auto_step: auto.then_some(crate::scrolling::AUTO_STEP),
                ..Default::default()
            };
            match Recording::start(el, origin, (iw, ih), (x, y, w, h), opts, source, &self.font) {
                Ok(r) => {
                    self.recording = Some(r);
                    self.rec_mode = mode;
                    self.next_refresh = Instant::now() + Duration::from_millis(500);
                    if let (Some(m), Ok(esc)) = (&self.hotkeys, HotKey::from_str("Escape")) {
                        if m.register(esc).is_ok() {
                            self.hk_esc = Some(esc);
                        }
                    }
                    if let Some(t) = &self.tray {
                        let _ = t.set_tooltip(Some(t!("MarkdownShot - enregistrement en cours (Échap pour terminer)", "MarkdownShot - recording (Esc to finish)")));
                    }
                }
                Err(e) => self.notify(el, Kind::Error, t!("Enregistrement impossible", "Recording failed"), &e.to_string(), Some(8), None),
            }
            return;
        }
        let crop: RgbImage =
            image::DynamicImage::ImageRgba8(image::imageops::crop_imm(&o.shot, x, y, w, h).to_image()).to_rgb8();
        let zones: Vec<Zone> = zones.iter().map(|z| Zone::image(z.translate(-(x as f32), -(y as f32)))).collect();
        drop(o);
        if mode == Mode::Snipe {
            let _ = self.worker.send(Job::Analyze { image: crop, source, long: false, snipe: true });
            self.notify(el, Kind::Busy, t!("Analyse de la capture…", "Analyzing the capture…"), t!("L'aperçu du Markdown s'ouvre dans un instant.", "The Markdown preview opens in a moment."), None, None);
            return;
        }
        let _ = self.worker.send(Job::Capture { image: crop, zones, mode, source, long: false });
        self.notify(el, Kind::Busy, t!("Analyse de la capture…", "Analyzing the capture…"), t!("La note sera prête dans un instant.", "The note will be ready in a moment."), None, None);
    }

    fn stop_recording(&mut self, el: &ActiveEventLoop) {
        let Some(rec) = self.recording.take() else { return };
        if let (Some(m), Some(esc)) = (&self.hotkeys, self.hk_esc.take()) {
            let _ = m.unregister(esc);
        }
        self.sync_tray();
        let source = rec.source.clone();
        match rec.stop() {
            Some(img) => {
                let h = img.height();
                if self.rec_mode == Mode::Snipe {
                    let _ = self.worker.send(Job::Analyze { image: img, source, long: true, snipe: true });
                    let body = tf!("Page de {h} px : son Markdown s'affichera, prêt à copier.", "{h} px page: its Markdown will be shown, ready to copy.");
                    self.notify(el, Kind::Busy, t!("Analyse de l'enregistrement…", "Analyzing the recording…"), &body, None, None);
                } else if self.rec_mode == Mode::Manual {
                    let _ = self.worker.send(Job::Analyze { image: img, source, long: true, snipe: false });
                    self.notify(
                        el,
                        Kind::Busy,
                        t!("Analyse de l'enregistrement…", "Analyzing the recording…"),
                        &tf!("Page de {h} px : elle s'ouvrira pour marquer les images.", "{h} px page: it will open so you can mark the images."),
                        None,
                        None,
                    );
                } else {
                    let _ = self.worker.send(Job::Capture { image: img, zones: Vec::new(), mode: Mode::Auto, source, long: true });
                    self.notify(el, Kind::Busy, t!("Analyse de l'enregistrement…", "Analyzing the recording…"), &tf!("Page assemblée : {h} px de haut.", "Stitched page: {h} px high."), None, None);
                }
            }
            None => self.notify(el, Kind::Error, t!("Enregistrement vide", "Empty recording"), t!("Aucune image n'a pu être capturée.", "No image could be captured."), Some(6), None),
        }
    }

    fn open_review(&mut self, el: &ActiveEventLoop, ctx: ReviewCtx, zones: Vec<Zone>, title: &str) {
        self.review = None;
        match ReviewWindow::new(el, ctx.analysis.clone(), zones, title, ctx.snipe) {
            Ok(mut w) => {
                if ctx.replace.is_some() {
                    w.st.save_label = t!("Mettre à jour la note", "Update note").into();
                }
                w.st.dest = ctx
                    .replace
                    .as_ref()
                    .and_then(|l| l.note.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| self.cfg.output.notes_dir.clone());
                self.review = Some((w, ctx));
            }
            Err(e) => {
                log::error!("review window: {e}");
                self.notify(el, Kind::Error, t!("Revue impossible", "Review failed"), &e.to_string(), Some(8), None);
            }
        }
    }

    fn open_image(&mut self, el: &ActiveEventLoop, path: &std::path::Path, snipe: bool) {
        match image::open(path) {
            Ok(img) => {
                let source = path.file_name().map(|n| n.to_string_lossy().to_string());
                let img = img.to_rgb8();
                let long = img.height() > img.width() * 2;
                let _ = self.worker.send(Job::Analyze { image: img, source, long, snipe });
                let body = if snipe {
                    t!("Son Markdown s'affichera, prêt à copier.", "Its Markdown will be shown, ready to copy.")
                } else {
                    t!("Elle s'ouvrira pour marquer les images.", "It will open so you can mark the images.")
                };
                self.notify(el, Kind::Busy, t!("Analyse de l'image…", "Analyzing the image…"), body, None, None);
            }
            Err(e) => {
                let body = format!("{} : {e}", path.display());
                self.notify(el, Kind::Error, t!("Image illisible", "Unreadable image"), &body, Some(8), None);
            }
        }
    }

    fn review_last(&mut self, el: &ActiveEventLoop) {
        let Some(last) = self.last.clone() else { return };
        let ctx = ReviewCtx {
            analysis: last.analysis.clone(),
            source: last.source.clone(),
            long: last.long,
            replace: Some(last.clone()),
            snipe: false,
        };
        self.open_review(el, ctx, last.zones.clone(), &last.title);
    }

    fn on_review_actions(&mut self, el: &ActiveEventLoop, actions: Vec<review::Action>) {
        for a in actions {
            match a {
                review::Action::Preview(gen_, zones) => {
                    let Some((_, ctx)) = &self.review else { continue };
                    let analysis = ctx.analysis.clone();
                    let proxy = self.proxy.clone();
                    let wiki = self.cfg.output.wikilinks;
                    std::thread::spawn(move || {
                        let doc = analysis.build(&zones, markdownshot_core::Mode::Manual);
                        let md = doc.markdown(&|i| image_link(wiki, i));
                        let _ = proxy.send_event(UserEvent::Preview(gen_, md));
                    });
                }
                review::Action::Copy { zones, close } => {
                    let Some((w, ctx)) = self.review.as_mut() else { continue };
                    let wiki = self.cfg.output.wikilinks;
                    let md = ctx.analysis.build(&zones, markdownshot_core::Mode::Manual).markdown(&|i| image_link(wiki, i));
                    if self.clipboard.is_none() {
                        self.clipboard = arboard::Clipboard::new().map_err(|e| log::warn!("clipboard: {e}")).ok();
                    }
                    let ok = self.clipboard.as_mut().is_some_and(|c| {
                        c.set_text(md).map_err(|e| log::warn!("clipboard: {e}")).is_ok()
                    });
                    if !ok {
                        self.notify(el, Kind::Error, t!("Copie impossible", "Copy failed"), t!("Le presse-papier n'est pas disponible.", "The clipboard is not available."), Some(6), None);
                    } else if close {
                        self.review = None;
                        self.notify(el, Kind::Success, t!("Markdown copié", "Markdown copied"), t!("Prêt à coller.", "Ready to paste."), Some(3), None);
                    } else {
                        w.copied();
                    }
                }
                review::Action::ChooseFolder => {
                    let Some((w, _)) = self.review.as_mut() else { continue };
                    let pick = rfd::FileDialog::new()
                        .set_title(t!("Dossier de cette note", "Folder for this note"))
                        .set_directory(&w.st.dest)
                        .pick_folder();
                    if let Some(d) = pick {
                        w.st.dest = d;
                        w.win.window.request_redraw();
                    }
                }
                review::Action::Save(zones) => {
                    let Some((w, ctx)) = self.review.take() else { continue };
                    let dir = (w.st.dest != self.cfg.output.notes_dir || ctx.replace.is_some()).then(|| w.st.dest.clone());
                    let replacing = ctx.replace.is_some();
                    let _ = self.worker.send(Job::Write {
                        analysis: ctx.analysis,
                        zones,
                        source: ctx.source,
                        long: ctx.long,
                        replace: ctx.replace,
                        mode: if ctx.snipe { Mode::Snipe } else { Mode::Manual },
                        dir,
                    });
                    let title = if replacing { t!("Mise à jour de la note…", "Updating the note…") } else { t!("Enregistrement de la note…", "Saving the note…") };
                    let body = t!("Découpe des images et écriture du fichier.", "Cropping the images and writing the file.");
                    self.notify(el, Kind::Busy, title, body, None, None);
                }
                review::Action::Cancel => self.review = None,
            }
        }
    }

    fn on_settings_actions(&mut self, el: &ActiveEventLoop, actions: Vec<settings::Action>) {
        for a in actions {
            match a {
                settings::Action::Apply(mut cfg) => {
                    cfg.updates.last_check = self.cfg.updates.last_check.clone();
                    cfg.updates.skip_version = self.cfg.updates.skip_version.clone();
                    self.apply_config(el, *cfg)
                }
                settings::Action::CheckUpdate => self.check_update(true),
                settings::Action::SuspendHotkeys(on) => {
                    self.hotkeys_suspended = on;
                    if on {
                        self.unregister_hotkeys();
                    } else {
                        self.register_hotkeys(el);
                    }
                }
                settings::Action::Open(p) => platform::open_path(&p),
            }
        }
    }

    fn on_toolbar_actions(&mut self, el: &ActiveEventLoop, actions: Vec<toolbar::Action>) {
        for a in actions {
            let Some(o) = self.overlay.as_mut() else { return };
            let out = match a {
                toolbar::Action::Kind(k) => o.set_kind(k),
                toolbar::Action::Mode(m) => {
                    o.set_mode(m);
                    self.set_mode(el, m);
                    Outcome::Continue
                }
                toolbar::Action::AutoScroll(a) => {
                    o.auto_scroll = a;
                    Outcome::Continue
                }
                toolbar::Action::Settings => {
                    self.close_overlay();
                    self.open_settings(el, settings::Tab::Capture);
                    return;
                }
                toolbar::Action::Cancel => Outcome::Cancel,
            };
            if let Some(o) = &self.overlay {
                o.window.focus_window();
            }
            self.raise_toolbar();
            self.on_outcome(el, out);
        }
    }

    fn raise_toolbar(&self) {
        if let Some(t) = &self.toolbar {
            t.win.window.set_window_level(winit::window::WindowLevel::Normal);
            t.win.window.set_window_level(winit::window::WindowLevel::AlwaysOnTop);
            t.win.window.request_redraw();
        }
    }

    fn on_menu(&mut self, el: &ActiveEventLoop, ev: MenuEvent) {
        let Some(it) = &self.items else { return };
        let id = ev.id;
        if id == *it.capture.id() {
            self.pending = Some((Instant::now() + Duration::from_millis(250), CaptureKind::Zone));
        } else if id == *it.record.id() {
            if self.recording.is_some() {
                self.stop_recording(el);
            } else {
                self.pending = Some((Instant::now() + Duration::from_millis(250), CaptureKind::Record));
            }
        } else if id == *it.mode_auto.id() {
            self.set_mode(el, Mode::Auto);
        } else if id == *it.mode_snipe.id() {
            self.set_mode(el, Mode::Snipe);
        } else if id == *it.mode_manual.id() {
            self.set_mode(el, Mode::Manual);
        } else if id == *it.review_last.id() {
            self.review_last(el);
        } else if id == *it.open_image.id() {
            let pick = rfd::FileDialog::new()
                .set_title(t!("Ouvrir une image", "Open an image"))
                .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"])
                .pick_file();
            if let Some(p) = pick {
                self.open_image(el, &p, self.cfg.mode() == Mode::Snipe);
            }
        } else if id == *it.open_folder.id() {
            let d = self.cfg.output.notes_dir.clone();
            let _ = std::fs::create_dir_all(&d);
            platform::open_path(&d);
        } else if id == *it.check_update.id() {
            self.check_update(true);
        } else if id == *it.settings.id() {
            self.open_settings(el, settings::Tab::General);
        } else if id == *it.quit.id() {
            el.exit();
        }
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn visible_windows(origin: (i32, i32), w: u32, h: u32) -> Vec<Rect> {
    let me = std::process::id();
    let screen = Rect::new(0.0, 0.0, w as f32, h as f32);
    xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter(|win| win.pid().ok() != Some(me) && !win.is_minimized().unwrap_or(true))
        .filter_map(|win| {
            let (x, y) = (win.x().ok()? - origin.0, win.y().ok()? - origin.1);
            let (ww, wh) = (win.width().ok()?, win.height().ok()?);
            if ww < 40 || wh < 30 {
                return None;
            }
            let r = Rect::new(x as f32, y as f32, (x + ww as i32) as f32, (y + wh as i32) as f32);
            (r.inter_area(&screen) > 0.0).then_some(r)
        })
        .collect()
}

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            self.init(el);
        }
    }

    fn resumed(&mut self, _el: &ActiveEventLoop) {}

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        match &ev {
            UserEvent::Hotkey(e) => log::debug!("hotkey {} {:?}", e.id(), e.state()),
            UserEvent::Menu(e) => log::debug!("menu {:?}", e.id),
            UserEvent::Worker(r) => log::debug!("engine: {r:?}"),
            _ => {}
        }
        match ev {
            UserEvent::Hotkey(e) => {
                if e.state() != HotKeyState::Pressed {
                    return;
                }
                let id = e.id();
                if self.hk_esc.is_some_and(|h| h.id() == id) {
                    self.stop_recording(el);
                } else if self.hk_rec.is_some_and(|h| h.id() == id) {
                    if self.recording.is_some() {
                        self.stop_recording(el);
                    } else if self.overlay.is_none() {
                        self.start_overlay(el, CaptureKind::Record);
                    }
                } else if self.hk_snip.is_some_and(|h| h.id() == id) {
                    if self.overlay.is_none() {
                        self.start_overlay(el, CaptureKind::Zone);
                    }
                }
            }
            UserEvent::Menu(e) => self.on_menu(el, e),
            UserEvent::Tray(e) => match e {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                    if self.recording.is_some() {
                        self.stop_recording(el);
                    } else {
                        self.pending = Some((Instant::now() + Duration::from_millis(150), CaptureKind::Zone));
                    }
                }
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    self.pending = None;
                    self.open_settings(el, settings::Tab::General);
                }
                _ => {}
            },
            UserEvent::UpdateChecked { result, manual } => self.on_update_checked(el, result, manual),
            UserEvent::UpdateDownloaded(r) => self.on_update_downloaded(el, r),
            UserEvent::Preview(gen_, md) => {
                if let Some((w, _)) = self.review.as_mut() {
                    w.set_preview(gen_, md);
                }
            }
            UserEvent::Worker(r) => match r {
                JobResult::Ready => {
                    if self.first_run {
                        self.first_run = false;
                        let body = tf!(
                            "{} : capturer · {} : enregistrer un défilement",
                            "{}: capture · {}: record a scroll",
                            settings::pretty_hotkey(&self.cfg.hotkeys.capture),
                            settings::pretty_hotkey(&self.cfg.hotkeys.record)
                        );
                        self.notify(el, Kind::Info, t!("MarkdownShot est prêt (icône près de l'horloge)", "MarkdownShot is ready (icon next to the clock)"), &body, Some(10), None);
                    }
                }
                JobResult::LoadError(msg) => {
                    self.notify(el, Kind::Error, t!("Modèles introuvables", "Models not found"), &msg, None, None);
                }
                JobResult::Analyzed { analysis, source, long, snipe } => {
                    self.toast = None;
                    let zones = if snipe || self.cfg.capture.manual_suggestions {
                        analysis.suggestions.iter().map(|f| Zone::image(f.rect)).collect()
                    } else {
                        Vec::new()
                    };
                    let ctx = ReviewCtx { analysis, source, long, replace: None, snipe };
                    self.open_review(el, ctx, zones, if snipe { t!("Aperçu", "Preview") } else { t!("Marquer les images", "Mark the images") });
                }
                JobResult::Done { title, images, ms, mode, last } => {
                    let note = last.note.clone();
                    self.last = Some(last);
                    self.sync_tray();
                    let body = tf!(
                        "{} · {} · {} image{} · {:.1} s\nClic : ouvrir la note · clic droit : revoir les zones",
                        "{} · {} · {} image{} · {:.1} s\nClick: open the note · right-click: review the areas",
                        title,
                        mode.label(),
                        images,
                        if images > 1 { "s" } else { "" },
                        ms as f64 / 1000.0
                    );
                    self.notify(el, Kind::Success, t!("Note créée", "Note created"), &body, Some(9), Some(note.clone()));
                    match self.cfg.output.open_after.as_str() {
                        "note" => platform::open_path(&note),
                        "obsidian" => platform::open_in_obsidian(&note),
                        _ => {}
                    }
                }
                JobResult::Error(msg) => {
                    log::error!("{msg}");
                    self.notify(el, Kind::Error, t!("Échec de la conversion", "Conversion failed"), &msg, Some(12), None);
                }
            },
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        if let Some(o) = self.overlay.as_mut() {
            if o.id() == id {
                let out = o.handle(&ev, &self.font);
                self.on_outcome(el, out);
                return;
            }
        }
        if let Some(t) = self.toolbar.as_mut() {
            if t.win.id() == id {
                if let WindowEvent::KeyboardInput { event, .. } = &ev {
                    if event.state == ElementState::Pressed {
                        if let Some(o) = self.overlay.as_mut() {
                            let out = o.key(&event.logical_key);
                            o.window.focus_window();
                            self.on_outcome(el, out);
                        }
                    }
                    return;
                }
                if t.win.on_event(&ev) && !matches!(ev, WindowEvent::RedrawRequested) {
                    t.win.window.request_redraw();
                }
                if matches!(ev, WindowEvent::RedrawRequested) {
                    let actions = t.paint();
                    self.on_toolbar_actions(el, actions);
                }
                return;
            }
        }
        if let Some(s) = self.settings.as_mut() {
            if s.win.id() == id {
                let repaint = s.win.on_event(&ev);
                if s.win.close_requested {
                    if s.is_recording() {
                        self.hotkeys_suspended = false;
                        self.register_hotkeys(el);
                    }
                    self.settings = None;
                    return;
                }
                if matches!(ev, WindowEvent::RedrawRequested) {
                    let actions = s.paint();
                    self.on_settings_actions(el, actions);
                } else if repaint {
                    s.win.window.request_redraw();
                }
                return;
            }
        }
        if let Some((r, _)) = self.review.as_mut() {
            if r.win.id() == id {
                let repaint = r.win.on_event(&ev);
                if r.win.close_requested {
                    self.review = None;
                    return;
                }
                if matches!(ev, WindowEvent::RedrawRequested) {
                    let actions = r.paint();
                    self.on_review_actions(el, actions);
                } else if repaint {
                    r.win.window.request_redraw();
                }
                return;
            }
        }
        if let Some(t) = self.toast.as_mut() {
            if t.id() == id {
                let click = t.handle(&ev, &self.font);
                if self.toast_update && click != Click::None {
                    self.toast = None;
                    self.toast_update = false;
                    if click == Click::Left {
                        self.install_update(el);
                    } else {
                        self.skip_update(el);
                    }
                    return;
                }
                let Some(t) = self.toast.as_mut() else { return };
                match click {
                    Click::None => {}
                    Click::Left => {
                        if let Some(p) = t.open.clone() {
                            match self.cfg.output.open_after.as_str() {
                                "obsidian" => platform::open_in_obsidian(&p),
                                _ => platform::open_path(&p),
                            }
                        }
                        self.toast = None;
                    }
                    Click::Right => {
                        let reviewable = t.open.is_some() && self.last.is_some();
                        self.toast = None;
                        if reviewable {
                            self.review_last(el);
                        }
                    }
                }
                return;
            }
        }
        if let Some(r) = self.recording.as_mut() {
            if r.owns(id) {
                if let WindowEvent::RedrawRequested = ev {
                    r.refresh(&self.font);
                }
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let mut wake: Option<Instant> = None;
        let mut at = |t: Instant| wake = Some(wake.map_or(t, |w: Instant| w.min(t)));
        if let Some((t, kind)) = self.pending {
            if now >= t {
                self.pending = None;
                self.start_overlay(el, kind);
            } else {
                at(t);
            }
        }
        if self.toast.as_ref().is_some_and(|t| t.expired()) {
            self.toast = None;
        }
        if let Some(u) = self.toast.as_ref().and_then(|t| t.until) {
            at(u);
        }
        if self.recording.as_ref().is_some_and(|r| r.finished().is_some()) {
            self.stop_recording(el);
        }
        if let Some(r) = self.recording.as_mut() {
            if now >= self.next_refresh {
                r.refresh(&self.font);
                self.next_refresh = now + Duration::from_millis(250);
            }
            at(self.next_refresh);
        }
        let wins = [
            self.toolbar.as_ref().map(|t| &t.win),
            self.settings.as_ref().map(|s| &s.win),
            self.review.as_ref().map(|r| &r.0.win),
        ];
        for w in wins.into_iter().flatten() {
            if w.due() {
                w.window.request_redraw();
            } else if let Some(t) = w.next_repaint {
                at(t);
            }
        }
        el.set_control_flow(match wake {
            Some(w) => ControlFlow::WaitUntil(w),
            None => ControlFlow::Wait,
        });
    }
}
