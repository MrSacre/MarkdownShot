use std::collections::HashMap;

use image::RgbImage;

use crate::geom::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Push {
    First,
    Unchanged,
    Scrolled(i32),
    Gap,
}

#[derive(Clone)]
struct Frame {
    rgb: RgbImage,
    luma: Vec<u8>,
}

impl Frame {
    fn new(rgb: RgbImage) -> Frame {
        let luma =
            rgb.pixels().map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8).collect();
        Frame { rgb, luma }
    }
}

#[derive(Clone)]
pub struct Stitcher {
    w: u32,
    h: u32,
    canvas: Vec<u8>,
    state: Vec<u8>,
    dist: Vec<u16>,
    pasted: Vec<(i64, bool)>,
    rows: usize,
    top: i64,
    offset: i64,
    prev: Option<Frame>,
    pending: Option<(i64, bool)>,
    overlays: Vec<Rect>,
    pub frames_used: usize,
    pub frames_seen: usize,
    pub gaps: usize,
    settled: bool,
    small_seen: Vec<Rect>,
    recent: Vec<i32>,
}

impl Default for Stitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Stitcher {
    pub fn new() -> Self {
        Stitcher {
            w: 0,
            h: 0,
            canvas: Vec::new(),
            state: Vec::new(),
            dist: Vec::new(),
            pasted: Vec::new(),
            rows: 0,
            top: 0,
            offset: 0,
            prev: None,
            pending: None,
            overlays: Vec::new(),
            frames_used: 0,
            frames_seen: 0,
            gaps: 0,
            settled: false,
            small_seen: Vec::new(),
            recent: Vec::new(),
        }
    }

    pub fn settled() -> Self {
        Stitcher { settled: true, ..Self::new() }
    }

    pub fn height(&self) -> u32 {
        let pending = self.pending.map(|(off, _)| (off - self.top).max(0) as u32 + self.h).unwrap_or(0);
        (self.rows as u32).max(pending)
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn overlays(&self) -> &[Rect] {
        &self.overlays
    }

    pub fn push(&mut self, img: RgbImage) -> Push {
        self.push_expect(img, None)
    }

    pub fn push_expect(&mut self, img: RgbImage, expect: Option<i32>) -> Push {
        self.frames_seen += 1;
        let frame = Frame::new(img);
        let Some(prev) = self.prev.take() else {
            self.w = frame.rgb.width();
            self.h = frame.rgb.height();
            self.pending = Some((0, true));
            self.prev = Some(frame);
            self.frames_used = 1;
            return Push::First;
        };
        if frame.rgb.dimensions() != (self.w, self.h) {
            self.prev = Some(prev);
            return Push::Unchanged;
        }
        let mut est = self.estimate(&prev, &frame);
        if let Some(e) = expect.filter(|&e| e > 0 && (e as u32) < self.h) {
            est = self.with_expectation(&prev, &frame, est, e);
        } else if est.is_none() && self.recent.len() >= 2 {
            let mut r = self.recent.clone();
            r.sort();
            let e = r[r.len() / 2];
            if e > 0 && (e as u32) < self.h {
                est = self.with_expectation(&prev, &frame, None, e);
            }
        }
        match est {
            Some(d) if d != 0 => {
                self.recent.push(d);
                if self.recent.len() > 3 {
                    self.recent.remove(0);
                }
            }
            Some(_) => {}
            None => self.recent.clear(),
        }
        let result = match est {
            Some(0) => {
                let first = self.frames_used == 1 && self.offset == 0;
                if first {
                    self.prev = Some(prev);
                    return Push::Unchanged;
                }
                self.pending = Some((self.offset, self.pending.is_some_and(|p| p.1)));
                self.prev = Some(frame);
                return Push::Unchanged;
            }
            Some(dy) => {
                self.detect_overlays(&prev, &frame, dy);
                self.offset += dy as i64;
                Push::Scrolled(dy)
            }
            None => {
                self.gaps += 1;
                self.offset += self.h as i64;
                Push::Gap
            }
        };
        if let Some((off, first)) = self.pending.take() {
            let dy = (self.offset - off) as i32;
            let unsure = if first || result == Push::Gap { None } else { Some(self.transient(&prev, &frame, dy)) };
            self.paste_masked(&prev, off, first, unsure.as_deref());
        }
        self.pending = Some((self.offset, false));
        self.frames_used += 1;
        self.prev = Some(frame);
        result
    }

    fn flush(&mut self) {
        if let (Some((off, first)), Some(f)) = (self.pending.take(), self.prev.take()) {
            self.paste_masked(&f, off, first, None);
            self.prev = Some(f);
        }
    }

    pub fn finish(&self) -> RgbImage {
        if self.pending.is_some() {
            let mut s = self.clone();
            s.flush();
            return s.finish();
        }
        if self.rows == 0 {
            return RgbImage::new(1, 1);
        }
        let mut buf = self.canvas.clone();
        let bg = self.prev.as_ref().map(|f| crate::style::dominant_color(&f.rgb)).unwrap_or([255, 255, 255]);
        for (i, s) in self.state.iter().enumerate() {
            if *s <= 1 {
                buf[i * 3..i * 3 + 3].copy_from_slice(&bg);
            }
        }
        RgbImage::from_raw(self.w, self.rows as u32, buf).expect("consistent dimensions")
    }

    fn in_overlay(&self, x: u32, y: u32) -> bool {
        self.overlays.iter().any(|r| r.contains_point(x as f32 + 0.5, y as f32 + 0.5))
    }

    fn edge_dist(&self, y: u16) -> u32 {
        let mid = self.h as i64 / 2;
        ((y as i64 - mid).abs() - (self.h as i64 * 3 / 10)).max(0) as u32
    }

    fn transient(&self, a: &Frame, b: &Frame, dy: i32) -> Vec<bool> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut m = vec![false; w * h];
        for y in 0..h {
            let yb = y as i32 - dy;
            if yb < 0 || yb >= h as i32 {
                continue;
            }
            for x in 0..w {
                if a.luma[y * w + x].abs_diff(b.luma[yb as usize * w + x]) > 60 && !self.in_overlay(x as u32, yb as u32)
                {
                    m[y * w + x] = true;
                }
            }
        }
        let mut d = m.clone();
        for y in 0..h {
            for x in 0..w {
                if m[y * w + x] {
                    for yy in y.saturating_sub(2)..(y + 3).min(h) {
                        for xx in x.saturating_sub(2)..(x + 3).min(w) {
                            d[yy * w + xx] = true;
                        }
                    }
                }
            }
        }
        d
    }

    fn paste_masked(&mut self, f: &Frame, page_off: i64, first: bool, unsure: Option<&[bool]>) {
        let w = self.w as usize;
        if self.rows == 0 {
            self.top = page_off;
        }
        if page_off < self.top {
            let add = (self.top - page_off) as usize;
            let mut c = vec![0u8; add * w * 3];
            c.extend_from_slice(&self.canvas);
            self.canvas = c;
            let mut s = vec![0u8; add * w];
            s.extend_from_slice(&self.state);
            self.state = s;
            let mut d = vec![u16::MAX; add * w];
            d.extend_from_slice(&self.dist);
            self.dist = d;
            self.rows += add;
            self.top = page_off;
        }
        let need = (page_off - self.top) as usize + self.h as usize;
        if need > self.rows {
            self.canvas.resize(need * w * 3, 0);
            self.state.resize(need * w, 0);
            self.dist.resize(need * w, u16::MAX);
            self.rows = need;
        }
        let base = (page_off - self.top) as usize;
        let has_overlays = !self.overlays.is_empty() && !first;
        self.pasted.push((page_off, first));
        let (fw, fh) = (self.w as f32, self.h as f32);
        let sticky = self.overlays.iter().any(|o| o.w() > fw * 0.6 && (o.y0 <= 1.0 || o.y1 >= fh - 1.0));
        let nocentral = !sticky && !self.settled;
        for y in 0..self.h as usize {
            let row = base + y;
            let d = y.min(u16::MAX as usize - 1) as u16;
            let de = self.edge_dist(d);
            for x in 0..w {
                let under = has_overlays && self.in_overlay(x as u32, y as u32);
                let strong = !under && !unsure.is_some_and(|u| u[y * w + x]);
                let si = row * w + x;
                let cur = self.state[si];
                let better = cur != 2 || self.dist[si] == u16::MAX || de <= self.edge_dist(self.dist[si]) || nocentral;
                let unsure_better = !strong && !under && (cur == 0 || cur == 1);
                if (strong && better) || cur == 0 || unsure_better {
                    let p = f.rgb.get_pixel(x as u32, y as u32).0;
                    self.canvas[si * 3..si * 3 + 3].copy_from_slice(&p);
                    self.state[si] = if strong {
                        2
                    } else if under {
                        1.max(cur)
                    } else if cur == 2 {
                        2
                    } else {
                        3
                    };
                    if strong {
                        self.dist[si] = d;
                    }
                }
            }
        }
    }

    fn row_excluded(&self, y: usize) -> bool {
        let yf = y as f32 + 0.5;
        self.overlays.iter().any(|r| yf >= r.y0 && yf <= r.y1 && r.w() >= self.w as f32 * 0.6)
    }

    fn estimate(&self, a: &Frame, b: &Frame) -> Option<i32> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut pts: Vec<(usize, usize)> = Vec::new();
        let step = if w * h > 1_500_000 { 3 } else { 2 };
        for y in (0..h - 1).step_by(step) {
            for x in (0..w - 1).step_by(step) {
                let i = y * w + x;
                let v = b.luma[i];
                if v.abs_diff(b.luma[i + w]) > 40 {
                    if !self.in_overlay(x as u32, y as u32) {
                        pts.push((x, y));
                    }
                }
            }
        }
        if pts.len() < 50 {
            let diff = a.luma.iter().zip(&b.luma).filter(|(p, q)| p.abs_diff(**q) > 14).count();
            return if (diff as f32) < (w * h) as f32 * 0.01 { Some(0) } else { None };
        }
        let moving: Vec<(usize, usize)> =
            pts.iter().copied().filter(|&(x, y)| b.luma[y * w + x].abs_diff(a.luma[y * w + x]) > 20).collect();
        let still = 1.0 - moving.len() as f32 / pts.len() as f32;
        if still >= 0.8 || moving.len() < 30 {
            return Some(0);
        }
        let mut cands: Vec<i32> = Vec::new();
        {
            let ha = row_hashes(&a.luma, w, h);
            let hb = row_hashes(&b.luma, w, h);
            let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
            for (y, hv) in ha.iter().enumerate() {
                if let Some(hv) = hv {
                    if !self.row_excluded(y) {
                        index.entry(*hv).or_default().push(y);
                    }
                }
            }
            let mut votes: HashMap<i32, u32> = HashMap::new();
            for (y, hv) in hb.iter().enumerate() {
                let Some(hv) = hv else { continue };
                if let Some(list) = index.get(hv) {
                    if list.len() <= 4 {
                        for &ya in list {
                            *votes.entry(ya as i32 - y as i32).or_insert(0) += 1;
                        }
                    }
                }
            }
            let mut v: Vec<(i32, u32)> = votes.into_iter().filter(|(d, n)| *d != 0 && *n >= 3).collect();
            v.sort_by(|x, y| y.1.cmp(&x.1));
            cands.extend(v.iter().take(4).map(|x| x.0));
        }
        {
            let sa = row_signatures(&a.luma, w, h);
            let sb = row_signatures(&b.luma, w, h);
            let info: Vec<usize> = (0..h)
                .filter(|&y| !self.row_excluded(y))
                .filter(|&y| {
                    let r = &sb[y];
                    let (mn, mx) = r.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
                    mx - mn > 3.0
                })
                .collect();
            let min_ov = (h / 8).max(24) as i32;
            let mut scored: Vec<(f32, i32)> = Vec::new();
            for dy in -(h as i32 - min_ov)..=(h as i32 - min_ov) {
                if dy == 0 {
                    continue;
                }
                let mut cost = 0f32;
                let mut n = 0f32;
                for &y in info.iter().step_by(2) {
                    let ya = y as i32 + dy;
                    if ya < 0 || ya >= h as i32 {
                        continue;
                    }
                    cost += sb[y].iter().zip(&sa[ya as usize]).map(|(p, q)| (p - q).abs()).sum::<f32>();
                    n += 1.0;
                }
                if n >= 10.0 {
                    scored.push((cost / n, dy));
                }
            }
            scored.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            cands.extend(scored.iter().take(6).map(|x| x.1));
        }
        cands.sort();
        cands.dedup();
        let score_on = |set: &[(usize, usize)], dy: i32| -> (f32, u32) {
            let (mut ok, mut n) = (0u32, 0u32);
            for &(x, y) in set {
                let ya = y as i32 + dy;
                if ya < 0 || ya >= h as i32 {
                    continue;
                }
                n += 1;
                let d = b.luma[y * w + x].abs_diff(a.luma[ya as usize * w + x]);
                if d <= 8 {
                    ok += 10;
                } else if d <= 28 {
                    ok += 6;
                }
            }
            (if n == 0 { 0.0 } else { ok as f32 / (10 * n) as f32 }, n)
        };
        let score = |dy: i32| -> f32 {
            let (sm, nm) = score_on(&moving, dy);
            if nm >= 30 {
                return sm;
            }
            let (sa, na) = score_on(&pts, dy);
            if na >= 30 { sa } else { 0.0 }
        };
        let mut best: Option<(i32, f32)> = None;
        for &dy in &cands {
            for d in dy - 2..=dy + 2 {
                if d == 0 {
                    continue;
                }
                let sc = score(d);
                if best.is_none_or(|b| sc > b.1) {
                    best = Some((d, sc));
                }
            }
        }
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            let mut sc: Vec<(i32, f32)> = cands.iter().map(|&d| (d, score(d))).collect();
            sc.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            eprintln!(
                "  points {} moving {} still {:.2} best {:?} candidates {:?}",
                pts.len(),
                moving.len(),
                still,
                best,
                &sc[..sc.len().min(6)]
            );
        }
        let distinct = |dy: i32, sc: f32| -> bool {
            let rival = cands.iter().filter(|&&d| (d - dy).abs() > 3).map(|&d| score(d)).fold(0.0f32, f32::max);
            let neigh = [dy - 1, dy + 1].iter().filter(|&&d| d != 0).map(|&d| score(d)).fold(0.0f32, f32::max);
            sc >= 0.3 && sc >= rival * 1.6 && sc >= neigh * 1.4
        };
        match best {
            Some((dy, sc)) if sc >= 0.6 || distinct(dy, sc) => Some(refine_dense(a, b, w, h, dy)),
            _ if still >= 0.5 => Some(0),
            _ => dense_match(a, b, w, h, &cands),
        }
    }

    fn content_diff(&self, a: &Frame, b: &Frame, dy: i32) -> Option<f32> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut rows: Vec<f32> = Vec::new();
        for y in (0..h).step_by(2) {
            let ya = y as i32 + dy;
            if ya < 0 || ya >= h as i32 || self.row_excluded(y) || self.row_excluded(ya as usize) {
                continue;
            }
            let rb = &b.luma[y * w..(y + 1) * w];
            let (mn, mx) = rb.iter().step_by(3).fold((255u8, 0u8), |(p, q), &v| (p.min(v), q.max(v)));
            if mx - mn < 30 {
                continue;
            }
            let ra = &a.luma[ya as usize * w..(ya as usize + 1) * w];
            let (mut s, mut n) = (0u32, 0u32);
            for x in (0..w).step_by(2) {
                if self.in_overlay(x as u32, y as u32) || self.in_overlay(x as u32, ya as u32) {
                    continue;
                }
                s += rb[x].abs_diff(ra[x]) as u32;
                n += 1;
            }
            if n > 0 {
                rows.push(s as f32 / n as f32);
            }
        }
        if rows.len() < 8 {
            return None;
        }
        rows.sort_by(|p, q| p.partial_cmp(q).unwrap());
        Some(rows[rows.len() / 2])
    }

    fn with_expectation(&self, a: &Frame, b: &Frame, est: Option<i32>, e: i32) -> Option<i32> {
        let (w, h) = (self.w as usize, self.h as usize);
        let tol = (e / 20).max(6);
        match est {
            Some(0) if self.content_diff(a, b, 0).is_none_or(|m| m <= 1.0) => est,
            Some(0) => self.with_expectation(a, b, None, e),
            Some(d) => {
                let (md, me) = (self.content_diff(a, b, d), self.content_diff(a, b, e));
                if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                    eprintln!("  expected scroll {e}: measured {d} (diff {md:?}), expected (diff {me:?})");
                }
                match (md, me) {
                    (Some(md), Some(me)) if me <= md * 1.05 + 0.2 => Some(e),
                    _ => est,
                }
            }
            None => {
                let me = mean_abs_diff(a, b, w, h, e);
                let best = (e - tol..=e + tol)
                    .filter_map(|d| mean_abs_diff(a, b, w, h, d).map(|m| (m, d)))
                    .min_by(|x, y| x.0.partial_cmp(&y.0).unwrap().then((x.1 - e).abs().cmp(&(y.1 - e).abs())));
                let pick = match (best, me) {
                    (Some((mb, d)), Some(me)) if mb < me * 0.7 => d,
                    _ => e,
                };
                if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
                    eprintln!("  expected scroll {e} ({me:?}): no measurement, best neighbor {best:?}, picked {pick}");
                }
                Some(pick)
            }
        }
    }

    fn detect_overlays(&mut self, a: &Frame, b: &Frame, dy: i32) {
        let (w, h) = (self.w as usize, self.h as usize);
        if dy.unsigned_abs() as usize >= h || dy.unsigned_abs() < 20 {
            return;
        }
        const C: usize = 8;
        let (gw, gh) = (w.div_ceil(C), h.div_ceil(C));
        let mut cnt = vec![0u32; gw * gh];
        let same = |p: u8, q: u8| p.abs_diff(q) <= 10;
        for y in 0..h.saturating_sub(1) {
            for x in 0..w {
                let i = y * w + x;
                let j = i + w;
                let static_edge =
                    same(a.luma[i], b.luma[i]) && same(a.luma[j], b.luma[j]) && b.luma[i].abs_diff(b.luma[j]) > 24;
                if !static_edge {
                    continue;
                }
                let ys = y as i32 + dy;
                let follows = ys >= 0
                    && ((ys + 1) as usize) < h
                    && same(b.luma[i], a.luma[ys as usize * w + x])
                    && same(b.luma[j], a.luma[(ys as usize + 1) * w + x]);
                if !follows {
                    cnt[(y / C) * gw + x / C] += 1;
                }
            }
        }
        let thr = 4u32;
        let cells: Vec<bool> = cnt.iter().map(|&c| c >= thr).collect();
        let prev_small = std::mem::take(&mut self.small_seen);
        let mut small = Vec::new();
        let mut seen = vec![false; gw * gh];
        let mut stack = Vec::new();
        for s in 0..gw * gh {
            if !cells[s] || seen[s] {
                continue;
            }
            seen[s] = true;
            stack.push(s);
            let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0);
            while let Some(i) = stack.pop() {
                let (cx, cy) = (i % gw, i / gw);
                x0 = x0.min(cx);
                y0 = y0.min(cy);
                x1 = x1.max(cx);
                y1 = y1.max(cy);
                n += 1;
                for ddy in -2i32..=2 {
                    for ddx in -2i32..=2 {
                        let (nx, ny) = (cx as i32 + ddx, cy as i32 + ddy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < gw && (ny as usize) < gh {
                            let j = ny as usize * gw + nx as usize;
                            if cells[j] && !seen[j] {
                                seen[j] = true;
                                stack.push(j);
                            }
                        }
                    }
                }
            }
            if n < 3 {
                continue;
            }
            let r = Rect::new(
                (x0 * C) as f32 - 2.0,
                (y0 * C) as f32 - 2.0,
                ((x1 + 1) * C) as f32 + 2.0,
                ((y1 + 1) * C) as f32 + 2.0,
            )
            .clamp(w as f32, h as f32);
            if self.settled {
                small.push(r);
                if !prev_small.iter().any(|p| p.expand(8.0).inter_area(&r) > 0.0) {
                    continue;
                }
            }
            self.add_overlay(r);
        }
        self.small_seen = small;
    }

    fn forget_under(&mut self, r: &Rect) {
        let w = self.w as usize;
        let (x0, x1) = (r.x0.max(0.0) as usize, (r.x1.min(self.w as f32)) as usize);
        let (y0, y1) = (r.y0.max(0.0) as i64, (r.y1.min(self.h as f32)) as i64);
        let small = r.w() <= 96.0 && r.h() <= 96.0;
        for &(off, first) in &self.pasted {
            if first && (!small || self.settled) {
                continue;
            }
            for y in y0..y1 {
                let row = off + y - self.top;
                if row < 0 || row as usize >= self.rows {
                    continue;
                }
                let d = y.min(u16::MAX as i64 - 1) as u16;
                for x in x0..x1 {
                    let si = row as usize * w + x;
                    if self.state[si] == 2 && self.dist[si] == d {
                        self.state[si] = 1;
                        self.dist[si] = u16::MAX;
                    }
                }
            }
        }
    }

    fn add_overlay(&mut self, r: Rect) {
        let mut r = r.expand(6.0).clamp(self.w as f32, self.h as f32);
        if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
            eprintln!("  fixed element found {r:?} (frame {})", self.frames_seen);
        }
        let (w, h) = (self.w as f32, self.h as f32);
        let side = |o: &Rect| if o.x1 <= w * 0.4 { 1 } else if o.x0 >= w * 0.6 { 2 } else { 0 };
        let column = |o: &Rect, r: &Rect| {
            self.settled
                && side(o) != 0
                && side(o) == side(r)
                && o.y0.max(r.y0) - o.y1.min(r.y1) <= 24.0
        };
        loop {
            let pos = self.overlays.iter().position(|o| mergeable(o, &r) || column(o, &r));
            match pos {
                Some(p) => {
                    r = r.union(&self.overlays.remove(p));
                }
                None => break,
            }
        }
        if self.settled && r.h() > h * 0.5 {
            if side(&r) == 1 && r.x0 <= 48.0 {
                r.x0 = 0.0;
            }
            if side(&r) == 2 && r.x1 >= w - 48.0 {
                r.x1 = w;
            }
        }
        if r.w() > w * 0.6 {
            if r.y0 < h * 0.25 {
                r.y0 = 0.0;
                r.x0 = 0.0;
                r.x1 = w;
            }
            if r.y1 > h * 0.75 {
                r.y1 = h;
                r.x0 = 0.0;
                r.x1 = w;
            }
        }
        self.forget_under(&r);
        self.overlays.push(r);
        self.merge_bands();
    }

    fn merge_bands(&mut self) {
        let (w, h) = (self.w as f32, self.h as f32);
        for top in [true, false] {
            let at_edge = |o: &Rect| o.h() < h * 0.3 && if top { o.y0 <= 8.0 } else { o.y1 >= h - 8.0 };
            let parts: Vec<Rect> = self.overlays.iter().filter(|o| at_edge(o)).copied().collect();
            if parts.len() < 2 || parts.iter().any(|o| o.w() >= w - 1.0) {
                continue;
            }
            let (x0, x1) = parts.iter().fold((f32::MAX, f32::MIN), |(a, b), o| (a.min(o.x0), b.max(o.x1)));
            if x1 - x0 < w * 0.6 {
                continue;
            }
            let band = if top {
                Rect::new(0.0, 0.0, w, (parts.iter().map(|o| o.y1).fold(0.0, f32::max) + 4.0).min(h))
            } else {
                Rect::new(0.0, (parts.iter().map(|o| o.y0).fold(h, f32::min) - 4.0).max(0.0), w, h)
            };
            self.overlays.retain(|o| !at_edge(o));
            self.forget_under(&band);
            self.overlays.push(band);
        }
    }
}

fn mergeable(o: &Rect, r: &Rect) -> bool {
    if o.expand(16.0).inter_area(r) <= 0.0 {
        return false;
    }
    if o.inter_area(r) >= 0.8 * o.area().min(r.area()) {
        return true;
    }
    let xo = (o.x1.min(r.x1) - o.x0.max(r.x0)) / o.w().max(r.w()).max(1.0);
    let yo = (o.y1.min(r.y1) - o.y0.max(r.y0)) / o.h().max(r.h()).max(1.0);
    (xo >= 0.8 || yo >= 0.8) && o.union(r).area() <= 1.4 * (o.area() + r.area())
}

fn row_hashes(luma: &[u8], w: usize, h: usize) -> Vec<Option<u64>> {
    (0..h)
        .map(|y| {
            let row = &luma[y * w..(y + 1) * w];
            let (mut mn, mut mx) = (255u8, 0u8);
            let mut hv: u64 = 0xcbf29ce484222325;
            for &p in row {
                let q = p >> 3;
                mn = mn.min(p);
                mx = mx.max(p);
                hv ^= q as u64;
                hv = hv.wrapping_mul(0x100000001b3);
            }
            if mx - mn < 24 { None } else { Some(hv) }
        })
        .collect()
}

fn row_signatures(luma: &[u8], w: usize, h: usize) -> Vec<[f32; 32]> {
    (0..h)
        .map(|y| {
            let row = &luma[y * w..(y + 1) * w];
            let mut s = [0f32; 32];
            let mut n = [0f32; 32];
            for (x, &p) in row.iter().enumerate() {
                let b = x * 32 / w;
                s[b] += p as f32;
                n[b] += 1.0;
            }
            for i in 0..32 {
                if n[i] > 0.0 {
                    s[i] /= n[i];
                }
            }
            s
        })
        .collect()
}

fn mean_abs_diff(a: &Frame, b: &Frame, w: usize, h: usize, dy: i32) -> Option<f32> {
    let (mut s, mut n) = (0u64, 0u64);
    for y in (0..h).step_by(2) {
        let ya = y as i32 + dy;
        if ya < 0 || ya >= h as i32 {
            continue;
        }
        for x in (0..w).step_by(2) {
            s += b.luma[y * w + x].abs_diff(a.luma[ya as usize * w + x]) as u64;
            n += 1;
        }
    }
    (n as usize >= w * h / 32).then(|| s as f32 / n as f32)
}

fn refine_dense(a: &Frame, b: &Frame, w: usize, h: usize, dy: i32) -> i32 {
    (dy - 2..=dy + 2)
        .filter(|&d| d != 0)
        .filter_map(|d| mean_abs_diff(a, b, w, h, d).map(|m| (m, d)))
        .min_by(|x, y| x.0.partial_cmp(&y.0).unwrap().then((x.1 - dy).abs().cmp(&(y.1 - dy).abs())))
        .map(|x| x.1)
        .unwrap_or(dy)
}

fn dense_match(a: &Frame, b: &Frame, w: usize, h: usize, cands: &[i32]) -> Option<i32> {
    let useful: Vec<bool> = (0..h)
        .map(|y| {
            let row = &b.luma[y * w..(y + 1) * w];
            let (mn, mx) = row.iter().step_by(4).fold((255u8, 0u8), |(p, q), &v| (p.min(v), q.max(v)));
            let still =
                row.iter().zip(&a.luma[y * w..(y + 1) * w]).step_by(4).filter(|(p, q)| p.abs_diff(**q) > 12).count();
            mx - mn > 40 && still * 4 * 50 > w
        })
        .collect();
    let row_med = |dy: i32, ys: usize, xs: usize| -> Option<f32> {
        let mut rows: Vec<f32> = Vec::new();
        for y in (0..h).step_by(ys) {
            let ya = y as i32 + dy;
            if ya < 0 || ya >= h as i32 || !useful[y] {
                continue;
            }
            let (mut s, mut n) = (0u32, 0u32);
            for x in (0..w).step_by(xs) {
                s += b.luma[y * w + x].abs_diff(a.luma[ya as usize * w + x]) as u32;
                n += 1;
            }
            rows.push(s as f32 / n.max(1) as f32);
        }
        if rows.len() * ys < h / 16 {
            return None;
        }
        rows.sort_by(|p, q| p.partial_cmp(q).unwrap());
        Some(rows[rows.len() / 2])
    };
    let min_ov = (h / 8).max(24) as i32;
    let mut wide: Vec<(f32, i32)> = (-(h as i32 - min_ov)..=(h as i32 - min_ov))
        .filter(|&d| d != 0)
        .filter_map(|d| row_med(d, 4, 8).map(|m| (m, d)))
        .collect();
    wide.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut cands: Vec<i32> = cands.to_vec();
    cands.extend(wide.iter().take(3).map(|x| x.1));
    let mut scored: Vec<(f32, i32)> = Vec::new();
    for &c in &cands {
        for d in c - 2..=c + 2 {
            if d != 0 && !scored.iter().any(|x| x.1 == d) {
                if let Some(m) = row_med(d, 2, 2) {
                    scored.push((m, d));
                }
            }
        }
    }
    scored.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let &(best, dy) = scored.first()?;
    let rival = scored.iter().find(|x| (x.1 - dy).abs() > 3).map(|x| x.0).unwrap_or(f32::MAX);
    if std::env::var_os("MARKDOWNSHOT_DEBUG").is_some() {
        eprintln!("  fallback alignment: best {dy} ({best:.2}), rival {rival:.2}");
    }
    ((best < 6.0 && rival > best * 1.6 + 1.0) || (best < 12.0 && rival > best * 2.5)).then_some(dy)
}
