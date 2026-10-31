use std::path::Path;

use anyhow::{Result, anyhow};
use image::{RgbImage, imageops};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;

use crate::geom::Rect;
use crate::runtime;

#[derive(Debug, Clone)]
pub struct CharPos {
    pub ch: char,
    pub x: f32,
    pub p: f32,
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub rect: Rect,
    pub text: String,
    pub conf: f32,
    pub chars: Vec<CharPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Fast,
    Balanced,
    Accurate,
}

pub struct Ocr {
    det: Session,
    rec: Session,
    charset: Vec<String>,
    rec_path: std::path::PathBuf,
    pool: Vec<Session>,
    workers: usize,
    ascii: Vec<bool>,
    ascii_only: bool,
}

struct Item {
    idx: usize,
    rect: Rect,
    crop: RgbImage,
    width: u32,
}

const PAR_MIN: usize = 24;

const DET_TILE: u32 = 2048;
const DET_OVERLAP: u32 = 256;
const REC_H: u32 = 48;

impl Ocr {
    pub fn load(dir: &Path, quality: Quality, threads: usize) -> Result<Self> {
        let tiny_det = dir.join("det_tiny.onnx").is_file();
        let tiny_rec = dir.join("rec_tiny.onnx").is_file();
        let det_f = if quality != Quality::Accurate && tiny_det { "det_tiny.onnx" } else { "det.onnx" };
        let (rec_f, dict_f) = if quality == Quality::Fast && tiny_rec {
            ("rec_tiny.onnx", "rec_tiny_dict.txt")
        } else {
            ("rec.onnx", "rec_dict.txt")
        };
        let det = runtime::load_session(&dir.join(det_f), threads)?;
        let rec = runtime::load_session(&dir.join(rec_f), threads)?;
        let meta = rec.metadata().map_err(|e| anyhow!("{e}"))?;
        let chars = match meta.custom("character") {
            Some(c) => c,
            None => std::fs::read_to_string(dir.join(dict_f))
                .map_err(|_| anyhow!("{rec_f}: character dictionary not found ({dict_f})"))?,
        };
        drop(meta);
        let mut charset: Vec<String> = vec![String::new()];
        charset.extend(
            chars
                .split('\n')
                .map(|s| s.strip_suffix('\r').unwrap_or(s))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        );
        charset.push(" ".to_string());
        let workers = std::env::var("MARKDOWNSHOT_REC_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| (runtime::default_threads() / 2).clamp(1, 4))
            .clamp(1, 16);
        let workers = if runtime::gpu_active() { 1 } else { workers };
        let ascii = charset.iter().enumerate().map(|(k, c)| k == 0 || (!c.is_empty() && c.chars().all(|ch| ch.is_ascii_graphic() || ch == ' '))).collect();
        Ok(Self { det, rec, charset, rec_path: dir.join(rec_f), pool: Vec::new(), workers, ascii, ascii_only: false })
    }

    pub fn run(&mut self, img: &RgbImage) -> Result<Vec<TextLine>> {
        let boxes = self.detect(img)?;
        self.recognize(img, &boxes)
    }

    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Rect>> {
        let (w, h) = img.dimensions();
        let mut out = Vec::new();
        if h <= DET_TILE + DET_OVERLAP {
            out = self.detect_tile(img, 0)?;
        } else {
            let step = DET_TILE - DET_OVERLAP;
            let mut y = 0u32;
            loop {
                let th = DET_TILE.min(h - y);
                let tile = imageops::crop_imm(img, 0, y, w, th).to_image();
                let own0 = if y == 0 { 0.0 } else { (y + DET_OVERLAP / 2) as f32 };
                let last = y + th >= h;
                let own1 = if last { h as f32 } else { (y + th - DET_OVERLAP / 2) as f32 };
                for r in self.detect_tile(&tile, y)? {
                    if r.cy() >= own0 && r.cy() < own1 {
                        out.push(r);
                    }
                }
                if last {
                    break;
                }
                y += step;
            }
        }
        let mut out = merge_row_fragments(out);
        out.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap().then(a.x0.partial_cmp(&b.x0).unwrap()));
        Ok(out)
    }

    fn detect_tile(&mut self, img: &RgbImage, y_off: u32) -> Result<Vec<Rect>> {
        let (w, h) = img.dimensions();
        let maxs = w.max(h) as f32;
        let mut ratio = if maxs > 4000.0 { 4000.0 / maxs } else { 1.0 };
        if (w.min(h) as f32) * ratio < 64.0 {
            ratio = 64.0 / w.min(h) as f32;
        }
        let rw = (((w as f32 * ratio) / 32.0).round() as u32 * 32).max(32);
        let rh = (((h as f32 * ratio) / 32.0).round() as u32 * 32).max(32);
        let resized = if rw == w && rh == h {
            img.clone()
        } else {
            imageops::resize(img, rw, rh, imageops::FilterType::Triangle)
        };
        let mut input = Array4::<f32>::zeros((1, 3, rh as usize, rw as usize));
        for (x, y, p) in resized.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = p[2 - c] as f32 / 127.5 - 1.0;
            }
        }
        let outputs = self
            .det
            .run(ort::inputs![Tensor::from_array(input).map_err(|e| anyhow!("{e}"))?])
            .map_err(|e| anyhow!("detection: {e}"))?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
        let mh = shape[2] as usize;
        let mw = shape[3] as usize;
        let sx = w as f32 / mw as f32;
        let sy = h as f32 / mh as f32;

        let thresh = 0.3f32;
        let mut bin = vec![false; mw * mh];
        for y in 0..mh {
            for x in 0..mw {
                let v = data[y * mw + x] > thresh
                    || (x > 0 && data[y * mw + x - 1] > thresh)
                    || (y > 0 && data[(y - 1) * mw + x] > thresh)
                    || (x > 0 && y > 0 && data[(y - 1) * mw + x - 1] > thresh);
                bin[y * mw + x] = v;
            }
        }
        let mut seen = vec![false; mw * mh];
        let mut stack = Vec::new();
        let mut rects = Vec::new();
        for start in 0..mw * mh {
            if !bin[start] || seen[start] {
                continue;
            }
            seen[start] = true;
            stack.push(start);
            let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
            let mut sum = 0f32;
            let mut n = 0usize;
            while let Some(i) = stack.pop() {
                let (x, y) = (i % mw, i / mw);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                sum += data[i];
                n += 1;
                let mut push = |j: usize| {
                    if bin[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                };
                if x > 0 {
                    push(i - 1);
                }
                if x + 1 < mw {
                    push(i + 1);
                }
                if y > 0 {
                    push(i - mw);
                }
                if y + 1 < mh {
                    push(i + mw);
                }
            }
            let bw = (x1 - x0 + 1) as f32;
            let bh = (y1 - y0 + 1) as f32;
            if n < 4 || bw.min(bh) < 2.0 {
                continue;
            }
            if sum / (n as f32) < 0.5 {
                continue;
            }
            let d = bw * bh * 1.6 / (2.0 * (bw + bh));
            let r = Rect::new(
                (x0 as f32 - d) * sx,
                (y0 as f32 - d) * sy,
                (x1 as f32 + 1.0 + d) * sx,
                (y1 as f32 + 1.0 + d) * sy,
            )
            .clamp(w as f32, h as f32);
            if r.h() < 5.0 || r.w() < 4.0 {
                continue;
            }
            rects.push(r.translate(0.0, y_off as f32));
        }
        Ok(rects)
    }

    pub fn recognize_ascii(&mut self, img: &RgbImage, boxes: &[Rect]) -> Result<Vec<TextLine>> {
        self.ascii_only = true;
        let r = self.recognize(img, boxes);
        self.ascii_only = false;
        r
    }

    pub fn recognize(&mut self, img: &RgbImage, boxes: &[Rect]) -> Result<Vec<TextLine>> {
        let (iw, ih) = img.dimensions();
        let mut items: Vec<Item> = Vec::new();
        for (idx, r) in boxes.iter().enumerate() {
            let (x, y, w, h) = r.to_px(iw, ih);
            if w < 2 || h < 2 {
                continue;
            }
            let rect = Rect::new(x as f32, y as f32, (x + w) as f32, (y + h) as f32);
            let crop = imageops::crop_imm(img, x, y, w, h).to_image();
            let width = ((REC_H as f32 * w as f32 / h as f32).ceil() as u32).clamp(16, 3200);
            items.push(Item { idx, rect, crop, width });
        }
        items.sort_by_key(|it| it.width);
        let gpu = runtime::gpu_active();
        let batch = std::env::var("MARKDOWNSHOT_REC_BATCH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(if gpu { 16 } else { 1usize })
            .max(1);

        let mut results: Vec<Option<TextLine>> = vec![None; boxes.len()];
        if self.workers > 1 && items.len() >= PAR_MIN {
            while self.pool.len() < self.workers {
                self.pool.push(runtime::load_session(&self.rec_path, 1)?);
            }
            let n = self.workers;
            let chunks: Vec<Vec<&Item>> = (0..n).map(|k| items.iter().skip(k).step_by(n).collect()).collect();
            let charset = &self.charset;
            let mask = self.ascii_only.then_some(&self.ascii[..]);
            let outs: Vec<Result<Vec<(usize, TextLine)>>> = std::thread::scope(|sc| {
                let hs: Vec<_> = self
                    .pool
                    .iter_mut()
                    .zip(chunks)
                    .map(|(sess, chunk)| sc.spawn(move || read_items(sess, charset, mask, &chunk, batch, 8)))
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow!("recognition: thread interrupted")))).collect()
            });
            for o in outs {
                for (idx, l) in o? {
                    results[idx] = Some(l);
                }
            }
        } else {
            let refs: Vec<&Item> = items.iter().collect();
            let mask = self.ascii_only.then_some(&self.ascii[..]);
            for (idx, l) in read_items(&mut self.rec, &self.charset, mask, &refs, batch, if gpu { 64 } else { 8 })? {
                results[idx] = Some(l);
            }
        }
        Ok(results.into_iter().flatten().collect())
    }
}

fn read_items(
    sess: &mut Session,
    charset: &[String],
    mask: Option<&[bool]>,
    items: &[&Item],
    batch: usize,
    round: u32,
) -> Result<Vec<(usize, TextLine)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let mut j = i + 1;
        while j < items.len() && j - i < batch && items[j].width as f32 <= items[i].width as f32 * 1.6 + 32.0 {
            j += 1;
        }
        let group = &items[i..j];
        let pad_w = group.iter().map(|it| it.width).max().unwrap();
        let pad_w = pad_w.div_ceil(round) * round;
        let mut input = Array4::<f32>::zeros((group.len(), 3, REC_H as usize, pad_w as usize));
        for (b, it) in group.iter().enumerate() {
            let r = imageops::resize(&it.crop, it.width, REC_H, imageops::FilterType::Triangle);
            for (x, y, p) in r.enumerate_pixels() {
                for c in 0..3 {
                    input[[b, c, y as usize, x as usize]] = p[2 - c] as f32 / 127.5 - 1.0;
                }
            }
        }
        let outputs = sess
            .run(ort::inputs![Tensor::from_array(input).map_err(|e| anyhow!("{e}"))?])
            .map_err(|e| anyhow!("recognition: {e}"))?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
        let t_len = shape[1] as usize;
        let n_cls = shape[2] as usize;
        let step = pad_w as f32 / t_len as f32;
        for (b, it) in group.iter().enumerate() {
            let mut text = String::new();
            let mut chars = Vec::new();
            let mut confs = Vec::new();
            let mut prev = 0usize;
            let scale = it.rect.w() / it.width as f32;
            for t in 0..t_len {
                let row = &data[(b * t_len + t) * n_cls..(b * t_len + t + 1) * n_cls];
                let (mut best, mut bv) = (0usize, f32::MIN);
                for (k, &v) in row.iter().enumerate() {
                    if mask.is_some_and(|m| !m.get(k).copied().unwrap_or(false)) {
                        continue;
                    }
                    if v > bv {
                        bv = v;
                        best = k;
                    }
                }
                if best != 0 && best != prev {
                    let x_res = (t as f32 + 0.5) * step;
                    if x_res <= it.width as f32 + step {
                        let s = charset.get(best).map(String::as_str).unwrap_or("");
                        for ch in s.chars() {
                            text.push(ch);
                            chars.push(CharPos { ch, x: it.rect.x0 + x_res * scale, p: bv });
                        }
                        confs.push(bv);
                    }
                }
                prev = best;
            }
            let conf = if confs.is_empty() { 0.0 } else { confs.iter().sum::<f32>() / confs.len() as f32 };
            if text.trim().is_empty() || conf < if mask.is_some() { 0.3 } else { 0.5 } {
                continue;
            }
            out.push((it.idx, TextLine { rect: it.rect, text, conf, chars }));
        }
        i = j;
    }
    Ok(out)
}

pub(crate) fn merge_row_fragments(mut boxes: Vec<Rect>) -> Vec<Rect> {
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                let h = a.h().min(b.h());
                let v = a.y1.min(b.y1) - a.y0.max(b.y0);
                let x = a.x1.min(b.x1) - a.x0.max(b.x0);
                let ratio = a.h().max(b.h()) / h.max(1.0);
                let icon = |r: &Rect| r.w() < r.h() * 1.4;
                let (left, right) = if a.x0 <= b.x0 { (a, b) } else { (b, a) };
                let leading = left.w() < right.h() * 1.8 && left.x1 < right.x1;
                if v >= 0.6 * h && ratio <= 1.5 && x >= -0.15 * h && !icon(&a) && !icon(&b) && !leading {
                    boxes[i] = a.union(&b);
                    boxes.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return boxes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_pieces_merged() {
        let b = vec![Rect::new(21.0, 203.0, 68.0, 228.0), Rect::new(62.0, 202.0, 204.0, 230.0), Rect::new(188.0, 203.0, 725.0, 229.0)];
        let m = merge_row_fragments(b);
        assert_eq!(m.len(), 2);
        assert!(m.contains(&Rect::new(62.0, 202.0, 725.0, 230.0)));
    }

    #[test]
    fn icon_before_heading_kept_apart() {
        let b = vec![Rect::new(273.0, 1641.0, 300.0, 1664.0), Rect::new(287.0, 1638.0, 490.0, 1667.0)];
        assert_eq!(merge_row_fragments(b).len(), 2);
        let b = vec![Rect::new(270.0, 1642.0, 334.0, 1669.0), Rect::new(327.0, 1643.0, 368.0, 1668.0), Rect::new(356.0, 1643.0, 786.0, 1668.0)];
        assert_eq!(merge_row_fragments(b).len(), 1);
        let b = vec![Rect::new(30.0, 614.0, 101.0, 640.0), Rect::new(103.0, 615.0, 169.0, 639.0)];
        assert_eq!(merge_row_fragments(b).len(), 1);
        let b = vec![Rect::new(330.0, 900.0, 352.0, 922.0), Rect::new(348.0, 896.0, 900.0, 926.0)];
        assert_eq!(merge_row_fragments(b).len(), 2);
    }

    #[test]
    fn adjacent_columns_and_lines_split() {
        let b = vec![Rect::new(10.0, 10.0, 120.0, 34.0), Rect::new(140.0, 10.0, 260.0, 34.0), Rect::new(10.0, 30.0, 250.0, 54.0)];
        assert_eq!(merge_row_fragments(b).len(), 3);
    }
}
