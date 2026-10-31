use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::engine::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    WebP,
    Jpeg,
}

impl ImageFormat {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "webp" => Self::WebP,
            "jpeg" | "jpg" => Self::Jpeg,
            _ => Self::Png,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub format: ImageFormat,
    pub quality: u8,
    pub max_width: u32,
    pub dedupe: bool,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self { format: ImageFormat::Png, quality: 90, max_width: 0, dedupe: true }
    }
}

pub fn encode(img: &image::RgbImage, o: &ImageOptions) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let resized;
    let img = if o.max_width > 0 && img.width() > o.max_width {
        let h = ((img.height() as u64 * o.max_width as u64) / img.width() as u64).max(1) as u32;
        resized = image::imageops::resize(img, o.max_width, h, image::imageops::FilterType::CatmullRom);
        &resized
    } else {
        img
    };
    let mut buf = Vec::new();
    let (w, h) = img.dimensions();
    match o.format {
        ImageFormat::Png => image::codecs::png::PngEncoder::new_with_quality(
            &mut buf,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8)?,
        ImageFormat::WebP => buf.extend_from_slice(&webp::Encoder::from_rgb(img.as_raw(), w, h).encode_lossless()),
        ImageFormat::Jpeg => image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, o.quality.clamp(1, 100))
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8)?,
    }
    Ok(buf)
}

fn same_picture(a: &image::RgbImage, b: &image::RgbImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    let n = a.as_raw().len().max(1);
    let off = a.as_raw().iter().zip(b.as_raw()).filter(|(x, y)| x.abs_diff(**y) > 12).count();
    off * 1000 <= n
}

fn existing_copy(dir: &Path, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    let len = bytes.len() as u64;
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
            && p.metadata().is_ok_and(|m| m.len() == len)
            && std::fs::read(p).is_ok_and(|b| b == bytes)
    })
}

#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub notes_dir: PathBuf,
    pub images_dir: PathBuf,
    pub wikilinks: bool,
    pub filename: String,
    pub front_matter: bool,
    pub tags: Vec<String>,
    pub images: ImageOptions,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            images_dir: PathBuf::from("images"),
            wikilinks: true,
            filename: "{date} {time} - {title}".into(),
            front_matter: true,
            tags: vec!["capture".into()],
            images: ImageOptions::default(),
        }
    }
}

pub struct Written {
    pub note: PathBuf,
    pub images: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
    pub markdown: String,
}

fn sanitize(s: &str) -> String {
    let mut o: String =
        s.chars().map(|c| if "\\/:*?\"<>|#^[]".contains(c) || c.is_control() { ' ' } else { c }).collect();
    o = o.split_whitespace().collect::<Vec<_>>().join(" ");
    o.trim_end_matches(['.', ' ']).to_string()
}

fn unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for i in 2..1000 {
        let p = dir.join(format!("{stem} ({i}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    path
}

fn relative(from: &Path, to: &Path) -> PathBuf {
    let f: Vec<_> = from.components().collect();
    let t: Vec<_> = to.components().collect();
    let common = f.iter().zip(t.iter()).take_while(|(a, b)| a == b).count();
    let mut p = PathBuf::new();
    for _ in common..f.len() {
        p.push("..");
    }
    for c in &t[common..] {
        p.push(c.as_os_str());
    }
    p
}

pub fn write(
    doc: &Document,
    opts: &OutputOptions,
    source: Option<&str>,
    now: (String, String, String),
) -> Result<Written> {
    let (date, time, iso) = now;
    std::fs::create_dir_all(&opts.notes_dir).with_context(|| format!("creating {}", opts.notes_dir.display()))?;
    let images_dir =
        if opts.images_dir.is_absolute() { opts.images_dir.clone() } else { opts.notes_dir.join(&opts.images_dir) };

    let name = opts.filename.replace("{date}", &date).replace("{time}", &time).replace("{title}", &doc.title);
    let mut stem = sanitize(&name);
    if stem.is_empty() {
        stem = format!("{date} {time}");
    }
    let note = unique(opts.notes_dir.join(format!("{stem}.md")));
    let stem = note.file_stem().unwrap().to_string_lossy().to_string();

    let mut image_paths = Vec::new();
    if !doc.images.is_empty() {
        std::fs::create_dir_all(&images_dir).with_context(|| format!("creating {}", images_dir.display()))?;
    }
    let io = &opts.images;
    let ext = io.format.ext();
    let mut written: Vec<(usize, PathBuf)> = Vec::new();
    let mut created: Vec<PathBuf> = Vec::new();
    for (i, img) in doc.images.iter().enumerate() {
        if io.dedupe {
            if let Some((_, p)) = written.iter().find(|(j, _)| same_picture(&doc.images[*j], img)) {
                image_paths.push(p.clone());
                continue;
            }
        }
        let bytes = encode(img, io).with_context(|| format!("encoding image {}", i + 1))?;
        if let Some(p) = io.dedupe.then(|| existing_copy(&images_dir, &bytes, ext)).flatten() {
            written.push((i, p.clone()));
            image_paths.push(p);
            continue;
        }
        let p = unique(images_dir.join(format!("{stem} - {}.{ext}", written.len() + 1)));
        std::fs::write(&p, &bytes).with_context(|| format!("saving {}", p.display()))?;
        created.push(p.clone());
        written.push((i, p.clone()));
        image_paths.push(p);
    }
    let rel_dir = relative(&opts.notes_dir, &images_dir);
    let markdown = doc.markdown(&|i| {
        let Some(p) = image_paths.get(i) else { return String::new() };
        let file = p.file_name().unwrap().to_string_lossy().to_string();
        if opts.wikilinks {
            format!("![[{file}]]")
        } else {
            let rel = rel_dir.join(&file).to_string_lossy().replace('\\', "/");
            format!("![{}](<{rel}>)", i + 1)
        }
    });

    let mut content = String::new();
    if opts.front_matter {
        content.push_str("---\n");
        content.push_str(&format!("created: {iso}\n"));
        if let Some(s) = source.filter(|s| !s.trim().is_empty()) {
            content.push_str(&format!("source: \"{}\"\n", s.replace('"', "'")));
        }
        if !opts.tags.is_empty() {
            content.push_str(&format!("tags: [{}]\n", opts.tags.join(", ")));
        }
        content.push_str("---\n\n");
    }
    content.push_str(&markdown);
    std::fs::write(&note, &content).with_context(|| format!("writing {}", note.display()))?;
    let mut images: Vec<PathBuf> = Vec::new();
    for p in image_paths {
        if !images.contains(&p) {
            images.push(p);
        }
    }
    Ok(Written { note, images, created, markdown })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Document, Stats};
    use crate::markdown::Block;

    fn doc(images: Vec<image::RgbImage>) -> Document {
        let blocks = (0..images.len()).map(|index| Block::Image { index }).collect();
        Document { blocks, images, figures: Vec::new(), title: "Essai".into(), stats: Stats::default() }
    }

    fn picture(seed: u8) -> image::RgbImage {
        image::RgbImage::from_fn(300, 120, |x, y| image::Rgb([(x as u8).wrapping_mul(seed), y as u8, seed]))
    }

    #[test]
    fn formats_duplicates_and_resize() {
        let dir = std::env::temp_dir().join(format!("markdownshot-essai-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut o = OutputOptions { notes_dir: dir.clone(), front_matter: false, ..Default::default() };
        o.images = ImageOptions { format: ImageFormat::WebP, quality: 90, max_width: 200, dedupe: true };
        let now = ("2026-10-29".to_string(), "10-00".to_string(), "2026-10-29T10:00".to_string());
        let w = write(&doc(vec![picture(3), picture(7), picture(3)]), &o, None, now.clone()).unwrap();
        assert_eq!(w.images.len(), 2);
        assert_eq!(w.created.len(), 2);
        assert!(w.images.iter().all(|p| p.extension().unwrap() == "webp"));
        let links: Vec<&str> = w.markdown.lines().filter(|l| l.starts_with("![[")).collect();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0], links[2]);
        let bytes = std::fs::read(&w.images[0]).unwrap();
        let back = webp::Decoder::new(&bytes).decode().unwrap();
        assert_eq!((back.width(), back.height()), (200, 80));
        let w2 = write(&doc(vec![picture(7)]), &o, None, now).unwrap();
        assert!(w2.created.is_empty());
        assert_eq!(w2.images, vec![w.images[1].clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
