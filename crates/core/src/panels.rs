use image::RgbImage;

use crate::geom::Rect;
use crate::ocr::TextLine;
use crate::style::{Rgb, color_dist};

#[derive(Debug, Clone)]
pub struct Panel {
    pub rect: Rect,
    pub bg: Rgb,
    pub cell: bool,
}

fn noise_level(img: &RgbImage, bg: Rgb) -> f32 {
    let (w, h) = img.dimensions();
    let mut d: Vec<f32> = Vec::new();
    let step = ((w as u64 * h as u64 / 200_000).max(1) as f32).sqrt().ceil() as u32 + 1;
    for y in (0..h).step_by(step as usize) {
        for x in (0..w.saturating_sub(1)).step_by(step as usize) {
            let p = img.get_pixel(x, y).0;
            if color_dist(p, bg) < 20.0 {
                let q = img.get_pixel(x + 1, y).0;
                d.push(color_dist(p, q));
            }
        }
    }
    if d.is_empty() {
        return 0.0;
    }
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    d[(d.len() - 1) * 9 / 10]
}

fn flood(
    img: &RgbImage,
    mask: &mut [u8],
    tag: u8,
    seeds: &[(u32, u32)],
    color: Rgb,
    tol: f32,
    max_area: usize,
) -> Option<(usize, Rect)> {
    let (w, h) = img.dimensions();
    let mut stack: Vec<u32> = Vec::new();
    let mut n = 0usize;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for &(x, y) in seeds {
        let i = (y * w + x) as usize;
        if mask[i] == 0 && color_dist(img.get_pixel(x, y).0, color) <= tol {
            mask[i] = tag;
            stack.push(y * w + x);
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        n += 1;
        if n > max_area {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
        let mut visit = |nx: u32, ny: u32| {
            let j = (ny * w + nx) as usize;
            if mask[j] == 0 && color_dist(img.get_pixel(nx, ny).0, color) <= tol {
                mask[j] = tag;
                stack.push(ny * w + nx);
            }
        };
        if x > 0 {
            visit(x - 1, y);
        }
        if x + 1 < w {
            visit(x + 1, y);
        }
        if y > 0 {
            visit(x, y - 1);
        }
        if y + 1 < h {
            visit(x, y + 1);
        }
    }
    if n == 0 {
        return Some((0, Rect::default()));
    }
    Some((n, Rect::new(x0 as f32, y0 as f32, (x1 + 1) as f32, (y1 + 1) as f32)))
}

#[allow(clippy::too_many_arguments)]
fn flood_tracked(
    img: &RgbImage,
    mask: &mut [u8],
    tag: u8,
    seeds: &[(u32, u32)],
    color: Rgb,
    tol: f32,
    max_area: usize,
    visited: &mut Vec<u32>,
) -> Option<(usize, Rect)> {
    let (w, h) = img.dimensions();
    let mut stack: Vec<u32> = Vec::new();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for &(x, y) in seeds {
        let i = y * w + x;
        if mask[i as usize] == 0 && color_dist(img.get_pixel(x, y).0, color) <= tol {
            mask[i as usize] = tag;
            visited.push(i);
            stack.push(i);
        }
    }
    let mut n = 0usize;
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        n += 1;
        if n > max_area {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
        let mut visit = |nx: u32, ny: u32| {
            let j = ny * w + nx;
            if mask[j as usize] == 0 && color_dist(img.get_pixel(nx, ny).0, color) <= tol {
                mask[j as usize] = tag;
                visited.push(j);
                stack.push(j);
            }
        };
        if x > 0 {
            visit(x - 1, y);
        }
        if x + 1 < w {
            visit(x + 1, y);
        }
        if y > 0 {
            visit(x, y - 1);
        }
        if y + 1 < h {
            visit(x, y + 1);
        }
    }
    if n == 0 {
        return Some((0, Rect::default()));
    }
    Some((n, Rect::new(x0 as f32, y0 as f32, (x1 + 1) as f32, (y1 + 1) as f32)))
}

pub fn absorb_gutters(panels: &[Panel], lines: &mut Vec<TextLine>) -> Vec<Rect> {
    let mut gutters = Vec::new();
    let is_num = |t: &str| {
        let t = t.trim().trim_end_matches(|c: char| "-–—▾▼▸›>v~*".contains(c)).trim_end();
        !t.is_empty() && t.len() <= 4 && t.chars().all(|c| c.is_ascii_digit())
    };
    let mut drop = vec![false; lines.len()];
    for p in panels.iter().filter(|p| !p.cell) {
        let px0 = p.rect.x0;
        let in_rows = |r: &Rect| r.cy() >= p.rect.y0 && r.cy() <= p.rect.y1;
        let gutter: Vec<usize> = (0..lines.len())
            .filter(|&i| {
                let r = lines[i].rect;
                in_rows(&r) && r.x0 >= px0 - 100.0 && r.x0 < px0 - 2.0 && {
                    let t = lines[i].text.trim_start();
                    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
                    (r.x1 <= px0 + 6.0 && is_num(t)) || (!digits.is_empty() && digits.len() <= 4 && r.x1 > px0 + 6.0)
                }
            })
            .collect();
        let inside = lines.iter().filter(|l| in_rows(&l.rect) && l.rect.x0 >= px0 - 2.0 && l.rect.x1 <= p.rect.x1 + 2.0).count();
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() && !gutter.is_empty() {
            eprintln!("  line number gutter? {:?}: {:?}, {inside} line(s) inside", p.rect, gutter.iter().map(|&i| &lines[i].text).collect::<Vec<_>>());
        }
        if gutter.len() < 3 || gutter.len() * 2 < inside {
            continue;
        }
        let mut order = gutter.clone();
        order.sort_by(|&a, &b| lines[a].rect.y0.partial_cmp(&lines[b].rect.y0).unwrap());
        let nums: Vec<u32> = order
            .iter()
            .filter_map(|&i| lines[i].text.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok())
            .collect();
        let steps = nums.windows(2).filter(|w| w[1] > w[0] && w[1] <= w[0] + 3).count();
        if (steps + 1) * 10 < nums.len() * 8 {
            continue;
        }
        let gx0 = order.iter().map(|&i| lines[i].rect.x0).fold(px0, f32::min);
        gutters.push(Rect { x0: gx0 - 40.0, y0: p.rect.y0 - 12.0, x1: px0 + 10.0, y1: p.rect.y1 + 12.0 });
        for &i in &order {
            let l = &mut lines[i];
            if l.rect.x1 <= px0 + 6.0 {
                drop[i] = true;
                continue;
            }
            let keep: Vec<crate::ocr::CharPos> = l.chars.iter().filter(|c| c.x >= px0 - 2.0).cloned().collect();
            let lead = keep.iter().take_while(|c| c.ch == ' ').count();
            let keep = keep[lead..].to_vec();
            if keep.is_empty() {
                drop[i] = true;
                continue;
            }
            l.text = keep.iter().map(|c| c.ch).collect();
            l.chars = keep;
            l.rect.x0 = l.rect.x0.max(px0);
        }
    }
    let mut k = 0;
    lines.retain(|_| {
        k += 1;
        !drop[k - 1]
    });
    gutters
}

const REFUSED: u8 = 255;

pub fn find(img: &RgbImage, page_bg: Rgb, lines: &[TextLine]) -> Vec<Panel> {
    let (w, h) = img.dimensions();
    if w < 8 || h < 8 || lines.is_empty() {
        return Vec::new();
    }
    let tol = (noise_level(img, page_bg) * 2.0 + 3.0).clamp(5.0, 12.0);
    let mut mask = vec![0u8; (w * h) as usize];
    let mut seeds = Vec::new();
    for x in 0..w {
        seeds.push((x, 0));
        seeds.push((x, h - 1));
    }
    for y in 0..h {
        seeds.push((0, y));
        seeds.push((w - 1, y));
    }
    flood(img, &mut mask, 1, &seeds, page_bg, tol, usize::MAX);
    let mut hs: Vec<f32> = lines.iter().map(|l| l.rect.h()).collect();
    let lh = crate::geom::median(&mut hs).max(8.0);
    let max_area = (w as usize * h as usize) * 9 / 10;
    let col_w = text_column(lines).map(|(a, b)| b - a).unwrap_or(w as f32);
    let mut out: Vec<Panel> = Vec::new();
    for l in lines {
        let r = l.rect;
        let (px, py, pw, ph) = r.to_px(w, h);
        if pw < 3 || ph < 3 {
            continue;
        }
        let corners = [(px + 1, py + 1), (px + pw - 2, py + 1), (px + 1, py + ph - 2), (px + pw - 2, py + ph - 2)];
        let on_page = corners.iter().filter(|&&(x, y)| mask[(y * w + x) as usize] == 1).count();
        if on_page >= 2 {
            continue;
        }
        let on_panel = corners.iter().filter(|&&(x, y)| mask[(y * w + x) as usize] >= 2).count();
        let parent = out.iter().find(|p| p.rect.contains_point(r.cx(), r.cy()));
        if on_panel >= 2 || (on_panel >= 1 && parent.is_some()) || parent.is_some_and(|p| p.rect.w() < col_w * 0.85) {
            continue;
        }
        let ring = r.expand(6.0).clamp(w as f32 - 1.0, h as f32 - 1.0);
        let (qx0, qy0, qx1, qy1) = (ring.x0 as u32, ring.y0 as u32, ring.x1 as u32, ring.y1 as u32);
        let around = [
            (qx0, qy0),
            (qx1, qy0),
            (qx0, qy1),
            (qx1, qy1),
            ((qx0 + qx1) / 2, qy0),
            ((qx0 + qx1) / 2, qy1),
            (qx0, (qy0 + qy1) / 2),
            (qx1, (qy0 + qy1) / 2),
        ];
        let tag = (out.len() as u8).saturating_add(2).min(254);
        let mut found: Option<(Rect, Rgb)> = None;
        let mut found_visited: Vec<u32> = Vec::new();
        let cy = py + ph / 2;
        let mids = [(px + 1, cy), (px + pw - 2, cy)];
        for pts in [&corners[..], &mids[..], &around[..]] {
            let free: Vec<(u32, u32)> = pts.iter().cloned().filter(|&(x, y)| mask[(y * w + x) as usize] == 0).collect();
            if free.is_empty() {
                continue;
            }
            let mut ch: [Vec<u8>; 3] = Default::default();
            for &(x, y) in &free {
                let p = img.get_pixel(x, y).0;
                for c in 0..3 {
                    ch[c].push(p[c]);
                }
            }
            let mut color = [0u8; 3];
            for c in 0..3 {
                ch[c].sort_unstable();
                color[c] = ch[c][ch[c].len() / 2];
            }
            let seeds: Vec<(u32, u32)> =
                free.iter().cloned().filter(|&(x, y)| color_dist(img.get_pixel(x, y).0, color) <= tol).collect();
            let mut visited = Vec::new();
            let res = flood_tracked(img, &mut mask, tag, &seeds, color, tol, max_area, &mut visited);
            let ok = matches!(res, Some((n, bb)) if n > 0 && bb.h() >= lh * 1.5 && bb.w() >= lh * 3.0);
            if res.is_none() {
                for i in visited {
                    mask[i as usize] = REFUSED;
                }
                continue;
            }
            if ok {
                found = Some((res.unwrap().1, color));
                found_visited = visited;
                break;
            }
            for i in visited {
                mask[i as usize] = 0;
            }
        }
        let Some((bb, color)) = found else { continue };
        let touches = [bb.x0 <= 0.0, bb.y0 <= 0.0, bb.x1 >= w as f32, bb.y1 >= h as f32];
        if touches.iter().filter(|t| **t).count() >= 2 {
            for i in found_visited.drain(..) {
                mask[i as usize] = REFUSED;
            }
            continue;
        }
        out.push(Panel { rect: bb.expand(1.0).clamp(w as f32, h as f32), bg: color, cell: false });
    }
    merge_stacks(&mut out);
    mark_cells(&mut out);
    out
}

fn merge_stacks(panels: &mut Vec<Panel>) {
    let same_column = |a: &Rect, b: &Rect| (a.x0 - b.x0).abs() <= 3.0 && (a.x1 - b.x1).abs() <= 3.0;
    let below = |a: &Rect, b: &Rect| (b.y0 - a.y1).abs() <= 4.0;
    loop {
        let n = panels.len();
        let mut found = None;
        'outer: for i in 0..n {
            for j in 0..n {
                if i == j || !(same_column(&panels[i].rect, &panels[j].rect) && below(&panels[i].rect, &panels[j].rect)) {
                    continue;
                }
                for k in 0..n {
                    if k == i || k == j || !(same_column(&panels[j].rect, &panels[k].rect) && below(&panels[j].rect, &panels[k].rect)) {
                        continue;
                    }
                    if color_dist(panels[i].bg, panels[k].bg) > 6.0 {
                        continue;
                    }
                    let side = panels.iter().enumerate().any(|(m, p)| {
                        m != i && m != j && m != k && {
                            let q = p.rect;
                            [panels[i].rect, panels[j].rect, panels[k].rect]
                                .iter()
                                .any(|r| ((r.x1 - q.x0).abs() <= 4.0 || (q.x1 - r.x0).abs() <= 4.0) && r.v_overlap(&q) > 0.6)
                        }
                    });
                    if !side {
                        found = Some((i, j, k));
                        break 'outer;
                    }
                }
            }
        }
        let Some((i, j, k)) = found else { break };
        let rect = panels[i].rect.union(&panels[j].rect).union(&panels[k].rect);
        let bg = panels[i].bg;
        let mut idx = [i, j, k];
        idx.sort_unstable();
        for &m in idx.iter().rev() {
            panels.remove(m);
        }
        panels.push(Panel { rect, bg, cell: false });
    }
}

pub fn text_column(lines: &[TextLine]) -> Option<(f32, f32)> {
    let long: Vec<&TextLine> = lines.iter().filter(|l| l.text.trim().chars().count() >= 40).collect();
    if long.len() < 3 {
        return None;
    }
    let mut x0: Vec<f32> = long.iter().map(|l| l.rect.x0).collect();
    let mut x1: Vec<f32> = long.iter().map(|l| l.rect.x1).collect();
    x0.sort_by(|a, b| a.partial_cmp(b).unwrap());
    x1.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some((x0[x0.len() / 10], x1[x1.len() * 9 / 10]))
}

pub fn unread_bands(img: &RgbImage, panel: &Panel, lines: &[TextLine]) -> Vec<Rect> {
    let (iw, ih) = img.dimensions();
    let inner = panel.rect.expand(-4.0).clamp(iw as f32, ih as f32);
    let (x0, y0, w, h) = inner.to_px(iw, ih);
    if w < 8 || h < 8 {
        return Vec::new();
    }
    let mut hs: Vec<f32> = lines.iter().map(|l| l.rect.h()).collect();
    let lh = crate::geom::median(&mut hs).max(8.0);
    let covered: Vec<Rect> = lines.iter().map(|l| l.rect.expand(2.0)).filter(|r| r.inter_area(&inner) > 0.0).collect();
    let ink = |x: u32, y: u32| -> bool {
        color_dist(img.get_pixel(x, y).0, panel.bg) > 40.0
            && !covered.iter().any(|r| r.contains_point(x as f32 + 0.5, y as f32 + 0.5))
    };
    let rows: Vec<bool> = (y0..y0 + h).map(|y| (x0..x0 + w).any(|x| ink(x, y))).collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < rows.len() {
        if !rows[i] {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        let mut j = i + 1;
        while j < rows.len() && j <= end + 3 {
            if rows[j] {
                end = j;
            }
            j += 1;
        }
        i = end + 1;
        let (by0, by1) = (y0 + start as u32, y0 + end as u32 + 1);
        if ((by1 - by0) as f32) < (lh * 0.3).max(6.0) || ((by1 - by0) as f32) > lh * 1.5 {
            continue;
        }
        let cols: Vec<bool> = (x0..x0 + w).map(|x| (by0..by1).any(|y| ink(x, y))).collect();
        let mut k = 0usize;
        while k < cols.len() {
            if !cols[k] {
                k += 1;
                continue;
            }
            let a = k;
            let mut b = k;
            let mut m = k + 1;
            while m < cols.len() && (m as f32) <= b as f32 + lh {
                if cols[m] {
                    b = m;
                }
                m += 1;
            }
            k = b + 1;
            let n: usize = (x0 + a as u32..=x0 + b as u32).map(|x| (by0..by1).filter(|&y| ink(x, y)).count()).sum();
            if b - a >= 1 && n >= 8 {
                let pad = ((by1 - by0) as f32 * 0.35).max(3.0);
                out.push(
                    Rect::new((x0 + a as u32) as f32, by0 as f32, (x0 + b as u32 + 1) as f32, by1 as f32)
                        .expand(pad)
                        .clamp(iw as f32, ih as f32),
                );
            }
        }
    }
    out
}

pub fn unread_prefixes(img: &RgbImage, panel: &Panel, lines: &[TextLine]) -> Vec<(usize, Rect)> {
    let (iw, ih) = img.dimensions();
    let inner = panel.rect.expand(-3.0).clamp(iw as f32, ih as f32);
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let r = l.rect;
        if r.covered_by(&panel.rect) < 0.5 || r.h() < 6.0 {
            continue;
        }
        let mut left = inner.x0.max(r.x0 - r.h() * 3.0);
        for m in lines.iter() {
            if !std::ptr::eq(m, l) && m.rect.v_overlap(&r) > 0.5 && m.rect.x1 <= r.x0 + 2.0 {
                left = left.max(m.rect.x1 + 1.0);
            }
        }
        let ya = (r.y0 + r.h() * 0.2).max(0.0) as u32;
        let yb = ((r.y1 - r.h() * 0.2).max(0.0) as u32).min(ih);
        let (xa, xb) = (left.max(0.0) as u32, (r.x0 - 1.0).max(0.0) as u32);
        if xb <= xa + 1 || yb <= ya + 1 {
            continue;
        }
        let ink_col = |x: u32| (ya..yb).filter(|&y| color_dist(img.get_pixel(x, y).0, panel.bg) > 60.0).count();
        let first = (xa..xb).find(|&x| ink_col(x) > 0);
        let Some(x) = first else { continue };
        let n: usize = (x..xb).map(ink_col).sum();
        if n < 3 {
            continue;
        }
        let ext = Rect::new(x as f32 - 3.0, r.y0, r.x1, r.y1);
        if lines.iter().any(|m| !std::ptr::eq(m, l) && m.rect.v_overlap(&r) > 0.5 && m.rect.x1 > ext.x0 && m.rect.x0 < r.x0) {
            continue;
        }
        out.push((i, ext));
    }
    out
}

pub fn is_code_panel(p: &Panel, lines: &[TextLine]) -> bool {
    if is_code_block(p, lines) {
        return true;
    }
    if p.cell {
        return false;
    }
    let col_w = text_column(lines).map(|(a, b)| b - a).unwrap_or(0.0);
    let inside: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&p.rect) > 0.5).collect();
    p.rect.w() >= col_w * 0.85 && inside.len() == 1 && grid_deviation(&inside[0].chars).is_some_and(|d| d < 0.25)
}

pub fn grid_deviation(chars: &[crate::ocr::CharPos]) -> Option<f32> {
    if chars.len() < 12 {
        return None;
    }
    let n = chars.len() - 1;
    let pitch = (chars[n].x - chars[0].x) / n as f32;
    if pitch <= 1.0 {
        return None;
    }
    let dev = chars.iter().enumerate().map(|(i, c)| (c.x - (chars[0].x + pitch * i as f32)).abs()).sum::<f32>() / chars.len() as f32;
    Some(dev / pitch)
}

pub fn is_code_block(p: &Panel, lines: &[TextLine]) -> bool {
    if p.cell {
        return false;
    }
    let col_w = text_column(lines).map(|(a, b)| b - a).unwrap_or(0.0);
    let inside: Vec<&TextLine> = lines.iter().filter(|l| l.rect.covered_by(&p.rect) > 0.5).collect();
    let text: String = inside.iter().map(|l| l.text.as_str()).collect();
    let n = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let sym = text.chars().filter(|c| "{}[]()<>=;:/\\_|`$#@*+\"'&%!".contains(*c)).count();
    p.rect.w() >= col_w * 0.85 && inside.len() >= 2 && sym as f32 >= n as f32 * 0.05
}

fn adjacent(a: &Rect, b: &Rect) -> bool {
    let horiz = ((a.x1 - b.x0).abs() <= 4.0 || (b.x1 - a.x0).abs() <= 4.0) && a.v_overlap(b) > 0.6;
    let vert = ((a.y1 - b.y0).abs() <= 4.0 || (b.y1 - a.y0).abs() <= 4.0) && a.h_overlap(b) > 0.6;
    horiz || vert
}

fn cell_groups(panels: &[Panel]) -> Vec<Vec<usize>> {
    let n = panels.len();
    let mut seen = vec![false; n];
    let mut groups = Vec::new();
    for i in 0..n {
        if seen[i] {
            continue;
        }
        seen[i] = true;
        let mut g = vec![i];
        let mut k = 0;
        while k < g.len() {
            let a = panels[g[k]].rect;
            for j in 0..n {
                if !seen[j] && adjacent(&a, &panels[j].rect) {
                    seen[j] = true;
                    g.push(j);
                }
            }
            k += 1;
        }
        let side_by_side = g.iter().any(|&a| {
            g.iter().any(|&b| {
                a != b && {
                    let (p, q) = (panels[a].rect, panels[b].rect);
                    ((p.x1 - q.x0).abs() <= 4.0 || (q.x1 - p.x0).abs() <= 4.0) && p.v_overlap(&q) > 0.6
                }
            })
        });
        if g.len() >= 3 || (g.len() == 2 && side_by_side) {
            groups.push(g);
        }
    }
    groups
}

fn mark_cells(panels: &mut [Panel]) {
    for g in cell_groups(panels) {
        for i in g {
            panels[i].cell = true;
        }
    }
}

pub fn tables(panels: &[Panel]) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    let cells: Vec<Panel> = panels.iter().filter(|p| p.cell).cloned().collect();
    for g in cell_groups(&cells) {
        let r = g.iter().map(|&i| cells[i].rect).reduce(|a, b| a.union(&b)).unwrap();
        out.push(r);
    }
    out
}

#[derive(Debug, Clone)]
pub struct TextTable {
    pub rect: Rect,
    pub n_rows: usize,
    pub n_cols: usize,
    pub cells: Vec<(usize, usize, usize, usize, Rect, Rgb)>,
}

impl TextTable {
    pub fn cell_at(&self, x: f32, y: f32) -> Option<(usize, usize, Rgb)> {
        self.cells.iter().find(|c| c.4.contains_point(x, y)).map(|c| (c.0, c.1, c.5))
    }
}

fn cluster(mut v: Vec<f32>, tol: f32) -> Vec<f32> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut out: Vec<(f32, usize)> = Vec::new();
    for x in v {
        match out.last_mut() {
            Some((m, n)) if x - *m / *n as f32 <= tol => {
                *m += x;
                *n += 1;
            }
            _ => out.push((x, 1)),
        }
    }
    out.into_iter().map(|(m, n)| m / n as f32).collect()
}

fn grid_lines(spans: Vec<(f32, f32)>) -> Vec<f32> {
    let all = cluster(spans.iter().flat_map(|&(a, b)| [a, b]).collect(), 5.0);
    let n = all.len();
    all.iter()
        .enumerate()
        .filter(|&(k, &x)| {
            k == 0
                || k == n - 1
                || (spans.iter().any(|s| (s.1 - x).abs() <= 5.0) && spans.iter().any(|s| (s.0 - x).abs() <= 5.0))
        })
        .map(|(_, &x)| x)
        .collect()
}

fn mode_color(img: &RgbImage, pts: &mut dyn Iterator<Item = (u32, u32)>) -> Option<Rgb> {
    let mut hist: std::collections::HashMap<u32, (u32, [u32; 3])> = std::collections::HashMap::new();
    for (x, y) in pts {
        let p = img.get_pixel(x, y).0;
        let k = ((p[0] as u32 >> 3) << 10) | ((p[1] as u32 >> 3) << 5) | (p[2] as u32 >> 3);
        let e = hist.entry(k).or_insert((0, [0; 3]));
        e.0 += 1;
        for c in 0..3 {
            e.1[c] += p[c] as u32;
        }
    }
    hist.values().max_by_key(|e| e.0).map(|e| [(e.1[0] / e.0) as u8, (e.1[1] / e.0) as u8, (e.1[2] / e.0) as u8])
}

fn cell_drawing(img: &RgbImage, cell: &Rect, lines: &[TextLine]) -> f32 {
    let (iw, ih) = img.dimensions();
    let inner = cell.expand(-4.0);
    let (x0, y0, w, h) = inner.to_px(iw, ih);
    if w < 4 || h < 4 {
        return 0.0;
    }
    let Some(bg) = mode_color(img, &mut (y0..y0 + h).step_by(2).flat_map(|y| (x0..x0 + w).step_by(2).map(move |x| (x, y))))
    else {
        return 0.0;
    };
    let texts: Vec<Rect> = lines
        .iter()
        .filter(|l| l.rect.inter_area(cell) > 0.0)
        .map(|l| l.rect.expand((l.rect.h() * 0.25).max(2.0)))
        .collect();
    let (mut n, mut ink) = (0u32, 0u32);
    for y in (y0..y0 + h).step_by(2) {
        for x in (x0..x0 + w).step_by(2) {
            if texts.iter().any(|t| t.contains_point(x as f32, y as f32)) {
                continue;
            }
            n += 1;
            if color_dist(img.get_pixel(x, y).0, bg) > 60.0 {
                ink += 1;
            }
        }
    }
    if n == 0 { 0.0 } else { ink as f32 / n as f32 }
}

pub fn text_tables(img: &RgbImage, page_bg: Rgb, panels: &[Panel], lines: &[TextLine]) -> Vec<TextTable> {
    let cells: Vec<Panel> = panels.iter().filter(|p| p.cell).cloned().collect();
    let others: Vec<&Panel> = panels.iter().filter(|p| !p.cell).collect();
    let mut out = Vec::new();
    for g in cell_groups(&cells) {
        let rect = g.iter().map(|&i| cells[i].rect).reduce(|a, b| a.union(&b)).unwrap();
        if others.iter().any(|o| rect.covered_by(&o.rect.expand(2.0)) > 0.9 && o.rect.area() > rect.area() * 1.02) {
            continue;
        }
        let (iw, ih) = img.dimensions();
        let outer = rect.expand(8.0).clamp(iw as f32, ih as f32);
        let (ox0, oy0, ow, oh) = outer.to_px(iw, ih);
        let ring = (ox0..ox0 + ow)
            .step_by(2)
            .flat_map(|x| [oy0, oy0 + 1, oy0 + oh.saturating_sub(2), oy0 + oh.saturating_sub(1)].into_iter().map(move |y| (x, y)))
            .chain(
                (oy0..oy0 + oh)
                    .step_by(2)
                    .flat_map(|y| [ox0, ox0 + 1, ox0 + ow.saturating_sub(2), ox0 + ow.saturating_sub(1)].into_iter().map(move |x| (x, y))),
            );
        let dbg = std::env::var_os("MARKDOWNSHOT_DEBUG").is_some();
        if mode_color(img, &mut ring.into_iter()).is_some_and(|m| color_dist(m, page_bg) > 8.0) {
            if dbg {
                eprintln!("  table {rect:?}: on a colored frame -> image");
            }
            continue;
        }
        let rects: Vec<Rect> = g.iter().map(|&i| cells[i].rect).collect();
        let xs = grid_lines(rects.iter().map(|r| (r.x0, r.x1)).collect());
        let ys = grid_lines(rects.iter().map(|r| (r.y0, r.y1)).collect());
        if xs.len() < 3 || ys.len() < 3 {
            continue;
        }
        let drawn = (0..ys.len() - 1).find_map(|r| {
            (0..xs.len() - 1).find_map(|c| {
                let cr = Rect::new(xs[c], ys[r], xs[c + 1], ys[r + 1]);
                let d = cell_drawing(img, &cr, lines);
                (d > 0.006).then_some((cr, d))
            })
        });
        if let Some((cr, d)) = drawn {
            if dbg {
                eprintln!("  table {rect:?}: drawing in cell {cr:?} ({d:.4}) -> image");
                for l in lines.iter().filter(|l| l.rect.inter_area(&cr) > 0.0) {
                    eprintln!("    line {:?} \"{}\"", l.rect, l.text);
                }
            }
            continue;
        }
        let near = |v: &[f32], x: f32| {
            v.iter().enumerate().min_by(|a, b| (a.1 - x).abs().partial_cmp(&(b.1 - x).abs()).unwrap()).map(|(i, _)| i).unwrap()
        };
        let mut tcells = Vec::new();
        for &i in &g {
            let r = cells[i].rect;
            let (c0, c1) = (near(&xs, r.x0), near(&xs, r.x1));
            let (r0, r1) = (near(&ys, r.y0), near(&ys, r.y1));
            if c1 > c0 && r1 > r0 {
                tcells.push((r0, c0, r1 - r0, c1 - c0, r, cells[i].bg));
            }
        }
        let (n_rows, n_cols) = (ys.len() - 1, xs.len() - 1);
        let covered: usize = tcells.iter().map(|c| c.2 * c.3).sum();
        if n_cols < 2 || n_rows < 2 || covered * 2 < n_rows * n_cols {
            continue;
        }
        out.push(TextTable { rect, n_rows, n_cols, cells: tcells });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::TextLine;

    fn grid(xs: &[u32], ys: &[u32], w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
        for &x in xs {
            for y in ys[0]..=*ys.last().unwrap() {
                img.put_pixel(x, y, image::Rgb([40, 40, 40]));
            }
        }
        for &y in ys {
            for x in xs[0]..=*xs.last().unwrap() {
                img.put_pixel(x, y, image::Rgb([40, 40, 40]));
            }
        }
        img
    }

    fn cell(x0: u32, y0: u32, x1: u32, y1: u32) -> Panel {
        Panel { rect: Rect::new(x0 as f32, y0 as f32, x1 as f32 + 1.0, y1 as f32 + 1.0), bg: [255, 255, 255], cell: true }
    }

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> TextLine {
        TextLine { rect: Rect::new(x0, y0, x1, y1), text: "lorem".into(), conf: 1.0, chars: vec![] }
    }

    #[test]
    fn cluster_groups_close_bounds() {
        assert_eq!(cluster(vec![10.0, 12.0, 100.0, 101.0, 11.0], 5.0), vec![11.0, 100.5]);
    }

    #[test]
    fn isolated_bounds_make_no_line() {
        let xs = grid_lines(vec![(0.0, 100.0), (100.0, 200.0), (200.0, 300.0), (8.0, 100.0), (100.0, 200.0), (200.0, 300.0)]);
        assert_eq!(xs, vec![0.0, 100.0, 200.0, 300.0]);
        let ys = grid_lines(vec![(0.0, 50.0), (50.0, 140.0), (50.0, 180.0), (180.0, 230.0), (0.0, 50.0), (50.0, 180.0)]);
        assert_eq!(ys, vec![0.0, 50.0, 180.0, 230.0]);
    }

    #[test]
    fn grid_with_merged_cell() {
        let img = grid(&[10, 110, 210], &[10, 50, 90, 130], 230, 150);
        let mut panels = vec![cell(10, 10, 210, 50), cell(10, 50, 110, 90), cell(110, 50, 210, 90), cell(10, 90, 110, 130), cell(110, 90, 210, 130)];
        let mut img = img;
        for y in 11..50 {
            img.put_pixel(110, y, image::Rgb([255, 255, 255]));
        }
        mark_cells(&mut panels);
        let lines = vec![line(20.0, 20.0, 90.0, 40.0), line(20.0, 60.0, 90.0, 80.0), line(120.0, 100.0, 190.0, 120.0)];
        let t = text_tables(&img, [255, 255, 255], &panels, &lines);
        assert_eq!(t.len(), 1);
        let t = &t[0];
        assert_eq!((t.n_rows, t.n_cols), (3, 2));
        assert!(t.cells.iter().any(|c| c.0 == 0 && c.1 == 0 && c.2 == 1 && c.3 == 2));
        assert_eq!(t.cell_at(150.0, 110.0).map(|c| (c.0, c.1)), Some((2, 1)));
    }

    #[test]
    fn drawn_cell_or_tinted_frame_is_not_a_text_table() {
        let xs = [10, 110, 210];
        let ys = [10, 50, 90];
        let panels0 = vec![cell(10, 10, 110, 50), cell(110, 10, 210, 50), cell(10, 50, 110, 90), cell(110, 50, 210, 90)];
        let mut panels = panels0.clone();
        mark_cells(&mut panels);
        let lines = vec![line(20.0, 20.0, 90.0, 40.0), line(120.0, 20.0, 190.0, 40.0), line(20.0, 60.0, 90.0, 80.0)];
        let img = grid(&xs, &ys, 230, 110);
        assert_eq!(text_tables(&img, [255, 255, 255], &panels, &lines).len(), 1);
        let mut img2 = img.clone();
        for k in 0..14u32 {
            for d in 0..3u32 {
                img2.put_pixel(150 + k, 75 - k / 2 + d, image::Rgb([30, 120, 30]));
            }
        }
        assert!(text_tables(&img2, [255, 255, 255], &panels, &lines).is_empty());
        let mut img3 = RgbImage::from_pixel(230, 110, image::Rgb([226, 220, 203]));
        for (x, y, p) in img.enumerate_pixels() {
            if (9..=211).contains(&x) && (9..=91).contains(&y) {
                img3.put_pixel(x, y, *p);
            }
        }
        assert!(text_tables(&img3, [255, 255, 255], &panels, &lines).is_empty());
    }
}
