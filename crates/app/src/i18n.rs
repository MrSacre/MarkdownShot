use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn resolve(setting: &str) -> &'static str {
    match setting.trim().to_lowercase().as_str() {
        "fr" | "francais" | "français" => "fr",
        "en" | "english" | "anglais" => "en",
        _ => system(),
    }
}

pub fn set(setting: &str) {
    ENGLISH.store(resolve(setting) == "en", Ordering::Relaxed);
}

pub fn is_en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn system() -> &'static str {
    #[cfg(windows)]
    {
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        if id & 0x3ff == 0x0c { "fr" } else { "en" }
    }
    #[cfg(not(windows))]
    {
        let v = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
        match v {
            Some(v) if v.to_lowercase().starts_with("fr") => "fr",
            Some(_) => "en",
            None => "fr",
        }
    }
}

macro_rules! t {
    ($fr:expr, $en:expr $(,)?) => {
        if $crate::i18n::is_en() { $en } else { $fr }
    };
}

macro_rules! tf {
    ($fr:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::is_en() { format!($en $(, $a)*) } else { format!($fr $(, $a)*) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_setting() {
        assert_eq!(resolve("fr"), "fr");
        assert_eq!(resolve("EN"), "en");
        assert!(["fr", "en"].contains(&resolve("auto")));
        set("en");
        let n = 3;
        assert_eq!(t!("Capturer", "Capture"), "Capture");
        assert_eq!(tf!("{n} images", "{n} pictures"), "3 pictures");
        set("fr");
        assert_eq!(tf!("{} images", "{} pictures", n), "3 images");
    }
}
