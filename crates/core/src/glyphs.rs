use image::RgbImage;

use crate::geom::median;
use crate::style::{Rgb, color_dist};

const INK: f32 = 70.0;

#[derive(Clone, Copy)]
pub struct Grid {
    pub x0: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy)]
pub struct Row {
    pub y0: f32,
    pub y1: f32,
    pub top: f32,
    pub base: f32,
}

struct Ink {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    n: u32,
}

fn window(g: Grid, cx: f32, iw: u32) -> (u32, u32) {
    let a = (cx - g.pitch * 0.5 + 0.6).max(0.0).round() as u32;
    let b = ((cx + g.pitch * 0.5 - 0.6).max(0.0).round() as u32).min(iw);
    (a, b)
}

fn ink(img: &RgbImage, bg: Rgb, g: Grid, k: usize, r: Row) -> Option<Ink> {
    let cx = g.x0 + (k as f32 + 0.5) * g.pitch;
    let (a, b) = window(g, cx, img.width());
    let (y0, y1) = (r.y0.max(0.0) as u32, (r.y1.max(0.0) as u32).min(img.height()));
    let (mut bx0, mut by0, mut bx1, mut by1, mut n) = (u32::MAX, u32::MAX, 0, 0, 0);
    for y in y0..y1 {
        for x in a..b {
            if color_dist(img.get_pixel(x, y).0, bg) > INK {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
                n += 1;
            }
        }
    }
    (n >= 3).then_some(Ink { x0: bx0, y0: by0, x1: bx1, y1: by1, n })
}

pub fn inked(img: &RgbImage, bg: Rgb, g: Grid, k: usize, r: Row) -> bool {
    ink(img, bg, g, k, r).is_some()
}

pub fn row_metrics(img: &RgbImage, bg: Rgb, g: Grid, text: &[char], y0: f32, y1: f32) -> Option<(f32, f32)> {
    let probe = Row { y0, y1, top: y0, base: y1 };
    let (mut tops, mut bases) = (Vec::new(), Vec::new());
    for (k, c) in text.iter().enumerate() {
        if !c.is_alphanumeric() || *c == 'A' {
            continue;
        }
        let Some(i) = ink(img, bg, g, k, probe) else { continue };
        if c.is_ascii_uppercase() || c.is_ascii_digit() || "bdfhklt".contains(*c) {
            tops.push(i.y0 as f32);
        }
        if "acemnorsuvwxzACEMNORSUVWXZ0123456789".contains(*c) {
            bases.push(i.y1 as f32);
        }
    }
    if tops.is_empty() || bases.len() < 2 {
        return None;
    }
    let top = median(&mut tops);
    let base = median(&mut bases);
    (base - top >= 4.0).then_some((top, base))
}

pub fn classify(img: &RgbImage, bg: Rgb, g: Grid, k: usize, r: Row) -> Option<char> {
    let hh = r.base - r.top;
    if hh < 4.0 {
        return None;
    }
    let r = Row { y0: r.y0.max(r.top - hh * 0.3), y1: r.y1.min(r.base + hh * 0.55), ..r };
    let i = ink(img, bg, g, k, r)?;
    let (w, h) = ((i.x1 - i.x0) as f32 / g.pitch, (i.y1 - i.y0) as f32 / hh);
    let (t, bot) = ((i.y0 as f32 - r.top) / hh, (i.y1 as f32 - r.top) / hh);
    let fill = i.n as f32 / ((i.x1 - i.x0) * (i.y1 - i.y0)).max(1) as f32;
    let is_ink = |x: u32, y: u32| color_dist(img.get_pixel(x, y).0, bg) > INK;
    let cx = g.x0 + (k as f32 + 0.5) * g.pitch;
    let (a, b) = window(g, cx, img.width());
    let b = b.saturating_sub(1);
    let touches = |x: u32| (i.y0..i.y1).any(|y| is_ink(x, y));
    let spill = (i.x0 == a && a > 0 && touches(a - 1)) || (i.x1 > b && b + 1 < img.width() && touches(b + 1));
    if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() {
        eprintln!("    col {k}: w {w:.2} h {h:.2} top {t:.2} bottom {bot:.2} spill {spill} (y {:.0}-{:.0}, letters {:.0}-{:.0})", r.y0, r.y1, r.top, r.base);
    }
    if spill {
        return box_drawing(img, bg, g, k, r, &i);
    }
    if w <= 0.34 && t <= 0.12 && bot >= 1.08 && h >= 1.0 {
        return Some('|');
    }
    if t <= 0.22 && bot <= 0.58 && h >= 0.15 {
        if w >= 0.42 {
            return caret(img, bg, &i, g.pitch).then_some('^');
        }
        let mean_x = |ya: u32, yb: u32| -> Option<f32> {
            let mut s = 0u32;
            let mut n = 0u32;
            for y in ya..yb {
                for x in i.x0..i.x1 {
                    if is_ink(x, y) {
                        s += x;
                        n += 1;
                    }
                }
            }
            (n > 0).then(|| s as f32 / n as f32)
        };
        let mid = (i.y0 + i.y1) / 2;
        if let (Some(up), Some(down)) = (mean_x(i.y0, mid.max(i.y0 + 1)), mean_x(mid, i.y1)) {
            if down - up >= g.pitch * 0.04 {
                return Some('`');
            }
        }
        return (h <= 0.32 && w <= 0.4).then_some('`');
    }
    if let Some(c) = box_drawing(img, bg, g, k, r, &i) {
        return Some(c);
    }
    if h <= 0.2 && w >= 0.35 && fill >= 0.6 {
        let cy = (t + bot) / 2.0;
        if (0.45..=0.85).contains(&cy) && h <= w * 0.6 {
            return Some('-');
        }
        if (0.95..=1.3).contains(&bot) && w >= 0.6 {
            return Some('_');
        }
    }
    None
}

pub fn classify_alone(img: &RgbImage, bg: Rgb, g: Grid, k: usize, r: Row, hh: f32) -> Option<char> {
    let i = ink(img, bg, g, k, r)?;
    if hh < 4.0 {
        return None;
    }
    let (w, h) = ((i.x1 - i.x0) as f32 / g.pitch, (i.y1 - i.y0) as f32 / hh);
    if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() {
        eprintln!("    lone col {k}: w {w:.2} h {h:.2} (y {:.0}-{:.0}, ink {}-{})", r.y0, r.y1, i.y0, i.y1);
    }
    let mid = (r.y0 + r.y1) / 2.0;
    let rr = Row { y0: r.y0, y1: r.y1, top: mid - hh * 0.6, base: mid + hh * 0.4 };
    if let Some(c) = box_drawing(img, bg, g, k, rr, &i) {
        return Some(c);
    }
    if w <= 0.34 && h >= 0.8 {
        return Some('|');
    }
    if w >= 0.42 && (0.2..=0.6).contains(&h) && caret(img, bg, &i, g.pitch) {
        return Some('^');
    }
    let cy = (i.y0 + i.y1) as f32 / 2.0;
    let fill = i.n as f32 / ((i.x1 - i.x0) * (i.y1 - i.y0)).max(1) as f32;
    let dy = (cy - mid) / hh;
    let (wpx, hpx) = ((i.x1 - i.x0) as f32, (i.y1 - i.y0) as f32);
    let (a, b) = window(g, g.x0 + (k as f32 + 0.5) * g.pitch, img.width());
    let apart = i.x0 > a && i.x1 < b;
    if apart && h <= 0.45 && w <= 0.6 && dy < -0.2 {
        return Some('`');
    }
    if apart && h <= 0.3 && w <= 0.95 && fill >= 0.5 && (-0.2..=0.9).contains(&dy) {
        if (wpx >= hpx * 1.5 && w >= 0.3) || (dy < 0.18 && w >= 0.25) {
            return Some('-');
        }
        if w <= 0.45 && (wpx - hpx).abs() <= 2.0 {
            return Some('.');
        }
    }
    None
}

fn box_drawing(img: &RgbImage, bg: Rgb, g: Grid, k: usize, r: Row, i: &Ink) -> Option<char> {
    let hh = r.base - r.top;
    let is_ink = |x: u32, y: u32| color_dist(img.get_pixel(x, y).0, bg) > INK;
    let cx = g.x0 + (k as f32 + 0.5) * g.pitch;
    let (a, b) = window(g, cx, img.width());
    if b <= a + 2 {
        return None;
    }
    let w = (b - a) as f32;
    let rows: Vec<(u32, usize)> = (i.y0..i.y1).map(|y| (y, (a..b).filter(|&x| is_ink(x, y)).count())).collect();
    let (hy, hn) = rows.iter().copied().max_by_key(|v| v.1).unwrap_or((0, 0));
    let thick = rows.iter().filter(|v| v.1 as f32 >= w * 0.4).count() as f32;
    let rel = (hy as f32 - r.top) / hh;
    let horiz = hn as f32 >= w * 0.4 && thick <= (hh * 0.22).max(3.0) && (0.25..=0.85).contains(&rel);
    let near = |y: u32, x: u32| (y.saturating_sub(1)..=y + 1).any(|yy| yy >= i.y0 && yy < i.y1 && is_ink(x, yy));
    let (left, right) = if horiz { (near(hy, a), near(hy, b - 1)) } else { (false, false) };
    let cols: Vec<(u32, usize)> = (a..b).map(|x| (x, (i.y0..i.y1).filter(|&y| is_ink(x, y)).count())).collect();
    let (vx, vn) = cols.iter().copied().max_by_key(|v| v.1).unwrap_or((0, 0));
    let vwide = cols.iter().filter(|v| v.1 as f32 >= vn as f32 * 0.6).count() as f32;
    let vert = vn >= 4 && vwide <= (g.pitch * 0.35).max(2.0);
    let mid = if horiz { hy } else { ((r.top + r.base) / 2.0) as u32 };
    let col_ink = |y: u32| (vx.saturating_sub(1)..=vx + 1).any(|x| x >= a && x < b && is_ink(x, y));
    let (mut top, mut bot) = (mid, mid);
    while top > i.y0 && col_ink(top - 1) {
        top -= 1;
    }
    while bot + 1 < i.y1 && col_ink(bot + 1) {
        bot += 1;
    }
    let up = vert && (top as f32 - r.top) / hh <= -0.15 && mid as f32 - top as f32 >= hh * 0.4;
    let down = vert && (bot as f32 - r.top) / hh >= 1.2 && bot as f32 - mid as f32 >= hh * 0.4;
    let strokes = if horiz { hn as f32 * thick } else { 0.0 } + if vert { vn as f32 * vwide } else { 0.0 };
    if i.n as f32 > strokes * 1.25 + 3.0 {
        return None;
    }
    if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() && (horiz || vert) {
        eprintln!("      frame? col {k} horiz {horiz} ({left}/{right}, {rel:.2}) vert {vert} up {up} down {down} (stroke {top}-{bot}, mid {mid}, row {:.1}-{:.1}, letters {:.1}-{:.1})", r.y0, r.y1, r.top, r.base);
    }
    let c = match (up, down, horiz && left, horiz && right) {
        (true, true, false, false) if !horiz => '│',
        (true, true, false, true) => '├',
        (true, true, true, false) => '┤',
        (true, true, true, true) => '┼',
        (true, false, false, true) => '└',
        (true, false, true, false) => '┘',
        (true, false, true, true) => '┴',
        (false, true, false, true) => '┌',
        (false, true, true, false) => '┐',
        (false, true, true, true) => '┬',
        (false, false, l, rr) if horiz && !vert && (l || rr) => '─',
        _ => return None,
    };
    Some(c)
}

fn caret(img: &RgbImage, bg: Rgb, i: &Ink, pitch: f32) -> bool {
    let span = |ya: u32, yb: u32| -> f32 {
        let xs: Vec<u32> =
            (ya..yb).flat_map(|y| (i.x0..i.x1).filter(move |&x| color_dist(img.get_pixel(x, y).0, bg) > INK)).collect();
        match (xs.iter().min(), xs.iter().max()) {
            (Some(a), Some(b)) => (b - a + 1) as f32,
            _ => 0.0,
        }
    };
    let (top, bottom) = (span(i.y0, i.y0 + 1), span(i.y1 - 1, i.y1));
    if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() {
        eprintln!("      accent? top {top} bottom {bottom} pitch {pitch:.1}");
    }
    bottom >= top + pitch * 0.25
}

pub fn refine_grid(img: &RgbImage, g: Grid, rows: &[(f32, f32, Rgb)], cols: usize) -> Grid {
    let (w, h) = img.dimensions();
    let mut col_ink = vec![0u32; w as usize];
    for &(y0, y1, bg) in rows {
        let (ya, yb) = (y0.max(0.0) as u32, (y1.max(0.0) as u32).min(h));
        for x in 0..w {
            col_ink[x as usize] += (ya..yb).filter(|&y| color_dist(img.get_pixel(x, y).0, bg) > INK).count() as u32;
        }
    }
    let cost = |x0: f32, pitch: f32| -> u32 {
        (0..=cols)
            .map(|k| {
                let x = (x0 + k as f32 * pitch).round();
                if x < 0.0 || x >= w as f32 { 0 } else { col_ink[x as usize] }
            })
            .sum()
    };
    let mut best = (cost(g.x0, g.pitch), g);
    for ps in -15i32..=15 {
        let pitch = g.pitch * (1.0 + ps as f32 * 0.002);
        for step in -5i32..=5 {
            let x0 = g.x0 + step as f32 * pitch / 10.0;
            let c = cost(x0, pitch);
            if c < best.0 {
                best = (c, Grid { x0, pitch });
            }
        }
    }
    best.1
}

pub fn mono_score(img: &RgbImage, rows: &[(f32, f32, Rgb)], x0: f32, x1: f32, p0: f32, p1: f32) -> Option<(f32, Grid)> {
    let (w, h) = img.dimensions();
    let (xa, xb) = (x0.max(0.0) as usize, (x1.max(0.0) as usize).min(w as usize));
    if xb <= xa + 20 || p1 <= p0 || p0 < 3.0 {
        return None;
    }
    let mut col_ink = vec![0f32; w as usize];
    for &(y0, y1, bg) in rows {
        let (ya, yb) = (y0.max(0.0) as u32, (y1.max(0.0) as u32).min(h));
        for x in xa..xb {
            col_ink[x] += (ya..yb).filter(|&y| color_dist(img.get_pixel(x as u32, y).0, bg) > INK).count() as f32;
        }
    }
    let mean = col_ink[xa..xb].iter().sum::<f32>() / (xb - xa) as f32;
    if mean <= 0.0 {
        return None;
    }
    let mut best: Option<(f32, Grid)> = None;
    let mut pitch = p0;
    while pitch <= p1 {
        let cols = ((xb - xa) as f32 / pitch) as usize;
        if cols >= 6 {
            let mut phase = 0.0;
            while phase < pitch {
                let gx0 = xa as f32 + phase;
                let s: f32 = (0..=cols)
                    .map(|k| {
                        let x = (gx0 + k as f32 * pitch).round() as usize;
                        if x < xb { col_ink[x] } else { 0.0 }
                    })
                    .sum();
                let r = s / ((cols + 1) as f32 * mean);
                if best.is_none_or(|b| r < b.0) {
                    best = Some((r, Grid { x0: gx0, pitch }));
                }
                phase += 0.5;
            }
        }
        pitch += 0.02;
    }
    best
}

pub fn ink_columns(img: &RgbImage, bg: Rgb, g: Grid, r: Row, cols: usize) -> Vec<bool> {
    let hh = r.base - r.top;
    let r = if hh >= 4.0 { Row { y0: r.y0.max(r.top - hh * 0.3), y1: r.y1.min(r.base + hh * 0.55), ..r } } else { r };
    (0..cols).map(|k| ink(img, bg, g, k, r).is_some()).collect()
}

pub fn first_ink(img: &RgbImage, bg: Rgb, xa: f32, xb: f32, band: (f32, f32), from_left: bool) -> Option<f32> {
    let (w, h) = img.dimensions();
    let (xa, xb) = (xa.max(0.0) as u32, (xb.max(0.0) as u32).min(w));
    let (ya, yb) = (band.0.max(0.0) as u32, (band.1.max(0.0) as u32).min(h));
    let col = |x: u32| (ya..yb).any(|y| color_dist(img.get_pixel(x, y).0, bg) > INK);
    if from_left { (xa..xb).find(|&x| col(x)) } else { (xa..xb).rev().find(|&x| col(x)) }.map(|x| x as f32)
}
