use std::time::Instant;

use markdownshot_core::{ocr::{Ocr, Quality}, runtime};

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("image");
    let img = image::open(&path)?.to_rgb8();
    let dir = runtime::models_dir()?;
    let mut ocr = Ocr::load(&dir, Quality::Balanced, runtime::default_threads())?;
    let _ = ocr.run(&img)?;
    let t = Instant::now();
    let boxes = ocr.detect(&img)?;
    let det = t.elapsed().as_millis();
    let t = Instant::now();
    let lines = ocr.recognize(&img, &boxes)?;
    let rec = t.elapsed().as_millis();
    let t = Instant::now();
    for b in &boxes {
        let _ = ocr.recognize(&img, std::slice::from_ref(b))?;
    }
    let rec1 = t.elapsed().as_millis();
    let up: Vec<_> = boxes.iter().map(|b| *b).collect();
    let t = Instant::now();
    let _ = ocr.recognize(&img, &up)?;
    let rec2 = t.elapsed().as_millis();
    println!(
        "{} lines, detection {det} ms, batched recognition {rec} ms, line by line {rec1} ms, second full pass {rec2} ms",
        lines.len()
    );
    Ok(())
}
