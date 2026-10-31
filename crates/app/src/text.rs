use std::cell::RefCell;
use std::collections::HashMap;

static FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/LiberationSans-Bold.ttf");

pub struct Font {
    font: fontdue::Font,
    cache: RefCell<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>,
}

impl Font {
    pub fn new() -> Font {
        let font =
            fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default()).expect("valid embedded font");
        Font { font, cache: RefCell::new(HashMap::new()) }
    }

    fn glyph(&self, c: char, size: f32) -> (fontdue::Metrics, Vec<u8>) {
        let key = (c, (size * 10.0) as u32);
        if let Some(g) = self.cache.borrow().get(&key) {
            return g.clone();
        }
        let g = self.font.rasterize(c, size);
        self.cache.borrow_mut().insert(key, g.clone());
        g
    }

    pub fn measure(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.glyph(c, size).0.advance_width).sum()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        buf: &mut [u32],
        bw: usize,
        bh: usize,
        x: f32,
        y: f32,
        size: f32,
        color: u32,
        text: &str,
    ) -> f32 {
        let (cr, cg, cb) = ((color >> 16) & 255, (color >> 8) & 255, color & 255);
        let mut pen = x;
        for c in text.chars() {
            let (m, bmp) = self.glyph(c, size);
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (y - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                let py = gy + row as i32;
                if py < 0 || py as usize >= bh {
                    continue;
                }
                for col in 0..m.width {
                    let px = gx + col as i32;
                    if px < 0 || px as usize >= bw {
                        continue;
                    }
                    let a = bmp[row * m.width + col] as u32;
                    if a == 0 {
                        continue;
                    }
                    let i = py as usize * bw + px as usize;
                    let d = buf[i];
                    let (dr, dg, db) = ((d >> 16) & 255, (d >> 8) & 255, d & 255);
                    let r = (cr * a + dr * (255 - a)) / 255;
                    let g = (cg * a + dg * (255 - a)) / 255;
                    let b = (cb * a + db * (255 - a)) / 255;
                    buf[i] = (r << 16) | (g << 8) | b;
                }
            }
            pen += m.advance_width;
        }
        pen - x
    }
}

pub fn fill(buf: &mut [u32], bw: usize, bh: usize, x0: i32, y0: i32, x1: i32, y1: i32, color: u32, alpha: u32) {
    let (cr, cg, cb) = ((color >> 16) & 255, (color >> 8) & 255, color & 255);
    for y in y0.max(0)..y1.min(bh as i32) {
        let row = y as usize * bw;
        for x in x0.max(0)..x1.min(bw as i32) {
            let i = row + x as usize;
            if alpha >= 255 {
                buf[i] = color;
            } else {
                let d = buf[i];
                let (dr, dg, db) = ((d >> 16) & 255, (d >> 8) & 255, d & 255);
                buf[i] = (((cr * alpha + dr * (255 - alpha)) / 255) << 16)
                    | (((cg * alpha + dg * (255 - alpha)) / 255) << 8)
                    | ((cb * alpha + db * (255 - alpha)) / 255);
            }
        }
    }
}

pub fn frame(buf: &mut [u32], bw: usize, bh: usize, x0: i32, y0: i32, x1: i32, y1: i32, t: i32, color: u32) {
    fill(buf, bw, bh, x0 - t, y0 - t, x1 + t, y0, color, 255);
    fill(buf, bw, bh, x0 - t, y1, x1 + t, y1 + t, color, 255);
    fill(buf, bw, bh, x0 - t, y0, x0, y1, color, 255);
    fill(buf, bw, bh, x1, y0, x1 + t, y1, color, 255);
}
