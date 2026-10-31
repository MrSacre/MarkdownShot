#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn w(&self) -> f32 {
        (self.x1 - self.x0).max(0.0)
    }

    pub fn h(&self) -> f32 {
        (self.y1 - self.y0).max(0.0)
    }

    pub fn area(&self) -> f32 {
        self.w() * self.h()
    }

    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) * 0.5
    }

    pub fn cy(&self) -> f32 {
        (self.y0 + self.y1) * 0.5
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect::new(self.x0.max(o.x0), self.y0.max(o.y0), self.x1.min(o.x1), self.y1.min(o.y1))
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(self.x0.min(o.x0), self.y0.min(o.y0), self.x1.max(o.x1), self.y1.max(o.y1))
    }

    pub fn inter_area(&self, o: &Rect) -> f32 {
        self.intersect(o).area()
    }

    pub fn iou(&self, o: &Rect) -> f32 {
        let i = self.inter_area(o);
        let u = self.area() + o.area() - i;
        if u <= 0.0 { 0.0 } else { i / u }
    }

    pub fn covered_by(&self, o: &Rect) -> f32 {
        let a = self.area();
        if a <= 0.0 { 0.0 } else { self.inter_area(o) / a }
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    pub fn expand(&self, d: f32) -> Rect {
        Rect::new(self.x0 - d, self.y0 - d, self.x1 + d, self.y1 + d)
    }

    pub fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x0 + dx, self.y0 + dy, self.x1 + dx, self.y1 + dy)
    }

    pub fn clamp(&self, w: f32, h: f32) -> Rect {
        Rect::new(self.x0.clamp(0.0, w), self.y0.clamp(0.0, h), self.x1.clamp(0.0, w), self.y1.clamp(0.0, h))
    }

    pub fn v_overlap(&self, o: &Rect) -> f32 {
        let ov = (self.y1.min(o.y1) - self.y0.max(o.y0)).max(0.0);
        let m = self.h().min(o.h());
        if m <= 0.0 { 0.0 } else { ov / m }
    }

    pub fn h_overlap(&self, o: &Rect) -> f32 {
        let ov = (self.x1.min(o.x1) - self.x0.max(o.x0)).max(0.0);
        let m = self.w().min(o.w());
        if m <= 0.0 { 0.0 } else { ov / m }
    }

    pub fn to_px(&self, img_w: u32, img_h: u32) -> (u32, u32, u32, u32) {
        let x0 = self.x0.floor().clamp(0.0, img_w as f32) as u32;
        let y0 = self.y0.floor().clamp(0.0, img_h as f32) as u32;
        let x1 = self.x1.ceil().clamp(0.0, img_w as f32) as u32;
        let y1 = self.y1.ceil().clamp(0.0, img_h as f32) as u32;
        (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
    }
}

pub fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) }
}

pub fn weighted_median(pairs: &mut [(f32, f32)]) -> f32 {
    if pairs.is_empty() {
        return 0.0;
    }
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total: f32 = pairs.iter().map(|p| p.1).sum();
    let mut acc = 0.0;
    for p in pairs.iter() {
        acc += p.1;
        if acc >= total * 0.5 {
            return p.0;
        }
    }
    pairs[pairs.len() - 1].0
}
