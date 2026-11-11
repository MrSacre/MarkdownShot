use std::sync::Mutex;

use anyhow::{Result, anyhow};
use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;

use crate::style::{Rgb, color_dist};

const MODEL: &[u8] = include_bytes!("../assets/glyphs.onnx");
const W: usize = 24;
const H: usize = 32;
const TOP: f32 = 1.9;
const BOTTOM: f32 = 0.7;
const MIN_INK: f32 = 12.0;
pub const CLASSES: usize = 95;

static NET: Mutex<Option<Session>> = Mutex::new(None);

pub fn load() -> Result<()> {
    let mut net = NET.lock().map_err(|_| anyhow!("glyph model lock"))?;
    if net.is_none() {
        *net = Some(crate::runtime::load_memory(MODEL)?);
    }
    Ok(())
}

#[derive(Clone)]
pub struct Cell {
    pub best: char,
    pub p: f32,
    probs: Vec<f32>,
}

impl Cell {
    pub fn prob(&self, c: char) -> f32 {
        let i = c as usize;
        if (32..127).contains(&i) { self.probs[i - 32] } else { 0.0 }
    }

    pub fn best_of(&self, set: &str) -> (char, f32) {
        set.chars().map(|c| (c, self.prob(c))).fold((' ', -1.0), |a, b| if b.1 > a.1 { b } else { a })
    }
}

fn sample(img: &RgbImage, bg: Rgb, x: f32, y: f32) -> f32 {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xx: i64, yy: i64| -> f32 {
        if xx < 0 || yy < 0 || xx >= iw || yy >= ih { 0.0 } else { color_dist(img.get_pixel(xx as u32, yy as u32).0, bg) }
    };
    let (xi, yi) = (x0 as i64, y0 as i64);
    let a = at(xi, yi) * (1.0 - fx) + at(xi + 1, yi) * fx;
    let b = at(xi, yi + 1) * (1.0 - fx) + at(xi + 1, yi + 1) * fx;
    a * (1.0 - fy) + b * fy
}

pub fn read(img: &RgbImage, bg: Rgb, centers: &[f32], baseline: f32, pitch: f32) -> Vec<Option<Cell>> {
    let mut out: Vec<Option<Cell>> = vec![None; centers.len()];
    let mut data: Vec<f32> = Vec::new();
    let mut index: Vec<usize> = Vec::new();
    let mut patch = [0f32; W * H];
    for (n, &cx) in centers.iter().enumerate() {
        let mut max = 0f32;
        for v in 0..H {
            let y = baseline - TOP * pitch + (v as f32 + 0.5) / H as f32 * (TOP + BOTTOM) * pitch - 0.5;
            for u in 0..W {
                let x = cx - pitch + (u as f32 + 0.5) / W as f32 * 2.0 * pitch - 0.5;
                let d = sample(img, bg, x, y);
                patch[v * W + u] = d;
                max = max.max(d);
            }
        }
        if max < MIN_INK {
            continue;
        }
        data.extend(patch.iter().map(|d| ((d / max) * 255.0).round() / 255.0));
        index.push(n);
    }
    if index.is_empty() {
        return out;
    }
    let Ok(mut net) = NET.lock() else { return out };
    let Some(sess) = net.as_mut() else { return out };
    let Ok(input) = Array4::from_shape_vec((index.len(), 1, H, W), data) else { return out };
    let Ok(tensor) = Tensor::from_array(input) else { return out };
    let Ok(outputs) = sess.run(ort::inputs![tensor]) else { return out };
    let Ok((_, logits)) = outputs[0].try_extract_tensor::<f32>() else { return out };
    for (j, &n) in index.iter().enumerate() {
        let row = &logits[j * CLASSES..(j + 1) * CLASSES];
        let m = row.iter().cloned().fold(f32::MIN, f32::max);
        let exp: Vec<f32> = row.iter().map(|v| (v - m).exp()).collect();
        let sum: f32 = exp.iter().sum();
        let probs: Vec<f32> = exp.iter().map(|v| v / sum).collect();
        let (k, p) = probs.iter().cloned().enumerate().fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        out[n] = Some(Cell { best: (k as u8 + 32) as char, p, probs });
    }
    out
}
