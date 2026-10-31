use std::path::Path;

use anyhow::{Result, anyhow};
use image::{RgbImage, imageops};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;

use crate::geom::Rect;
use crate::runtime;

#[derive(Debug, Clone)]
pub struct Region {
    pub rect: Rect,
    pub label: String,
    pub score: f32,
}

pub struct Layout {
    session: Session,
    labels: Vec<String>,
    in_w: u32,
    in_h: u32,
}

impl Layout {
    pub fn load(path: &Path, threads: usize) -> Result<Self> {
        let session = runtime::load_session(path, threads)?;
        let meta = session.metadata().map_err(|e| anyhow!("{e}"))?;
        let labels: Vec<String> = meta
            .custom("character")
            .unwrap_or_default()
            .split('\n')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        drop(meta);
        let shape =
            session.inputs()[0].dtype().tensor_shape().map(|s| s.to_vec()).unwrap_or_else(|| vec![1, 3, 800, 608]);
        let in_h = if shape.len() == 4 && shape[2] > 0 { shape[2] as u32 } else { 800 };
        let in_w = if shape.len() == 4 && shape[3] > 0 { shape[3] as u32 } else { 608 };
        Ok(Self { session, labels, in_w, in_h })
    }

    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Region>> {
        let (w, h) = img.dimensions();
        let tile_h = ((w as f32) * self.in_h as f32 / self.in_w as f32 * 1.15) as u32;
        let tile_h = tile_h.max(400);
        if h <= tile_h + tile_h / 4 {
            return self.detect_tile(img, 0);
        }
        let overlap = tile_h / 3;
        let mut all = Vec::new();
        let mut y = 0;
        loop {
            let th = tile_h.min(h - y);
            let tile = imageops::crop_imm(img, 0, y, w, th).to_image();
            let last = y + th >= h;
            for mut r in self.detect_tile(&tile, y)? {
                let touches_top = y > 0 && r.rect.y0 <= y as f32 + 2.0;
                let touches_bottom = !last && r.rect.y1 >= (y + th) as f32 - 2.0;
                if touches_top || touches_bottom {
                    r.score *= 0.5;
                }
                all.push(r);
            }
            if last {
                break;
            }
            y += tile_h - overlap;
        }
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        let mut keep: Vec<Region> = Vec::new();
        for r in all {
            if r.score < 0.3 {
                continue;
            }
            if keep
                .iter()
                .any(|k| k.label == r.label && (k.rect.iou(&r.rect) > 0.4 || r.rect.covered_by(&k.rect) > 0.8))
            {
                continue;
            }
            keep.push(r);
        }
        Ok(keep)
    }

    fn detect_tile(&mut self, img: &RgbImage, y_off: u32) -> Result<Vec<Region>> {
        let (w, h) = img.dimensions();
        let resized = imageops::resize(img, self.in_w, self.in_h, imageops::FilterType::Triangle);
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        let mut input = Array4::<f32>::zeros((1, 3, self.in_h as usize, self.in_w as usize));
        for (x, y, p) in resized.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (p[2 - c] as f32 / 255.0 - mean[c]) / std[c];
            }
        }
        let outputs = self
            .session
            .run(ort::inputs![Tensor::from_array(input).map_err(|e| anyhow!("{e}"))?])
            .map_err(|e| anyhow!("layout: {e}"))?;
        let n_out = outputs.len();
        if n_out % 2 != 0 || n_out == 0 {
            return Err(anyhow!("unexpected layout model output format"));
        }
        let levels = n_out / 2;
        let strides = [8f32, 16.0, 32.0, 64.0];
        let mut cands: Vec<(Rect, usize, f32)> = Vec::new();
        for l in 0..levels {
            let (sshape, scores) = outputs[l].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
            let (_, boxes) = outputs[l + levels].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
            let n = sshape[1] as usize;
            let n_cls = sshape[2] as usize;
            let reg = 8usize;
            let stride = strides[l.min(3)];
            let fm_w = (self.in_w as f32 / stride).ceil() as usize;
            for i in 0..n {
                let srow = &scores[i * n_cls..(i + 1) * n_cls];
                let (mut cls, mut best) = (0usize, 0f32);
                for (k, &v) in srow.iter().enumerate() {
                    if v > best {
                        best = v;
                        cls = k;
                    }
                }
                if best < 0.4 {
                    continue;
                }
                let cx = ((i % fm_w) as f32 + 0.5) * stride;
                let cy = ((i / fm_w) as f32 + 0.5) * stride;
                let brow = &boxes[i * 4 * reg..(i + 1) * 4 * reg];
                let mut dist = [0f32; 4];
                for (s, d) in dist.iter_mut().enumerate() {
                    let seg = &brow[s * reg..(s + 1) * reg];
                    let m = seg.iter().cloned().fold(f32::MIN, f32::max);
                    let mut den = 0.0;
                    let mut num = 0.0;
                    for (k, &v) in seg.iter().enumerate() {
                        let e = (v - m).exp();
                        den += e;
                        num += e * k as f32;
                    }
                    *d = num / den * stride;
                }
                let r = Rect::new(cx - dist[0], cy - dist[1], cx + dist[2], cy + dist[3]);
                cands.push((r, cls, best));
            }
        }
        cands.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
        let mut kept: Vec<(Rect, usize, f32)> = Vec::new();
        for c in cands {
            if kept.iter().filter(|k| k.1 == c.1).any(|k| k.0.iou(&c.0) > 0.5) {
                continue;
            }
            kept.push(c);
            if kept.len() >= 200 {
                break;
            }
        }
        let sx = w as f32 / self.in_w as f32;
        let sy = h as f32 / self.in_h as f32;
        Ok(kept
            .into_iter()
            .map(|(r, cls, score)| Region {
                rect: Rect::new(r.x0 * sx, r.y0 * sy + y_off as f32, r.x1 * sx, r.y1 * sy + y_off as f32)
                    .clamp(w as f32, (h + y_off) as f32),
                label: self.labels.get(cls).cloned().unwrap_or_else(|| format!("class{cls}")),
                score,
            })
            .collect())
    }
}
