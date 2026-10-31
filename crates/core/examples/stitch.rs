use markdownshot_core::stitch::{Push, Stitcher};

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).expect("folder");
    let out = std::env::args().nth(2).unwrap_or_else(|| "page.png".into());
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "png" || e == "jpg"))
        .collect();
    files.sort();
    let settled = files.iter().any(|f| f.with_extension("expected").exists());
    let mut st = if settled { Stitcher::settled() } else { Stitcher::new() };
    let t = std::time::Instant::now();
    let mut line = String::new();
    for f in &files {
        let expect = std::fs::read_to_string(f.with_extension("expected")).ok().and_then(|t| t.trim().parse().ok());
        let r = st.push_expect(image::open(f)?.to_rgb8(), expect);
        line.push_str(&match r {
            Push::First => "D ".into(),
            Push::Unchanged => ". ".into(),
            Push::Scrolled(d) => format!("{d} "),
            Push::Gap => "GAP ".into(),
        });
    }
    println!("{line}");
    let page = st.finish();
    println!(
        "{} frames, {} used, {} gaps, page {}x{}, fixed {:?}, {:?}",
        files.len(),
        st.frames_used,
        st.gaps,
        page.width(),
        page.height(),
        st.overlays(),
        t.elapsed()
    );
    page.save(out)?;
    Ok(())
}
