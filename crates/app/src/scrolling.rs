use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use image::RgbImage;
use markdownshot_core::stitch::{Push, Stitcher};

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub fps: u32,
    pub auto_step: Option<f32>,
    pub settle_max: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options { fps: 8, auto_step: None, settle_max: Duration::from_millis(1200) }
    }
}

pub const AUTO_STEP: f32 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Stopped,
    Bottom,
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub frames: usize,
    pub height: u32,
    pub gaps: usize,
    pub auto: bool,
    pub auto_interrupted: bool,
    pub auto_failed: bool,
    pub finished: Option<End>,
}

pub fn nearly_same(a: &RgbImage, b: &RgbImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    let (w, h) = a.dimensions();
    let (mut n, mut diff) = (0u32, 0u32);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            n += 1;
            if (0..3).any(|i| p[i].abs_diff(q[i]) > 16) {
                diff += 1;
            }
        }
    }
    (diff as f32) < n as f32 * 0.002
}

fn stable<G: FnMut() -> Option<RgbImage>>(grab: &mut G, max: Duration, stop: &AtomicBool) -> Option<RgbImage> {
    let t = Instant::now();
    let mut a = grab()?;
    let mut same = 0;
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let Some(b) = grab() else { return Some(a) };
        same = if nearly_same(&a, &b) { same + 1 } else { 0 };
        if same >= 2 || t.elapsed() >= max || stop.load(Ordering::Relaxed) {
            return Some(b);
        }
        a = b;
    }
}

fn update(status: &Mutex<Status>, st: &Stitcher, frame_h: u32) {
    if let Ok(mut s) = status.lock() {
        s.frames = st.frames_used;
        s.height = st.height().max(frame_h);
        s.gaps = st.gaps;
    }
}

pub fn record<G, S, U>(
    mut grab: G,
    mut scroll: S,
    mut user_moved: U,
    opts: Options,
    stop: &AtomicBool,
    status: &Mutex<Status>,
) -> (Stitcher, End)
where
    G: FnMut() -> Option<RgbImage>,
    S: FnMut(u32) -> bool,
    U: FnMut() -> bool,
{
    let prof = std::env::var_os("MARKDOWNSHOT_PROFILE").is_some();
    let mut st = if opts.auto_step.is_some() { Stitcher::settled() } else { Stitcher::new() };
    let dump = std::env::var_os("MARKDOWNSHOT_FRAMES").map(std::path::PathBuf::from);
    if let Some(d) = &dump {
        let _ = std::fs::create_dir_all(d);
    }
    let mut n_dump = 0u32;
    let mut push = |st: &mut Stitcher, img: RgbImage, expect: Option<i32>| -> Push {
        if let Some(d) = &dump {
            n_dump += 1;
            let _ = img.save(d.join(format!("{n_dump:05}.png")));
            if let Some(e) = expect {
                let _ = std::fs::write(d.join(format!("{n_dump:05}.expected")), e.to_string());
            }
        }
        st.push_expect(img, expect)
    };
    std::thread::sleep(Duration::from_millis(120));
    let Some(first) = stable(&mut grab, Duration::from_millis(1500), stop) else { return (st, End::Stopped) };
    let h = first.height();
    push(&mut st, first, None);
    update(status, &st, h);

    if let Some(step) = opts.auto_step {
        if let Ok(mut s) = status.lock() {
            s.auto = true;
        }
        let mut notches = 1u32;
        let mut per: Option<f32> = None;
        let mut moved_once = false;
        let mut still = 0;
        let mut pending: Option<(RgbImage, u32)> = None;
        let mut handle = |st: &mut Stitcher, f: RgbImage, n: u32, per: &mut Option<f32>, notches: &mut u32| -> Option<End> {
            let t = Instant::now();
            let r = push(st, f, per.map(|p| (p * n as f32).round() as i32));
            if prof {
                eprintln!("  stitch {} ms -> {r:?}", t.elapsed().as_millis());
            }
            log::debug!("auto-scroll: {n} notch(es) -> {r:?}");
            match r {
                Push::Scrolled(dy) if dy > 0 => {
                    still = 0;
                    moved_once = true;
                    let expected = per.map(|p| p * n as f32);
                    if expected.is_none_or(|e| dy as f32 >= e - 2.0) {
                        *per = Some(dy as f32 / n as f32);
                    }
                    let p = per.unwrap_or(dy as f32);
                    let useful = (h as f32 - fixed_bands(st, h)).max(h as f32 * 0.3);
                    *notches = ((step * useful / p).floor() as u32).clamp(1, 80);
                }
                Push::Gap => *notches = (*notches / 2).max(1),
                _ => {
                    still += 1;
                    if still >= 2 {
                        return Some(if moved_once { End::Bottom } else { End::Stopped });
                    }
                }
            }
            update(status, st, h);
            None
        };
        let end = loop {
            if stop.load(Ordering::Relaxed) {
                break Some(End::Stopped);
            }
            if user_moved() {
                if let Ok(mut s) = status.lock() {
                    s.auto = false;
                    s.auto_interrupted = true;
                }
                log::info!("auto-scroll interrupted (mouse)");
                break None;
            }
            let t0 = Instant::now();
            let ahead = pending.is_none() || per.is_some();
            let mut sent = notches;
            if ahead && !scroll(sent) {
                break None;
            }
            if let Some((f, n)) = pending.take() {
                if let Some(e) = handle(&mut st, f, n, &mut per, &mut notches) {
                    break Some(e);
                }
            }
            if !ahead {
                sent = notches;
                if !scroll(sent) {
                    break None;
                }
            }
            let t1 = Instant::now();
            std::thread::sleep(Duration::from_millis(30));
            let Some(f) = stable(&mut grab, opts.settle_max, stop) else { break Some(End::Stopped) };
            if prof {
                eprintln!("  step: {sent} notch(es), image still after {} ms (full step {} ms)", t1.elapsed().as_millis(), t0.elapsed().as_millis());
            }
            pending = Some((f, sent));
        };
        if let Some((f, n)) = pending.take() {
            let _ = handle(&mut st, f, n, &mut per, &mut notches);
        }
        drop(handle);
        let end = match end {
            Some(End::Stopped) if !moved_once && still >= 2 => {
                log::info!("auto-scroll: the area does not scroll, continuing manually");
                if let Ok(mut s) = status.lock() {
                    s.auto_failed = true;
                }
                None
            }
            e => e,
        };
        if let Ok(mut s) = status.lock() {
            s.auto = false;
        }
        if let Some(e) = end {
            if let Ok(mut s) = status.lock() {
                if e == End::Bottom {
                    s.finished = Some(e);
                }
            }
            update(status, &st, h);
            return (st, e);
        }
    }

    let period = Duration::from_millis(1000 / opts.fps.clamp(1, 60) as u64);
    loop {
        let t = Instant::now();
        let last = stop.load(Ordering::Relaxed);
        if let Some(img) = grab() {
            if push(&mut st, img, None) != Push::Unchanged {
                update(status, &st, h);
            }
        }
        if last {
            break;
        }
        let el = t.elapsed();
        if el < period {
            std::thread::sleep(period - el);
        }
    }
    (st, End::Stopped)
}

fn fixed_bands(st: &Stitcher, h: u32) -> f32 {
    let w = st.width() as f32;
    let (mut top, mut bottom) = (0f32, 0f32);
    for o in st.overlays() {
        if o.w() > w * 0.6 {
            if o.y0 <= 1.0 {
                top = top.max(o.y1);
            } else if o.y1 >= h as f32 - 1.0 {
                bottom = bottom.max(h as f32 - o.y0);
            }
        }
    }
    top + bottom
}

pub fn wheel_down(at: (i32, i32), notches: u32, park: (i32, i32)) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;
        SetCursorPos(at.0, at.1);
        let one = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (-120i32) as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = vec![one; notches as usize];
        let ok = SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32;
        std::thread::sleep(std::time::Duration::from_millis(15));
        SetCursorPos(park.0, park.1);
        ok
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdotool")
            .args(["mousemove", &at.0.to_string(), &at.1.to_string(), "click", "--repeat", &notches.to_string(), "--delay", "1", "5"])
            .args(["sleep", "0.015", "mousemove", &park.0.to_string(), &park.1.to_string()])
            .status()
            .is_ok_and(|s| s.success())
    }
}

pub fn cursor() -> Option<(i32, i32)> {
    crate::platform::cursor_pos()
}

pub fn spawn(
    origin: (i32, i32),
    region: (u32, u32, u32, u32),
    opts: Options,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
) -> std::io::Result<std::thread::JoinHandle<(Stitcher, End)>> {
    let (sx, sy, sw, sh) = region;
    let probe = (origin.0 + sx as i32 + 1, origin.1 + sy as i32 + 1);
    let at = (origin.0 + (sx + sw / 2) as i32, origin.1 + (sy + sh / 2) as i32);
    std::thread::Builder::new().name("markdownshot-rec".into()).spawn(move || {
        let monitor = match xcap::Monitor::from_point(probe.0, probe.1) {
            Ok(m) => m,
            Err(e) => {
                log::error!("recording: screen not found: {e}");
                return (Stitcher::new(), End::Stopped);
            }
        };
        let grab = || match monitor.capture_region(sx, sy, sw, sh) {
            Ok(img) => Some(image::DynamicImage::ImageRgba8(img).to_rgb8()),
            Err(e) => {
                log::warn!("capture: {e}");
                None
            }
        };
        let (mx, my) = (monitor.x().unwrap_or(0), monitor.y().unwrap_or(0));
        let (mw, mh) = (monitor.width().unwrap_or(0) as i32, monitor.height().unwrap_or(0) as i32);
        let (zx0, zy0) = (origin.0 + sx as i32, origin.1 + sy as i32);
        let (zx1, zy1) = (zx0 + sw as i32, zy0 + sh as i32);
        let park = [(zx1 + 12, at.1), (zx0 - 12, at.1), (at.0, zy1 + 12), (at.0, zy0 - 12)]
            .into_iter()
            .find(|&(x, y)| x >= mx && y >= my && x < mx + mw && y < my + mh)
            .unwrap_or((zx1 - 1, zy1 - 1));
        let placed = Arc::new(Mutex::new(None::<(i32, i32)>));
        let p2 = placed.clone();
        let scroll = move |n: u32| {
            let ok = wheel_down(at, n, park);
            if let Ok(mut p) = p2.lock() {
                *p = cursor().or(Some(park));
            }
            ok
        };
        let user_moved = move || {
            let (Some(p), Some(c)) = (placed.lock().ok().and_then(|p| *p), cursor()) else { return false };
            (p.0 - c.0).abs() + (p.1 - c.1).abs() > 12
        };
        record(grab, scroll, user_moved, opts, &stop, &status)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sim {
        page: RgbImage,
        vh: u32,
        pos: f32,
        target: f32,
        px_per_notch: f32,
    }

    impl Sim {
        fn new(h: u32) -> Sim {
            let w = 320;
            let font = crate::text::Font::new();
            let mut buf = vec![0xffffffu32; (w * h) as usize];
            let words = ["lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do",
                "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua", "Ut", "enim"];
            let mut seed = 7u32;
            let mut rnd = || {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                (seed >> 16) as usize
            };
            for line in 0..h / 20 {
                let mut txt = String::new();
                let n = 2 + rnd() % 6;
                for _ in 0..n {
                    txt.push_str(words[rnd() % words.len()]);
                    txt.push(' ');
                }
                font.draw(&mut buf, w as usize, h as usize, 10.0, (line * 20 + 15) as f32, 14.0, 0x202020, &txt);
            }
            let raw: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
            let page = RgbImage::from_raw(w, h, raw).unwrap();
            Sim { page, vh: 240, pos: 0.0, target: 0.0, px_per_notch: 53.0 }
        }
        fn grab(&mut self) -> RgbImage {
            self.pos += (self.target - self.pos) * 0.5;
            if (self.target - self.pos).abs() < 0.5 {
                self.pos = self.target;
            }
            let y = self.pos.round() as u32;
            image::imageops::crop_imm(&self.page, 0, y, self.page.width(), self.vh).to_image()
        }
        fn scroll(&mut self, n: u32) {
            let max = (self.page.height() - self.vh) as f32;
            self.target = (self.target + n as f32 * self.px_per_notch).min(max);
        }
    }

    #[test]
    fn auto_scroll_to_bottom() {
        let sim = std::cell::RefCell::new(Sim::new(2000));
        let stop = AtomicBool::new(false);
        let status = Mutex::new(Status::default());
        let opts = Options { auto_step: Some(AUTO_STEP), ..Default::default() };
        let (st, end) = record(
            || Some(sim.borrow_mut().grab()),
            |n| {
                sim.borrow_mut().scroll(n);
                true
            },
            || false,
            opts,
            &stop,
            &status,
        );
        assert_eq!(end, End::Bottom);
        let out = st.finish();
        let page = &sim.borrow().page;
        assert_eq!(out.dimensions(), page.dimensions(), "whole page, no duplicates");
        let bad: Vec<u32> = (0..page.height())
            .filter(|&y| (0..page.width()).any(|x| out.get_pixel(x, y) != page.get_pixel(x, y)))
            .collect();
        assert!(bad.is_empty(), "differing rows: {} ({:?}...); fixed elements {:?}", bad.len(), &bad[..bad.len().min(8)], st.overlays());
        assert!(st.frames_used <= 14, "{} frames", st.frames_used);
        assert_eq!(status.lock().unwrap().finished, Some(End::Bottom));
    }

    #[test]
    fn wheel_without_effect() {
        let sim = std::cell::RefCell::new(Sim::new(2000));
        let stop = AtomicBool::new(false);
        let status = Mutex::new(Status::default());
        let opts = Options { auto_step: Some(AUTO_STEP), ..Default::default() };
        let n = std::cell::Cell::new(0);
        let (_, end) = record(
            || {
                n.set(n.get() + 1);
                if n.get() > 40 {
                    stop.store(true, Ordering::Relaxed);
                }
                Some(sim.borrow_mut().grab())
            },
            |_| true,
            || false,
            Options { fps: 60, ..opts },
            &stop,
            &status,
        );
        assert_eq!(end, End::Stopped);
        assert!(status.lock().unwrap().auto_failed);
    }
}
