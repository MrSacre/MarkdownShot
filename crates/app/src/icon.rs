const PNG: &[u8] = include_bytes!("../../../assets/markdownshot.png");

pub fn rgba(size: u32) -> Vec<u8> {
    match image::load_from_memory(PNG) {
        Ok(img) => img.resize_exact(size, size, image::imageops::FilterType::Lanczos3).to_rgba8().into_raw(),
        Err(e) => {
            log::warn!("icon: {e}");
            [0x5b, 0x21, 0xb6, 0xff].repeat((size * size) as usize)
        }
    }
}
