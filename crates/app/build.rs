fn main() {
    println!("cargo:rerun-if-changed=markdownshot.rc");
    println!("cargo:rerun-if-changed=markdownshot.manifest");
    println!("cargo:rerun-if-changed=../../assets/markdownshot.ico");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut bin: Vec<String> = version.split(['.', '-']).take(3).map(|p| p.parse::<u16>().unwrap_or(0).to_string()).collect();
    bin.resize(4, "0".into());
    let esc = |p: &std::path::Path| format!("{}{}", p.display(), std::path::MAIN_SEPARATOR).replace('\\', "\\\\");
    let assets = dir.join("..").join("..").join("assets");
    let rc = std::fs::read_to_string(dir.join("markdownshot.rc"))
        .unwrap()
        .replace("@VERSION_BIN@", &bin.join(","))
        .replace("@VERSION@", &version)
        .replace("@ASSETS@", &esc(&assets))
        .replace("@DIR@", &esc(&dir));
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("markdownshot.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE).manifest_required().unwrap();
}
