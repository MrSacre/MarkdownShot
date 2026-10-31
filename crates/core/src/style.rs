use image::RgbImage;

use crate::geom::{Rect, median};
use crate::ocr::TextLine;

pub type Rgb = [u8; 3];

pub fn color_dist(a: Rgb, b: Rgb) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

pub fn luma(p: Rgb) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

pub fn dominant_color(img: &RgbImage) -> Rgb {
    let mut hist = vec![0u32; 1 << 18];
    let mut sums = vec![[0u64; 3]; 1 << 18];
    let (w, h) = img.dimensions();
    let step = if (w as u64 * h as u64) > 2_000_000 { 3 } else { 2 };
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let p = img.get_pixel(x, y).0;
            let k = ((p[0] as usize >> 2) << 12) | ((p[1] as usize >> 2) << 6) | (p[2] as usize >> 2);
            hist[k] += 1;
            for c in 0..3 {
                sums[k][c] += p[c] as u64;
            }
        }
    }
    let (k, &n) = hist.iter().enumerate().max_by_key(|(_, n)| **n).unwrap();
    if n == 0 {
        return [255, 255, 255];
    }
    [(sums[k][0] / n as u64) as u8, (sums[k][1] / n as u64) as u8, (sums[k][2] / n as u64) as u8]
}

pub fn page_color(img: &RgbImage) -> Rgb {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return dominant_color(img);
    }
    let mode = |pts: &mut dyn Iterator<Item = [u8; 3]>| -> Option<(Rgb, f32)> {
        let mut hist: std::collections::HashMap<u32, (u32, [u64; 3])> = std::collections::HashMap::new();
        let mut total = 0u32;
        for p in pts {
            let k = ((p[0] as u32 >> 2) << 12) | ((p[1] as u32 >> 2) << 6) | (p[2] as u32 >> 2);
            let e = hist.entry(k).or_insert((0, [0; 3]));
            e.0 += 1;
            for c in 0..3 {
                e.1[c] += p[c] as u64;
            }
            total += 1;
        }
        let &(n, sum) = hist.values().max_by_key(|e| e.0)?;
        let c = [(sum[0] / n as u64) as u8, (sum[1] / n as u64) as u8, (sum[2] / n as u64) as u8];
        Some((c, n as f32 / total.max(1) as f32))
    };
    let sides = [
        mode(&mut (0..3u32).flat_map(|b| (0..w).step_by(2).map(move |x| (x, b))).map(|(x, y)| img.get_pixel(x, y).0)),
        mode(&mut (0..3u32).flat_map(|b| (0..w).step_by(2).map(move |x| (x, h - 1 - b))).map(|(x, y)| img.get_pixel(x, y).0)),
        mode(&mut (0..3u32).flat_map(|b| (0..h).step_by(2).map(move |y| (b, y))).map(|(x, y)| img.get_pixel(x, y).0)),
        mode(&mut (0..3u32).flat_map(|b| (0..h).step_by(2).map(move |y| (w - 1 - b, y))).map(|(x, y)| img.get_pixel(x, y).0)),
    ];
    let mut votes: Vec<(Rgb, u32)> = Vec::new();
    for (c, share) in sides.into_iter().flatten() {
        if share < 0.6 {
            continue;
        }
        match votes.iter_mut().find(|v| color_dist(v.0, c) <= 10.0) {
            Some(v) => v.1 += 1,
            None => votes.push((c, 1)),
        }
    }
    let Some(best) = votes.iter().map(|v| v.1).max() else { return dominant_color(img) };
    let tied: Vec<Rgb> = votes.iter().filter(|v| v.1 == best).map(|v| v.0).collect();
    if tied.len() == 1 {
        return tied[0];
    }
    let dom = dominant_color(img);
    let share = |c: Rgb| {
        let (mut n, mut t) = (0u32, 0u32);
        for y in (0..h).step_by(7) {
            for x in (0..w).step_by(7) {
                t += 1;
                if color_dist(img.get_pixel(x, y).0, c) <= 10.0 {
                    n += 1;
                }
            }
        }
        n as f32 / t.max(1) as f32
    };
    if tied.iter().any(|c| color_dist(*c, dom) <= 10.0) {
        return dom;
    }
    tied.into_iter().max_by(|a, b| share(*a).partial_cmp(&share(*b)).unwrap()).unwrap()
}

#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub x0: f32,
    pub x1: f32,
    pub weight: f32,
    pub cov: f32,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Debug, Clone)]
pub struct LineStyle {
    pub xh: f32,
    pub ink_x0: f32,
    pub ink_x1: f32,
    pub ink_y0: f32,
    pub ink_y1: f32,
    pub bg: Rgb,
    pub fg: Rgb,
    pub words: Vec<Word>,
    pub weight: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub ocr: TextLine,
    pub st: LineStyle,
}

impl Line {
    pub fn rect(&self) -> Rect {
        self.ocr.rect
    }
    pub fn text(&self) -> &str {
        self.ocr.text.trim()
    }
    pub fn ink(&self) -> Rect {
        Rect::new(self.st.ink_x0, self.st.ink_y0, self.st.ink_x1, self.st.ink_y1)
    }
}

struct InkMap {
    x0: u32,
    w: u32,
    h: u32,
    v: Vec<f32>,
    bg: Rgb,
    fg: Rgb,
}

impl InkMap {
    fn new(img: &RgbImage, r: &Rect) -> Option<InkMap> {
        let (iw, ih) = img.dimensions();
        let (x0, y0, w, h) = r.to_px(iw, ih);
        if w < 3 || h < 3 {
            return None;
        }
        let mut border: Vec<Rgb> = Vec::with_capacity(2 * (w + h) as usize);
        for x in x0..x0 + w {
            border.push(img.get_pixel(x, y0).0);
            border.push(img.get_pixel(x, y0 + h - 1).0);
        }
        for y in y0..y0 + h {
            border.push(img.get_pixel(x0, y).0);
            border.push(img.get_pixel(x0 + w - 1, y).0);
        }
        let mut chans: [Vec<f32>; 3] = Default::default();
        for p in &border {
            for c in 0..3 {
                chans[c].push(p[c] as f32);
            }
        }
        let bg = [median(&mut chans[0]) as u8, median(&mut chans[1]) as u8, median(&mut chans[2]) as u8];
        let mut dists: Vec<(f32, Rgb)> = Vec::with_capacity((w * h) as usize);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let p = img.get_pixel(x, y).0;
                dists.push((color_dist(p, bg), p));
            }
        }
        let mut sorted: Vec<f32> = dists.iter().map(|d| d.0).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let contrast = sorted[((sorted.len() - 1) as f32 * 0.98) as usize].max(1.0);
        let fg = dists.iter().filter(|d| d.0 >= contrast * 0.9).map(|d| d.1).next().unwrap_or([0, 0, 0]);
        let v = dists.iter().map(|d| (d.0 / contrast).min(1.0)).collect();
        Some(InkMap { x0, w, h, v, bg, fg })
    }

    fn at(&self, x: u32, y: u32) -> f32 {
        self.v[(y * self.w + x) as usize]
    }
}

pub fn analyze_line(img: &RgbImage, line: &TextLine) -> LineStyle {
    let r = line.rect;
    let fallback = LineStyle {
        xh: r.h() * 0.4,
        ink_x0: r.x0,
        ink_x1: r.x1,
        ink_y0: r.y0,
        ink_y1: r.y1,
        bg: [255, 255, 255],
        fg: [0, 0, 0],
        words: vec![],
        weight: 0.0,
        pitch: r.h() * 0.5,
    };
    let Some(m) = InkMap::new(img, &r) else { return fallback };

    let mut rows = vec![0f32; m.h as usize];
    let mut cols = vec![0f32; m.w as usize];
    for y in 0..m.h {
        for x in 0..m.w {
            let v = m.at(x, y);
            if v > 0.5 {
                rows[y as usize] += 1.0;
                cols[x as usize] += 1.0;
            }
        }
    }
    let maxr = rows.iter().cloned().fold(0.0, f32::max);
    if maxr <= 0.0 {
        return fallback;
    }
    let core_rows = rows.iter().filter(|&&v| v >= maxr * 0.3).count() as f32;
    let first_row = rows.iter().position(|&v| v > 0.0).unwrap_or(0) as f32;
    let last_row = rows.iter().rposition(|&v| v > 0.0).unwrap_or(m.h as usize - 1) as f32;
    let first_col = cols.iter().position(|&v| v > 0.0).unwrap_or(0) as f32;
    let last_col = cols.iter().rposition(|&v| v > 0.0).unwrap_or(m.w as usize - 1) as f32;

    let letters: Vec<char> = line.text.chars().filter(|c| c.is_alphabetic()).collect();
    let lower = letters.iter().filter(|c| c.is_lowercase()).count();
    let mut xh = core_rows.max(2.0);
    if letters.is_empty() || (lower as f32) < letters.len() as f32 * 0.5 {
        xh /= 1.4;
    }

    let (iw, ih) = img.dimensions();
    let (_, py0, _, _) = r.to_px(iw, ih);
    let (ya, yb) = (first_row as u32, (last_row as u32 + 1).min(m.h));
    let mut pads: Vec<(f32, f32)> = Vec::new();
    if yb > ya + 2 {
        let full = |x: u32| {
            let n = (ya..yb).filter(|&y| color_dist(img.get_pixel(m.x0 + x, py0 + y).0, m.bg) > 18.0).count();
            n as f32 >= (yb - ya) as f32 * 0.95
        };
        let mut x = 0u32;
        while x < m.w {
            if !full(x) {
                x += 1;
                continue;
            }
            let a = x;
            while x < m.w && full(x) {
                x += 1;
            }
            if (x - a) as f32 >= xh * 1.5 && ((x - a) as f32) < m.w as f32 * 0.9 {
                pads.push(((m.x0 + a) as f32, (m.x0 + x) as f32));
            }
        }
    }
    let in_pad = |x: f32| pads.iter().any(|&(a, b)| x >= a && x <= b);

    let chars = &line.chars;
    let pitch =
        if chars.len() >= 2 { (chars[chars.len() - 1].x - chars[0].x) / (chars.len() - 1) as f32 } else { xh }.max(1.0);
    let mut words: Vec<Word> = Vec::new();
    let mut cur = String::new();
    let mut cur_code = false;
    let mut wx0 = 0f32;
    let mut wx1 = 0f32;
    let push = |words: &mut Vec<Word>, cur: &mut String, x0: f32, x1: f32, code: bool| {
        if !cur.is_empty() {
            words.push(Word { text: std::mem::take(cur), x0, x1, weight: 0.0, cov: 0.0, bold: false, italic: false, code });
        }
    };
    for (i, c) in chars.iter().enumerate() {
        if c.ch == ' ' {
            let prev = chars[..i].iter().rev().find(|d| d.ch != ' ');
            let next = chars[i + 1..].iter().find(|d| d.ch != ' ');
            if let (Some(p), Some(n)) = (prev, next) {
                if in_pad(p.x) && in_pad(n.x) && n.x - p.x < pitch * 1.6 {
                    continue;
                }
            }
            push(&mut words, &mut cur, wx0, wx1, cur_code);
            continue;
        }
        let code = in_pad(c.x);
        if !cur.is_empty() && code != cur_code {
            push(&mut words, &mut cur, wx0, wx1, cur_code);
        }
        if cur.is_empty() {
            wx0 = if i == 0 { r.x0 + first_col } else { c.x - pitch * 0.5 };
            cur_code = code;
        }
        cur.push(c.ch);
        wx1 = c.x + pitch * 0.5;
    }
    let last_x1 = (r.x0 + last_col + 1.0).max(wx1);
    push(&mut words, &mut cur, wx0, last_x1, cur_code);

    let y_a = first_row as u32;
    let y_b = (last_row as u32 + 1).min(m.h);
    let mut word_xh: Vec<f32> = Vec::new();
    for w in words.iter_mut() {
        let a = ((w.x0 - m.x0 as f32).max(0.0) as u32).min(m.w);
        let b = ((w.x1 - m.x0 as f32).max(0.0) as u32).min(m.w);
        if w.code || b <= a + 1 || y_b <= y_a + 1 {
            continue;
        }
        let mut wrows = vec![0f32; (y_b - y_a) as usize];
        let mut full = 0u32;
        for y in y_a..y_b {
            for x in a..b {
                if m.at(x, y) > 0.5 {
                    full += 1;
                    wrows[(y - y_a) as usize] += 1.0;
                }
            }
        }
        let fill = full as f32 / ((b - a) * (y_b - y_a)) as f32;
        if fill > 0.55 {
            w.code = true;
            continue;
        }
        let letters: Vec<char> = w.text.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() >= 2 {
            let mx = wrows.iter().cloned().fold(0.0, f32::max);
            let mut core = wrows.iter().filter(|&&v| v >= mx * 0.3).count() as f32;
            if letters.iter().filter(|c| c.is_lowercase()).count() * 2 < letters.len() {
                core /= 1.4;
            }
            word_xh.push(core.max(2.0));
        }
    }
    if word_xh.len() >= 2 {
        xh = median(&mut word_xh);
    }

    let mut all_w = Vec::new();
    for w in words.iter_mut() {
        let a = ((w.x0 - m.x0 as f32).max(0.0) as u32).min(m.w);
        let b = ((w.x1 - m.x0 as f32).max(0.0) as u32).min(m.w);
        if b <= a + 1 || w.code {
            continue;
        }
        let mut runs: Vec<f32> = Vec::new();
        let mut coverage = 0.0f32;
        for y in y_a..y_b {
            let mut run = 0.0f32;
            for x in a..b {
                let v = m.at(x, y);
                coverage += v;
                if v > 0.35 {
                    run += v.min(1.0);
                } else if run > 0.0 {
                    runs.push(run);
                    run = 0.0;
                }
            }
            if run > 0.0 {
                runs.push(run);
            }
        }
        if runs.len() < 3 {
            continue;
        }
        let stroke = median(&mut runs);
        let cov = coverage / ((b - a) as f32 * xh.max(1.0));
        if w.text.chars().filter(|c| c.is_alphabetic()).count() >= 3 && y_b > y_a + 4 {
            let yc = (y_a + y_b) as f32 / 2.0;
            let mut pts: Vec<(f32, f32)> = Vec::new();
            for y in y_a..y_b {
                for x in a..b {
                    if m.at(x, y) > 0.5 {
                        pts.push((x as f32, y as f32 - yc));
                    }
                }
            }
            if pts.len() >= 20 {
                let score = |sh: f32| -> f32 {
                    let mut hist = vec![0f32; (b - a) as usize + 40];
                    for &(x, dy) in &pts {
                        let k = (x + sh * dy - a as f32 + 20.0).round();
                        if k >= 0.0 && (k as usize) < hist.len() {
                            hist[k as usize] += 1.0;
                        }
                    }
                    hist.iter().map(|v| v * v).sum::<f32>()
                };
                let s0 = score(0.0);
                let (best, sb) = (1..=8).map(|k| k as f32 * 0.05).map(|sh| (sh, score(sh))).fold((0.0, s0), |acc, v| if v.1 > acc.1 { v } else { acc });
                let back = score(-0.15);
                w.italic = best >= 0.15 && sb >= s0 * 1.12 && sb >= back * 1.2;
            }
        }
        w.weight = stroke / xh.max(1.0);
        w.cov = cov;
        all_w.push(w.weight);
    }
    let weight = if all_w.is_empty() { 0.0 } else { median(&mut all_w) };

    LineStyle {
        xh,
        ink_x0: r.x0 + first_col,
        ink_x1: r.x0 + last_col + 1.0,
        ink_y0: r.y0 + first_row,
        ink_y1: r.y0 + last_row + 1.0,
        bg: m.bg,
        fg: m.fg,
        words,
        weight,
        pitch,
    }
}

pub fn has_bullet_blob(img: &RgbImage, line: &Line, page_bg: Rgb) -> bool {
    bullet_blob_x(img, line, page_bg).is_some()
}

pub fn bullet_blob_x(img: &RgbImage, line: &Line, page_bg: Rgb) -> Option<f32> {
    let xh = line.st.xh.max(4.0);
    let ink = line.ink();
    let first_char = line.ocr.chars.iter().find(|c| !c.ch.is_whitespace()).map(|c| c.x);
    let (x_a, x_b) = match first_char {
        Some(cx) if cx - xh * 0.5 - ink.x0 > xh * 0.8 => {
            ((ink.x0 - 1.0).max(0.0) as u32, (cx - xh * 0.6).max(0.0) as u32)
        }
        _ => ((ink.x0 - xh * 3.2).max(0.0) as u32, (ink.x0 - xh * 0.25).max(0.0) as u32),
    };
    let base = if line.text().chars().any(|c| "gjpqy".contains(c)) { ink.y1 - xh * 0.45 } else { ink.y1 };
    let y_a = (base - xh * 1.6).max(0.0) as u32;
    let y_b = ((base + xh * 0.2) as u32).min(img.height());
    if x_b <= x_a + 1 || y_b <= y_a + 1 {
        return None;
    }
    let bg = line.st.bg;
    let _ = page_bg;
    let inked = |x: u32| (y_a..y_b).any(|y| color_dist(img.get_pixel(x, y).0, line.st.bg) > 60.0);
    let mut x_end = x_b.min(img.width());
    if let Some(start) = (x_a..x_end).find(|&x| inked(x)) {
        if let Some(stop) = (start..x_end).find(|&x| !inked(x)) {
            x_end = stop;
        }
    }
    let (mut bx0, mut by0, mut bx1, mut by1, mut n) = (u32::MAX, u32::MAX, 0u32, 0u32, 0u32);
    for y in y_a..y_b {
        for x in x_a..x_end {
            if color_dist(img.get_pixel(x, y).0, bg) > 60.0 {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x);
                by1 = by1.max(y);
                n += 1;
            }
        }
    }
    if n == 0 {
        return None;
    }
    let bw = (bx1 - bx0 + 1) as f32;
    let bh = (by1 - by0 + 1) as f32;
    let fill = n as f32 / (bw * bh);
    let cy = (by0 + by1) as f32 * 0.5;
    let arrow = bw >= xh * 0.5
        && bw <= xh * 1.8
        && bh >= xh * 0.25
        && bh <= xh * 1.1
        && bw / bh >= 1.1
        && bw / bh <= 4.0
        && fill > 0.2
        && cy > base - xh * 1.0
        && cy < base;
    let dot = bw >= xh * 0.2
        && bw <= xh * 0.95
        && bh >= xh * 0.2
        && bh <= xh * 0.95
        && (bw / bh) > 0.6
        && (bw / bh) < 1.6
        && fill > 0.3
        && cy > base - xh * 1.1
        && cy < base + xh * 0.1;
    (arrow || dot).then_some(bx0 as f32)
}

pub fn has_quote_bar(img: &RgbImage, line: &Line) -> bool {
    let ink = line.ink();
    let h = ink.h().max(4.0);
    let x_a = (ink.x0 - 48.0).max(0.0) as u32;
    let x_b = (ink.x0 - 4.0).max(0.0) as u32;
    let y_a = ink.y0.max(0.0) as u32;
    let y_b = (ink.y1 as u32).min(img.height());
    if x_b <= x_a || y_b <= y_a + 2 {
        return false;
    }
    let bg = line.st.bg;
    let mut best_run = 0u32;
    let mut best_end = 0u32;
    let mut run = 0u32;
    for x in x_a..x_b.min(img.width()) {
        let mut n = 0u32;
        let ref_px = img.get_pixel(x, (y_a + y_b) / 2).0;
        for y in y_a..y_b {
            let p = img.get_pixel(x, y).0;
            if color_dist(p, bg) > 25.0 && color_dist(p, ref_px) < 20.0 {
                n += 1;
            }
        }
        if n as f32 >= (y_b - y_a) as f32 * 0.95 {
            run += 1;
            if run > best_run {
                best_run = run;
                best_end = x;
            }
        } else {
            run = 0;
        }
    }
    if !(best_run >= 2 && (best_run as f32) <= h * 0.5 && best_run <= 10) {
        return false;
    }
    let gap: Vec<u32> = (best_end + 1..(ink.x0.max(0.0) as u32).min(img.width())).collect();
    let clear = gap
        .iter()
        .filter(|&&x| {
            let n = (y_a..y_b).filter(|&y| color_dist(img.get_pixel(x, y).0, bg) > 25.0).count();
            (n as f32) <= (y_b - y_a) as f32 * 0.1
        })
        .count();
    clear >= 4
}
