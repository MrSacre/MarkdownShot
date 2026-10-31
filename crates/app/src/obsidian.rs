use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Vault {
    pub path: PathBuf,
    pub name: String,
    pub ts: u64,
}

#[derive(Deserialize)]
struct ObsidianJson {
    #[serde(default)]
    vaults: std::collections::HashMap<String, VaultEntry>,
}

#[derive(Deserialize)]
struct VaultEntry {
    path: PathBuf,
    #[serde(default)]
    ts: u64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AppJson {
    attachment_folder_path: Option<String>,
    #[serde(default)]
    use_markdown_links: bool,
}

pub fn config_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.config_dir().join("obsidian").join("obsidian.json"))
}

pub fn vaults() -> Vec<Vault> {
    config_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse_vaults(&t)).unwrap_or_default()
}

fn parse_vaults(json: &str) -> Vec<Vault> {
    let Ok(o) = serde_json::from_str::<ObsidianJson>(json) else { return Vec::new() };
    let mut v: Vec<Vault> = o
        .vaults
        .into_values()
        .filter(|e| e.path.is_dir())
        .map(|e| Vault {
            name: e.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| e.path.display().to_string()),
            path: e.path,
            ts: e.ts,
        })
        .collect();
    v.sort_by(|a, b| b.ts.cmp(&a.ts).then(a.name.cmp(&b.name)));
    v
}

pub fn vault_of(dir: &Path) -> Option<PathBuf> {
    let mut d = Some(dir);
    while let Some(p) = d {
        if p.join(".obsidian").is_dir() {
            return Some(p.to_path_buf());
        }
        d = p.parent();
    }
    None
}

fn app_json(vault: &Path) -> AppJson {
    std::fs::read_to_string(vault.join(".obsidian").join("app.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultRules {
    pub vault: PathBuf,
    pub attachments: PathBuf,
    pub wikilinks: bool,
}

pub fn rules_for(notes_dir: &Path) -> Option<VaultRules> {
    let vault = vault_of(notes_dir)?;
    let app = app_json(&vault);
    Some(VaultRules {
        attachments: attachments_dir(&vault, notes_dir, app.attachment_folder_path.as_deref()),
        wikilinks: !app.use_markdown_links,
        vault,
    })
}

fn attachments_dir(vault: &Path, notes_dir: &Path, setting: Option<&str>) -> PathBuf {
    match setting.map(str::trim) {
        None | Some("") | Some("/") => vault.to_path_buf(),
        Some(s) if s == "." || s == "./" => notes_dir.to_path_buf(),
        Some(s) if s.starts_with("./") => notes_dir.join(s.trim_start_matches("./")),
        Some(s) => vault.join(s.trim_start_matches('/')),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_folder() {
        let v = Path::new("/vault");
        let n = Path::new("/vault/Captures");
        assert_eq!(attachments_dir(v, n, None), PathBuf::from("/vault"));
        assert_eq!(attachments_dir(v, n, Some("/")), PathBuf::from("/vault"));
        assert_eq!(attachments_dir(v, n, Some("./")), PathBuf::from("/vault/Captures"));
        assert_eq!(attachments_dir(v, n, Some("./img")), PathBuf::from("/vault/Captures/img"));
        assert_eq!(attachments_dir(v, n, Some("Attachments")), PathBuf::from("/vault/Attachments"));
    }

    #[test]
    fn vaults_and_rules() {
        let tmp = std::env::temp_dir().join(format!("ms-obsidian-{}", std::process::id()));
        let a = tmp.join("Notes");
        let b = tmp.join("Travail");
        std::fs::create_dir_all(a.join(".obsidian")).unwrap();
        std::fs::create_dir_all(b.join("Captures")).unwrap();
        std::fs::create_dir_all(b.join(".obsidian")).unwrap();
        std::fs::write(b.join(".obsidian/app.json"), r#"{"attachmentFolderPath":"./assets","useMarkdownLinks":true}"#).unwrap();
        let json = format!(
            r#"{{"vaults":{{"1":{{"path":{},"ts":10}},"2":{{"path":{},"ts":20,"open":true}},"3":{{"path":"/does/not/exist","ts":30}}}}}}"#,
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        let v = parse_vaults(&json);
        assert_eq!(v.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Travail", "Notes"]);
        let r = rules_for(&b.join("Captures")).unwrap();
        assert_eq!(r.vault, b);
        assert_eq!(r.attachments, b.join("Captures/assets"));
        assert!(!r.wikilinks);
        let r = rules_for(&a).unwrap();
        assert_eq!(r.attachments, a);
        assert!(r.wikilinks);
        assert!(rules_for(&tmp).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
