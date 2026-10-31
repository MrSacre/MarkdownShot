use std::path::PathBuf;

use markdownshot_core::stitch::Stitcher;
use markdownshot_core::{Engine, EngineOptions};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine() -> Option<Engine> {
    let models = root().join("models");
    if !models.join("det.onnx").is_file() {
        eprintln!("models missing: test skipped");
        return None;
    }
    unsafe { std::env::set_var("MARKDOWNSHOT_MODELS", &models) };
    Some(Engine::new(&models, EngineOptions::default()).expect("loading the engine"))
}

fn words(md: &str) -> Vec<String> {
    md.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

fn recall(expected: &[String], got: &[String]) -> f32 {
    let (n, m) = (expected.len(), got.len());
    let mut prev = vec![0u32; m + 1];
    for i in 1..=n {
        let mut cur = vec![0u32; m + 1];
        for j in 1..=m {
            cur[j] = if expected[i - 1] == got[j - 1] { prev[j - 1] + 1 } else { prev[j].max(cur[j - 1]) };
        }
        prev = cur;
    }
    prev[m] as f32 / n.max(1) as f32
}

fn check_full_page(md: &str, n_images: usize) {
    let expected = std::fs::read_to_string(root().join("testdata/reference/expected.md")).unwrap();
    let expected = expected.replace("![[image-1.png]]", "").replace("![[image-2.png]]", "");
    let r = recall(&words(&expected), &words(md));
    assert!(r >= 0.98, "text recall too low: {r:.3}\n{md}");
    for h in [
        "# Prendre des notes à partir de captures d'écran",
        "## Pourquoi le Markdown ?",
        "## Les trois étapes du traitement",
        "### Le cas des images",
        "### Un exemple de configuration",
        "## Bonnes pratiques",
    ] {
        assert!(md.lines().any(|l| l == h), "missing heading: {h}\n{md}");
    }
    for b in ["**ni cherchable ni modifiable**", "**sauvegarder**", "**gras**", "**base de connaissances**"] {
        assert!(md.contains(b), "missing bold: {b}\n{md}");
    }
    assert_eq!(md.matches("\n- ").count(), 3, "bullets\n{md}");
    assert!(md.contains("\n1. Capturez") && md.contains("\n3. Ajoutez"), "numbered list\n{md}");
    assert!(md.contains("```toml\n[sortie]\ndossier_notes = \"C:/Vault/Captures\""), "code block\n{md}");
    assert!(md.contains("> Une note que l'on ne retrouve pas"), "quote\n{md}");
    assert_eq!(n_images, 2, "image count");
    assert!(!md.contains("Tesseract") && !md.contains("9200"), "chart text copied\n{md}");
}

#[test]
fn full_page_100_and_125() {
    let Some(mut eng) = engine() else { return };
    for f in ["full_100.png", "full_125.png"] {
        let img = image::open(root().join("testdata/reference").join(f)).unwrap().to_rgb8();
        let doc = eng.process(&img, &[]).unwrap();
        let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
        check_full_page(&md, doc.images.len());
    }
}

#[test]
fn recording_without_duplicates() {
    let Some(mut eng) = engine() else { return };
    for (dir, full) in [("frames_100", "full_100.png"), ("frames_125", "full_125.png")] {
        let mut files: Vec<_> =
            std::fs::read_dir(root().join("testdata/reference").join(dir)).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        let mut st = Stitcher::new();
        for f in &files {
            st.push(image::open(f).unwrap().to_rgb8());
        }
        assert_eq!(st.gaps, 0, "{dir}: gap in the stitching");
        let page = st.finish();
        let truth = image::open(root().join("testdata/reference").join(full)).unwrap().to_rgb8();
        assert_eq!(page.dimensions(), truth.dimensions(), "{dir}: page height");
        let doc = eng.process(&page, &[]).unwrap();
        let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
        assert_eq!(md.matches("Articles Outils").count(), 1, "{dir}: repeated header\n{md}");
        assert!(!md.contains("cookies"), "{dir}: cookie banner present\n{md}");
        check_full_page(&md, doc.images.len());
    }
}

#[test]
fn manual_zone() {
    let Some(mut eng) = engine() else { return };
    let img = image::open(root().join("testdata/reference/snip_top.png")).unwrap().to_rgb8();
    let zone = markdownshot_core::Rect::new(276.0, 95.0, 900.0, 208.0);
    let doc = eng.process(&img, &[zone]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    assert!(md.contains("![[image-1.png]]"));
    assert!(!md.contains("# Prendre des notes"), "the text of the manual zone must stay in the image\n{md}");
    assert_eq!(doc.images[0].dimensions(), (624, 113));
}

#[test]
fn text_beside_images() {
    let Some(mut eng) = engine() else { return };
    let img = image::open(root().join("testdata/sidebar/page.png")).unwrap().to_rgb8();
    let doc = eng.process(&img, &[]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    assert!(!md.contains("Vue depuis le col"), "side caption left in the text\n{md}");
    assert!(!md.contains("soleil se lève"), "annotation left in the text\n{md}");
    let (w, _) = doc.images[0].dimensions();
    assert!(w > 700, "image 1 not widened ({w} px)");
    assert!(md.contains("Cette colonne de droite est un vrai texte courant"), "column swallowed\n{md}");
    assert!(doc.images[1].width() < 500, "image 2 wrongly widened");
    assert!(md.contains("Dernier paragraphe"));
    assert_eq!(doc.images.len(), 2);
}

#[test]
fn grids_lists_tables_and_diagram() {
    let Some(mut eng) = engine() else { return };
    let img = image::open(root().join("testdata/grid/page.png")).unwrap().to_rgb8();
    let doc = eng.process(&img, &[]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    assert!(md.starts_with("# Télémétrie EDR"), "heading on a band\n{md}");
    let expected = [
        "CreateThread",
        "CreateRemoteThread",
        "GetThreadContext",
        "SetThreadContext",
        "OpenProcess",
        "OpenThread",
        "VirtualAlloc",
        "VirtualAllocEx",
        "WriteProcessMemory",
        "VirtualQuery",
        "VirtualFree",
    ];
    let items: Vec<&str> = md.lines().filter_map(|l| l.strip_prefix("- ")).collect();
    assert_eq!(items, expected, "list\n{md}");
    assert!(md.contains("| Source | Niveau | Exemple |\n| --- | --- | --- |"), "table header\n{md}");
    assert!(md.contains("| Callbacks | Noyau | PsSetCreateProcessNotifyRoutine |"), "table row\n{md}");
    assert_eq!(doc.images.len(), 1, "a single image (the diagram)\n{md}");
    for t in ["Application", "appel système", "\nEDR\n"] {
        assert!(!md.contains(t), "diagram text copied: {t}\n{md}");
    }
    assert!(md.trim_end().ends_with("ce paragraphe doit rester du texte normal."));
}

#[test]
fn compressed_video_recording() {
    let Some(mut eng) = engine() else { return };
    let mut files: Vec<_> =
        std::fs::read_dir(root().join("testdata/reference/frames_video")).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut st = Stitcher::new();
    for f in &files {
        st.push(image::open(f).unwrap().to_rgb8());
    }
    assert_eq!(st.gaps, 0, "gap in the stitching");
    let page = st.finish();
    assert!((page.height() as i32 - 2602).abs() <= 8, "page height {}", page.height());
    let doc = eng.process(&page, &[]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    assert_eq!(md.matches("Articles Outils").count(), 1, "repeated header\n{md}");
    assert!(!md.contains("cookies"), "cookie banner present\n{md}");
    let expected = std::fs::read_to_string(root().join("testdata/reference/expected.md")).unwrap();
    let r = recall(&words(&expected.replace("![[image-1.png]]", "").replace("![[image-2.png]]", "")), &words(&md));
    assert!(r >= 0.97, "text recall too low: {r:.3}\n{md}");
    assert_eq!(doc.images.len(), 2, "images\n{md}");
}

#[test]
fn modes_and_zones() {
    use markdownshot_core::{Mode, Rect, Zone, ZoneKind};
    let Some(mut eng) = engine() else { return };
    let img = image::open(root().join("testdata/reference/full_100.png")).unwrap().to_rgb8();
    let a = eng.analyze(img.clone()).unwrap();
    let md = |d: &markdownshot_core::Document| d.markdown(&|i| format!("![[image-{}.png]]", i + 1));

    let auto = a.build(&[], Mode::Auto);
    assert_eq!(auto.images.len(), 2);
    assert_eq!(md(&auto), md(&eng.process(&img, &[]).unwrap()));

    let manual = a.build(&[], Mode::Manual);
    assert!(manual.images.is_empty());
    assert!(md(&manual).contains("Tesseract"), "{}", md(&manual));

    let photo = Rect::new(284.0, 630.0, 996.0, 1010.0);
    let one = a.build(&[Zone::image(photo)], Mode::Manual);
    assert_eq!(one.images.len(), 1);
    assert_eq!(one.images[0].dimensions(), (712, 380));

    let chart = Rect::new(280.0, 1470.0, 1000.0, 1840.0);
    let t = a.build(&[Zone { rect: chart, kind: ZoneKind::Text }], Mode::Auto);
    assert_eq!(t.images.len(), 1);
    assert!(md(&t).contains("Tesseract"));

    assert!(md(&auto).contains("cookies"));
    let banner = Rect::new(1015.0, 690.0, 1275.0, 750.0);
    let ig = a.build(&[Zone { rect: banner, kind: ZoneKind::Ignore }], Mode::Auto);
    assert!(!md(&ig).contains("cookies"), "{}", md(&ig));
    assert_eq!(ig.images.len(), 2);
}

#[test]
fn bordered_table_to_markdown() {
    let Some(mut eng) = engine() else { return };
    let dir = root().join("testdata/calibration");
    let img = image::open(dir.join("table.png")).unwrap().to_rgb8();
    let doc = eng.process(&img, &[]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    let want = std::fs::read_to_string(dir.join("table.md")).unwrap();
    assert!(doc.images.is_empty(), "the table must not be an image\n{md}");
    assert_eq!(md.trim(), want.trim(), "Markdown table");
}

#[test]
fn drawn_tables_stay_images() {
    let Some(mut eng) = engine() else { return };
    let dir = root().join("testdata/calibration");
    for f in ["table-checks.png", "table-frame.png"] {
        let img = image::open(dir.join(f)).unwrap().to_rgb8();
        let doc = eng.process(&img, &[]).unwrap();
        let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
        assert_eq!(doc.images.len(), 1, "{f}: a single image\n{md}");
        assert!(!md.lines().any(|l| l.starts_with('|')), "{f}: no Markdown table\n{md}");
        assert!(!md.contains("DelegataExecutio"), "{f}: table text copied\n{md}");
        assert!(md.contains("Lorem ipsum dolor sit amet"), "{f}: paragraph above\n{md}");
    }
    let img = image::open(dir.join("table-frame.png")).unwrap().to_rgb8();
    let doc = eng.process(&img, &[]).unwrap();
    let r = doc.figures[0].rect;
    assert!(r.x0 <= 13.0 && r.x1 >= 907.0, "tinted frame cut: {r:?}");
}

#[test]
fn table_under_two_images_dark_video() {
    let Some(mut eng) = engine() else { return };
    let img = image::open(root().join("testdata/calibration/table-video.png")).unwrap().to_rgb8();
    let doc = eng.process(&img, &[]).unwrap();
    let md = doc.markdown(&|i| format!("![[image-{}.png]]", i + 1));
    let table = "| Modus | Iussum | Effectus |\n| --- | --- | --- |\n\
                 | `--lorem` | `ipsum -d 0` | egestas aliquip est pellentesque veniam |\n\
                 | `--sit` | do nisi `amet()` | anim est senectus aliquip |";
    assert!(md.contains(table), "table\n{md}");
    assert_eq!(doc.images.len(), 2, "two images, without the table\n{md}");
    assert!(md.find("![[image-2.png]]").unwrap() < md.find("| Modus").unwrap(), "order\n{md}");
    assert!(doc.images.iter().all(|i| i.width() < 480), "images merged together");
}
