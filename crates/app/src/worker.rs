use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use image::RgbImage;
use markdownshot_core::{Analysis, Engine, EngineOptions, Zone, ZoneKind, output};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::config::{Config, Mode, now_strings};

pub enum Job {
    Capture { image: RgbImage, zones: Vec<Zone>, mode: Mode, source: Option<String>, long: bool },
    Analyze { image: RgbImage, source: Option<String>, long: bool, snipe: bool },
    Write {
        analysis: Arc<Analysis>,
        zones: Vec<Zone>,
        source: Option<String>,
        long: bool,
        replace: Option<Arc<LastCapture>>,
        mode: Mode,
        dir: Option<PathBuf>,
    },
    Config(Box<Config>),
}

pub struct LastCapture {
    pub analysis: Arc<Analysis>,
    pub zones: Vec<Zone>,
    pub note: PathBuf,
    pub images: Vec<PathBuf>,
    pub source: Option<String>,
    pub title: String,
    pub long: bool,
}

pub enum JobResult {
    Ready,
    LoadError(String),
    Done { title: String, images: usize, ms: u128, mode: Mode, last: Arc<LastCapture> },
    Analyzed { analysis: Arc<Analysis>, source: Option<String>, long: bool, snipe: bool },
    Error(String),
}

impl std::fmt::Debug for JobResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobResult::Ready => write!(f, "ready"),
            JobResult::LoadError(e) => write!(f, "loading error: {e}"),
            JobResult::Done { last, images, ms, .. } => write!(f, "note {} ({images} images, {ms} ms)", last.note.display()),
            JobResult::Analyzed { analysis, .. } => write!(f, "analyse {:?}", analysis.image.dimensions()),
            JobResult::Error(e) => write!(f, "error: {e}"),
        }
    }
}

pub fn spawn(proxy: EventLoopProxy<UserEvent>, cfg: Config) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    std::thread::Builder::new()
        .name("markdownshot-worker".into())
        .spawn(move || run(rx, proxy, cfg))
        .expect("creating the worker thread");
    tx
}

fn load(opts: &EngineOptions) -> anyhow::Result<Engine> {
    let dir = markdownshot_core::runtime::models_dir()?;
    Engine::new(&dir, opts.clone())
}

fn run(rx: Receiver<Job>, proxy: EventLoopProxy<UserEvent>, mut cfg: Config) {
    let send = |r: JobResult| {
        let _ = proxy.send_event(UserEvent::Worker(r));
    };
    let mut opts = cfg.engine_options();
    let mut engine = match load(&opts) {
        Ok(e) => {
            send(JobResult::Ready);
            Some(e)
        }
        Err(e) => {
            log::error!("loading models: {e:#}");
            send(JobResult::LoadError(format!("{e:#}")));
            None
        }
    };
    while let Ok(job) = rx.recv() {
        match job {
            Job::Config(c) => {
                let new_opts = c.engine_options();
                let reload = engine.is_none()
                    || new_opts.quality != opts.quality
                    || new_opts.use_layout != opts.use_layout
                    || new_opts.auto_images != opts.auto_images
                    || new_opts.full_width_images != opts.full_width_images
                    || new_opts.gpu != opts.gpu;
                cfg = *c;
                if reload {
                    opts = new_opts;
                    engine = match load(&opts) {
                        Ok(e) => Some(e),
                        Err(e) => {
                            send(JobResult::LoadError(format!("{e:#}")));
                            None
                        }
                    };
                }
            }
            Job::Capture { image, zones, mode, source, long } => {
                let t = Instant::now();
                let r = match analyze(engine.as_mut(), image) {
                    Ok(a) => write(&cfg, a, &zones, mode, false, source, long, None, None, t),
                    Err(e) => JobResult::Error(e),
                };
                send(r);
            }
            Job::Analyze { image, source, long, snipe } => {
                let r = match analyze(engine.as_mut(), image) {
                    Ok(analysis) => JobResult::Analyzed { analysis, source, long, snipe },
                    Err(e) => JobResult::Error(e),
                };
                send(r);
            }
            Job::Write { analysis, zones, source, long, replace, mode, dir } => {
                send(write(&cfg, analysis, &zones, mode, true, source, long, replace, dir, Instant::now()));
            }
        }
    }
}

fn analyze(engine: Option<&mut Engine>, image: RgbImage) -> Result<Arc<Analysis>, String> {
    let Some(engine) = engine else {
        return Err(t!("Les modèles ne sont pas chargés.", "The models are not loaded.").into());
    };
    engine.analyze(image).map(Arc::new).map_err(|e| format!("{e:#}"))
}

#[allow(clippy::too_many_arguments)]
fn write(
    cfg: &Config,
    analysis: Arc<Analysis>,
    zones: &[Zone],
    mode: Mode,
    exact: bool,
    source: Option<String>,
    long: bool,
    replace: Option<Arc<LastCapture>>,
    dir: Option<PathBuf>,
    t: Instant,
) -> JobResult {
    let doc = analysis.build(zones, if exact { markdownshot_core::Mode::Manual } else { mode.engine() });
    if let Some(old) = &replace {
        for p in old.images.iter().chain(std::iter::once(&old.note)) {
            if let Err(e) = std::fs::remove_file(p) {
                log::warn!("deleting {}: {e}", p.display());
            }
        }
    }
    let opts = match &dir {
        Some(d) => cfg.output_options_in(d),
        None => cfg.output_options(),
    };
    let written = match output::write(&doc, &opts, source.as_deref(), now_strings()) {
        Ok(w) => w,
        Err(e) => return JobResult::Error(format!("{e:#}")),
    };
    if long && cfg.capture.keep_long_capture {
        let stem = written.note.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let dir =
            if opts.images_dir.is_absolute() { opts.images_dir.clone() } else { opts.notes_dir.join(&opts.images_dir) };
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = analysis.image.save(dir.join(tf!("{stem} - capture complète.png", "{stem} - full capture.png"))) {
            log::warn!("saving the long capture: {e}");
        }
    }
    if cfg.output.copy_markdown {
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(written.markdown.clone())) {
            log::warn!("clipboard: {e}");
        }
    }
    let ms = t.elapsed().as_millis();
    log::info!(
        "note {} ({} images, {} ms; OCR {} ms, layout {} ms)",
        written.note.display(),
        written.images.len(),
        ms,
        doc.stats.ocr_ms,
        doc.stats.layout_ms
    );
    let mut kept: Vec<Zone> = doc.figures.iter().map(|f| Zone::image(f.rect)).collect();
    kept.extend(zones.iter().filter(|z| z.kind == ZoneKind::Ignore).copied());
    let last = Arc::new(LastCapture {
        analysis,
        zones: kept,
        note: written.note,
        images: written.created,
        source,
        title: doc.title.clone(),
        long,
    });
    JobResult::Done { title: doc.title, images: written.images.len(), ms, mode, last }
}
