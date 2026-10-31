use image::RgbImage;

use crate::geom::Rect;
use crate::layout::Region;
use crate::ocr::TextLine;
use crate::panels::{self, Panel};
use crate::style::{Rgb, color_dist};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigSource {
    Manual,
    Layout,
    Pixels,
    Formula,
}

#[derive(Debug, Clone)]
pub struct Figure {
    pub rect: Rect,
    pub source: FigSource,
}

const CELL: u32 = 4;

struct Grid {
    gw: usize,
    gh: usize,
    nonbg: Vec<f32>,
    mean: Vec<Rgb>,
    texture: Vec<f32>,
    text: Vec<bool>,
}

impl Grid {
    fn new(img: &RgbImage, bg: Rgb, lines: &[TextLine]) -> Grid {
        let (w, h) = img.dimensions();
        let gw = w.div_ceil(CELL) as usize;
        let gh = h.div_ceil(CELL) as usize;
        let mut nonbg = vec![0f32; gw * gh];
        let mut mean = vec![[0u8; 3]; gw * gh];
        let mut texture = vec![0f32; gw * gh];
        for gy in 0..gh {
            for gx in 0..gw {
                let (x0, y0) = (gx as u32 * CELL, gy as u32 * CELL);
                let (x1, y1) = ((x0 + CELL).min(w), (y0 + CELL).min(h));
                let mut n = 0f32;
                let mut cnt = 0f32;
                let mut s = [0f32; 3];
                let mut s2 = 0f32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let p = img.get_pixel(x, y).0;
                        cnt += 1.0;
                        if color_dist(p, bg) > 10.0 {
                            n += 1.0;
                        }
                        for c in 0..3 {
                            s[c] += p[c] as f32;
                        }
                        let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                        s2 += l * l;
                    }
                }
                let i = gy * gw + gx;
                nonbg[i] = n / cnt;
                let m = [s[0] / cnt, s[1] / cnt, s[2] / cnt];
                mean[i] = [m[0] as u8, m[1] as u8, m[2] as u8];
                let ml = 0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2];
                texture[i] = (s2 / cnt - ml * ml).max(0.0).sqrt();
            }
        }
        let mut text = vec![false; gw * gh];
        for l in lines {
            let r = l.rect.expand(1.0);
            let gx0 = (r.x0.max(0.0) as u32 / CELL) as usize;
            let gy0 = (r.y0.max(0.0) as u32 / CELL) as usize;
            let gx1 = ((r.x1.max(0.0) as u32).div_ceil(CELL) as usize).min(gw);
            let gy1 = ((r.y1.max(0.0) as u32).div_ceil(CELL) as usize).min(gh);
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    text[gy * gw + gx] = true;
                }
            }
        }
        Grid { gw, gh, nonbg, mean, texture, text }
    }
}

pub fn refine(img: &RgbImage, bg: Rgb, r: &Rect, lines: &[TextLine]) -> Rect {
    let (iw, ih) = img.dimensions();
    let mut margin = (r.w().min(r.h()) * 0.15).max(16.0);
    let mut out = *r;
    for _ in 0..5 {
        let (res, area) = refine_once(img, bg, r, lines, margin);
        out = res;
        let touches = (res.x0 <= area.x0 + 3.0 && area.x0 > 0.0)
            || (res.y0 <= area.y0 + 3.0 && area.y0 > 0.0)
            || (res.x1 >= area.x1 - 3.0 && area.x1 < iw as f32)
            || (res.y1 >= area.y1 - 3.0 && area.y1 < ih as f32);
        if !touches {
            break;
        }
        margin *= 2.5;
    }
    out
}

fn refine_once(img: &RgbImage, bg: Rgb, r: &Rect, lines: &[TextLine], margin: f32) -> (Rect, Rect) {
    let (iw, ih) = img.dimensions();
    let area = r.expand(margin).clamp(iw as f32, ih as f32);
    let (ax, ay, aw, ah) = area.to_px(iw, ih);
    if aw < 4 || ah < 4 {
        return (*r, area);
    }
    let (aw, ah) = (aw as usize, ah as usize);
    let mut grain: Vec<f32> = Vec::new();
    for y in (0..ah).step_by(2) {
        for x in (0..aw.saturating_sub(1)).step_by(3) {
            let p = img.get_pixel(ax + x as u32, ay + y as u32).0;
            let q = img.get_pixel(ax + x as u32 + 1, ay + y as u32).0;
            let d = color_dist(p, q);
            if d < 12.0 && color_dist(p, bg) < 24.0 {
                grain.push(d);
            }
        }
    }
    grain.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise = if grain.is_empty() { 0.0 } else { grain[(grain.len() - 1) * 9 / 10] };
    let thr = (noise * 2.0 + 2.0).clamp(4.0, 12.0);
    let mut mask = vec![false; aw * ah];
    for y in 0..ah {
        for x in 0..aw {
            let p = img.get_pixel(ax + x as u32, ay + y as u32).0;
            mask[y * aw + x] = color_dist(p, bg) > thr;
        }
    }
    let mut dil = vec![false; aw * ah];
    for y in 0..ah {
        for x in 0..aw {
            if !mask[y * aw + x] {
                continue;
            }
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < aw && (ny as usize) < ah {
                        dil[ny as usize * aw + nx as usize] = true;
                    }
                }
            }
        }
    }
    let seed = Rect::new(
        r.x0 + r.w() * 0.2 - ax as f32,
        r.y0 + r.h() * 0.2 - ay as f32,
        r.x1 - r.w() * 0.2 - ax as f32,
        r.y1 - r.h() * 0.2 - ay as f32,
    );
    let inner = r.translate(-(ax as f32), -(ay as f32));
    let mut label = vec![0u32; aw * ah];
    let mut next = 1u32;
    let mut stack = Vec::new();
    let mut comps: Vec<(Rect, bool)> = Vec::new();
    for start in 0..aw * ah {
        if !dil[start] || label[start] != 0 {
            continue;
        }
        label[start] = next;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let mut touches_seed = false;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % aw, i / aw);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            if seed.contains_point(x as f32, y as f32) {
                touches_seed = true;
            }
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < aw && ny < ah {
                    let j = ny * aw + nx;
                    if dil[j] && label[j] == 0 {
                        label[j] = next;
                        stack.push(j);
                    }
                }
            }
        }
        next += 1;
        let c = Rect::new(x0 as f32 + 2.0, y0 as f32 + 2.0, x1 as f32 - 1.0, y1 as f32 - 1.0);
        let inside = c.covered_by(&inner) > 0.9;
        if (touches_seed || inside) && c.area() > 64.0 {
            comps.push((c.translate(ax as f32, ay as f32), touches_seed));
        }
    }
    let keep: Vec<(Rect, bool)> = comps
        .into_iter()
        .filter(|(c, _)| {
            let is_line = lines.iter().any(|l| c.covered_by(&l.rect.expand(3.0)) > 0.6);
            if is_line {
                return false;
            }
            let has_text = lines.iter().any(|l| c.contains_point(l.rect.cx(), l.rect.cy()));
            !(has_text && graphic_fraction(img, c, lines, bg) < 0.03)
        })
        .collect();
    let mut out: Option<Rect> =
        keep.iter().filter(|(_, t)| *t).map(|(c, _)| *c).max_by(|a, b| a.area().partial_cmp(&b.area()).unwrap());
    if let Some(mut u) = out {
        let mut used = vec![false; keep.len()];
        loop {
            let mut grew = false;
            for (i, (c, _)) in keep.iter().enumerate() {
                if !used[i] && u.expand(12.0).inter_area(c) > 0.0 {
                    used[i] = true;
                    u = u.union(c);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        out = Some(u);
    }
    let out = out.map(|o| o.translate(-(ax as f32), -(ay as f32)));
    let mut res = match out {
        Some(o) if o.area() > r.area() * 0.2 => o.translate(ax as f32, ay as f32).clamp(iw as f32, ih as f32),
        _ => *r,
    };
    if res.x0 <= 3.0 {
        res.x0 = 0.0;
    }
    if res.y0 <= 3.0 {
        res.y0 = 0.0;
    }
    if res.x1 >= iw as f32 - 3.0 {
        res.x1 = iw as f32;
    }
    if res.y1 >= ih as f32 - 3.0 {
        res.y1 = ih as f32;
    }
    (res, area)
}

fn graphic_fraction(img: &RgbImage, r: &Rect, lines: &[TextLine], page_bg: Rgb) -> f32 {
    graphic_fraction_bgs(img, r, lines, &[page_bg], &[])
}

fn text_kids(img: &RgbImage, r: &Rect, lines: &[TextLine], panel_bg: Rgb, page_bg: Rgb, panels: &[Panel]) -> Vec<Rect> {
    panels
        .iter()
        .filter(|q| !q.cell && q.rect.covered_by(&r.expand(4.0)) >= 0.98 && q.rect.area() < r.area() * 0.9 && q.rect.w() >= r.w() * 0.75)
        .filter(|q| {
            let inner = q.rect.expand(-3.0);
            text_rows_coverage(&inner, lines) >= 0.3 && graphic_fraction_impl(img, &inner, lines, &[q.bg, panel_bg, page_bg], &[], true) < 0.05
        })
        .map(|q| q.rect)
        .collect()
}

fn panel_graphic_fraction(img: &RgbImage, r: &Rect, lines: &[TextLine], panel_bg: Rgb, page_bg: Rgb, panels: &[Panel]) -> f32 {
    let kids = text_kids(img, r, lines, panel_bg, page_bg, panels);
    graphic_fraction_impl(img, r, lines, &[panel_bg, page_bg], &kids, true)
}

fn graphic_fraction_ex(img: &RgbImage, r: &Rect, lines: &[TextLine], page_bg: Rgb, frames: &[Rect]) -> f32 {
    graphic_fraction_bgs(img, r, lines, &[page_bg], frames)
}

fn edge_bands(img: &RgbImage, x0: u32, y0: u32, w: u32, h: u32, dom: Rgb) -> (u32, u32) {
    let iw = img.width();
    let uniform = |x: u32| {
        let p0 = img.get_pixel(x, y0).0;
        color_dist(p0, dom) > 20.0 && (y0..y0 + h).step_by(2).all(|y| color_dist(img.get_pixel(x, y).0, p0) <= 12.0)
    };
    let mut left = x0;
    if x0 == 0 {
        while left < x0 + w && uniform(left) {
            left += 1;
        }
    }
    let mut right = x0 + w;
    if x0 + w == iw {
        while right > left && uniform(right - 1) {
            right -= 1;
        }
    }
    (left, right)
}

fn graphic_fraction_bgs(img: &RgbImage, r: &Rect, lines: &[TextLine], bgs: &[Rgb], frames: &[Rect]) -> f32 {
    graphic_fraction_impl(img, r, lines, bgs, frames, false)
}

fn graphic_fraction_impl(img: &RgbImage, r: &Rect, lines: &[TextLine], bgs: &[Rgb], frames: &[Rect], bands: bool) -> f32 {
    let (iw, ih) = img.dimensions();
    let (x0, y0, w, h) = r.to_px(iw, ih);
    if w < 2 || h < 2 {
        return 0.0;
    }
    let mut ch: [Vec<u8>; 3] = Default::default();
    for y in (y0..y0 + h).step_by(3) {
        for x in (x0..x0 + w).step_by(3) {
            let p = img.get_pixel(x, y).0;
            for c in 0..3 {
                ch[c].push(p[c]);
            }
        }
    }
    let mut dom = [0u8; 3];
    for c in 0..3 {
        ch[c].sort_unstable();
        dom[c] = ch[c][ch[c].len() / 2];
    }
    let texts: Vec<Rect> = lines
        .iter()
        .map(|l| l.rect.expand((l.rect.h() * 0.4).max(2.0)))
        .chain(frames.iter().map(|f| f.expand(2.0)))
        .filter(|t| t.inter_area(r) > 0.0)
        .collect();
    let mut covered = vec![false; (w * h) as usize];
    for t in &texts {
        let tx0 = (t.x0.ceil().max(x0 as f32)) as i64;
        let tx1 = (t.x1.floor().min((x0 + w - 1) as f32)) as i64;
        let ty0 = (t.y0.ceil().max(y0 as f32)) as i64;
        let ty1 = (t.y1.floor().min((y0 + h - 1) as f32)) as i64;
        for y in ty0..=ty1 {
            let row = (y - y0 as i64) as usize * w as usize;
            for x in tx0..=tx1 {
                covered[row + (x - x0 as i64) as usize] = true;
            }
        }
    }
    let (left, right) = edge_bands(img, x0, y0, w, h, dom);
    for y in 0..h {
        let row = (y * w) as usize;
        for x in (x0..left).chain(right..x0 + w) {
            covered[row + (x - x0) as usize] = true;
        }
    }
    let mut band_row = vec![false; h as usize];
    if bands && w >= 16 {
        for y in 0..h {
            let row = (y * w) as usize;
            let pts: Vec<[u8; 3]> =
                (2..w - 2).step_by(3).filter(|&x| !covered[row + x as usize]).map(|x| img.get_pixel(x0 + x, y0 + y).0).collect();
            if pts.len() < 8 {
                continue;
            }
            let c = pts[pts.len() / 2];
            let same = pts.iter().filter(|p| color_dist(**p, c) <= 12.0).count();
            if same * 10 >= pts.len() * 9 {
                band_row[y as usize] = true;
            }
        }
    }
    let (mut n, mut g) = (0u32, 0u32);
    for cy in (y0..y0 + h).step_by(8) {
        for cx in (x0..x0 + w).step_by(8) {
            let (mut tot, mut drawn) = (0u32, 0u32);
            for y in cy..(cy + 8).min(y0 + h) {
                for x in cx..(cx + 8).min(x0 + w) {
                    if covered[((y - y0) * w + (x - x0)) as usize] {
                        continue;
                    }
                    tot += 1;
                    if band_row[(y - y0) as usize] {
                        continue;
                    }
                    let p = img.get_pixel(x, y).0;
                    if color_dist(p, dom) > 20.0 && bgs.iter().all(|b| color_dist(p, *b) > 20.0) {
                        drawn += 1;
                    }
                }
            }
            if tot >= 32 {
                n += 1;
                if drawn * 2 >= tot {
                    g += 1;
                }
            }
        }
    }
    if n == 0 { 0.0 } else { g as f32 / n as f32 }
}

pub fn line_art(img: &RgbImage, r: &Rect, lines: &[TextLine], page_bg: Rgb, frames: &[Rect]) -> f32 {
    line_strokes(img, r, lines, page_bg, frames).0
}

fn line_strokes(img: &RgbImage, r: &Rect, lines: &[TextLine], page_bg: Rgb, frames: &[Rect]) -> (f32, Option<Rect>) {
    let (iw, ih) = img.dimensions();
    let (x0, y0, w, h) = r.to_px(iw, ih);
    if w < 24 || h < 24 {
        return (0.0, None);
    }
    let mut ch: [Vec<u8>; 3] = Default::default();
    for y in (y0..y0 + h).step_by(3) {
        for x in (x0..x0 + w).step_by(3) {
            let p = img.get_pixel(x, y).0;
            for c in 0..3 {
                ch[c].push(p[c]);
            }
        }
    }
    let mut dom = [0u8; 3];
    for c in 0..3 {
        ch[c].sort_unstable();
        dom[c] = ch[c][ch[c].len() / 2];
    }
    let (bl, br) = edge_bands(img, x0, y0, w, h, dom);
    let (bl, br) = ((bl - x0) as usize, (br - x0) as usize);
    let (w, h) = (w as usize, h as usize);
    let mut mask = vec![false; w * h];
    for y in 0..h {
        for x in bl..br {
            let p = img.get_pixel(x0 + x as u32, y0 + y as u32).0;
            mask[y * w + x] = color_dist(p, dom) > 40.0 && color_dist(p, page_bg) > 40.0;
        }
    }
    for l in lines {
        let t = l.rect.expand(2.0);
        if t.inter_area(r) <= 0.0 {
            continue;
        }
        let tx0 = ((t.x0 - x0 as f32).max(0.0) as usize).min(w);
        let tx1 = ((t.x1 - x0 as f32).max(0.0).ceil() as usize).min(w);
        let ty0 = ((t.y0 - y0 as f32).max(0.0) as usize).min(h);
        let ty1 = ((t.y1 - y0 as f32).max(0.0).ceil() as usize).min(h);
        for y in ty0..ty1 {
            for x in tx0..tx1 {
                mask[y * w + x] = false;
            }
        }
    }
    for f in frames {
        let ring_out = f.expand(3.0);
        if ring_out.inter_area(r) <= 0.0 {
            continue;
        }
        let fx0 = ((ring_out.x0 - x0 as f32).max(0.0) as usize).min(w);
        let fx1 = ((ring_out.x1 - x0 as f32).max(0.0).ceil() as usize).min(w);
        let fy0 = ((ring_out.y0 - y0 as f32).max(0.0) as usize).min(h);
        let fy1 = ((ring_out.y1 - y0 as f32).max(0.0).ceil() as usize).min(h);
        for y in fy0..fy1 {
            for x in fx0..fx1 {
                mask[y * w + x] = false;
            }
        }
    }
    let min_run = 20usize;
    let edge = 4usize;
    let mut total = 0usize;
    let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for y in edge..h.saturating_sub(edge) {
        let mut run = 0usize;
        for x in edge..=w.saturating_sub(edge) {
            if x < w - edge && mask[y * w + x] {
                run += 1;
            } else {
                if run >= min_run {
                    total += run;
                    bx0 = bx0.min(x - run);
                    bx1 = bx1.max(x);
                    by0 = by0.min(y);
                    by1 = by1.max(y + 1);
                }
                run = 0;
            }
        }
    }
    for x in edge..w.saturating_sub(edge) {
        let mut run = 0usize;
        for y in edge..=h.saturating_sub(edge) {
            if y < h - edge && mask[y * w + x] {
                run += 1;
            } else {
                if run >= min_run {
                    total += run;
                    by0 = by0.min(y - run);
                    by1 = by1.max(y);
                    bx0 = bx0.min(x);
                    bx1 = bx1.max(x + 1);
                }
                run = 0;
            }
        }
    }
    let bbox = (bx0 != usize::MAX).then(|| {
        Rect::new(
            (x0 as usize + bx0) as f32,
            (y0 as usize + by0) as f32,
            (x0 as usize + bx1) as f32,
            (y0 as usize + by1) as f32,
        )
    });
    (total as f32 / 2.0 / (w + h) as f32, bbox)
}

fn has_drawing(img: &RgbImage, r: &Rect, lines: &[TextLine], bg: Rgb, frames: &[Rect]) -> bool {
    let in_frames: f32 = frames.iter().map(|f| r.inter_area(&f.expand(8.0))).sum();
    if in_frames > r.area() * 0.85 {
        return false;
    }
    graphic_fraction_ex(img, r, lines, bg, frames) >= 0.02 || line_art(img, r, lines, bg, frames) >= LINE_ART_MIN
}

const LINE_ART_MIN: f32 = 0.6;

fn prose_lines(img: &RgbImage, bg: Rgb, lines: &[TextLine]) -> Vec<Rect> {
    let mut hs: Vec<f32> = lines.iter().filter(|l| l.text.chars().count() >= 20).map(|l| l.rect.h()).collect();
    let lh = crate::geom::median(&mut hs);
    if lh <= 0.0 {
        return Vec::new();
    }
    let body =
        |l: &TextLine| (0.92..=1.6).contains(&(l.rect.h() / lh)) && color_dist(border_color(img, &l.rect), bg) < 30.0;
    let long: Vec<bool> = lines.iter().map(|l| body(l) && l.text.trim().chars().count() >= 28).collect();
    let pitch_above = |j: usize| -> Option<f32> {
        let m = &lines[j].rect;
        lines
            .iter()
            .enumerate()
            .filter(|(k, p)| {
                *k != j && long[*k] && (p.rect.x0 - m.x0).abs() < lh * 0.5 && m.y0 - p.rect.y1 >= -2.0 && m.y0 - p.rect.y1 < lh * 0.6
            })
            .map(|(_, p)| m.y0 - p.rect.y1)
            .reduce(f32::min)
    };
    lines
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            long[*i]
                || (body(l)
                    && lines.iter().enumerate().any(|(j, m)| {
                        let gap = l.rect.y0 - m.rect.y1;
                        long[j]
                            && (m.rect.x0 - l.rect.x0).abs() < lh * 0.5
                            && gap >= -2.0
                            && gap < lh * 0.6
                            && pitch_above(j).is_none_or(|p| gap <= p + (lh * 0.15).max(3.0))
                    }))
        })
        .map(|(_, l)| l.rect)
        .collect()
}

fn clip_prose(img: &RgbImage, bg: Rgb, r: Rect, walls: &[Rect]) -> Rect {
    let mut r = r;
    let orig = r;
    for p in walls {
        if p.h_overlap(&r) < 0.5 || p.inter_area(&r) <= 0.0 || p.covered_by(&r) < 0.5 || p.area() > r.area() * 0.7 {
            continue;
        }
        if p.cy() < r.y0 + r.h() * 0.25 {
            r.y0 = r.y0.max(p.y1 + 1.0);
        } else if p.cy() > r.y1 - r.h() * 0.25 {
            r.y1 = r.y1.min(p.y0 - 1.0);
        }
    }
    if r != orig && r.h() > 4.0 {
        if let Some(c) = content_bbox_t(img, bg, 30.0, &r) {
            r = Rect::new(r.x0, c.y0.max(r.y0), r.x1, c.y1.min(r.y1));
        }
    }
    r
}

fn split_gutters(img: &RgbImage, bg: Rgb, r: &Rect, solid: &dyn Fn(&Rect) -> bool) -> Vec<Rect> {
    let (iw, ih) = img.dimensions();
    let (x0, y0, w, h) = r.to_px(iw, ih);
    if w < 80 || h < 32 {
        return vec![*r];
    }
    let empty: Vec<bool> = (x0..x0 + w)
        .map(|x| {
            ((y0..y0 + h).filter(|&y| color_dist(img.get_pixel(x, y).0, bg) > 30.0).count() as f32) <= h as f32 * 0.01
        })
        .collect();
    let mut cuts: Vec<(u32, u32)> = Vec::new();
    let mut i = 0usize;
    while i < empty.len() {
        if !empty[i] {
            i += 1;
            continue;
        }
        let a = i;
        while i < empty.len() && empty[i] {
            i += 1;
        }
        if i - a >= 12 && a > 8 && i < empty.len() - 8 {
            cuts.push((x0 + a as u32, x0 + i as u32));
        }
    }
    if cuts.is_empty() {
        return vec![*r];
    }
    let mut out = Vec::new();
    let mut left = x0 as f32;
    for (a, b) in cuts.iter().chain(std::iter::once(&(x0 + w, x0 + w))) {
        let piece = Rect::new(left, r.y0, *a as f32, r.y1);
        if piece.w() >= 24.0 {
            out.push(content_bbox_t(img, bg, 30.0, &piece).unwrap_or(piece));
        }
        left = *b as f32;
    }
    if out.len() < 2 || out.iter().any(|p| p.w() < r.w() * 0.15) || !out.iter().all(|p| solid(p)) {
        return vec![*r];
    }
    out
}

fn carve(reg: &Rect, prose: &[Rect]) -> Vec<Rect> {
    let mut cuts: Vec<(f32, f32)> = prose
        .iter()
        .filter(|p| p.h_overlap(reg) > 0.5 && p.y1 > reg.y0 && p.y0 < reg.y1)
        .map(|p| (p.y0, p.y1))
        .collect();
    if cuts.is_empty() {
        return vec![*reg];
    }
    cuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut segs = Vec::new();
    let mut top = reg.y0;
    for (a, b) in cuts {
        if a - top > 24.0 {
            segs.push(Rect::new(reg.x0, top, reg.x1, a - 1.0));
        }
        top = top.max(b + 1.0);
    }
    if reg.y1 - top > 24.0 {
        segs.push(Rect::new(reg.x0, top, reg.x1, reg.y1));
    }
    segs
}

fn line_art_box(img: &RgbImage, bg: Rgb, seg: &Rect, core: Rect, walls: &[Rect], lines: &[TextLine], lh: f32) -> Rect {
    let ih = img.height() as f32;
    let mut b = core;
    let near = (lh * 1.2).max(16.0);
    let far = (lh * 5.0).max(60.0);
    let hits_wall = |c: &Rect| walls.iter().any(|p| p.inter_area(&c.expand(2.0)) > 0.0);
    let diagram_like = |c: &Rect| -> bool {
        let band = c.expand(2.0);
        let small = lines.iter().filter(|l| l.rect.inter_area(&band) > 0.0).all(|l| l.rect.h() < lh * 0.92);
        small || line_strokes(img, &band, lines, bg, &[]).1.is_some()
    };
    b.x0 = b.x0.min(seg.x0);
    b.x1 = b.x1.max(seg.x1);
    for up in [true, false] {
        loop {
            let probe = if up {
                Rect::new(b.x0, (b.y0 - far).max(0.0), b.x1, b.y0)
            } else {
                Rect::new(b.x0, b.y1, b.x1, (b.y1 + far).min(ih))
            };
            if probe.h() < 1.0 {
                break;
            }
            let Some(c) = content_bbox_t(img, bg, 30.0, &probe) else { break };
            let row_empty = |y: f32| content_bbox_t(img, bg, 30.0, &Rect::new(b.x0, y, b.x1, y + 1.0)).is_none();
            let piece = if up {
                let mut y = c.y1 - 1.0;
                while y > (c.y1 - lh).max(c.y0) && !row_empty(y - 1.0) {
                    y -= 1.0;
                }
                content_bbox_t(img, bg, 30.0, &Rect::new(b.x0, y, b.x1, c.y1)).unwrap_or(c)
            } else {
                let mut y = c.y0 + 1.0;
                while y < (c.y0 + lh).min(c.y1) && !row_empty(y) {
                    y += 1.0;
                }
                content_bbox_t(img, bg, 30.0, &Rect::new(b.x0, c.y0, b.x1, y)).unwrap_or(c)
            };
            let dist = if up { b.y0 - piece.y1 } else { piece.y0 - b.y1 };
            if hits_wall(&piece) || (dist > near && !diagram_like(&piece)) {
                break;
            }
            if up {
                b.y0 = piece.y0;
            } else {
                b.y1 = piece.y1;
            }
        }
    }
    content_bbox_t(img, bg, 30.0, &b).unwrap_or(core)
}

fn trim_captions(img: &RgbImage, bg: Rgb, r: Rect, lines: &[TextLine], captions: &[Rect]) -> Rect {
    let mut r = r;
    let on_page_bg = |l: &Rect| -> bool {
        let ring = l.expand(4.0);
        let (iw, ih) = img.dimensions();
        let mut ds: Vec<f32> = Vec::new();
        let (x0, y0, w, h) = ring.to_px(iw, ih);
        for x in (x0..x0 + w).step_by(2) {
            for y in [y0, (y0 + h).saturating_sub(1)] {
                ds.push(color_dist(img.get_pixel(x, y).0, bg));
            }
        }
        for y in (y0..y0 + h).step_by(2) {
            for x in [x0, (x0 + w).saturating_sub(1)] {
                ds.push(color_dist(img.get_pixel(x, y).0, bg));
            }
        }
        crate::geom::median(&mut ds) < 4.0
    };
    let detached = |r: &Rect, l: &Rect, below: bool| -> bool {
        let (iw, ih) = img.dimensions();
        let (x0, _, w, _) = r.to_px(iw, ih);
        let empty = |y: u32| -> bool {
            if y >= ih {
                return false;
            }
            let n = (x0..x0 + w).filter(|&x| color_dist(img.get_pixel(x, y).0, bg) > 30.0).count();
            n as f32 <= w as f32 * 0.005
        };
        let run4 = |ys: &mut dyn Iterator<Item = u32>| -> bool {
            let mut run = 0;
            for y in ys {
                run = if empty(y) { run + 1 } else { 0 };
                if run >= 4 {
                    return true;
                }
            }
            false
        };
        let cy = l.cy().max(0.0) as u32;
        if below {
            let stop = (l.y0 - 8.0).max(r.y0).max(0.0) as u32;
            run4(&mut (stop..cy))
        } else {
            let stop = ((l.y1 + 8.0).min(r.y1).max(0.0) as u32).min(ih);
            run4(&mut (cy..stop))
        }
    };
    let same_align = |a: &Rect, b: &Rect| (a.cx() - b.cx()).abs() < a.h() || (a.x0 - b.x0).abs() < a.h() * 0.5;
    let outer_caption_below = |r: &Rect, l: &Rect| {
        lines.iter().any(|m| {
            m.rect.covered_by(r) < 0.5
                && m.rect.y0 >= r.y1 - 4.0
                && m.rect.y0 - r.y1 < m.rect.h() * 0.8
                && m.rect.h() <= l.h() * 1.15
                && m.rect.h_overlap(r) > 0.5
                && m.text.trim().chars().count() >= 15
        })
    };
    let lh = {
        let mut hs: Vec<f32> = lines.iter().filter(|l| l.text.chars().count() >= 20).map(|l| l.rect.h()).collect();
        crate::geom::median(&mut hs).max(8.0)
    };
    let is_caption = |r: &Rect, l: &TextLine, below: bool| -> bool {
        captions.iter().any(|c| l.rect.covered_by(&c.expand(4.0)) > 0.5)
            || (below
                && (l.rect.h() >= lh * 0.92
                    || ((l.rect.cx() - r.cx()).abs() < r.w() * 0.05 && l.rect.w() < r.w() * 0.8))
                && l.text.trim().chars().count() >= 25
                && detached(r, &l.rect, below)
                && !outer_caption_below(r, &l.rect))
    };
    let mut last_bottom: Option<Rect> = None;
    let mut last_top: Option<Rect> = None;
    for _ in 0..4 {
        let inside: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&r) > 0.5).collect();
        let bottom =
            inside.iter().filter(|l| l.rect.y1 >= r.y1 - 6.0).min_by(|a, b| a.rect.y0.partial_cmp(&b.rect.y0).unwrap());
        let top =
            inside.iter().filter(|l| l.rect.y0 <= r.y0 + 6.0).max_by(|a, b| a.rect.y1.partial_cmp(&b.rect.y1).unwrap());
        let mut changed = false;
        if let Some(l) = bottom {
            if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                eprintln!(
                    "  caption? \"{}\" background {} caption {} detached {} below {}",
                    l.text,
                    on_page_bg(&l.rect),
                    is_caption(&r, l, true),
                    detached(&r, &l.rect, true),
                    outer_caption_below(&r, &l.rect)
                );
            }
            let follows = last_bottom.is_none_or(|p| p.y0 - l.rect.y1 < l.rect.h() * 0.6 && same_align(&p, &l.rect));
            if follows && on_page_bg(&l.rect) && l.rect.y0 - r.y0 > r.h() * 0.5 && is_caption(&r, l, true) {
                r.y1 = l.rect.y0 - 1.0;
                last_bottom = Some(l.rect);
                changed = true;
            }
        }
        if let Some(l) = top {
            let follows = last_top.is_none_or(|p| l.rect.y0 - p.y1 < l.rect.h() * 0.6 && same_align(&p, &l.rect));
            if follows && on_page_bg(&l.rect) && r.y1 - l.rect.y1 > r.h() * 0.5 && is_caption(&r, l, false) {
                r.y0 = l.rect.y1 + 1.0;
                last_top = Some(l.rect);
                changed = true;
            }
        }
        if !changed {
            break;
        }
        if let Some(t) = content_bbox(img, bg, &r) {
            r.y0 = r.y0.max(t.y0);
            r.y1 = r.y1.min(t.y1);
        }
    }
    r
}

fn text_line_between(a: &Rect, b: &Rect, l: &Rect) -> bool {
    let gap = if a.y1 <= b.y0 {
        Rect::new(a.x0.min(b.x0), a.y1, a.x1.max(b.x1), b.y0)
    } else if b.y1 <= a.y0 {
        Rect::new(a.x0.min(b.x0), b.y1, a.x1.max(b.x1), a.y0)
    } else if a.x1 <= b.x0 {
        Rect::new(a.x1, a.y0.min(b.y0), b.x0, a.y1.max(b.y1))
    } else {
        Rect::new(b.x1, a.y0.min(b.y0), a.x0, a.y1.max(b.y1))
    };
    gap.contains_point(l.cx(), l.cy())
}

fn is_admonition(img: &RgbImage, page_bg: Rgb, r: &Rect, lines: &[TextLine], panels: &[Panel]) -> bool {
    let lh = {
        let mut hs: Vec<f32> = lines.iter().map(|l| l.rect.h()).collect();
        crate::geom::median(&mut hs).max(8.0)
    };
    let (iw, ih) = img.dimensions();
    panels.iter().filter(|p| !p.cell).any(|p| {
        let b = p.rect;
        if b.covered_by(&r.expand(6.0)) < 0.95 || b.w() < r.w() * 0.9 || (r.y1 - b.y1).abs() > 14.0 {
            return false;
        }
        let band = Rect::new(b.x0, r.y0.max(b.y0 - lh * 3.0), b.x1, b.y0);
        if band.h() < lh * 0.8 {
            return false;
        }
        let band_lines: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&band.expand(2.0)) > 0.5).collect();
        if band_lines.is_empty() || !band_lines.iter().all(|l| l.rect.v_overlap(&band_lines[0].rect) > 0.5) {
            return false;
        }
        let (x0, y0, w, h) = band.to_px(iw, ih);
        let mut px: Vec<[u8; 3]> = Vec::new();
        for y in (y0..y0 + h).step_by(2) {
            for x in (x0..x0 + w).step_by(3) {
                if !band_lines.iter().any(|l| l.rect.expand(3.0).contains_point(x as f32, y as f32)) {
                    px.push(img.get_pixel(x, y).0);
                }
            }
        }
        if px.len() < 20 {
            return false;
        }
        let mut ch: [Vec<u8>; 3] = Default::default();
        for p in &px {
            for c in 0..3 {
                ch[c].push(p[c]);
            }
        }
        let mut mode = [0u8; 3];
        for c in 0..3 {
            ch[c].sort_unstable();
            mode[c] = ch[c][ch[c].len() / 2];
        }
        let uniform = px.iter().filter(|p| color_dist(**p, mode) < 16.0).count() as f32 / px.len() as f32;
        let inner = b.expand(-3.0);
        if dbg_on() {
            eprintln!(
                "  boxed? {r:?} band {band:?} uniform {uniform:.2} color {mode:?} rows {:.2} graphic {:.3}",
                text_rows_coverage(&inner, lines),
                panel_graphic_fraction(img, &inner, lines, p.bg, page_bg, panels)
            );
        }
        uniform >= 0.75
            && color_dist(mode, page_bg) > 12.0
            && text_rows_coverage(&inner, lines) >= 0.3
            && panel_graphic_fraction(img, &inner, lines, p.bg, page_bg, panels) < 0.05
            && line_art(img, &inner, lines, p.bg, &text_kids(img, &inner, lines, p.bg, page_bg, panels)) < LINE_ART_MIN
    })
}

fn text_rows_coverage(r: &Rect, lines: &[TextLine]) -> f32 {
    let mut iv: Vec<(f32, f32)> = lines
        .iter()
        .filter(|l| l.rect.covered_by(r) > 0.5)
        .map(|l| (l.rect.y0.max(r.y0), l.rect.y1.min(r.y1)))
        .collect();
    iv.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (mut tot, mut cur): (f32, Option<(f32, f32)>) = (0.0, None);
    for (a, b) in iv {
        cur = match cur {
            Some((c0, c1)) if a <= c1 => Some((c0, c1.max(b))),
            Some((c0, c1)) => {
                tot += c1 - c0;
                Some((a, b))
            }
            None => Some((a, b)),
        };
    }
    if let Some((c0, c1)) = cur {
        tot += c1 - c0;
    }
    tot / r.h().max(1.0)
}

fn text_coverage(r: &Rect, lines: &[TextLine]) -> f32 {
    let a = r.area().max(1.0);
    lines.iter().map(|l| l.rect.inter_area(r)).sum::<f32>() / a
}

fn pixel_candidates(img: &RgbImage, bg: Rgb, grid: &Grid) -> Vec<Rect> {
    let (gw, gh) = (grid.gw, grid.gh);
    let mut g = vec![false; gw * gh];
    for i in 0..gw * gh {
        g[i] = !grid.text[i] && grid.nonbg[i] > 0.5;
    }
    let r = 2i32;
    let mut closed = vec![false; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            if !g[gy * gw + gx] {
                continue;
            }
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (gx as i32 + dx, gy as i32 + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < gw && (ny as usize) < gh {
                        closed[ny as usize * gw + nx as usize] = true;
                    }
                }
            }
        }
    }
    let mut seen = vec![false; gw * gh];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..gw * gh {
        if !closed[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut cells = Vec::new();
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % gw, i / gw);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            if g[i] {
                cells.push(i);
            }
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < gw && ny < gh {
                    let j = ny * gw + nx;
                    if closed[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let bw = ((x1 - x0 + 1) as u32 * CELL) as f32;
        let bh = ((y1 - y0 + 1) as u32 * CELL) as f32;
        if bw < 32.0 || bh < 24.0 || cells.len() < 20 {
            continue;
        }
        let textured = cells.iter().filter(|&&i| grid.texture[i] > 6.0).count() as f32 / cells.len() as f32;
        let mut hist: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for &i in &cells {
            if grid.texture[i] < 4.0 {
                let m = grid.mean[i];
                let k = ((m[0] as u32 >> 3) << 10) | ((m[1] as u32 >> 3) << 5) | (m[2] as u32 >> 3);
                *hist.entry(k).or_insert(0) += 1;
            }
        }
        let n_colors = hist.values().filter(|&&n| n as f32 >= cells.len() as f32 * 0.08).count();
        let fill = cells.len() as f32 / ((x1 - x0 + 1) * (y1 - y0 + 1)) as f32;
        let gradient = n_colors == 0 && textured > 0.15;
        let strong = cells.iter().filter(|&&i| grid.texture[i] > 20.0).count() as f32 / cells.len() as f32;
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!(
                "  component {}x{} at ({},{}) cells {} texture {:.2} edges {:.2} colors {} fill {:.2}",
                bw,
                bh,
                x0 as u32 * CELL,
                y0 as u32 * CELL,
                cells.len(),
                textured,
                strong,
                n_colors,
                fill
            );
        }
        if textured > 0.3 || n_colors >= 2 || gradient || (fill > 0.6 && textured > 0.15) || strong > 0.12 {
            let rect = Rect::new(
                (x0 as u32 * CELL) as f32,
                (y0 as u32 * CELL) as f32,
                ((x1 + 1) as u32 * CELL) as f32,
                ((y1 + 1) as u32 * CELL) as f32,
            );
            out.push(rect);
        }
    }
    let _ = (img, bg);
    out
}

fn dbg_on() -> bool {
    std::env::var_os("MARKDOWNSHOT_DEBUG").is_some()
}

pub fn detect(
    img: &RgbImage,
    bg: Rgb,
    lines: &[TextLine],
    layout: &[Region],
    panels: &[Panel],
    manual: &[Rect],
    gutters: &[Rect],
    auto: bool,
) -> Vec<Figure> {
    let (iw, ih) = img.dimensions();
    let mut figs: Vec<Figure> = manual
        .iter()
        .map(|r| Figure { rect: r.clamp(iw as f32, ih as f32), source: FigSource::Manual })
        .filter(|f| f.rect.w() >= 4.0 && f.rect.h() >= 4.0)
        .collect();
    if !auto {
        return figs;
    }
    let grid = Grid::new(img, bg, lines);
    let mut cands: Vec<Figure> = Vec::new();
    let prose = prose_lines(img, bg, lines);
    let col = panels::text_column(lines).unwrap_or((0.0, iw as f32));
    let col_w = col.1 - col.0;
    let text_tabs: Vec<Rect> = panels::text_tables(img, bg, panels, lines).into_iter().map(|t| t.rect).collect();
    let tables: Vec<Rect> =
        panels::tables(panels).into_iter().filter(|t| !text_tabs.iter().any(|x| x.iou(t) > 0.8)).collect();
    if dbg_on() {
        for t in &text_tabs {
            eprintln!("  text table {t:?}");
        }
    }
    let panels: Vec<Panel> = panels.iter().filter(|p| !p.cell).cloned().collect();
    let panels = &panels[..];
    let code_rects: Vec<Rect> = panels.iter().filter(|p| panels::is_code_panel(p, lines)).map(|p| p.rect).collect();
    let code_inside = |p: &Panel| -> Vec<Rect> {
        code_rects.iter().filter(|q| **q != p.rect && q.covered_by(&p.rect.expand(1.0)) > 0.95).cloned().collect()
    };
    let is_code_or_container = |p: &Panel| -> bool {
        if code_rects.contains(&p.rect) {
            return true;
        }
        let n_lines = lines.iter().filter(|l| l.rect.covered_by(&p.rect) > 0.5).count();
        if n_lines <= 1
            && code_rects.iter().any(|q| (q.y0 - p.rect.y1).abs() <= 4.0 && (q.x0 - p.rect.x0).abs() <= 4.0 && (q.x1 - p.rect.x1).abs() <= 4.0)
        {
            return true;
        }
        let inner = code_inside(p);
        inner.iter().map(|q| q.area()).sum::<f32>() >= p.rect.area() * 0.4
            && graphic_fraction_bgs(img, &p.rect.expand(-3.0), lines, &[p.bg, bg], &inner) < 0.05
    };
    let text_aligned = |p: &Panel| -> bool {
        let inside: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&p.rect) > 0.5).collect();
        if inside.is_empty() {
            return false;
        }
        let lh = {
            let mut hs: Vec<f32> = inside.iter().map(|l| l.rect.h()).collect();
            crate::geom::median(&mut hs).max(8.0)
        };
        let left = inside.iter().map(|l| l.rect.x0).fold(f32::MAX, f32::min);
        let aligned = inside.iter().filter(|l| l.rect.x0 - left < lh * 0.8).count();
        let longest = inside.iter().map(|l| l.rect.w()).fold(0.0, f32::max);
        left - p.rect.x0 < lh * 3.0 && aligned * 3 >= inside.len() * 2 && longest >= p.rect.w() * 0.4
    };
    let (narrow, wide): (Vec<&Panel>, Vec<&Panel>) = panels.iter().partition(|p| {
        let inside: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&p.rect) > 0.5).collect();
        let text: String = inside.iter().map(|l| l.text.as_str()).collect();
        let n = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
        let sym = text.chars().filter(|c| "{}[]()<>=;:/\\_|`$#@*+\"'&%.,!?-".contains(*c)).count();
        if dbg_on() {
            eprintln!(
                "  narrow? {:?} lines {} symbols {:.3} graphic {:.3} width {:.0}/{:.0}",
                p.rect,
                inside.len(),
                sym as f32 / n as f32,
                panel_graphic_fraction(img, &p.rect.expand(-3.0), lines, p.bg, bg, panels),
                p.rect.w(),
                col_w
            );
        }
        inside.len() >= 2
            && (sym as f32 >= n as f32 * 0.05
                || panel_graphic_fraction(img, &p.rect.expand(-3.0), lines, p.bg, bg, panels) >= 0.02
                || (inside.len() >= 4 && !text_aligned(p)))
            && p.rect.w() < col_w * 0.85
            && p.rect.x0 >= col.0 - 24.0
            && p.rect.x1 <= col.1 + 24.0
    });
    let frames: Vec<Rect> = wide
        .iter()
        .filter(|p| p.rect.w() >= col_w * 0.85)
        .filter(|p| {
            let inner = p.rect.expand(-3.0);
            let (gf, la) = (panel_graphic_fraction(img, &inner, lines, p.bg, bg, panels), line_art(img, &inner, lines, p.bg, &text_kids(img, &inner, lines, p.bg, bg, panels)));
            if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                eprintln!("  wide panel {:?} graphic {gf:.3} stroke {la:.2}", p.rect);
            }
            (gf < 0.05 && la < LINE_ART_MIN && (text_rows_coverage(&inner, lines) >= 0.45 || text_aligned(p) || code_rects.contains(&p.rect)))
                || (!code_rects.contains(&p.rect) && is_code_or_container(p))
        })
        .map(|p| p.rect)
        .collect();
    let lh_body = {
        let mut hs: Vec<f32> = lines.iter().filter(|l| l.text.chars().count() >= 20).map(|l| l.rect.h()).collect();
        crate::geom::median(&mut hs).max(10.0)
    };
    let pictures: Vec<Rect> = wide
        .iter()
        .filter(|p| {
            let inner = p.rect.expand(-3.0);
            if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                eprintln!(
                    "  full-bleed image? {:?} text rows {:.2} column {:?} aligned {} code {}",
                    p.rect,
                    text_rows_coverage(&inner, lines),
                    col,
                    text_aligned(p),
                    is_code_or_container(p)
                );
            }
            let drawn = || {
                panel_graphic_fraction(img, &inner, lines, p.bg, bg, panels) >= 0.1
                    || line_art(img, &inner, lines, p.bg, &text_kids(img, &inner, lines, p.bg, bg, panels)) >= LINE_ART_MIN * 4.0
            };
            p.rect.h() >= lh_body * 4.0
                && p.rect.x0 >= col.0 - 80.0
                && p.rect.x1 <= col.1 + 80.0
                && lines.iter().any(|l| l.rect.covered_by(&p.rect) > 0.5)
                && ((text_rows_coverage(&inner, lines) < 0.6 && !text_aligned(p) && !is_code_or_container(p)) || drawn())
        })
        .map(|p| p.rect)
        .collect();
    let frames: Vec<Rect> =
        frames.into_iter().filter(|f| !pictures.iter().any(|p| p == f)).chain(gutters.iter().copied()).collect();
    let mut shots: Vec<Rect> = narrow
        .iter()
        .map(|p| {
            let mut r = p.rect;
            for _ in 0..12 {
                match content_bbox_t(img, bg, 30.0, &r.expand(4.0).clamp(iw as f32, ih as f32)) {
                    Some(e) if e.area() > r.area() + 1.0 => r = r.union(&e),
                    _ => break,
                }
            }
            r
        })
        .collect();
    shots.extend(pictures.iter().cloned());
    let clip_walls: Vec<Rect> = prose.iter().chain(frames.iter()).cloned().collect();
    for t in &tables {
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!("  bordered table {t:?}");
        }
        cands.push(Figure { rect: *t, source: FigSource::Pixels });
    }
    for r in &shots {
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!("  screenshot {r:?}");
        }
        cands.push(Figure { rect: *r, source: FigSource::Pixels });
    }
    let walls: Vec<Rect> = {
        let mut hs: Vec<f32> = lines.iter().filter(|l| l.text.chars().count() >= 20).map(|l| l.rect.h()).collect();
        let lh = crate::geom::median(&mut hs).max(10.0);
        let heads = lines
            .iter()
            .filter(|l| l.rect.h() >= lh * 1.25 && color_dist(border_color(img, &l.rect), bg) < 30.0)
            .map(|l| l.rect);
        prose.iter().cloned().chain(heads).chain(frames.iter().cloned()).chain(shots.iter().cloned()).collect()
    };
    let captions: Vec<Rect> =
        layout.iter().filter(|r| r.label.ends_with("caption") && r.score >= 0.5).map(|r| r.rect).collect();
    let lh = {
        let mut hs: Vec<f32> = lines.iter().filter(|l| l.text.chars().count() >= 20).map(|l| l.rect.h()).collect();
        crate::geom::median(&mut hs).max(10.0)
    };
    for reg in layout {
        let ok = match reg.label.as_str() {
            "figure" | "image" | "chart" => reg.score >= 0.45,
            "table" => reg.score >= 0.6,
            "equation" | "isolate_formula" => reg.score >= 0.7,
            _ => false,
        };
        if !ok {
            continue;
        }
        let is_table = reg.label == "table";
        let segs = if is_table { vec![reg.rect] } else { carve(&reg.rect, &prose) };
        let carved = segs.len() > 1 || segs.first().is_some_and(|s| *s != reg.rect);
        for seg in segs {
            let seg = seg.clamp(iw as f32, ih as f32);
            let (la, strokes) = line_strokes(img, &seg, lines, bg, &frames);
            let rr = match strokes {
                Some(core)
                    if !is_table && la >= LINE_ART_MIN && graphic_fraction_ex(img, &seg, lines, bg, &frames) < 0.05 =>
                {
                    let wide = Rect::new(seg.x0 - 40.0, seg.y0, seg.x1 + 40.0, seg.y1).clamp(iw as f32, ih as f32);
                    line_art_box(img, bg, &wide, core, &walls, lines, lh)
                }
                _ => refine(img, bg, &seg, lines),
            };
            if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                eprintln!(
                    "  layout {} {:?} -> slice {:?} -> {:?} graphic {:.3} stroke {:.2}",
                    reg.label,
                    reg.rect,
                    seg,
                    rr,
                    graphic_fraction_ex(img, &rr, lines, bg, &frames),
                    line_art(img, &rr, lines, bg, &frames)
                );
            }
            let rr = match shots.iter().find(|s| rr.covered_by(&s.expand(8.0)) > 0.6 && s.inter_area(&seg) > 0.0) {
                Some(s) => *s,
                None => rr,
            };
            if carved && rr == seg {
                continue;
            }
            let rr = clip_prose(img, bg, rr, &clip_walls);
            if !is_table && !has_drawing(img, &rr, lines, bg, &frames) {
                continue;
            }
            cands.push(Figure { rect: rr, source: FigSource::Layout });
        }
    }
    let dbg = std::env::var_os("MARKDOWNSHOT_DEBUG").is_some();
    if dbg {
        for c in &cands {
            eprintln!("  layout candidate {:?}", c.rect);
        }
    }
    for r in pixel_candidates(img, bg, &grid) {
        if dbg {
            eprintln!("  pixel candidate {:?} -> {:?}", r, refine(img, bg, &r, lines));
        }
        let rr = refine(img, bg, &r, lines);
        let is_shot = shots.iter().any(|s| s.inter_area(&rr) > 0.0);
        let rr = match line_strokes(img, &rr, lines, bg, &frames) {
            (la, Some(core))
                if !is_shot && la >= LINE_ART_MIN && graphic_fraction_ex(img, &rr, lines, bg, &frames) < 0.05 =>
            {
                let wide =
                    Rect::new(rr.x0.min(col.0 - 8.0), rr.y0, rr.x1.max(col.1 + 8.0), rr.y1).clamp(iw as f32, ih as f32);
                line_art_box(img, bg, &wide, core.union(&rr), &walls, lines, lh)
            }
            _ => rr,
        };
        let rr = clip_prose(img, bg, rr, &clip_walls);
        let n_text = lines.iter().filter(|l| l.rect.covered_by(&rr) > 0.5).count();
        if dbg {
            eprintln!("    text {n_text} graphic {:.3}", graphic_fraction(img, &rr, lines, bg));
        }
        if n_text >= 1 && !has_drawing(img, &rr, lines, bg, &frames) {
            continue;
        }
        cands.push(Figure { rect: rr, source: FigSource::Pixels });
    }
    let wholes: Vec<Rect> = shots.iter().chain(tables.iter()).chain(text_tabs.iter()).cloned().collect();
    for c in cands.iter_mut() {
        if wholes.iter().any(|w| w.iou(&c.rect) > 0.8) {
            continue;
        }
        let orig = c.rect;
        for w in &wholes {
            let r = c.rect;
            if w.covered_by(&r) > 0.9 {
                let bands = [
                    Rect::new(r.x0, r.y0, r.x1, w.y0 - 2.0),
                    Rect::new(r.x0, w.y1 + 2.0, r.x1, r.y1),
                    Rect::new(r.x0, r.y0, w.x0 - 2.0, r.y1),
                    Rect::new(w.x1 + 2.0, r.y0, r.x1, r.y1),
                ];
                if bands.iter().all(|b| b.w() < 2.0 || b.h() < 2.0 || content_bbox_t(img, bg, 30.0, b).is_none()) {
                    c.rect = *w;
                }
                continue;
            }
            if r.inter_area(w) <= 0.0 || r.covered_by(&w.expand(6.0)) > 0.9 {
                continue;
            }
            if w.cy() < r.cy() && w.y1 < r.y1 {
                c.rect.y0 = c.rect.y0.max(w.y1 + 1.0);
            } else if w.cy() > r.cy() && w.y0 > r.y0 {
                c.rect.y1 = c.rect.y1.min(w.y0 - 1.0);
            }
        }
        if c.rect != orig && c.rect.h() > 4.0 {
            if let Some(t) = content_bbox_t(img, bg, 30.0, &c.rect) {
                c.rect = t;
            }
        }
        if dbg && c.rect != orig {
            eprintln!("  trimmed to whole objects {orig:?} -> {:?}", c.rect);
        }
    }
    let cands: Vec<Figure> = cands.into_iter().filter(|c| c.rect.h() > 4.0 && c.rect.w() > 4.0).collect();
    let mut merged: Vec<Figure> = Vec::new();
    for c in cands {
        merged.push(c);
        loop {
            let last = merged.len() - 1;
            let cur = merged[last].rect;
            let overlapping = merged[..last].iter().position(|m| m.rect.inter_area(&cur) > 0.0);
            let pos = overlapping.or_else(|| {
                merged[..last].iter().position(|m| {
                    let shot = |r: &Rect| {
                        shots.iter().chain(tables.iter()).any(|s| s.covered_by(r) > 0.8 && r.area() < s.area() * 1.6)
                    };
                    if shot(&m.rect) || shot(&cur) {
                        return false;
                    }
                    let small = |r: &Rect| r.w() < 48.0 || r.h() < 32.0;
                    let reach = if small(&m.rect) || small(&cur) { 3.0 } else { 20.0 };
                    if m.rect.expand(reach).inter_area(&cur.expand(reach)) <= 0.0 {
                        return false;
                    }
                    let u = m.rect.union(&cur);
                    !lines.iter().any(|l| {
                        l.rect.covered_by(&u) > 0.5
                            && l.rect.covered_by(&m.rect) < 0.5
                            && l.rect.covered_by(&cur) < 0.5
                            && text_line_between(&m.rect, &cur, &l.rect)
                    })
                })
            });
            match pos {
                Some(p) => {
                    let m = merged.remove(p);
                    let last = merged.len() - 1;
                    if dbg {
                        eprintln!("  merge {:?} + {:?}", merged[last].rect, m.rect);
                    }
                    merged[last].rect = merged[last].rect.union(&m.rect);
                    if m.source == FigSource::Layout {
                        merged[last].source = FigSource::Layout;
                    }
                }
                None => break,
            }
        }
    }
    let merged: Vec<Figure> = merged
        .into_iter()
        .flat_map(|f| {
            let Some(t) = text_tabs
                .iter()
                .find(|t| t.covered_by(&f.rect.expand(4.0)) > 0.9 && f.rect.area() > t.area() * 1.3)
            else {
                return vec![f];
            };
            if dbg {
                eprintln!("  text table {t:?} removed from {:?}", f.rect);
            }
            [Rect::new(f.rect.x0, f.rect.y0, f.rect.x1, t.y0 - 2.0), Rect::new(f.rect.x0, t.y1 + 2.0, f.rect.x1, f.rect.y1)]
                .into_iter()
                .filter(|r| r.h() >= 32.0)
                .map(|r| Figure { rect: content_bbox(img, bg, &r).unwrap_or(r), source: f.source })
                .filter(|g| g.rect.h() >= 32.0 && g.rect.w() >= 48.0)
                .collect()
        })
        .collect();
    let merged: Vec<Figure> = merged
        .into_iter()
        .flat_map(|f| {
            let solid = |p: &Rect| {
                shots.iter().chain(tables.iter()).any(|s| s.iou(p) > 0.8) || graphic_fraction(img, p, lines, bg) >= 0.1
            };
            split_gutters(img, bg, &f.rect, &solid).into_iter().map(move |r| Figure { rect: r, source: f.source })
        })
        .collect();
    for mut c in merged {
        if c.rect.w() >= col_w * 0.85 && is_admonition(img, bg, &c.rect, lines, panels) {
            if dbg {
                eprintln!("  banded box (text) {:?}", c.rect);
            }
            continue;
        }
        if text_tabs.iter().any(|t| c.rect.covered_by(&t.expand(8.0)) > 0.7 || (t.covered_by(&c.rect) > 0.9 && c.rect.area() < t.area() * 1.3)) {
            continue;
        }
        if !tables
            .iter()
            .chain(shots.iter())
            .any(|t| t.covered_by(&c.rect.expand(4.0)) > 0.9 && c.rect.area() < t.area() * 1.15)
        {
            c.rect = trim_captions(img, bg, c.rect, lines, &captions);
        }
        let r = c.rect;
        if r.w() < 48.0 || r.h() < 32.0 {
            continue;
        }
        let is_shot = shots.iter().any(|p| p.covered_by(&r) > 0.9 && r.area() < p.area() * 1.6);
        if !is_shot && text_coverage(&r, lines) > 0.45 {
            continue;
        }
        let n_text = lines.iter().filter(|l| l.rect.covered_by(&r) > 0.5).count();
        if !is_shot && n_text >= 2 && !has_drawing(img, &r, lines, bg, &frames) {
            continue;
        }
        if figs.iter().any(|m| m.source == FigSource::Manual && (m.rect.iou(&r) > 0.2 || r.covered_by(&m.rect) > 0.5)) {
            continue;
        }
        figs.push(c);
    }
    figs.sort_by(|a, b| a.rect.y0.partial_cmp(&b.rect.y0).unwrap());
    if dbg {
        for f in &figs {
            eprintln!("  detected {:?} {:?}", f.source, f.rect);
        }
    }
    figs
}

pub fn widen(img: &RgbImage, bg: Rgb, figs: Vec<Figure>, lines: &[TextLine], panels: &[Panel]) -> Vec<Figure> {
    let (iw, ih) = img.dimensions();
    let mut heights: Vec<f32> = lines.iter().map(|l| l.rect.h()).collect();
    let lh = crate::geom::median(&mut heights).max(8.0);
    let mut out: Vec<Figure> = Vec::new();
    for f in figs.iter() {
        if f.source == FigSource::Manual {
            out.push(f.clone());
            continue;
        }
        let fr = f.rect;
        let in_band = |l: &TextLine| {
            l.rect.cy() >= fr.y0
                && l.rect.cy() <= fr.y1
                && l.rect.covered_by(&fr) < 0.5
                && color_dist(border_color(img, &l.rect), bg) < 30.0
                && !panels.iter().any(|p| p.rect.contains_point(l.rect.cx(), l.rect.cy()))
                && !figs.iter().any(|g| g.rect != fr && l.rect.covered_by(&g.rect) > 0.5)
        };
        let left: Vec<&TextLine> = lines.iter().filter(|l| in_band(l) && l.rect.cx() < fr.x0).collect();
        let right: Vec<&TextLine> = lines.iter().filter(|l| in_band(l) && l.rect.cx() > fr.x1).collect();
        let is_column = |side: &[&TextLine]| -> bool {
            if side.is_empty() {
                return false;
            }
            let x0 = side.iter().map(|l| l.rect.x0).fold(f32::MAX, f32::min);
            let x1 = side.iter().map(|l| l.rect.x1).fold(f32::MIN, f32::max);
            let span = Rect::new(x0, 0.0, x1, 1.0);
            lines.iter().any(|l| {
                let r = l.rect;
                let outside = r.cy() < fr.y0 || r.cy() > fr.y1;
                let near = (fr.y0 - r.y1).max(r.y0 - fr.y1) < lh * 1.5;
                let same_col = Rect::new(r.x0, 0.0, r.x1, 1.0).h_overlap(&span) > 0.5 && r.x1 <= fr.x0 + 2.0
                    || Rect::new(r.x0, 0.0, r.x1, 1.0).h_overlap(&span) > 0.5 && r.x0 >= fr.x1 - 2.0;
                outside && near && same_col
            })
        };
        let take_left = !left.is_empty() && !is_column(&left);
        let take_right = !right.is_empty() && !is_column(&right);
        if !take_left && !take_right {
            out.push(f.clone());
            continue;
        }
        let mut absorbed = fr;
        for l in left.iter().filter(|_| take_left).chain(right.iter().filter(|_| take_right)) {
            absorbed = absorbed.union(&l.rect);
        }
        let band = absorbed.clamp(iw as f32, ih as f32);
        let mut tight = content_bbox(img, bg, &band).unwrap_or(band).union(&fr);
        tight = tight.expand(4.0).intersect(&band.expand(4.0)).clamp(iw as f32, ih as f32).union(&fr);
        out.push(Figure { rect: tight, source: f.source });
    }
    let mut merged: Vec<Figure> = Vec::new();
    for f in out {
        if let Some(m) = merged.iter_mut().find(|m| {
            m.rect.inter_area(&f.rect) > 0.0 && m.source != FigSource::Manual && f.source != FigSource::Manual
        }) {
            m.rect = m.rect.union(&f.rect);
        } else {
            merged.push(f);
        }
    }
    merged
}

fn border_color(img: &RgbImage, r: &Rect) -> Rgb {
    let (iw, ih) = img.dimensions();
    let (x0, y0, w, h) = r.expand(2.0).to_px(iw, ih);
    if w < 2 || h < 2 {
        return [255, 255, 255];
    }
    let mut ch: [Vec<u8>; 3] = Default::default();
    let mut push = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        for c in 0..3 {
            ch[c].push(p[c]);
        }
    };
    for x in x0..x0 + w {
        push(x, y0);
        push(x, y0 + h - 1);
    }
    for y in y0..y0 + h {
        push(x0, y);
        push(x0 + w - 1, y);
    }
    let mut out = [0u8; 3];
    for c in 0..3 {
        ch[c].sort_unstable();
        out[c] = ch[c][ch[c].len() / 2];
    }
    out
}

fn content_bbox(img: &RgbImage, bg: Rgb, r: &Rect) -> Option<Rect> {
    content_bbox_t(img, bg, 12.0, r)
}

pub(crate) fn content_bbox_t(img: &RgbImage, bg: Rgb, thr: f32, r: &Rect) -> Option<Rect> {
    let (iw, ih) = img.dimensions();
    let (x0, y0, w, h) = r.to_px(iw, ih);
    let (mut bx0, mut by0, mut bx1, mut by1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            if color_dist(img.get_pixel(x, y).0, bg) > thr {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x);
                by1 = by1.max(y);
            }
        }
    }
    if bx0 == u32::MAX { None } else { Some(Rect::new(bx0 as f32, by0 as f32, (bx1 + 1) as f32, (by1 + 1) as f32)) }
}
