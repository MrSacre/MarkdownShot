use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use image::{RgbImage, imageops};

use crate::figures::{self, FigSource, Figure};
use crate::geom::Rect;
use crate::layout::Layout;
use crate::markdown::{self, Block};
use crate::ocr::{Ocr, Quality};
use crate::style::{self, Line, Rgb};

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub use_layout: bool,
    pub auto_images: bool,
    pub quality: Quality,
    pub full_width_images: bool,
    pub threads: usize,
    pub gpu: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            use_layout: true,
            auto_images: true,
            quality: Quality::Balanced,
            full_width_images: true,
            threads: crate::runtime::default_threads(),
            gpu: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    Image,
    Text,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    pub rect: Rect,
    pub kind: ZoneKind,
}

impl Zone {
    pub fn image(rect: Rect) -> Zone {
        Zone { rect, kind: ZoneKind::Image }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub ocr_ms: u128,
    pub layout_ms: u128,
    pub build_ms: u128,
    pub lines: usize,
}

pub struct Document {
    pub blocks: Vec<Block>,
    pub images: Vec<RgbImage>,
    pub figures: Vec<Figure>,
    pub title: String,
    pub stats: Stats,
}

impl Document {
    pub fn markdown(&self, link: &dyn Fn(usize) -> String) -> String {
        markdown::render(&self.blocks, link)
    }
}

pub struct Analysis {
    pub image: Arc<RgbImage>,
    pub page_bg: Rgb,
    lines: Vec<Line>,
    pub panels: Vec<crate::panels::Panel>,
    pub suggestions: Vec<Figure>,
    pub stats: Stats,
}

pub struct Engine {
    ocr: Ocr,
    layout: Option<Layout>,
    opts: EngineOptions,
}

impl Engine {
    pub fn new(models_dir: &Path, opts: EngineOptions) -> Result<Self> {
        crate::runtime::set_gpu(opts.gpu);
        let ocr = Ocr::load(models_dir, opts.quality, opts.threads)?;
        let layout_path = models_dir.join("layout.onnx");
        let layout = if opts.use_layout && layout_path.is_file() {
            match Layout::load(&layout_path, opts.threads) {
                Ok(l) => Some(l),
                Err(e) => {
                    log::warn!("layout model skipped: {e:#}");
                    None
                }
            }
        } else {
            None
        };
        Ok(Self { ocr, layout, opts })
    }

    pub fn analyze(&mut self, img: RgbImage) -> Result<Analysis> {
        self.analyze_shared(Arc::new(img))
    }

    pub fn analyze_shared(&mut self, img: Arc<RgbImage>) -> Result<Analysis> {
        let mut stats = Stats::default();
        let t = Instant::now();
        let prof = std::env::var_os("MARKDOWNSHOT_PROFILE").is_some();
        let mut tp = Instant::now();
        let mut lap = |what: &str| {
            if prof {
                eprintln!("  profile {what:<14} {:>6} ms", tp.elapsed().as_millis());
            }
            tp = Instant::now();
        };
        let page_bg = style::page_color(&img);
        let boxes = self.ocr.detect(&img)?;
        lap("detection");
        let mut ocr_lines = self.ocr.recognize(&img, &boxes)?;
        lap("recognition");
        let first_panels = crate::panels::find(&img, page_bg, &ocr_lines);
        lap("panels 1");
        let missed: Vec<crate::geom::Rect> = first_panels
            .iter()
            .filter(|p| crate::panels::is_code_block(p, &ocr_lines))
            .flat_map(|p| crate::panels::unread_bands(&img, p, &ocr_lines))
            .collect();
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!("  bands to re-read: {missed:?}");
        }
        if !missed.is_empty() {
            let mut extra = self.ocr.recognize(&img, &missed)?;
            let ascii = |t: &str| t.chars().any(|c| c.is_ascii_graphic());
            extra.retain(|l| ascii(&l.text));
            let mut lh: Vec<f32> = ocr_lines.iter().filter(|l| l.text.chars().count() >= 8).map(|l| l.rect.h()).collect();
            let lh = if lh.is_empty() { 0.0 } else { crate::geom::median(&mut lh) };
            let again: Vec<crate::geom::Rect> = missed
                .iter()
                .filter(|m| lh > 0.0 && m.h() <= lh * 1.4 && m.w() <= m.h() * 3.0)
                .filter(|m| !extra.iter().any(|l| l.rect.inter_area(m) > 0.0))
                .map(|m| {
                    let h = m.h();
                    crate::geom::Rect::new(m.x0 - h * 0.8, m.y0, m.x1 + h * 0.8, m.y1).clamp(img.width() as f32, img.height() as f32)
                })
                .collect();
            if !again.is_empty() {
                extra.extend(self.ocr.recognize_ascii(&img, &again)?.into_iter().filter(|l| !l.text.trim().is_empty()));
            }
            if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                for l in &extra {
                    eprintln!("  re-read in panel {:?} \"{}\"", l.rect, l.text);
                }
            }
            ocr_lines.extend(extra);
            ocr_lines.sort_by(|a, b| {
                a.rect.y0.partial_cmp(&b.rect.y0).unwrap().then(a.rect.x0.partial_cmp(&b.rect.x0).unwrap())
            });
        }
        lap("re-read");
        let code_panels: Vec<crate::panels::Panel> = crate::panels::find(&img, page_bg, &ocr_lines)
            .into_iter()
            .filter(|p| crate::panels::is_code_block(p, &ocr_lines))
            .collect();
        lap("panels 2");
        let prefixes: Vec<(usize, crate::geom::Rect)> =
            code_panels.iter().flat_map(|p| crate::panels::unread_prefixes(&img, p, &ocr_lines)).collect();
        if !prefixes.is_empty() {
            let boxes: Vec<crate::geom::Rect> = prefixes.iter().map(|(_, r)| *r).collect();
            let reread = self.ocr.recognize(&img, &boxes)?;
            for nl in reread {
                let Some(&(i, _)) = prefixes.iter().find(|(_, r)| r.y0 == nl.rect.y0 && (r.x1 - nl.rect.x1).abs() < 1.5) else {
                    continue;
                };
                let old = ocr_lines[i].text.split_whitespace().collect::<String>();
                let new = nl.text.split_whitespace().collect::<String>();
                if new.len() > old.len() && new.ends_with(&old) {
                    if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                        eprintln!("  line start re-read \"{}\" -> \"{}\"", ocr_lines[i].text, nl.text);
                    }
                    ocr_lines[i] = nl;
                }
            }
        }
        lap("code line starts");
        stats.ocr_ms = t.elapsed().as_millis();
        stats.lines = ocr_lines.len();

        let t = Instant::now();
        let regions = match (&mut self.layout, self.opts.auto_images) {
            (Some(l), true) => l.detect(&img).unwrap_or_else(|e| {
                log::warn!("layout: {e:#}");
                Vec::new()
            }),
            _ => Vec::new(),
        };
        stats.layout_ms = t.elapsed().as_millis();
        lap("layout");

        let panels = crate::panels::find(&img, page_bg, &ocr_lines);
        let gutters = crate::panels::absorb_gutters(&panels, &mut ocr_lines);
        lap("panels 3");
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            for p in &panels {
                eprintln!("  panel {:?} background {:?}{}", p.rect, p.bg, if p.cell { " (cell)" } else { "" });
            }
        }
        let suggestions = if self.opts.auto_images {
            let figs = figures::detect(&img, page_bg, &ocr_lines, &regions, &panels, &[], &gutters, true);
            let figs: Vec<Figure> = figs.into_iter().filter(|f| !gutters.iter().any(|g| f.rect.covered_by(g) > 0.7)).collect();
            let mut figs = if self.opts.full_width_images { figures::widen(&img, page_bg, figs, &ocr_lines, &panels) } else { figs };
            for r in crate::formulas::find(&img, page_bg, &ocr_lines, &panels) {
                let r = r.clamp(img.width() as f32, img.height() as f32);
                let r = figures::content_bbox_t(&img, page_bg, 30.0, &r)
                    .map(|c| c.expand(1.0).clamp(img.width() as f32, img.height() as f32))
                    .unwrap_or(r);
                if !figs.iter().any(|f| f.rect.inter_area(&r) > 0.0) {
                    figs.push(Figure { rect: r, source: FigSource::Formula });
                }
            }
            figs
        } else {
            Vec::new()
        };
        lap("images");
        let lines = ocr_lines
            .into_iter()
            .map(|l| {
                let st = style::analyze_line(&img, &l);
                Line { ocr: l, st }
            })
            .collect();
        lap("styles");
        Ok(Analysis { image: img, page_bg, lines, panels, suggestions, stats })
    }

    pub fn process(&mut self, img: &RgbImage, zones: &[Rect]) -> Result<Document> {
        let a = self.analyze(img.clone())?;
        let zones: Vec<Zone> = zones.iter().map(|r| Zone::image(*r)).collect();
        Ok(a.build(&zones, Mode::Auto))
    }
}

impl Analysis {
    pub fn figures(&self, zones: &[Zone], mode: Mode) -> Vec<Figure> {
        let (iw, ih) = self.image.dimensions();
        let (iw, ih) = (iw as f32, ih as f32);
        let manual: Vec<Rect> = zones
            .iter()
            .filter(|z| z.kind == ZoneKind::Image)
            .map(|z| z.rect.clamp(iw, ih))
            .filter(|r| r.w() >= 4.0 && r.h() >= 4.0)
            .collect();
        let blockers: Vec<Rect> = zones.iter().filter(|z| z.kind != ZoneKind::Image).map(|z| z.rect).collect();
        let mut figs: Vec<Figure> = manual.iter().map(|r| Figure { rect: *r, source: FigSource::Manual }).collect();
        if mode == Mode::Auto {
            for s in &self.suggestions {
                let r = s.rect;
                if manual.iter().any(|m| m.iou(&r) > 0.2 || r.covered_by(m) > 0.5) {
                    continue;
                }
                if blockers.iter().any(|b| b.contains_point(r.cx(), r.cy()) || r.covered_by(b) > 0.5) {
                    continue;
                }
                figs.push(s.clone());
            }
        }
        if let Some(cut) = zones
            .iter()
            .filter(|z| z.kind == ZoneKind::Ignore && z.rect.w() >= iw * 0.95 && z.rect.y1 >= ih - 1.0)
            .map(|z| z.rect.y0)
            .reduce(f32::min)
        {
            for f in figs.iter_mut() {
                f.rect.y1 = f.rect.y1.min(cut);
            }
            figs.retain(|f| f.rect.h() >= 8.0);
        }
        figs.sort_by(|a, b| a.rect.y0.partial_cmp(&b.rect.y0).unwrap());
        figs
    }

    pub fn build(&self, zones: &[Zone], mode: Mode) -> Document {
        let t = Instant::now();
        let img = &*self.image;
        let figs = self.figures(zones, mode);
        let ignored: Vec<Rect> = zones.iter().filter(|z| z.kind == ZoneKind::Ignore).map(|z| z.rect).collect();
        let lines: Vec<Line> = self
            .lines
            .iter()
            .filter(|l| !figs.iter().any(|f| l.ocr.rect.covered_by(&f.rect) > 0.5))
            .filter(|l| !ignored.iter().any(|z| z.contains_point(l.ocr.rect.cx(), l.ocr.rect.cy())))
            .cloned()
            .collect();
        let blocks = markdown::build_blocks(img, &lines, &figs, &self.panels, self.page_bg);
        let images = figs
            .iter()
            .map(|f| {
                let (x, y, w, h) = f.rect.to_px(img.width(), img.height());
                imageops::crop_imm(img, x, y, w.max(1), h.max(1)).to_image()
            })
            .collect();
        let title = title_of(&blocks);
        let mut stats = self.stats.clone();
        stats.build_ms = t.elapsed().as_millis();
        Document { blocks, images, figures: figs, title, stats }
    }
}

fn title_of(blocks: &[Block]) -> String {
    let pick = blocks
        .iter()
        .find_map(|b| match b {
            Block::Heading { text, .. } => Some(text.clone()),
            _ => None,
        })
        .or_else(|| {
            blocks.iter().find_map(|b| match b {
                Block::Paragraph { text } | Block::ListItem { text, .. } | Block::Quote { text } => Some(text.clone()),
                _ => None,
            })
        })
        .unwrap_or_else(|| "Capture".to_string());
    let clean: String = pick.replace("**", "").replace('\\', "");
    let words: Vec<&str> = clean.split_whitespace().collect();
    let mut t = String::new();
    for w in words {
        if t.chars().count() + w.chars().count() > 70 {
            break;
        }
        if !t.is_empty() {
            t.push(' ');
        }
        t.push_str(w);
    }
    if t.is_empty() { "Capture".into() } else { t }
}
