use crate::geom::Rect;

const MIN_VGAP: f32 = 18.0;

pub fn reading_order(rects: &[Rect]) -> Vec<usize> {
    let idx: Vec<usize> = (0..rects.len()).collect();
    let mut out = Vec::with_capacity(rects.len());
    xy(rects, idx, &mut out, 0);
    out
}

fn intervals(items: &[usize], rects: &[Rect], vertical: bool) -> Vec<(f32, f32, Vec<usize>)> {
    let mut v: Vec<usize> = items.to_vec();
    let key = |i: usize| if vertical { (rects[i].x0, rects[i].x1) } else { (rects[i].y0, rects[i].y1) };
    v.sort_by(|&a, &b| key(a).0.partial_cmp(&key(b).0).unwrap());
    let mut groups: Vec<(f32, f32, Vec<usize>)> = Vec::new();
    for i in v {
        let (a, b) = key(i);
        match groups.last_mut() {
            Some(g) if a < g.1 => {
                g.1 = g.1.max(b);
                g.2.push(i);
            }
            _ => groups.push((a, b, vec![i])),
        }
    }
    groups
}

fn xy(rects: &[Rect], items: Vec<usize>, out: &mut Vec<usize>, depth: usize) {
    if items.len() <= 1 || depth > 64 {
        out.extend(items);
        return;
    }
    let bands = intervals(&items, rects, false);
    if bands.len() > 1 {
        let mut i = 0;
        while i < bands.len() {
            if let Some((end, gutter)) = column_run(rects, &bands, i) {
                let mut left = Vec::new();
                let mut right = Vec::new();
                for b in &bands[i..end] {
                    for &k in &b.2 {
                        if rects[k].cx() < gutter {
                            left.push(k);
                        } else {
                            right.push(k);
                        }
                    }
                }
                xy(rects, left, out, depth + 1);
                xy(rects, right, out, depth + 1);
                i = end;
            } else {
                xy(rects, bands[i].2.clone(), out, depth + 1);
                i += 1;
            }
        }
        return;
    }
    let cols = intervals(&items, rects, true);
    let mut split: Vec<Vec<usize>> = Vec::new();
    let mut prev_end = f32::MIN;
    for c in cols {
        if !split.is_empty() && c.0 - prev_end < MIN_VGAP {
            split.last_mut().unwrap().extend(c.2);
        } else {
            split.push(c.2);
        }
        prev_end = prev_end.max(c.1);
    }
    if split.len() > 1 {
        for s in split {
            xy(rects, s, out, depth + 1);
        }
        return;
    }
    let mut v = items;
    v.sort_by(|&a, &b| rects[a].cy().partial_cmp(&rects[b].cy()).unwrap());
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for k in v {
        match rows.last_mut() {
            Some(r) if rects[r[0]].v_overlap(&rects[k]) > 0.5 => r.push(k),
            _ => rows.push(vec![k]),
        }
    }
    for mut r in rows {
        r.sort_by(|&a, &b| rects[a].x0.partial_cmp(&rects[b].x0).unwrap());
        out.extend(r);
    }
}

fn column_run(rects: &[Rect], bands: &[(f32, f32, Vec<usize>)], start: usize) -> Option<(usize, f32)> {
    let first = &bands[start];
    let cols = intervals(&first.2, rects, true);
    if cols.len() < 2 {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for w in cols.windows(2) {
        let (mut g0, mut g1) = (w[0].1, w[1].0);
        if g1 - g0 < 32.0 {
            continue;
        }
        let mut end = start + 1;
        let mut both = 1;
        let mut left_n = 0;
        let mut right_n = 0;
        while end < bands.len() {
            let b = &bands[end];
            let crosses = b.2.iter().any(|&k| rects[k].x0 < g1 - 1.0 && rects[k].x1 > g0 + 1.0);
            if crosses {
                let mut ng0 = g0;
                let mut ng1 = g1;
                for &k in &b.2 {
                    let r = rects[k];
                    if r.x1 <= (g0 + g1) / 2.0 {
                        ng0 = ng0.max(r.x1);
                    } else if r.x0 >= (g0 + g1) / 2.0 {
                        ng1 = ng1.min(r.x0);
                    } else {
                        ng1 = ng0;
                    }
                }
                if ng1 - ng0 < 32.0 {
                    break;
                }
                g0 = ng0;
                g1 = ng1;
            }
            let l = b.2.iter().filter(|&&k| rects[k].x1 <= g0 + 1.0).count();
            let r = b.2.iter().filter(|&&k| rects[k].x0 >= g1 - 1.0).count();
            left_n += l;
            right_n += r;
            if l > 0 && r > 0 {
                both += 1;
            }
            end += 1;
        }
        let height: f32 = bands[end - 1].1 - first.0;
        if end - start >= 3 && both >= 2 && left_n >= 2 && right_n >= 2 && height > 0.0 {
            let x = (g0 + g1) / 2.0;
            if best.map(|b| end > b.0).unwrap_or(true) {
                best = Some((end, x));
            }
        }
    }
    best
}
