use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use markdownshot_core::EngineOptions;
use markdownshot_core::ocr::Quality;
use markdownshot_core::output::OutputOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub output: OutputCfg,
    pub hotkeys: HotkeysCfg,
    pub capture: CaptureCfg,
    pub engine: EngineCfg,
    pub interface: InterfaceCfg,
    pub updates: UpdatesCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputCfg {
    pub notes_dir: PathBuf,
    pub images_dir: PathBuf,
    pub wikilinks: bool,
    pub filename: String,
    pub front_matter: bool,
    pub tags: Vec<String>,
    pub copy_markdown: bool,
    pub open_after: String,
    pub follow_vault: bool,
    pub image_format: String,
    pub jpeg_quality: u8,
    pub max_image_width: u32,
    pub dedupe_images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysCfg {
    pub capture: String,
    pub record: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureCfg {
    pub mode: String,
    pub manual_suggestions: bool,
    pub frames_per_second: u32,
    pub auto_scroll: bool,
    pub keep_long_capture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineCfg {
    pub quality: String,
    pub full_width_images: bool,
    pub layout: bool,
    pub gpu: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfaceCfg {
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesCfg {
    pub check: bool,
    pub last_check: String,
    pub skip_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: OutputCfg::default(),
            hotkeys: HotkeysCfg::default(),
            capture: CaptureCfg::default(),
            engine: EngineCfg::default(),
            interface: InterfaceCfg::default(),
            updates: UpdatesCfg::default(),
        }
    }
}

impl Default for InterfaceCfg {
    fn default() -> Self {
        Self { language: "auto".into() }
    }
}

impl Default for UpdatesCfg {
    fn default() -> Self {
        Self { check: true, last_check: String::new(), skip_version: String::new() }
    }
}

impl Default for OutputCfg {
    fn default() -> Self {
        let docs = directories::UserDirs::new()
            .and_then(|u| u.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            notes_dir: docs.join("MarkdownShot"),
            images_dir: PathBuf::from("images"),
            wikilinks: true,
            filename: "{date} {time} - {title}".into(),
            front_matter: true,
            tags: vec!["capture".into()],
            copy_markdown: true,
            open_after: "none".into(),
            follow_vault: true,
            image_format: "png".into(),
            jpeg_quality: 90,
            max_image_width: 0,
            dedupe_images: true,
        }
    }
}

impl Default for HotkeysCfg {
    fn default() -> Self {
        Self { capture: "Ctrl+Alt+S".into(), record: "Ctrl+Alt+R".into() }
    }
}

impl Default for CaptureCfg {
    fn default() -> Self {
        Self { mode: "auto".into(), manual_suggestions: false, frames_per_second: 8, auto_scroll: true, keep_long_capture: false }
    }
}

impl Default for EngineCfg {
    fn default() -> Self {
        Self { quality: "balanced".into(), full_width_images: true, layout: true, gpu: false }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let portable = dir.join("markdownshot.toml");
                if portable.is_file() {
                    return portable;
                }
            }
        }
        directories::BaseDirs::new()
            .map(|b| b.config_dir().join("MarkdownShot").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("markdownshot.toml"))
    }

    pub fn load() -> Result<(Config, bool)> {
        let p = Self::path();
        if !p.is_file() {
            let c = Config::default();
            c.save()?;
            return Ok((c, true));
        }
        let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let c: Config = toml::from_str(&txt).with_context(|| format!("invalid format in {}", p.display()))?;
        Ok((c, false))
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let body = toml::to_string_pretty(self)?;
        let txt = format!(
            "# MarkdownShot configuration. It can be edited by hand; restart MarkdownShot afterwards.\n\
             #\n\
             # [output] images_dir: relative to notes_dir, or an absolute path.\n\
             #          filename: {{date}}, {{time}}, {{title}}.  open_after: none | note | obsidian\n\
             #          image_format: png | webp | jpeg  max_image_width: 0 keeps the original size\n\
             # [capture] mode: auto | manual | snipe\n\
             # [engine] quality: fast | balanced | accurate\n\
             # [interface] language: auto | en | fr\n\n{body}"
        );
        std::fs::write(&p, txt).with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }

    pub fn mode(&self) -> Mode {
        match self.capture.mode.to_lowercase().as_str() {
            "manual" => Mode::Manual,
            "snipe" => Mode::Snipe,
            _ => Mode::Auto,
        }
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.capture.mode = match m {
            Mode::Auto => "auto",
            Mode::Manual => "manual",
            Mode::Snipe => "snipe",
        }
        .into();
    }

    pub fn peek_language() -> String {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| toml::from_str::<Config>(&t).ok())
            .map(|c| c.interface.language)
            .unwrap_or_else(|| "auto".into())
    }

    pub fn update_check_due(&self, today: &str) -> bool {
        self.updates.check && self.updates.last_check != today
    }

    pub fn quality(&self) -> Quality {
        match self.engine.quality.to_lowercase().as_str() {
            "fast" => Quality::Fast,
            "accurate" => Quality::Accurate,
            _ => Quality::Balanced,
        }
    }

    pub fn engine_options(&self) -> EngineOptions {
        EngineOptions {
            use_layout: self.engine.layout,
            auto_images: true,
            quality: self.quality(),
            full_width_images: self.engine.full_width_images,
            gpu: self.engine.gpu,
            ..Default::default()
        }
    }

    pub fn output_options(&self) -> OutputOptions {
        self.output_options_in(&self.output.notes_dir)
    }

    pub fn output_options_in(&self, notes_dir: &Path) -> OutputOptions {
        let rules = if self.output.follow_vault { crate::obsidian::rules_for(notes_dir) } else { None };
        let (images_dir, wikilinks) = match rules {
            Some(r) => (r.attachments, r.wikilinks),
            None => (self.output.images_dir.clone(), self.output.wikilinks),
        };
        OutputOptions {
            notes_dir: notes_dir.to_path_buf(),
            images_dir,
            wikilinks,
            filename: self.output.filename.clone(),
            front_matter: self.output.front_matter,
            tags: self.output.tags.clone(),
            images: markdownshot_core::output::ImageOptions {
                format: markdownshot_core::output::ImageFormat::parse(&self.output.image_format),
                quality: self.output.jpeg_quality,
                max_width: self.output.max_image_width,
                dedupe: self.output.dedupe_images,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Auto,
    Manual,
    Snipe,
}

impl Mode {
    pub fn engine(self) -> markdownshot_core::Mode {
        match self {
            Mode::Manual => markdownshot_core::Mode::Manual,
            Mode::Auto | Mode::Snipe => markdownshot_core::Mode::Auto,
        }
    }

    pub fn next(self) -> Mode {
        match self {
            Mode::Auto => Mode::Snipe,
            Mode::Snipe => Mode::Manual,
            Mode::Manual => Mode::Auto,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Manual => t!("manuel", "manual"),
            Mode::Snipe => "snipe",
        }
    }
}

pub fn now_strings() -> (String, String, String) {
    let n = chrono::Local::now();
    (n.format("%Y-%m-%d").to_string(), n.format("%Hh%M").to_string(), n.format("%Y-%m-%dT%H:%M:%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_round_trip() {
        let mut c = Config::default();
        for m in [Mode::Auto, Mode::Snipe, Mode::Manual] {
            c.set_mode(m);
            assert_eq!(c.mode(), m, "{}", c.capture.mode);
        }
        c.set_mode(Mode::Snipe);
        assert_eq!(c.capture.mode, "snipe");
        c.capture.mode = "Snipe".into();
        assert_eq!(c.mode(), Mode::Snipe);
        c.capture.mode = "unknown".into();
        assert_eq!(c.mode(), Mode::Auto);
    }

    #[test]
    fn mode_cycle_and_engine() {
        assert_eq!(Mode::Auto.next(), Mode::Snipe);
        assert_eq!(Mode::Snipe.next(), Mode::Manual);
        assert_eq!(Mode::Manual.next(), Mode::Auto);
        assert_eq!(Mode::Snipe.engine(), markdownshot_core::Mode::Auto);
        assert_eq!(Mode::Manual.engine(), markdownshot_core::Mode::Manual);
    }
}
