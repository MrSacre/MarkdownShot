use markdownshot_core::{
    layout::Layout,
    ocr::{Ocr, Quality},
    runtime, style,
};

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("image");
    let img = image::open(&path)?.to_rgb8();
    let dir = runtime::models_dir()?;
    let mut ocr = Ocr::load(&dir, Quality::Balanced, runtime::default_threads())?;
    let bg = style::dominant_color(&img);
    println!("page background {bg:?}");
    for l in ocr.run(&img)? {
        let st = style::analyze_line(&img, &l);
        println!(
            "y{:5.0} x{:5.0}-{:5.0} h{:3.0} xh{:5.1} w{:.3} bg{:?} | {}",
            l.rect.y0,
            l.rect.x0,
            l.rect.x1,
            l.rect.h(),
            st.xh,
            st.weight,
            st.bg,
            l.text
        );
    }
    let mut lay = Layout::load(&dir.join("layout.onnx"), 2)?;
    for r in lay.detect(&img)? {
        println!("{:14} {:.2} {:?}", r.label, r.score, r.rect);
    }
    Ok(())
}
