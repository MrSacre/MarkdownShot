use image::RgbImage;

use crate::figures::Figure;
use crate::geom::{Rect, median, weighted_median};
use crate::order::reading_order;
use crate::panels::Panel;
use crate::style::{self, Line, Rgb, color_dist};

#[derive(Debug, Clone)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    ListItem {
        ordered: Option<String>,
        level: u8,
        text: String,
    },
    Code {
        text: String,
        lang: Option<&'static str>,
    },
    Quote {
        text: String,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Image {
        index: usize,
    },
}

type W = (String, f32, bool, bool);

#[derive(Debug, Clone)]
struct VLine {
    ink: Rect,
    xh: f32,
    bg: Rgb,
    words: Vec<W>,
    text: String,
    quote: bool,
    bullet_blob: Option<f32>,
    pitch_samples: Vec<f32>,
    panel: Option<usize>,
    text_x: f32,
    chars: Vec<(char, f32)>,
    boxr: Rect,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Kind {
    Heading(u8),
    Para,
    Item,
    Panel,
    Quote,
}

struct Cur {
    kind: Kind,
    panel: Option<usize>,
    last: Rect,
    first_x: f32,
    text_x: f32,
    xh: f32,
    bg: Rgb,
    lines: Vec<VLine>,
}

const BULLETS: &[char] = &[
    '•', '·', '●', '▪', '◦', '∘', '。', '°', '‣', '○', '■', '□', '►', '▸', '▶', '➢', '➤', '➔', '➜', '→', '⇒', '›', '✓',
    '✔', '–', '—', '-', '*',
];

struct Ctx<'a> {
    body_xh: f32,
    page_bg: Rgb,
    page_w: f32,
    img: &'a RgbImage,
    panels: &'a [Panel],
}

pub fn build_blocks(img: &RgbImage, lines: &[Line], figures: &[Figure], panels: &[Panel], page_bg: Rgb) -> Vec<Block> {
    let panel_of = |r: &Rect| {
        panels
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.cell && p.rect.contains_point(r.cx(), r.cy()))
            .min_by(|a, b| a.1.rect.area().partial_cmp(&b.1.rect.area()).unwrap())
            .map(|(i, _)| i)
    };
    let ocr: Vec<crate::ocr::TextLine> = lines.iter().map(|l| l.ocr.clone()).collect();
    let tables = crate::panels::text_tables(img, page_bg, panels, &ocr);
    let in_any_table = |r: &Rect| tables.iter().any(|t| t.rect.contains_point(r.cx(), r.cy()));
    let lines: Vec<Line> = prepare_lines(lines, page_bg, &panel_of, &in_any_table);
    let lines = &lines[..];
    let outside: Vec<(f32, f32)> = lines
        .iter()
        .filter(|l| panel_of(&l.rect()).is_none())
        .map(|l| (l.st.xh, l.text().chars().count() as f32))
        .collect();
    let mut pairs: Vec<(f32, f32)> = if outside.iter().map(|p| p.1).sum::<f32>() >= 60.0 {
        outside
    } else {
        lines.iter().map(|l| (l.st.xh, l.text().chars().count() as f32)).collect()
    };
    let body_xh = weighted_median(&mut pairs).max(3.0);
    let ctx = Ctx { body_xh, page_bg, page_w: img.width() as f32, img, panels };

    let mut base_weights: Vec<(f32, f32)> = Vec::new();
    {
        let mut sizes: Vec<f32> = lines.iter().map(|l| l.st.xh).collect();
        sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sizes.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        for s in sizes {
            let mut ws: Vec<f32> = lines
                .iter()
                .filter(|l| (l.st.xh - s).abs() <= s * 0.12)
                .flat_map(|l| l.st.words.iter())
                .filter(|w| w.weight > 0.0 && w.text.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
                .map(|w| w.weight)
                .collect();
            if ws.len() >= 3 {
                base_weights.push((s, median(&mut ws)));
            }
        }
    }
    let global_base = {
        let mut ws: Vec<f32> = lines
            .iter()
            .filter(|l| (l.st.xh - body_xh).abs() <= body_xh * 0.15)
            .flat_map(|l| l.st.words.iter())
            .filter(|w| w.weight > 0.0 && w.text.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
            .map(|w| w.weight)
            .collect();
        if ws.is_empty() { 0.16 } else { median(&mut ws) }
    };
    let base_for = |xh: f32| -> f32 {
        base_weights
            .iter()
            .filter(|(s, _)| (s - xh).abs() <= xh * 0.12)
            .map(|(_, w)| *w)
            .next()
            .map(|w| w.min(global_base * 1.15))
            .unwrap_or(global_base)
    };

    let mut head_sizes: Vec<f32> = lines
        .iter()
        .filter(|l| {
            let t = l.text();
            l.st.xh / body_xh >= 1.2
                && t.chars().count() <= 140
                && t.chars().filter(|c| c.is_alphanumeric()).count() >= 2
        })
        .map(|l| l.st.xh)
        .collect();
    head_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let mut tiers: Vec<f32> = Vec::new();
    for s in head_sizes {
        if tiers.last().is_none_or(|&t| s < t * 0.87) {
            tiers.push(s);
        }
    }
    let top_level: u8 = if tiers.first().is_some_and(|&t| t / body_xh >= 1.9) { 1 } else { 2 };
    let minor_level: u8 = (top_level + tiers.len() as u8).clamp(2, 4);
    let heading_level = |xh: f32| -> u8 {
        let tier = tiers.iter().position(|&t| xh >= t * 0.87).unwrap_or(tiers.len().saturating_sub(1));
        (top_level + tier as u8).min(4)
    };

    let mut in_table: Vec<Option<usize>> = vec![None; lines.len()];
    for (k, l) in lines.iter().enumerate() {
        let r = l.rect();
        in_table[k] = tables.iter().position(|t| t.rect.contains_point(r.cx(), r.cy()));
    }

    let exclude: Vec<Option<usize>> =
        lines.iter().enumerate().map(|(k, l)| in_table[k].or_else(|| panel_of(&l.rect()))).collect();
    let grids = detect_grids(img, lines, body_xh, page_bg, &exclude);
    let mut in_grid = vec![false; lines.len()];
    for g in &grids {
        for &k in &g.lines {
            in_grid[k] = true;
        }
    }
    for (k, t) in in_table.iter().enumerate() {
        if t.is_some() {
            in_grid[k] = true;
        }
    }

    #[derive(Clone)]
    enum Key {
        Lines(Vec<usize>),
        Fig(usize),
        Grid(usize),
        Table(usize),
    }
    let mut rects: Vec<Rect> = Vec::new();
    let mut keys: Vec<Key> = Vec::new();
    let mut used = vec![false; lines.len()];
    for k in 0..lines.len() {
        if in_grid[k] || used[k] {
            continue;
        }
        used[k] = true;
        let mut group = vec![k];
        let mut r = lines[k].ink();
        if let Some(p) = panel_of(&lines[k].rect()) {
            for j in k + 1..lines.len() {
                if !used[j]
                    && !in_grid[j]
                    && panel_of(&lines[j].rect()) == Some(p)
                    && lines[j].ink().v_overlap(&r) > 0.5
                {
                    used[j] = true;
                    r = r.union(&lines[j].ink());
                    group.push(j);
                }
            }
        }
        group.sort_by(|&a, &b| lines[a].ink().x0.partial_cmp(&lines[b].ink().x0).unwrap());
        rects.push(r);
        keys.push(Key::Lines(group));
    }
    for (i, f) in figures.iter().enumerate() {
        rects.push(f.rect);
        keys.push(Key::Fig(i));
    }
    for (i, g) in grids.iter().enumerate() {
        rects.push(g.rect);
        keys.push(Key::Grid(i));
    }
    for (i, t) in tables.iter().enumerate() {
        rects.push(t.rect);
        keys.push(Key::Table(i));
    }
    let order = reading_order(&rects);

    enum Item {
        V(VLine),
        Img(usize),
        Grid(usize),
        Table(usize),
    }
    let mut items: Vec<Item> = Vec::new();
    for o in order {
        let group = match &keys[o] {
            Key::Fig(i) => {
                items.push(Item::Img(*i));
                continue;
            }
            Key::Grid(i) => {
                items.push(Item::Grid(*i));
                continue;
            }
            Key::Table(i) => {
                items.push(Item::Table(*i));
                continue;
            }
            Key::Lines(g) => g.clone(),
        };
        let k = group[0];
        let l = &lines[k];
        let base = if color_dist(l.st.bg, page_bg) > 8.0 && l.st.weight > 0.0 {
            l.st.weight * 1.25
        } else {
            base_for(l.st.xh)
        };
        let words: Vec<W> =
            l.st.words
                .iter()
                .map(|w| {
                    let alnum = w.text.chars().filter(|c| c.is_alphanumeric()).count();
                    let ratio = if alnum >= 1 && w.weight > 0.0 && base > 0.0 { w.weight / base } else { 0.0 };
                    (w.text.clone(), ratio, w.code, w.italic)
                })
                .collect();
        let panel = panel_of(&l.rect());
        let quote = panel.is_none() && color_dist(l.st.bg, page_bg) < 12.0 && style::has_quote_bar(img, l);
        let bullet_blob = if panel.is_none() { style::bullet_blob_x(img, l, page_bg) } else { None };
        let mut v = VLine {
            ink: l.ink(),
            xh: l.st.xh,
            bg: panel.map(|p| panels[p].bg).unwrap_or(l.st.bg),
            words,
            text: l.text().to_string(),
            quote,
            bullet_blob,
            pitch_samples: vec![l.st.pitch],
            panel,
            chars: l.ocr.chars.iter().map(|c| (c.ch, c.x)).collect(),
            boxr: l.ocr.rect,
            text_x: l
                .st
                .words
                .iter()
                .find(|w| list_marker(&format!("{} x", w.text)).0.is_none())
                .map(|w| w.x0)
                .unwrap_or(l.ink().x0),
        };
        if v.panel.is_none() && hollow_bullet_o(&v.chars) {
            if let Some(i) = v.text.find('o') {
                v.text.replace_range(i..i + 1, "• ");
            }
            if let Some(w) = v.words.first_mut().filter(|w| w.0.starts_with('o')) {
                w.0.remove(0);
                v.words.insert(0, ("•".into(), 0.0, false, false));
            }
            if let Some(c) = v.chars.iter_mut().find(|c| !c.0.is_whitespace()) {
                c.0 = '•';
            }
        }
        if let Some(&j) = group.iter().max_by_key(|&&j| lines[j].text().chars().count()) {
            v.xh = lines[j].st.xh;
        }
        for &j in &group[1..] {
            let m = &lines[j];
            let pitch = l.st.pitch.max(m.st.pitch).max(1.0);
            let gap = m.ink().x0 - v.ink.x1;
            let n = ((gap / pitch).round() as usize).clamp(1, 60);
            v.text.push_str(&" ".repeat(n));
            v.text.push_str(m.text());
            v.words.extend(m.st.words.iter().map(|w| (w.text.clone(), 0.0, false, false)));
            v.pitch_samples.push(m.st.pitch);
            v.chars.push((' ', (v.ink.x1 + m.ink().x0) * 0.5));
            v.chars.extend(m.ocr.chars.iter().map(|c| (c.ch, c.x)));
            v.ink = v.ink.union(&m.ink());
        }
        if let Some(Item::V(prev)) = items.last_mut() {
            let lone_marker = prev.text.chars().count() == 1 && list_marker(&format!("{} x", prev.text)).0.is_some();
            let misread_marker = prev.panel.is_none()
                && prev.text.chars().count() == 1
                && "0oO°7>»".contains(prev.text.as_str())
                && prev.ink.w() <= v.xh * 1.8
                && v.ink.x0 - prev.ink.x1 >= v.xh * 0.6
                && v.text.chars().next().is_some_and(|c| c.is_alphabetic());
            if prev.ink.v_overlap(&v.ink) > 0.5
                && v.ink.x0 >= prev.ink.x1 - 2.0
                && ((prev.xh - v.xh).abs() <= prev.xh * 0.3 || lone_marker || misread_marker)
                && prev.panel == v.panel
            {
                prev.ink = prev.ink.union(&v.ink);
                if misread_marker {
                    prev.text = "•".into();
                    prev.words = vec![("•".into(), 0.0, false, false)];
                }
                if lone_marker || misread_marker {
                    prev.text_x = v.text_x;
                }
                if lone_marker || misread_marker || v.text.chars().count() > prev.text.chars().count() {
                    prev.xh = v.xh;
                }
                prev.text.push(' ');
                prev.text.push_str(&v.text);
                prev.words.extend(v.words);
                prev.pitch_samples.extend(v.pitch_samples);
                prev.chars.push((' ', (prev.ink.x1 + v.ink.x0) * 0.5));
                prev.chars.extend(v.chars);
                continue;
            }
        }
        items.push(Item::V(v));
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Cur> = None;
    let mut list_x: Vec<f32> = Vec::new();

    let flush = |cur: &mut Option<Cur>, blocks: &mut Vec<Block>, list_x: &mut Vec<f32>, ctx: &Ctx| {
        if let Some(c) = cur.take() {
            blocks.extend(finish(c.kind, &c.lines, c.first_x, list_x, ctx));
        }
    };

    for it in items {
        let v = match it {
            Item::Grid(g) => {
                flush(&mut cur, &mut blocks, &mut list_x, &ctx);
                list_x.clear();
                blocks.extend(grids[g].blocks());
                continue;
            }
            Item::Table(t) => {
                flush(&mut cur, &mut blocks, &mut list_x, &ctx);
                list_x.clear();
                let members: Vec<&Line> =
                    lines.iter().enumerate().filter(|(k, _)| in_table[*k] == Some(t)).map(|(_, l)| l).collect();
                blocks.push(Block::Table { rows: table_rows(img, &tables[t], &members) });
                continue;
            }
            Item::Img(i) => {
                flush(&mut cur, &mut blocks, &mut list_x, &ctx);
                list_x.clear();
                blocks.push(Block::Image { index: i });
                continue;
            }
            Item::V(v) => v,
        };
        let ratio = v.xh / ctx.body_xh;
        let boxed = v.panel.is_some() || color_dist(v.bg, ctx.page_bg) > 8.0;
        let in_list = !list_x.is_empty() || cur.as_ref().is_some_and(|c| c.kind == Kind::Item);
        let mut v = v;
        if in_list && !boxed && (v.text.starts_with("o ") || v.text.starts_with("0 ")) {
            v.text = format!("◦{}", &v.text[1..]);
            if let Some(w) = v.words.first_mut() {
                if w.0 == "o" || w.0 == "0" {
                    w.0 = "◦".into();
                }
            }
        }
        let (marker, _rest) = list_marker(&v.text);
        let is_item = !boxed && (marker.is_some() || v.bullet_blob.is_some());
        let alnum = v.text.chars().filter(|c| c.is_alphanumeric()).count();
        let kind = if boxed && !(ratio >= 1.45 && v.text.chars().count() <= 140 && alnum >= 2) {
            Kind::Panel
        } else if v.quote {
            Kind::Quote
        } else if ratio >= 1.2 && v.text.chars().count() <= 140 && !is_item && alnum >= 2 {
            Kind::Heading(heading_level(v.xh))
        } else if is_item {
            Kind::Item
        } else if ratio >= 1.05 && v.text.chars().count() <= 80 && alnum >= 2 && all_bold(&v.words) {
            Kind::Heading(minor_level)
        } else {
            Kind::Para
        };

        if let Some(c) = cur.as_mut() {
            let gap = v.ink.y0 - c.last.y1;
            let same_size = (v.xh - c.xh).abs() <= c.xh * 0.18;
            let cont = match (c.kind, kind) {
                (Kind::Heading(a), Kind::Heading(b)) => {
                    a == b && gap < c.xh * 1.2 && (v.ink.x0 - c.first_x).abs() < c.xh * 3.0
                }
                (Kind::Panel, Kind::Panel) => match (c.panel, v.panel) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => color_dist(v.bg, c.bg) < 10.0 && gap < c.xh * 3.0,
                    _ => false,
                },
                (Kind::Quote, Kind::Quote) => gap < c.xh * 1.8,
                (Kind::Para, Kind::Para) => {
                    same_size
                        && gap < c.xh * 1.8
                        && ((v.ink.x0 - c.text_x).abs() < c.xh * 2.0 || (v.ink.cx() - c.last.cx()).abs() < c.xh * 2.0)
                }
                (Kind::Item, Kind::Para) => same_size && gap < c.xh * 2.2 && (v.ink.x0 - c.text_x).abs() < c.xh * 2.0,
                _ => false,
            };
            if cont {
                c.last = v.ink;
                if c.lines.len() == 1 && c.kind != Kind::Item {
                    c.text_x = v.ink.x0;
                }
                c.lines.push(v);
                continue;
            }
        }
        flush(&mut cur, &mut blocks, &mut list_x, &ctx);
        if kind != Kind::Item {
            list_x.clear();
        }
        let text_x = if kind == Kind::Item && marker.is_some() { v.text_x } else { v.ink.x0 };
        let first_x = match (kind, v.bullet_blob) {
            (Kind::Item, Some(bx)) if marker.is_none() => bx,
            _ => v.ink.x0,
        };
        cur = Some(Cur { kind, panel: v.panel, last: v.ink, first_x, text_x, xh: v.xh, bg: v.bg, lines: vec![v] });
    }
    flush(&mut cur, &mut blocks, &mut list_x, &ctx);
    tab_labels(&mut blocks);
    blocks
}

fn tab_labels(blocks: &mut Vec<Block>) {
    let mut i = 0;
    while i + 1 < blocks.len() {
        let label = match &blocks[i] {
            Block::Paragraph { text } | Block::Quote { text } => crate::lang::tabs_line(text),
            _ => None,
        };
        if let (Some(tag), Block::Code { lang, .. }) = (label, &mut blocks[i + 1]) {
            *lang = Some(tag);
            blocks.remove(i);
            continue;
        }
        i += 1;
    }
}

fn all_bold(words: &[W]) -> bool {
    let real: Vec<&W> =
        words.iter().filter(|w| w.0.chars().filter(|c| c.is_alphanumeric()).count() >= 2).collect();
    !real.is_empty() && real.iter().filter(|w| w.1 > 1.3).count() * 10 >= real.len() * 8
}

fn prepare_lines(
    lines: &[Line],
    page_bg: Rgb,
    panel_of: &dyn Fn(&Rect) -> Option<usize>,
    in_table: &dyn Fn(&Rect) -> bool,
) -> Vec<Line> {
    let on_page = |l: &Line| color_dist(l.st.bg, page_bg) <= 8.0;
    let mut out: Vec<Line> = Vec::with_capacity(lines.len());
    for (k, l) in lines.iter().enumerate() {
        let n = l.text().chars().filter(|c| !c.is_whitespace()).count();
        let in_panel = panel_of(&l.rect()).is_some();
        let in_tab = in_table(&l.rect());
        if !in_panel && !in_tab && n <= 2 && !l.st.words.is_empty() && l.st.words.iter().all(|w| w.code) {
            continue;
        }
        let fg = l.st.fg;
        let saturated = fg.iter().max().unwrap() - fg.iter().min().unwrap() > 60;
        let t = l.text();
        if !in_panel && !in_tab && n == 1 && saturated && !"0oO°7>»".contains(t) && list_marker(&format!("{t} x")).0.is_none() {
            continue;
        }
        let mut l = l.clone();
        if !in_panel {
            fix_prose(&mut l);
        }
        if !in_panel && !on_page(&l) {
            let xh = l.st.xh.max(4.0);
            let neighbour = lines.iter().enumerate().any(|(j, m)| {
                j != k
                    && on_page(m)
                    && m.ink().v_overlap(&l.ink()) > 0.5
                    && (m.ink().x0 - l.ink().x1).max(l.ink().x0 - m.ink().x1) < xh * 3.0
            });
            if neighbour && l.ink().h() < xh * 3.5 {
                l.st.bg = page_bg;
                for w in l.st.words.iter_mut() {
                    w.code = true;
                }
            }
        }
        out.push(l);
    }
    out
}

fn fix_prose(l: &mut Line) {
    let mut prev_end: Option<char> = None;
    let mut changed = false;
    for w in l.st.words.iter_mut() {
        if w.code {
            prev_end = w.text.chars().last();
            continue;
        }
        let fixed = fix_prose_word(&w.text, prev_end);
        prev_end = w.text.chars().last();
        if fixed != w.text {
            w.text = fixed;
            changed = true;
        }
    }
    if changed {
        l.ocr.text = l.st.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    }
}

fn fix_prose_word(w: &str, prev_end: Option<char>) -> String {
    let c: Vec<char> = w.chars().collect();
    if prev_end.is_some_and(|p| ".!?".contains(p)) && c.len() >= 3 && c[0] == 'l' && c[1].is_lowercase() && c[1] != '\''
    {
        let mut out = String::from("I");
        out.extend(&c[1..]);
        return out;
    }
    let acronym_at = |j: usize| {
        c[j..].iter().take_while(|ch| ch.is_uppercase() || ch.is_ascii_digit()).count() >= 1
            && c[j..].iter().skip_while(|ch| ch.is_uppercase() || ch.is_ascii_digit()).all(|ch| *ch == 's')
            && c[j..].iter().skip_while(|ch| ch.is_uppercase() || ch.is_ascii_digit()).count() <= 1
    };
    if let Some(i) = (1..c.len().saturating_sub(1)).find(|&i| c[i] == 'l' && c[i - 1].is_lowercase() && c[i + 1].is_uppercase() && acronym_at(i + 1)) {
        let mut out = c.clone();
        out[i] = 'I';
        return out.into_iter().collect();
    }
    let core: Vec<char> = c.iter().cloned().filter(|x| x.is_alphanumeric()).collect();
    let upper = core.iter().filter(|x| x.is_uppercase()).count();
    let suspicious = core.iter().filter(|x| **x == 'l' || **x == '0').count();
    let trailing_s = core.last() == Some(&'s');
    let others_ok = core
        .iter()
        .enumerate()
        .all(|(i, x)| x.is_uppercase() || *x == 'l' || *x == '0' || (*x == 's' && i == core.len() - 1));
    if upper >= 1
        && suspicious >= 1
        && others_ok
        && upper + suspicious + trailing_s as usize == core.len()
        && core.len() >= 2
    {
        return c
            .iter()
            .map(|&x| {
                if x == 'l' {
                    'I'
                } else if x == '0' {
                    'O'
                } else {
                    x
                }
            })
            .collect();
    }
    w.to_string()
}

fn hollow_bullet_o(chars: &[(char, f32)]) -> bool {
    let c: Vec<(char, f32)> = chars.iter().copied().filter(|(ch, _)| !ch.is_whitespace()).take(6).collect();
    if c.len() < 4 || c[0].0 != 'o' || !c[1].0.is_uppercase() || !c[2].0.is_lowercase() {
        return false;
    }
    let pitch = (c[c.len() - 1].1 - c[1].1) / (c.len() - 2) as f32;
    pitch > 0.0 && c[1].1 - c[0].1 > pitch * 1.6
}

fn list_marker(text: &str) -> (Option<String>, String) {
    let t = text.trim_start();
    let mut chars = t.chars();
    if let Some(c) = chars.next() {
        if BULLETS.contains(&c) {
            let rest: String = chars.collect();
            if (c == '-' || c == '–' || c == '—' || c == '*') && !rest.starts_with(' ') {
                return (None, t.to_string());
            }
            return (Some("-".into()), rest.trim_start().to_string());
        }
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let after = &t[digits.len()..];
        if (after.starts_with(". ") || after.starts_with(") ")) && after.len() > 2 {
            return (Some(format!("{digits}.")), after[2..].trim_start().to_string());
        }
    }
    (None, t.to_string())
}

fn split_punct(w: &str) -> (&str, &str, &str) {
    let lead_end =
        w.char_indices().find(|(_, c)| !matches!(c, '(' | '«' | '"' | '“' | '[')).map(|(i, _)| i).unwrap_or(w.len());
    let core = &w[lead_end..];
    let trail_start = core
        .char_indices()
        .rev()
        .take_while(|(_, c)| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | ')' | '»' | '"' | '”' | ']'))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(core.len());
    (&w[..lead_end], &core[..trail_start], &core[trail_start..])
}

fn rich_text(lines: &[VLine], skip_first_marker: bool, allow_bold: bool, isolated_bold: bool) -> String {
    let mut words: Vec<W> = Vec::new();
    for (li, l) in lines.iter().enumerate() {
        let mut ws = l.words.clone();
        if li == 0 && skip_first_marker {
            if let Some((first, _, _, _)) = ws.first() {
                let (m, rest) = list_marker(first);
                if m.is_some() {
                    if rest.is_empty() {
                        ws.remove(0);
                    } else {
                        ws[0].0 = rest;
                    }
                } else if list_marker(&l.text).0.is_some() && ws.len() > 1 {
                    let joined = format!("{} {}", ws[0].0, ws[1].0);
                    let (m2, rest2) = list_marker(&joined);
                    if m2.is_some() {
                        ws.drain(0..2);
                        if !rest2.is_empty() {
                            ws.insert(0, (rest2, 0.0, false, false));
                        }
                    }
                }
            }
        }
        if let (Some(last), Some(first)) = (words.last_mut(), ws.first()) {
            if last.0.ends_with('-')
                && !last.0.ends_with("--")
                && first.0.chars().next().is_some_and(|c| c.is_alphabetic())
            {
                last.0.push_str(&first.0);
                ws.remove(0);
            }
        }
        words.extend(ws);
    }
    if words.is_empty() {
        return String::new();
    }
    let short = |w: &str| w.chars().filter(|c| c.is_alphanumeric()).count() <= 2;
    let strong: Vec<bool> = words.iter().map(|(t, r, _, _)| if short(t) { *r > 1.45 } else { *r > 1.35 }).collect();
    let code: Vec<bool> = words.iter().map(|w| w.2).collect();
    let n = words.len();
    let flags: Vec<bool> = (0..n)
        .map(|i| {
            if !short(&words[i].0) {
                return strong[i];
            }
            let left = i > 0 && strong[i - 1] && !short(&words[i - 1].0);
            let right = i + 1 < n && strong[i + 1] && !short(&words[i + 1].0);
            (left || right) && words[i].1 > 1.3
        })
        .collect();
    let mut flags = flags;
    if !isolated_bold {
        let f0 = flags.clone();
        for i in 0..n {
            let left = i > 0 && f0[i - 1];
            let right = i + 1 < n && f0[i + 1];
            flags[i] = f0[i] && (left || right);
        }
    }
    let slant: Vec<bool> = words.iter().map(|w| w.3).collect();
    let italic: Vec<bool> = (0..n)
        .map(|i| {
            slant[i]
                && !code[i]
                && ((i > 0 && slant[i - 1]) || (i + 1 < n && slant[i + 1]) || words[i].0.chars().filter(|c| c.is_alphabetic()).count() >= 6)
        })
        .collect();
    let mut italic = italic;
    for i in 1..n.saturating_sub(1) {
        let short = words[i].0.chars().filter(|c| c.is_alphanumeric()).count() <= 3;
        if !italic[i] && short && !code[i] && italic[i - 1] && italic[i + 1] {
            italic[i] = true;
        }
    }
    let n_it = italic.iter().filter(|&&v| v).count();
    let italic: Vec<bool> = if n >= 3 && n_it * 10 >= n * 8 { (0..n).map(|i| !code[i]).collect() } else { italic };
    let words: Vec<(String, bool)> = words.into_iter().zip(flags).map(|((t, _, _, _), f)| (t, f)).collect();
    let total = words.len();
    let n_bold = words.iter().filter(|w| w.1).count() + code.iter().filter(|&&c| c).count();
    let all_bold = allow_bold && n_bold == total && total <= 12;
    let mut out = String::new();
    let mut i = 0;
    while i < words.len() {
        let punct = i > 0 && code[i - 1] && !code[i] && words[i].0.chars().all(|c| ".,;:!?)»".contains(c));
        if !out.is_empty() && !punct {
            out.push(' ');
        }
        if code[i] {
            let mut j = i;
            while j < words.len() && code[j] {
                j += 1;
            }
            let joined = words[i..j].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
            let core = code_core(&joined);
            out.push('`');
            out.push_str(core.trim_matches('`'));
            out.push('`');
            out.push_str(&joined[core.len()..]);
            i = j;
            continue;
        }
        let bold = allow_bold && (words[i].1 || all_bold);
        if !bold && allow_bold && italic[i] {
            let mut j = i;
            while j < words.len() && italic[j] && !code[j] && !(words[j].1 || all_bold) {
                j += 1;
            }
            let joined = words[i..j].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
            let (lead, core, trail) = split_punct(&joined);
            if core.is_empty() {
                out.push_str(&escape(&joined));
            } else {
                out.push_str(lead);
                out.push('*');
                out.push_str(&escape(core));
                out.push('*');
                out.push_str(trail);
            }
            i = j;
            continue;
        }
        if !bold {
            out.push_str(&escape(&words[i].0));
            i += 1;
            continue;
        }
        let mut j = i;
        while j < words.len() && (words[j].1 || all_bold) {
            j += 1;
        }
        let span: Vec<&str> = words[i..j].iter().map(|w| w.0.as_str()).collect();
        let joined = span.join(" ");
        let (lead, core, trail) = split_punct(&joined);
        if core.is_empty() {
            out.push_str(&escape(&joined));
        } else {
            out.push_str(lead);
            out.push_str("**");
            out.push_str(&escape(core));
            out.push_str("**");
            out.push_str(trail);
        }
        i = j;
    }
    out
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '`' | '<') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

fn finish(kind: Kind, lines: &[VLine], first_x: f32, list_x: &mut Vec<f32>, ctx: &Ctx) -> Vec<Block> {
    match kind {
        Kind::Heading(level) => {
            let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
            vec![Block::Heading { level, text: escape(&text) }]
        }
        Kind::Quote => vec![Block::Quote { text: rich_text(lines, false, true, true) }],
        Kind::Item => {
            let (marker, _) = list_marker(&lines[0].text);
            let ordered = marker.filter(|m| m != "-");
            let x = first_x;
            let level = match list_x.iter().position(|&lx| (lx - x).abs() < ctx.body_xh * 1.2) {
                Some(p) => {
                    list_x.truncate(p + 1);
                    p
                }
                None => {
                    let deeper = list_x.last().is_none_or(|&lx| x > lx);
                    if !deeper {
                        list_x.clear();
                    }
                    list_x.push(x);
                    list_x.len() - 1
                }
            };
            let skip = list_marker(&lines[0].text).0.is_some();
            let first_bold = lines.len() > 1 && lines[0].xh >= ctx.body_xh * 1.05 && {
                let mut ws = lines[0].words.clone();
                if skip && !ws.is_empty() && list_marker(&format!("{} x", ws[0].0)).0.is_some() {
                    ws.remove(0);
                }
                all_bold(&ws)
            };
            let text = if first_bold {
                let head = rich_text(&lines[..1], skip, false, true);
                format!("#### {}\n{}", head.trim(), rich_text(&lines[1..], false, true, true))
            } else {
                rich_text(lines, skip, true, true)
            };
            vec![Block::ListItem { ordered, level: level.min(6) as u8, text }]
        }
        Kind::Panel => {
            let kept: Vec<VLine> = lines
                .iter()
                .filter(|l| {
                    let t: Vec<char> = l.text.chars().filter(|c| !c.is_whitespace()).collect();
                    lines.len() == 1 || t.len() > 1 || t.first().is_some_and(|c| !c.is_alphanumeric() && *c != '!')
                })
                .cloned()
                .collect();
            let frame = lines.first().and_then(|l| l.panel).and_then(|i| ctx.panels.get(i)).map(|p| p.rect);
            let mut label = None;
            let mut kept = kept;
            if let Some(f) = frame {
                if let Some(k) = kept.iter().position(|l| {
                    l.ink.x0 > f.x0 + f.w() * 0.6
                        && l.ink.y0 < f.y0 + l.boxr.h() * 2.0
                        && l.text.trim().chars().count() <= 12
                        && crate::lang::tag_of_name(l.text.trim()).is_some()
                }) {
                    label = crate::lang::tag_of_name(kept[k].text.trim());
                    if kept.len() > 1 {
                        kept.remove(k);
                    }
                }
            }
            let lines = &kept[..];
            if looks_like_code(lines, Some(ctx.img)) {
                {
                    let text = code_text(lines, Some(ctx.img), frame);
                    let lang = label.or_else(|| crate::lang::guess(&text));
                    vec![Block::Code { text, lang }]
                }
            } else {
                let x0 = lines.iter().map(|l| l.ink.x0).fold(f32::MAX, f32::min);
                let x1 = lines.iter().map(|l| l.ink.x1).fold(f32::MIN, f32::max);
                let mut text = rich_text(lines, false, true, false);
                if x1 - x0 < ctx.page_w * 0.85 {
                    let mut it = text.splitn(2, ' ');
                    if let (Some(first), Some(rest)) = (it.next(), it.next()) {
                        let f = first.trim_matches('*');
                        if f.chars().count() == 1 && !f.chars().all(|c| c.is_ascii_digit()) {
                            text = rest.to_string();
                        }
                    }
                    vec![Block::Quote { text }]
                } else {
                    vec![Block::Paragraph { text }]
                }
            }
        }
        Kind::Para => vec![Block::Paragraph { text: rich_text(lines, false, true, true) }],
    }
}

fn code_text(lines: &[VLine], img: Option<&RgbImage>, frame: Option<Rect>) -> String {
    let fixed: Vec<VLine>;
    let lines = match (img, frame) {
        (Some(img), Some(f)) => {
            fixed = lines
                .iter()
                .map(|l| {
                    let mut l = l.clone();
                    let band = (l.boxr.y0 + l.boxr.h() * 0.2, l.boxr.y1 - l.boxr.h() * 0.2);
                    if l.ink.x0 <= f.x0 + 2.0 {
                        if let Some(x) = crate::glyphs::first_ink(img, l.bg, f.x0 + 3.0, l.ink.x1, band, true) {
                            l.ink.x0 = x;
                        }
                    }
                    if l.ink.x1 >= f.x1 - 2.0 {
                        if let Some(x) = crate::glyphs::first_ink(img, l.bg, l.ink.x0, f.x1 - 3.0, band, false) {
                            l.ink.x1 = x + 1.0;
                        }
                    }
                    l
                })
                .collect();
            &fixed[..]
        }
        _ => lines,
    };
    let clip = frame.map(|f| f.x1);
    let pad_l = frame.map(|f| (lines.iter().map(|l| l.ink.x0).fold(f32::MAX, f32::min) - f.x0).max(0.0)).unwrap_or(0.0);
    let mut deltas: Vec<f32> = Vec::new();
    for l in lines {
        for w in l.chars.windows(2) {
            if !w[0].0.is_whitespace() && !w[1].0.is_whitespace() {
                deltas.push(w[1].1 - w[0].1);
            }
        }
    }
    let measured = deltas.len() >= 8;
    let pitch = if measured {
        let d = median(&mut deltas).max(1.0);
        let mut w: Vec<f32> = lines
            .iter()
            .filter(|l| {
                let n = l.chars.len();
                n >= 10
                    && l.chars.windows(2).all(|p| p[1].1 - p[0].1 < d * 1.5)
                    && l.chars.first().is_some_and(|c| !c.0.is_whitespace())
            })
            .map(|l| l.ink.w() / (l.chars.len() as f32 - 0.3))
            .collect();
        if !w.is_empty() { median(&mut w) } else { d }
    } else {
        let mut p: Vec<f32> = lines.iter().flat_map(|l| l.pitch_samples.iter().cloned()).collect();
        median(&mut p).max(1.0)
    };
    let minx = lines.iter().map(|l| l.ink.x0).fold(f32::MAX, f32::min);
    let mut hs: Vec<f32> = lines.iter().map(|l| l.boxr.h()).collect();
    let lh = median(&mut hs);
    let steps: Vec<f32> = lines.windows(2).map(|w| w[1].boxr.cy() - w[0].boxr.cy()).filter(|d| *d > 0.0).collect();
    let step = steps.iter().cloned().filter(|d| *d >= lh * 0.75).fold(lh * 1.45, f32::min);
    let mut out: Vec<(String, Option<usize>, f32, f32)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 && step > 0.0 {
            let d = l.boxr.cy() - lines[i - 1].boxr.cy();
            let blank = ((d / step).round() as usize).saturating_sub(1).min(3);
            if d > step * 1.6 {
                let n = blank.max(1);
                let (a, b) = (lines[i - 1].boxr.cy() + lh * 0.4, (l.boxr.cy() - lh * 0.4).max(lines[i - 1].boxr.cy() + lh * 0.4));
                for j in 0..n {
                    let (y0, y1) = (a + (b - a) * j as f32 / n as f32, a + (b - a) * (j + 1) as f32 / n as f32);
                    out.push((String::new(), None, y0, y1));
                }
            }
        }
        let first = l.chars.iter().find(|c| !c.0.is_whitespace()).map(|c| c.1 - pitch * 0.5);
        let start = match first {
            Some(f) if f - l.ink.x0 > pitch * 2.5 => f,
            _ => l.ink.x0,
        };
        let indent = ((start - minx) / pitch).round().max(0.0) as usize;
        let body = if l.chars.len() >= 2 { mono_line(&l.chars, pitch) } else { l.text.clone() };
        out.push((format!("{}{}", " ".repeat(indent.min(40)), body), Some(i), l.boxr.y0, l.boxr.y1));
    }
    if let Some(img) = img.filter(|_| measured && std::env::var_os("MARKDOWNSHOT_NO_GLYPHS").is_none()) {
        let (f, l) = (lines[0].boxr, lines[lines.len() - 1].boxr);
        let s = if step > 0.0 { step } else { lh * 1.3 };
        let above = (f.cy() - s - lh * 0.5, f.cy() - s + lh * 0.5);
        let below = (l.cy() + s - lh * 0.5, l.cy() + s + lh * 0.5);
        let edge_top = above.0 >= 0.0;
        let edge_bottom = below.1 <= img.height() as f32;
        if edge_top {
            out.insert(0, (String::new(), None, above.0, above.1));
        }
        if edge_bottom {
            out.push((String::new(), None, below.0, below.1));
        }
        repair_glyphs(img, lines, &mut out, minx, pitch, lh);
        let keep = |t: &str| !t.trim().is_empty() && t.chars().all(|c| c == '-' || c == '.' || c == '`' || c == ' ');
        if edge_bottom && !keep(&out[out.len() - 1].0) {
            out.pop();
        }
        if edge_top && !keep(&out[0].0) {
            out.remove(0);
        }
    }
    let text: Vec<String> = out
        .into_iter()
        .map(|r| {
            let t = r.0.trim_end();
            let t = match t.strip_suffix('+') {
                Some(rest) if rest.ends_with("  ") && !rest.trim().is_empty() => rest.trim_end().to_string(),
                _ => r.0.clone(),
            };
            match (r.1, clip) {
                (Some(i), Some(x1)) if (lines[i].ink.x1 >= x1 - 3.0 || lines[i].ink.x1 > x1 - pad_l - 1.0) && !t.is_empty() => {
                    format!("{t}…")
                }
                _ => t,
            }
        })
        .collect();
    fix_code(&strip_line_numbers(&text.join("\n")))
}

fn repair_glyphs(img: &RgbImage, lines: &[VLine], out: &mut [(String, Option<usize>, f32, f32)], minx: f32, pitch: f32, _lh: f32) {
    use crate::glyphs::{self, Grid, Row};
    let debug = std::env::var_os("MARKDOWNSHOT_DEBUG").is_some();
    let width = out.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let sample: Vec<(f32, f32, Rgb)> = lines.iter().map(|l| (l.boxr.y0, l.boxr.y1, l.bg)).collect();
    let g = glyphs::refine_grid(img, Grid { x0: minx - pitch * 0.1, pitch }, &sample, width);
    let g = {
        let mut offs: Vec<i64> = Vec::new();
        for row in out.iter() {
            let Some(i) = row.1 else { continue };
            let l = &lines[i];
            let h = l.boxr.h();
            let band = Row { y0: l.boxr.y0 + h * 0.3, y1: l.boxr.y1 - h * 0.3, top: 0.0, base: 0.0 };
            let cols = (((l.ink.x1 - g.x0) / g.pitch).ceil().max(0.0) as usize).min(400);
            let inked = glyphs::ink_columns(img, l.bg, g, band, cols);
            if let (Some(k0), Some(v0)) = (inked.iter().position(|&b| b), row.0.chars().position(|c| !c.is_whitespace())) {
                offs.push(k0 as i64 - v0 as i64);
            }
        }
        let mut best = (0usize, 0i64);
        for &o in offs.iter().filter(|o| o.abs() <= 1) {
            let n = offs.iter().filter(|&&v| v == o).count();
            if n > best.0 || (n == best.0 && o.abs() < best.1.abs()) {
                best = (n, o);
            }
        }
        if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() {
            eprintln!("  grid shift: {} column(s) ({offs:?})", best.1);
        }
        Grid { x0: g.x0 + best.1 as f32 * g.pitch, ..g }
    };
    if std::env::var_os("MARKDOWNSHOT_GLYPHS").is_some() {
        eprintln!("  grid: start {:.1} pitch {:.2} -> start {:.1} pitch {:.3}", minx - pitch * 0.1, pitch, g.x0, g.pitch);
    }
    let mut rel: Vec<(f32, f32)> = Vec::new();
    let mut metrics: Vec<Option<(f32, f32)>> = vec![None; out.len()];
    for (r, row) in out.iter().enumerate() {
        let Some(i) = row.1 else { continue };
        let l = &lines[i];
        let chars: Vec<char> = row.0.chars().collect();
        if let Some((t, b)) = glyphs::row_metrics(img, l.bg, g, &chars, l.boxr.y0, l.boxr.y1) {
            rel.push((t - l.boxr.y0, b - l.boxr.y0));
            metrics[r] = Some((t, b));
        }
    }
    if rel.is_empty() {
        return;
    }
    let mut rt: Vec<f32> = rel.iter().map(|v| v.0).collect();
    let mut rb: Vec<f32> = rel.iter().map(|v| v.1).collect();
    let (dt, db) = (median(&mut rt), median(&mut rb));
    let mut bg = lines[0].bg;
    let is_box = |c: char| ('─'..='╿').contains(&c);
    let mut drawn: Vec<(usize, usize)> = Vec::new();
    for (r, row) in out.iter_mut().enumerate() {
        if let Some(i) = row.1 {
            bg = lines[i].bg;
        }
        let (y0, y1) = (row.2, row.3);
        let (top, base) = metrics[r].unwrap_or((y0 + dt, y0 + db));
        let rr = Row { y0, y1, top, base };
        let mut chars: Vec<char> = row.0.chars().collect();
        let before = row.0.clone();
        if let Some(i) = row.1 {
            let cols = (((lines[i].ink.x1 - g.x0) / g.pitch).ceil().max(0.0) as usize).min(400);
            let inked = glyphs::ink_columns(img, bg, g, rr, cols);
            let at: Vec<usize> = (0..cols).filter(|&k| inked[k]).collect();
            let mut visible: Vec<(usize, char)> = chars.iter().copied().enumerate().filter(|c| !c.1.is_whitespace()).collect();
            if visible.len() == at.len() + 1 && visible.len() >= 3 && visible[0].1 == '|' && visible[1].0 == visible[0].0 + 1 {
                visible.remove(0);
                chars[0..].iter_mut().take_while(|c| **c == ' ' || **c == '|').for_each(|c| *c = ' ');
            }
            let near = at.len() == visible.len()
                && at.iter().zip(&visible).enumerate().all(|(n, (&k, v))| k.abs_diff(v.0) <= if n == 0 { 1 } else { 2 });
            let extra = at.len().saturating_sub(visible.len());
            let shifted = (1..=3).contains(&extra) && !visible.is_empty() && at[extra] > at[extra - 1] + 1 && {
                let d: Vec<i64> = at[extra..].iter().zip(&visible).map(|(&k, v)| k as i64 - v.0 as i64).collect();
                let m = d[0];
                d.iter().take(3).all(|&v| v == m) && d.iter().filter(|&&v| v == m).count() * 10 >= d.len() * 9
            } && at[..extra].iter().all(|&k| glyphs::classify(img, bg, g, k, rr).is_some());
            if !at.is_empty() && (near || shifted) {
                let mut placed = vec![' '; at[at.len() - 1] + 1];
                for (&k, v) in at[at.len() - visible.len()..].iter().zip(&visible) {
                    placed[k] = v.1;
                }
                chars = placed;
            } else if let (Some(&k0), Some(v0)) = (at.first(), visible.first()) {
                if k0.abs_diff(v0.0) == 1 {
                    if k0 > v0.0 {
                        chars.insert(0, ' ');
                    } else {
                        chars.remove(0);
                    }
                }
            }
            for k in 0..(chars.len() + 3).min(width.max(chars.len())) {
                let c = chars.get(k).copied().unwrap_or(' ');
                if c != ' ' && c != 'A' {
                    continue;
                }
                match glyphs::classify(img, bg, g, k, rr) {
                    Some('^' | '─') if c == 'A' => chars[k] = '^',
                    Some(s) if c == ' ' => {
                        while chars.len() <= k {
                            chars.push(' ');
                        }
                        chars[k] = s;
                        if is_box(s) {
                            drawn.push((r, k));
                        }
                    }
                    _ => {}
                }
            }
        } else {
            let mut found = Vec::new();
            let mut unknown = false;
            for k in 0..width {
                if !glyphs::inked(img, bg, g, k, rr) {
                    continue;
                }
                match glyphs::classify_alone(img, bg, g, k, rr, db - dt) {
                    Some(s) => found.push((k, s)),
                    None => {
                        unknown = true;
                        break;
                    }
                }
            }
            let ticks: Vec<usize> = found.iter().filter(|f| f.1 == '`').map(|f| f.0).collect();
            if !ticks.is_empty() {
                let fence = ticks.len() >= 3 && ticks.len() == found.len() && ticks.windows(2).all(|w| w[1] == w[0] + 1);
                if !fence {
                    unknown = true;
                }
            }
            if !unknown && !found.is_empty() {
                chars = vec![' '; found.last().unwrap().0 + 1];
                for (k, s) in found {
                    chars[k] = s;
                    if is_box(s) {
                        drawn.push((r, k));
                    }
                }
            }
        }
        let mut k = 0;
        while k < chars.len() {
            if !is_box(chars[k]) {
                k += 1;
                continue;
            }
            let start = k;
            while k < chars.len() && is_box(chars[k]) {
                k += 1;
            }
            let run = &chars[start..k];
            if run.iter().all(|&c| c == '─') && run.len() < 3 {
                let caret = (start > 0 && chars[start - 1] == '^') || chars.get(k) == Some(&'^');
                for c in &mut chars[start..k] {
                    *c = if caret { '^' } else { ' ' };
                }
            }
        }
        let after: String = chars.into_iter().collect::<String>().trim_end().to_string();
        if after != before.trim_end() {
            if debug {
                eprintln!("  symbols re-read: \"{before}\" -> \"{after}\"");
            }
            row.0 = after;
        }
    }
    let grid: Vec<Vec<char>> = out.iter().map(|r| r.0.chars().collect()).collect();
    let at = |r: usize, k: usize| grid.get(r).and_then(|row| row.get(k)).copied().is_some_and(is_box);
    for &(r, k) in &drawn {
        if !grid.get(r).and_then(|row| row.get(k)).copied().is_some_and(is_box) {
            continue;
        }
        let linked = (r > 0 && at(r - 1, k)) || at(r + 1, k) || (k > 0 && at(r, k - 1)) || at(r, k + 1);
        if !linked {
            let mut cs: Vec<char> = out[r].0.chars().collect();
            cs[k] = ' ';
            out[r].0 = cs.into_iter().collect::<String>().trim_end().to_string();
        }
    }
    if !out.iter().any(|r| r.0.contains(['├', '└', '┌', '┐', '┘', '┤', '┬', '┴', '┼', '─'])) {
        for r in out.iter_mut() {
            if r.0.contains('│') {
                r.0 = r.0.replace('│', "|");
            }
        }
    }
}

fn strip_line_numbers(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let num = |l: &str| -> Option<(u32, usize)> {
        let t = l.trim_start();
        let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if d.is_empty() || d.len() > 4 {
            return None;
        }
        let rest = &t[d.len()..];
        if !(rest.is_empty() || rest.starts_with(' ')) {
            return None;
        }
        Some((d.parse().ok()?, l.len() - rest.len()))
    };
    let nums: Vec<Option<(u32, usize)>> = lines.iter().map(|l| num(l)).collect();
    let numbered = nums.iter().filter(|n| n.is_some()).count();
    if lines.len() < 3 || numbered * 10 < lines.len() * 8 {
        return text.to_string();
    }
    let seq: Vec<u32> = nums.iter().flatten().map(|n| n.0).collect();
    let steps_ok = seq.windows(2).filter(|w| w[1] == w[0] + 1).count();
    if (steps_ok + 1) * 10 < seq.len() * 8 {
        return text.to_string();
    }
    let rests: Vec<String> = lines
        .iter()
        .zip(&nums)
        .map(|(l, n)| match n {
            Some((_, cut)) => l[*cut..].to_string(),
            None => l.to_string(),
        })
        .collect();
    let common = rests
        .iter()
        .filter(|r| !r.trim().is_empty())
        .map(|r| r.len() - r.trim_start().len())
        .min()
        .unwrap_or(0);
    rests
        .iter()
        .map(|r| if r.trim().is_empty() { String::new() } else { r[common.min(r.len() - r.trim_start().len())..].to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mono_line(chars: &[(char, f32)], pitch: f32) -> String {
    let mut out = String::new();
    let mut prev: Option<f32> = None;
    let mut saw_space = false;
    for &(c, x) in chars {
        if c.is_whitespace() {
            saw_space = true;
            continue;
        }
        if let Some(px) = prev {
            let g = (x - px) / pitch;
            let cells = g.round() as i32;
            let n = if saw_space {
                if g < 2.6 { 1 } else { (cells - 1).max(1) }
            } else if cells >= 3 {
                cells - 1
            } else {
                0
            };
            out.push_str(&" ".repeat(n.clamp(0, 60) as usize));
        }
        out.push(c);
        prev = Some(x);
        saw_space = false;
    }
    out
}

fn fix_comment(line: &str) -> String {
    let mut l = line.to_string();
    let opens = |s: &str| s.contains("/*") || s.contains("/k ") || s.contains("/ *");
    let closes = |s: &str| s.contains("*/") || s.contains(" k/") || s.contains("* /");
    if opens(&l) && closes(&l) {
        l = l.replacen("/k ", "/* ", 1).replacen("/ * ", "/* ", 1);
        if let Some(p) = l.rfind(" k/") {
            l.replace_range(p..p + 3, " */");
        }
        if let Some(p) = l.rfind(" * /") {
            l.replace_range(p..p + 4, " */");
        }
    }
    l
}

fn fix_code(text: &str) -> String {
    let windbg = text.split(|c: char| !(c.is_ascii_hexdigit() || c == '`')).any(|t| is_windbg_addr(t));
    text.split('\n')
        .map(|line| {
            let line = &fix_comment(line);
            let mut toks: Vec<String> = Vec::new();
            let mut cur = String::new();
            for c in line.chars() {
                if c.is_alphanumeric() || c == '_' || c == '`' {
                    cur.push(c);
                } else {
                    if !cur.is_empty() {
                        toks.push(std::mem::take(&mut cur));
                    }
                    toks.push(c.to_string());
                }
            }
            if !cur.is_empty() {
                toks.push(cur);
            }
            let operandish = line.contains([';', '(', '[', '=', '{']);
            let n = toks.len();
            let mut out = String::new();
            for i in 0..n {
                let t = &toks[i];
                let mut t = fix_code_token(t);
                if windbg {
                    t = fix_windbg(&t);
                }
                if (t == "I" || t == "l")
                    && operandish
                    && i >= 2
                    && i + 2 < n
                    && toks[i - 1] == " "
                    && toks[i + 1] == " "
                    && toks[i - 2].chars().last().is_some_and(|c| c.is_alphanumeric() || c == ']' || c == ')')
                    && toks[i + 2].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '(')
                {
                    t = "|".to_string();
                }
                out.push_str(&t);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fix_code_token(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    if c.len() < 2 {
        return t.to_string();
    }
    let mut c = c;
    if (c[0] == 'O' || c[0] == 'o') && (c[1] == 'x' || c[1] == 'X') && c.get(2).is_some_and(|d| d.is_ascii_hexdigit()) {
        c[0] = '0';
    }
    let hexlike = c.iter().all(|d| d.is_ascii_hexdigit() || *d == '`') || (c[0] == '0' && (c[1] == 'x' || c[1] == 'X'));
    if !hexlike {
        let caps = c.iter().filter(|d| d.is_ascii_uppercase()).count() >= 3
            && c.iter().all(|d| d.is_ascii_uppercase() || *d == '_' || *d == '0');
        for i in 0..c.len() {
            let after_letter = i == 0 || c[i - 1].is_alphabetic() || (caps && c[i - 1] == '_');
            let before_letter =
                c.get(i + 1).is_some_and(|d| d.is_alphabetic() && *d != 'x') || (caps && c.get(i + 1).is_none_or(|d| *d == '_'));
            if c[i] == '0' && after_letter && before_letter {
                c[i] = 'O';
            }
        }
    }
    c.into_iter().collect()
}

fn is_windbg_addr(t: &str) -> bool {
    let b = t.as_bytes();
    t.len() == 17 && b[8] == b'`' && t.chars().enumerate().all(|(i, c)| i == 8 || c.is_ascii_hexdigit())
}

fn fix_windbg(t: &str) -> String {
    let hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
    let plain = t.replace('`', "");
    if hex(&plain) && (plain.len() == 16 || plain.len() == 32) && t.len() <= 34 {
        let fmt = |a: &str| format!("{}`{}", &a[..8], &a[8..]);
        return if plain.len() == 16 { fmt(&plain) } else { format!("{} {}", fmt(&plain[..16]), fmt(&plain[16..])) };
    }
    t.to_string()
}

fn grid_deviation(l: &VLine) -> Option<f32> {
    let xs: Vec<f32> = l.chars.iter().map(|c| c.1).collect();
    if xs.len() < 12 {
        return None;
    }
    let n = xs.len() - 1;
    let pitch = (xs[n] - xs[0]) / n as f32;
    if pitch <= 1.0 {
        return None;
    }
    let dev = xs.iter().enumerate().map(|(i, &x)| (x - (xs[0] + pitch * i as f32)).abs()).sum::<f32>() / xs.len() as f32;
    Some(dev / pitch)
}

const MONO_MAX: f32 = 0.12;

fn looks_like_code(lines: &[VLine], img: Option<&RgbImage>) -> bool {
    if lines.len() == 1 && grid_deviation(&lines[0]).is_some_and(|d| d < 0.25) {
        return true;
    }
    let text: String = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    let n = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let sym = text.chars().filter(|c| "{}[]()<>=;:/\\_|`$#@*+\"'&%".contains(*c)).count();
    let sym_ratio = sym as f32 / n as f32;
    let digits = text.chars().filter(|c| c.is_ascii_digit()).count() as f32 / n as f32;
    if lines.len() >= 2 && digits > 0.2 {
        return true;
    }
    let mut devs: Vec<f32> = lines.iter().filter_map(grid_deviation).collect();
    if devs.len() >= 2 && median(&mut devs) < 0.2 {
        return true;
    }
    if let Some(img) = img.filter(|_| lines.len() >= 3) {
        let xh = {
            let mut v: Vec<f32> = lines.iter().map(|l| l.xh).collect();
            median(&mut v)
        };
        let rows: Vec<(f32, f32, Rgb)> = lines.iter().map(|l| (l.ink.y0, l.ink.y1, l.bg)).collect();
        let x0 = lines.iter().map(|l| l.ink.x0).fold(f32::MAX, f32::min);
        let x1 = lines.iter().map(|l| l.ink.x1).fold(f32::MIN, f32::max);
        let score = crate::glyphs::mono_score(img, &rows, x0 - 1.0, x1 + 1.0, xh * 0.9, xh * 1.9);
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!("  monospace? {:?} \"{}\"", score.map(|s| (s.0, s.1.pitch)), lines[0].text);
        }
        if score.is_some_and(|s| s.0 < MONO_MAX) {
            return true;
        }
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let plain = words
        .iter()
        .filter(|w| {
            let core = w.trim_matches(|c: char| ",.;:!?()\"'«»".contains(c));
            core.chars().count() >= 2 && core.chars().all(|c| c.is_alphabetic() || c == '-' || c == '\'')
        })
        .count();
    if words.len() >= 8 && plain * 10 >= words.len() * 7 && sym_ratio < 0.12 {
        return false;
    }
    if lines.len() >= 2 {
        let p: Vec<f32> = lines
            .iter()
            .filter(|l| l.text.chars().count() >= 6)
            .flat_map(|l| l.pitch_samples.iter().cloned())
            .collect();
        if p.len() >= 2 {
            let m = median(&mut p.clone());
            let dev = p.iter().map(|v| (v - m).abs()).sum::<f32>() / p.len() as f32 / m.max(1.0);
            if dev < 0.04 {
                return true;
            }
        }
        return sym_ratio > 0.06;
    }
    sym_ratio > 0.12
}

pub fn render(blocks: &[Block], image_link: &dyn Fn(usize) -> String) -> String {
    let mut out = String::new();
    let mut prev_item = false;
    let mut content_col: Vec<usize> = Vec::new();
    for b in blocks {
        let is_item = matches!(b, Block::ListItem { .. });
        if !out.is_empty() {
            out.push('\n');
            if !(is_item && prev_item) {
                out.push('\n');
            }
        }
        match b {
            Block::Heading { level, text } => {
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                out.push_str(text);
            }
            Block::Paragraph { text } => out.push_str(&protect_line_start(text)),
            Block::ListItem { ordered, level, text } => {
                let level = *level as usize;
                if !prev_item {
                    content_col.clear();
                }
                content_col.truncate(level);
                let indent = content_col.last().copied().unwrap_or(0);
                let marker = ordered.clone().unwrap_or_else(|| "-".to_string());
                while content_col.len() < level {
                    content_col.push(indent);
                }
                content_col.push(indent + marker.chars().count() + 1);
                out.push_str(&" ".repeat(indent));
                out.push_str(&marker);
                out.push(' ');
                let pad = " ".repeat(indent + marker.chars().count() + 1);
                out.push_str(&text.replace('\n', &format!("\n{pad}")));
            }
            Block::Code { text, lang } => {
                let run = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
                let fence = "`".repeat(run.max(2) + 1);
                out.push_str(&fence);
                out.push_str(lang.unwrap_or(""));
                out.push('\n');
                out.push_str(text);
                out.push('\n');
                out.push_str(&fence);
            }
            Block::Quote { text } => {
                out.push_str("> ");
                out.push_str(text);
            }
            Block::Table { rows } => {
                let n = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                let cell = |r: &Vec<String>, i: usize| r.get(i).map(|c| c.replace('|', "\\|")).unwrap_or_default();
                for (ri, r) in rows.iter().enumerate() {
                    let cells: Vec<String> = (0..n).map(|i| cell(r, i)).collect();
                    out.push_str(&format!("| {} |", cells.join(" | ")));
                    if ri == 0 {
                        out.push_str(&format!("\n|{}", " --- |".repeat(n)));
                    }
                    if ri + 1 < rows.len() {
                        out.push('\n');
                    }
                }
            }
            Block::Image { index } => out.push_str(&image_link(*index)),
        }
        prev_item = is_item;
    }
    out.push('\n');
    out
}

fn protect_line_start(t: &str) -> String {
    let first = t.chars().next();
    if matches!(first, Some('#') | Some('>') | Some('-') | Some('+')) {
        return format!("\\{t}");
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && t[digits.len()..].starts_with(". ") {
        return format!("{}\\{}", digits, &t[digits.len()..]);
    }
    t.to_string()
}

struct Grid {
    rect: Rect,
    lines: Vec<usize>,
    rows: Vec<Vec<(bool, String)>>,
}

impl Grid {
    fn blocks(&self) -> Vec<Block> {
        if self.rows.len() >= 2 && self.rows.iter().all(|r| r.len() == 2 && r[0].1.chars().count() == 1) {
            return self
                .rows
                .iter()
                .filter(|r| !r[1].1.is_empty())
                .map(|r| Block::ListItem { ordered: None, level: 0, text: escape(&r[1].1) })
                .collect();
        }
        let cells: Vec<&(bool, String)> = self.rows.iter().flatten().collect();
        let with_marker = cells.iter().filter(|c| c.0).count();
        if with_marker * 4 >= cells.len() {
            return cells
                .iter()
                .filter(|c| !c.1.is_empty())
                .map(|c| Block::ListItem { ordered: None, level: 0, text: escape(&c.1) })
                .collect();
        }
        vec![Block::Table { rows: self.rows.iter().map(|r| r.iter().map(|c| escape(&c.1)).collect()).collect() }]
    }
}

fn table_rows(img: &RgbImage, t: &crate::panels::TextTable, members: &[&Line]) -> Vec<Vec<String>> {
    let mut rows = vec![vec![String::new(); t.n_cols]; t.n_rows];
    let mut per_cell: Vec<Vec<&Line>> = vec![Vec::new(); t.n_rows * t.n_cols];
    let mut cell_bg: Vec<Rgb> = vec![[255, 255, 255]; t.n_rows * t.n_cols];
    for l in members {
        let r = l.rect();
        if let Some((ri, ci, bg)) = t.cell_at(r.cx(), r.cy()) {
            per_cell[ri * t.n_cols + ci].push(l);
            cell_bg[ri * t.n_cols + ci] = bg;
        }
    }
    let mode = |pts: &mut dyn Iterator<Item = (u32, u32)>| -> Option<Rgb> {
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
    };
    let (iw, ih) = img.dimensions();
    for c in t.cells.iter() {
        let r = c.4.expand(-3.0);
        let (x0, y0, w, h) = r.to_px(iw, ih);
        if w < 8 || h < 8 {
            continue;
        }
        let ring = (x0..x0 + w)
            .flat_map(|x| [y0, y0 + 1, y0 + 2, y0 + h - 3, y0 + h - 2, y0 + h - 1].into_iter().map(move |y| (x, y)))
            .chain((y0..y0 + h).flat_map(|y| [x0, x0 + 1, x0 + 2, x0 + w - 3, x0 + w - 2, x0 + w - 1].into_iter().map(move |x| (x, y))));
        if let Some(m) = mode(&mut ring.into_iter()) {
            cell_bg[c.0 * t.n_cols + c.1] = m;
        }
    }
    let on_pill = |l: &Line, w: &crate::style::Word, bg: Rgb| -> bool {
        let ink = l.ink();
        let (x0, y0, ww, hh) = Rect::new(w.x0, ink.y0, w.x1, ink.y1).to_px(iw, ih);
        if ww * hh < 8 {
            return false;
        }
        let mut pts = (y0..y0 + hh).flat_map(|y| (x0..x0 + ww).map(move |x| (x, y)));
        mode(&mut pts).is_some_and(|m| color_dist(m, bg) > 16.0)
    };
    for (idx, ls) in per_cell.iter_mut().enumerate() {
        if ls.is_empty() {
            continue;
        }
        let bg = cell_bg[idx];
        ls.sort_by(|a, b| {
            let (ra, rb) = (a.rect(), b.rect());
            if ra.v_overlap(&rb) > 0.5 { ra.x0.partial_cmp(&rb.x0).unwrap() } else { ra.y0.partial_cmp(&rb.y0).unwrap() }
        });
        let mut words: Vec<(String, bool, bool)> = Vec::new();
        for l in ls.iter() {
            let ws: Vec<(String, bool, bool)> = if l.st.words.is_empty() {
                l.text().split_whitespace().map(|w| (w.to_string(), false, true)).collect()
            } else {
                let mut prev: Option<&crate::style::Word> = None;
                l.st.words
                    .iter()
                    .map(|w| {
                        let code = w.code || on_pill(l, w, bg);
                        let sep = prev.is_none_or(|p| {
                            let cw = |w: &crate::style::Word| (w.x1 - w.x0) / w.text.chars().count().max(1) as f32;
                            w.x0 - p.x1 >= 0.5 * cw(p).min(cw(w))
                        });
                        prev = Some(w);
                        (w.text.clone(), code, sep)
                    })
                    .collect()
            };
            let mut ws = ws.into_iter().peekable();
            if let (Some(last), Some(first)) = (words.last_mut(), ws.peek()) {
                if last.0.ends_with('-') && !last.0.ends_with("--") && first.0.chars().next().is_some_and(|c| c.is_alphanumeric()) {
                    let first = ws.next().unwrap();
                    last.0.push_str(&first.0);
                    last.1 = last.1 || first.1;
                }
            }
            words.extend(ws);
        }
        let mut out = String::new();
        let mut i = 0;
        while i < words.len() {
            if !out.is_empty() {
                out.push(' ');
            }
            if words[i].1 {
                let mut j = i;
                while j < words.len() && words[j].1 {
                    j += 1;
                }
                let mut joined = String::new();
                for (k, w) in words[i..j].iter().enumerate() {
                    if k > 0 && w.2 {
                        joined.push(' ');
                    }
                    joined.push_str(&w.0);
                }
                let core = code_core(&joined);
                out.push('`');
                out.push_str(core.trim_matches('`'));
                out.push('`');
                out.push_str(&joined[core.len()..]);
                i = j;
            } else {
                out.push_str(&escape(&words[i].0));
                i += 1;
            }
        }
        rows[idx / t.n_cols][idx % t.n_cols] = out;
    }
    if let Some(head) = rows.first_mut() {
        fix_header_row(head);
    }
    rows
}

fn fix_header_row(head: &mut [String]) {
    let filled = head.iter().filter(|c| !c.is_empty()).count();
    let caps = head.iter().filter(|c| c.chars().next().is_some_and(|ch| ch.is_uppercase())).count();
    if filled < 2 || caps + 1 < filled {
        return;
    }
    for c in head.iter_mut() {
        let mut it = c.chars();
        if let (Some('l'), Some(n)) = (it.next(), it.next()) {
            if n.is_lowercase() {
                c.replace_range(0..1, "I");
            }
        }
    }
}

fn code_core(joined: &str) -> &str {
    let mut core = joined;
    loop {
        let before = core;
        core = core.trim_end_matches([',', '.', ';', ':']);
        if core.ends_with(')') {
            let opens = core.matches('(').count();
            let closes = core.matches(')').count();
            if closes > opens {
                core = &core[..core.len() - 1];
            }
        }
        if core == before {
            return core;
        }
    }
}

fn detect_grids(img: &RgbImage, lines: &[Line], body_xh: f32, page_bg: Rgb, exclude: &[Option<usize>]) -> Vec<Grid> {
    let on_page_bg = |k: usize| color_dist(lines[k].st.bg, page_bg) <= 14.0;
    let is_marker = |t: &str| {
        let t = t.trim();
        t.chars().count() == 1 && t.chars().next().is_some_and(|c| BULLETS.contains(&c))
    };
    let mut idx: Vec<usize> = (0..lines.len()).filter(|&k| on_page_bg(k) && exclude[k].is_none()).collect();
    idx.sort_by(|&a, &b| lines[a].rect().cy().partial_cmp(&lines[b].rect().cy()).unwrap());
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for k in idx {
        let r = lines[k].rect();
        match rows.last_mut() {
            Some(row)
                if {
                    let f = lines[row[0]].rect();
                    (r.cy() - f.cy()).abs() < f.h().max(r.h()) * 0.45
                } =>
            {
                row.push(k)
            }
            _ => rows.push(vec![k]),
        }
    }
    struct RowCells {
        y0: f32,
        y1: f32,
        cells: Vec<(f32, Vec<usize>, bool, String)>,
        ok: bool,
    }
    let mut rcs: Vec<RowCells> = Vec::new();
    for mut row in rows {
        row.sort_by(|&a, &b| lines[a].rect().x0.partial_cmp(&lines[b].rect().x0).unwrap());
        let mut cells: Vec<(f32, Vec<usize>, bool, String)> = Vec::new();
        let mut pending_marker: Option<(f32, usize)> = None;
        let mut last_x1 = f32::MIN;
        for &k in &row {
            let l = &lines[k];
            let r = l.ink();
            let xh = l.st.xh.max(body_xh * 0.6);
            let t = l.text().to_string();
            if is_marker(&t) {
                pending_marker = Some((r.x0, k));
                continue;
            }
            if let Some((mx, mk)) = pending_marker.take() {
                if r.x0 - mx < xh * 5.0 {
                    cells.push((r.x0, vec![mk, k], true, t));
                    last_x1 = r.x1;
                    continue;
                }
            }
            if let Some(c) = cells.last_mut() {
                if r.x0 - last_x1 < xh * 1.5 {
                    c.1.push(k);
                    c.3.push(' ');
                    c.3.push_str(&t);
                    last_x1 = r.x1;
                    continue;
                }
            }
            let (m, rest) = list_marker(&t);
            let blob = style::has_bullet_blob(img, l, [255, 255, 255]);
            let x_text = if m.is_some() { r.x0 + xh * 2.6 } else { r.x0 };
            cells.push((x_text, vec![k], m.is_some() || blob, if m.is_some() { rest } else { t }));
            last_x1 = r.x1;
        }
        let y0 = row.iter().map(|&k| lines[k].ink().y0).fold(f32::MAX, f32::min);
        let y1 = row.iter().map(|&k| lines[k].ink().y1).fold(f32::MIN, f32::max);
        let ok = cells.len() >= 2 && cells.iter().all(|c| c.3.chars().count() <= 60) && pending_marker.is_none();
        rcs.push(RowCells { y0, y1, cells, ok });
    }
    let tol = (body_xh * 3.5).max(24.0);
    let mut out = Vec::new();
    let mut i = 0;
    while i < rcs.len() {
        if !rcs[i].ok {
            i += 1;
            continue;
        }
        let anchors: Vec<f32> = rcs[i].cells.iter().map(|c| c.0).collect();
        let row_h = rcs[i].y1 - rcs[i].y0;
        let mut j = i + 1;
        while j < rcs.len() {
            let r = &rcs[j];
            let gap = r.y0 - rcs[j - 1].y1;
            let aligned = r.cells.iter().all(|c| anchors.iter().any(|a| (a - c.0).abs() < tol));
            if !(r.ok || r.cells.len() >= 2)
                || !aligned
                || gap > row_h * 2.5
                || r.cells.iter().any(|c| c.3.chars().count() > 60)
            {
                break;
            }
            j += 1;
        }
        if j - i >= 2 && anchors.len() >= 2 {
            let mut g = Grid { rect: Rect::default(), lines: Vec::new(), rows: Vec::new() };
            let mut first = true;
            for r in &rcs[i..j] {
                let mut row: Vec<(bool, String)> = vec![(false, String::new()); anchors.len()];
                for c in &r.cells {
                    let col = anchors
                        .iter()
                        .enumerate()
                        .min_by(|a, b| (a.1 - c.0).abs().partial_cmp(&(b.1 - c.0).abs()).unwrap())
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    row[col] = (c.2, c.3.trim().to_string());
                    for &k in &c.1 {
                        g.lines.push(k);
                        let ir = lines[k].ink();
                        g.rect = if first { ir } else { g.rect.union(&ir) };
                        first = false;
                    }
                }
                while row.len() > 1 && row.last().is_some_and(|c| c.1.is_empty()) {
                    row.pop();
                }
                g.rows.push(row);
            }
            out.push(g);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_capitals() {
        assert_eq!(fix_comment("  /k root component options */"), "  /* root component options */");
        assert_eq!(fix_comment("const app = createApp(/ * ... * /)"), "const app = createApp(/* ... */)");
        assert_eq!(fix_comment("a / k"), "a / k");
        assert_eq!(fix_code_token("FASTAPI_0TEL_AUT0_CONFIGURE"), "FASTAPI_OTEL_AUTO_CONFIGURE");
        assert_eq!(fix_code_token("v0_1"), "v0_1");
        assert_eq!(fix_code_token("HTTP_404"), "HTTP_404");
    }

    #[test]
    fn line_numbers_removed() {
        let t = " 9   fn main() {\n10       let x = 1;\n11\n12   }";
        assert_eq!(strip_line_numbers(t), "fn main() {\n    let x = 1;\n\n}");
        let t = "1 + 2\n40 * 3\n7 - 1";
        assert_eq!(strip_line_numbers(t), t);
    }

    #[test]
    fn hollow_bullet_read_as_o() {
        let c = |s: &str, xs: &[f32]| s.chars().zip(xs.iter().copied()).collect::<Vec<_>>();
        assert!(hollow_bullet_o(&c("oClavis", &[390.0, 412.0, 423.0, 433.0, 443.0, 453.0, 463.0])));
        assert!(!hollow_bullet_o(&c("oAuth", &[100.0, 110.0, 120.0, 130.0, 140.0])));
        assert!(!hollow_bullet_o(&c("ou bien", &[100.0, 110.0, 120.0, 130.0, 140.0, 150.0, 160.0])));
    }

    #[test]
    fn code_keeps_matched_parentheses() {
        assert_eq!(code_core("amet()"), "amet()");
        assert_eq!(code_core("amet(),"), "amet()");
        assert_eq!(code_core("x)"), "x");
        assert_eq!(code_core("f(a)."), "f(a)");
        assert_eq!(code_core("sit:"), "sit");
    }

    #[test]
    fn l_before_an_uppercase_acronym() {
        assert_eq!(fix_prose_word("ProglD", None), "ProgID");
        assert_eq!(fix_prose_word("ApplDs", None), "AppIDs");
        assert_eq!(fix_prose_word("VirtualAlloc", None), "VirtualAlloc");
        assert_eq!(fix_prose_word("VirtualAllocEx", None), "VirtualAllocEx");
    }

    #[test]
    fn heading_initial_l_becomes_i() {
        let mut h: Vec<String> = ["Modus", "lussum", "Effectus"].map(String::from).to_vec();
        fix_header_row(&mut h);
        assert_eq!(h, ["Modus", "Iussum", "Effectus"]);
        let mut h: Vec<String> = ["nom", "lieu", "date"].map(String::from).to_vec();
        fix_header_row(&mut h);
        assert_eq!(h, ["nom", "lieu", "date"]);
    }
}
