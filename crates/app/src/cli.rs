use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use markdownshot_core::stitch::Stitcher;
use markdownshot_core::{Engine, Rect, output};

use crate::config::{Config, now_strings};

pub const HELP: &str = "MarkdownShot : capture d'écran vers Markdown (texte, mise en forme, images), 100 % local.

Utilisation :
  markdownshot                          Lance l'application (icône près de l'horloge).
  markdownshot open IMAGE               Lance l'application et ouvre l'image pour y marquer les zones.
  markdownshot snipe IMAGE              Lance l'application et affiche l'image et son Markdown,
                                        à copier-coller (mode Snipe : aucune note n'est écrite).
  markdownshot convert IMAGE...         Convertit des images en notes Markdown.
  markdownshot stitch DOSSIER           Assemble les captures d'un défilement (PNG triés par nom)
                                        en une seule page, puis la convertit.
  markdownshot record --zone X,Y,L,H    Enregistre le défilement d'une zone de l'écran (coordonnées
                                        du bureau), puis convertit la page assemblée.

Options :
  --out DOSSIER          Dossier des notes (par défaut : celui de la configuration).
  --zone X,Y,L,H         Zone d'image imposée (pixels), répétable (convert).
  --image FICHIER.png    Enregistre aussi la page assemblée (stitch, record).
  --stdout               Affiche le Markdown au lieu d'écrire une note.
  --figures FICHIER.json Enregistre la position des images retenues (évaluation).
  --quality Q            fast | balanced | accurate.
  --auto                 Défilement automatique jusqu'en bas de page (record).
  --step F               Pas du défilement automatique, fraction de la hauteur (0.85 par défaut).
  --fps N                Images par seconde sans --auto (8 par défaut).
  --duration S           Durée maximale en secondes (record).
  --no-note              N'écrit pas de note (avec --image : page assemblée seulement).
  --version, --help";

pub const HELP_EN: &str = "MarkdownShot: screenshot to Markdown (text, formatting, images), 100% local.

Usage:
  markdownshot                          Starts the application (icon next to the clock).
  markdownshot open IMAGE               Starts the application and opens the image to mark its areas.
  markdownshot snipe IMAGE              Starts the application and shows the image and its Markdown,
                                        ready to copy (Snipe mode: no note is written).
  markdownshot convert IMAGE...         Converts images into Markdown notes.
  markdownshot stitch FOLDER            Stitches the screenshots of a scroll (PNG sorted by name)
                                        into a single page, then converts it.
  markdownshot record --zone X,Y,W,H    Records the scrolling of a screen area (desktop
                                        coordinates), then converts the stitched page.

Options:
  --out FOLDER           Notes folder (default: the one in the configuration).
  --zone X,Y,W,H         Forced image area (pixels), repeatable (convert).
  --image FILE.png       Also saves the stitched page (stitch, record).
  --stdout               Prints the Markdown instead of writing a note.
  --figures FILE.json    Saves the position of the kept images (evaluation).
  --quality Q            fast | balanced | accurate.
  --auto                 Scrolls automatically to the bottom of the page (record).
  --step F               Auto-scroll step, as a fraction of the height (default 0.85).
  --fps N                Frames per second without --auto (default 8).
  --duration S           Maximum duration in seconds (record).
  --no-note              Writes no note (with --image: stitched page only).
  --version, --help";

pub fn help() -> &'static str {
    t!(HELP, HELP_EN)
}

struct Opts {
    inputs: Vec<PathBuf>,
    out: Option<PathBuf>,
    zones: Vec<Rect>,
    image: Option<PathBuf>,
    stdout: bool,
    quality: Option<String>,
    figures: Option<PathBuf>,
    auto: bool,
    step: Option<f32>,
    fps: Option<u32>,
    duration: Option<f32>,
    no_note: bool,
}

fn parse(args: &[String]) -> Result<Opts> {
    let mut o = Opts {
        inputs: vec![],
        out: None,
        zones: vec![],
        image: None,
        stdout: false,
        quality: None,
        figures: None,
        auto: false,
        step: None,
        fps: None,
        duration: None,
        no_note: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut val = || -> Result<String> {
            i += 1;
            args.get(i).cloned().with_context(|| tf!("valeur manquante après {a}", "missing value after {a}"))
        };
        match a.as_str() {
            "--out" | "-o" => o.out = Some(PathBuf::from(val()?)),
            "--image" => o.image = Some(PathBuf::from(val()?)),
            "--quality" => o.quality = Some(val()?),
            "--stdout" => o.stdout = true,
            "--figures" => o.figures = Some(PathBuf::from(val()?)),
            "--auto" => o.auto = true,
            "--step" => o.step = Some(val()?.replace(',', ".").parse().context("--step")?),
            "--fps" => o.fps = Some(val()?.parse().context("--fps")?),
            "--duration" => o.duration = Some(val()?.replace(',', ".").parse().context("--duration")?),
            "--no-note" => o.no_note = true,
            "--zone" => {
                let v = val()?;
                let n: Vec<f32> = v
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .context("--zone X,Y,W,H")?;
                if n.len() != 4 {
                    bail!("{}", t!("--zone attend 4 nombres : X,Y,L,H", "--zone expects 4 numbers: X,Y,W,H"));
                }
                o.zones.push(Rect::new(n[0], n[1], n[0] + n[2], n[1] + n[3]));
            }
            s if s.starts_with("--") => bail!("{}", tf!("option inconnue : {s}", "unknown option: {s}")),
            s => o.inputs.push(PathBuf::from(s)),
        }
        i += 1;
    }
    Ok(o)
}

fn engine(cfg: &Config) -> Result<Engine> {
    let dir = markdownshot_core::runtime::models_dir()?;
    Engine::new(&dir, cfg.engine_options())
}

fn emit(eng: &mut Engine, cfg: &Config, img: &image::RgbImage, zones: &[Rect], o: &Opts, source: &str) -> Result<()> {
    let t = std::time::Instant::now();
    let doc = eng.process(img, zones)?;
    if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
        for f in &doc.figures {
            eprintln!("image {:?} {:?}", f.source, f.rect);
        }
    }
    if let Some(p) = &o.figures {
        let rects: Vec<String> = doc
            .figures
            .iter()
            .map(|f| format!("[{:.0}, {:.0}, {:.0}, {:.0}]", f.rect.x0, f.rect.y0, f.rect.x1, f.rect.y1))
            .collect();
        std::fs::write(p, format!("[{}]\n", rects.join(", ")))?;
    }
    if o.stdout {
        print!("{}", doc.markdown(&|i| format!("![[image-{}.png]]", i + 1)));
        eprintln!(
            "({} ms: OCR {} ms, layout {} ms, {} images{})",
            t.elapsed().as_millis(),
            doc.stats.ocr_ms,
            doc.stats.layout_ms,
            doc.images.len(),
            if markdownshot_core::runtime::gpu_active() { ", GPU" } else { "" }
        );
        return Ok(());
    }
    let mut opts = cfg.output_options();
    if let Some(d) = &o.out {
        opts.notes_dir = d.clone();
    }
    let w = output::write(&doc, &opts, Some(source), now_strings())?;
    println!("{}  ({} images, {} ms)", w.note.display(), w.images.len(), t.elapsed().as_millis());
    Ok(())
}

pub fn run(cmd: &str, args: &[String]) -> Result<()> {
    let mut o = parse(args)?;
    let mut cfg = Config::load().map(|c| c.0).unwrap_or_default();
    if let Some(q) = o.quality.take() {
        cfg.engine.quality = q;
    }
    let help = help();
    match cmd {
        "convert" => {
            if o.inputs.is_empty() {
                bail!("{}\n\n{help}", t!("indiquez au moins une image.", "give at least one image."));
            }
            let mut eng = engine(&cfg)?;
            for p in &o.inputs {
                let img = image::open(p).with_context(|| tf!("ouverture de {}", "opening {}", p.display()))?.to_rgb8();
                emit(&mut eng, &cfg, &img, &o.zones, &o, &p.to_string_lossy())?;
            }
        }
        "stitch" => {
            let Some(dir) = o.inputs.first().cloned() else { bail!("{}\n\n{help}", t!("indiquez le dossier des captures.", "give the screenshots folder.")) };
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .with_context(|| tf!("lecture de {}", "reading {}", dir.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "png" | "jpg" | "jpeg"))
                })
                .collect();
            files.sort();
            if files.is_empty() {
                bail!("{}", tf!("aucune image dans {}", "no image in {}", dir.display()));
            }
            let mut st = Stitcher::new();
            for f in &files {
                st.push(image::open(f)?.to_rgb8());
            }
            let page = st.finish();
            eprintln!(
                "{} of {} frames used, page {} px high, {} fixed element(s), {} gap(s)",
                st.frames_used,
                st.frames_seen,
                page.height(),
                st.overlays().len(),
                st.gaps
            );
            if let Some(p) = &o.image {
                page.save(p)?;
            }
            let mut eng = engine(&cfg)?;
            emit(&mut eng, &cfg, &page, &[], &o, &dir.to_string_lossy())?;
        }
        "record" => {
            let Some(z) = o.zones.first().copied() else {
                bail!("{}\n\n{help}", t!("indiquez la zone : --zone X,Y,L,H", "give the area: --zone X,Y,W,H"))
            };
            let page = record_screen(&o, z)?;
            if let Some(p) = &o.image {
                page.save(p)?;
            }
            if !o.no_note {
                let mut eng = engine(&cfg)?;
                emit(&mut eng, &cfg, &page, &[], &o, "recording")?;
            }
        }
        _ => bail!("{}\n\n{help}", tf!("commande inconnue : {cmd}", "unknown command: {cmd}")),
    }
    Ok(())
}

fn record_screen(o: &Opts, z: Rect) -> Result<image::RgbImage> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    let (x, y) = (z.x0 as i32, z.y0 as i32);
    let mon = xcap::Monitor::from_point(x + 1, y + 1).context("screen")?;
    let origin = (mon.x().unwrap_or(0), mon.y().unwrap_or(0));
    let region = ((x - origin.0) as u32, (y - origin.1) as u32, z.w() as u32, z.h() as u32);
    let opts = crate::scrolling::Options {
        fps: o.fps.unwrap_or(8),
        auto_step: o.auto.then(|| o.step.unwrap_or(crate::scrolling::AUTO_STEP)),
        ..Default::default()
    };
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(crate::scrolling::Status::default()));
    let t = std::time::Instant::now();
    let h = crate::scrolling::spawn(origin, region, opts, stop.clone(), status.clone())?;
    let max = std::time::Duration::from_secs_f32(o.duration.unwrap_or(600.0));
    let s2 = stop.clone();
    std::thread::spawn(move || {
        let mut l = String::new();
        if std::io::stdin().read_line(&mut l).is_ok_and(|n| n > 0) {
            s2.store(true, Ordering::Relaxed);
        }
    });
    while !h.is_finished() {
        if t.elapsed() >= max {
            stop.store(true, Ordering::Relaxed);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (st, end) = h.join().map_err(|_| anyhow::anyhow!("recording thread"))?;
    let page = st.finish();
    let s = status.lock().map(|s| s.clone()).unwrap_or_default();
    eprintln!(
        "(recording {} ms: {} of {} frames, {} gap(s), page {} px high, end {:?}{})",
        t.elapsed().as_millis(),
        st.frames_used,
        st.frames_seen,
        st.gaps,
        page.height(),
        end,
        if s.auto_failed { ", mouse wheel had no effect" } else { "" }
    );
    Ok(page)
}
