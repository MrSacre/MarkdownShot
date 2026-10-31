use crate::geom::{Rect, median};
use crate::ocr::TextLine;
use crate::panels::Panel;
use crate::style::{Rgb, color_dist};

const MATH: &str = "∑∏∫∮√∞≤≥≠≈≡∈∉∋⊂⊃⊆⊇∪∩∀∃∂∇⌊⌋⌈⌉⟨⟩";
const WEAK: &str = "→←↔⇒⇔·×÷±∓∘∧∨¬αβγδεζηθλμνξπρστφχψωΔΘΛΞΠΣΦΨΩ";

fn math_like(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.chars().any(|c| MATH.contains(c)) {
        return true;
    }
    let tokens: Vec<&str> = t.split_whitespace().collect();
    let words = tokens.iter().filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3 && w.chars().all(|c| c.is_alphabetic() || ",.;:'".contains(c))).count();
    let ops = t.chars().filter(|c| "=+<>^_|/*".contains(*c) || WEAK.contains(*c)).count();
    let long_ident = t
        .split(|c: char| !c.is_alphabetic())
        .any(|w| w.chars().count() >= 6);
    ops >= 1 && words * 3 <= tokens.len().max(1) && !t.contains('©') && !long_ident
}

fn prose(text: &str) -> bool {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let words = tokens.iter().filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3 && w.chars().all(|c| c.is_alphabetic() || ",.;:'".contains(c))).count();
    tokens.len() >= 4 && words * 2 > tokens.len()
}

pub fn find(img: &image::RgbImage, page_bg: Rgb, lines: &[TextLine], panels: &[Panel]) -> Vec<Rect> {
    let on_page = |r: &Rect| {
        let (w, h) = img.dimensions();
        let e = r.expand(3.0).clamp(w as f32, h as f32);
        let (x0, y0, x1, y1) = (e.x0 as u32, e.y0 as u32, (e.x1 as u32).min(w - 1), (e.y1 as u32).min(h - 1));
        let mut pts: Vec<Rgb> = Vec::new();
        for x in (x0..=x1).step_by(3) {
            pts.push(img.get_pixel(x, y0).0);
            pts.push(img.get_pixel(x, y1).0);
        }
        for y in (y0..=y1).step_by(3) {
            pts.push(img.get_pixel(x0, y).0);
            pts.push(img.get_pixel(x1, y).0);
        }
        let near = pts.iter().filter(|p| color_dist(**p, page_bg) <= 25.0).count();
        near * 10 >= pts.len() * 7
    };
    let long: Vec<&TextLine> = lines.iter().filter(|l| l.text.chars().count() >= 40).collect();
    if long.len() < 3 {
        return Vec::new();
    }
    let mut x0s: Vec<f32> = long.iter().map(|l| l.rect.x0).collect();
    let mut x1s: Vec<f32> = long.iter().map(|l| l.rect.x1).collect();
    x0s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    x1s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (c0, c1) = (x0s[x0s.len() / 10], x1s[x1s.len() * 9 / 10]);
    let cw = c1 - c0;
    if cw < 200.0 {
        return Vec::new();
    }
    let cc = (c0 + c1) / 2.0;
    let mut hs: Vec<f32> = long.iter().map(|l| l.rect.h()).collect();
    let lh = median(&mut hs);
    let in_panel = |r: &Rect| panels.iter().any(|p| p.rect.contains_point(r.cx(), r.cy()));
    let centered = |r: &Rect, tol: f32| (r.cx() - cc).abs() <= cw * tol && r.x0 >= c0 + cw * 0.08 && r.w() <= cw * 0.8;
    let mut used = vec![false; lines.len()];
    let mut out: Vec<Rect> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let strong = l.text.chars().any(|c| MATH.contains(c));
        let wide_ok = strong && (l.rect.cx() - cc).abs() <= cw * 0.05 && l.rect.x0 >= c0 + cw * 0.02;
        if used[i] || in_panel(&l.rect) || !(centered(&l.rect, 0.08) || wide_ok) || !math_like(&l.text) || !on_page(&l.rect) {
            continue;
        }
        let near_prose = lines.iter().any(|o| {
            o.text.chars().count() >= 40 && o.rect.h_overlap(&l.rect) > 0.0 && {
                let gap = if o.rect.cy() < l.rect.cy() { l.rect.y0 - o.rect.y1 } else { o.rect.y0 - l.rect.y1 };
                gap >= 0.0 && gap < lh * 0.25
            }
        });
        if near_prose {
            continue;
        }
        used[i] = true;
        let mut r = l.rect;
        for (j, o) in lines.iter().enumerate() {
            if !used[j] && !in_panel(&o.rect) && o.rect.v_overlap(&l.rect) > 0.5 && !prose(&o.text) {
                used[j] = true;
                r = r.union(&o.rect);
            }
        }
        loop {
            let mut grown = false;
            for (j, o) in lines.iter().enumerate() {
                if used[j] || in_panel(&o.rect) {
                    continue;
                }
                let gap = if o.rect.cy() < r.cy() { r.y0 - o.rect.y1 } else { o.rect.y0 - r.y1 };
                let small = o.rect.w() <= r.w() * 0.7 || math_like(&o.text);
                if gap < lh * 0.6 && o.rect.x0 >= r.x0 - lh && o.rect.x1 <= r.x1 + lh && small && o.text.chars().count() <= 24 {
                    used[j] = true;
                    r = r.union(&o.rect);
                    grown = true;
                }
            }
            if !grown {
                break;
            }
        }
        out.push(r.expand((lh * 0.25).max(3.0)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_math() {
        assert!(math_like("Ck+1[i][j] = ∑Ck[i][p] · G[p][j]"));
        assert!(math_like("Ck = G · G... G = Gk"));
        assert!(math_like("a^2 + b^2 = c^2"));
        assert!(!math_like("The result is equal to the sum of all values"));
        assert!(!math_like("Thus the solution of the problem can be represented as follows:"));
        assert!(!math_like("©2026 Digital Notebook · Legal notice · Contact"));
        assert!(!math_like("By Claire Martin · October 21, 2026 · 6 min read"));
        assert!(!math_like("+ - material/brightness-7 + material/brightness-4"));
    }
}
