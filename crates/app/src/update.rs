use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const REPO: &str = "MrSacre/MarkdownShot";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub setup_url: Option<String>,
    pub page: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

pub fn latest() -> Result<Release> {
    let tmp = std::env::temp_dir().join(format!("markdownshot-release-{}.json", std::process::id()));
    download(&format!("https://api.github.com/repos/{REPO}/releases/latest"), &tmp)?;
    let txt = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    parse_release(&txt)
}

fn parse_release(json: &str) -> Result<Release> {
    let r: ApiRelease = serde_json::from_str(json).context("unreadable GitHub response")?;
    let setup_url =
        r.assets.iter().find(|a| a.name.ends_with("-setup.exe")).map(|a| a.browser_download_url.clone());
    Ok(Release { version: r.tag_name.trim_start_matches('v').to_string(), setup_url, page: r.html_url })
}

pub fn is_newer(current: &str, other: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(other) > parse(current)
}

pub fn download(url: &str, dest: &Path) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol='Tls12'; \
             Invoke-WebRequest -UseBasicParsing -Uri '{}' -OutFile '{}' -Headers @{{'User-Agent'='MarkdownShot'}}",
            url.replace('\'', "''"),
            dest.display().to_string().replace('\'', "''")
        );
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script]);
        c
    } else {
        let mut c = Command::new("curl");
        c.args(["-fsSL", "-A", "MarkdownShot", "-o"]).arg(dest).arg(url);
        c
    };
    hide_window(&mut cmd);
    let out = cmd.output().context("download failed")?;
    if !out.status.success() || !dest.is_file() {
        bail!("downloading {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_: &mut Command) {}

pub fn installed_with_setup() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("unins000.exe").is_file()))
        .unwrap_or(false)
}

pub fn fetch_installer(rel: &Release) -> Result<PathBuf> {
    let Some(url) = &rel.setup_url else { bail!("no installer attached to release {}", rel.version) };
    let dest = std::env::temp_dir().join(format!("MarkdownShot-{}-windows-x64-setup.exe", rel.version));
    download(url, &dest)?;
    Ok(dest)
}

pub fn run_installer(setup: &Path) -> Result<()> {
    Command::new(setup)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH"])
        .spawn()
        .with_context(|| format!("lancement de {}", setup.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.3.0", "0.3.1"));
        assert!(is_newer("0.3.9", "0.4.0"));
        assert!(is_newer("0.3.0", "v1.0.0"));
        assert!(!is_newer("0.3.1", "0.3.1"));
        assert!(!is_newer("0.10.0", "0.9.9"));
    }

    #[test]
    fn release_parsed() {
        let json = r#"{"tag_name":"v0.3.2","html_url":"https://github.com/MrSacre/MarkdownShot/releases/tag/v0.3.2",
            "assets":[{"name":"MarkdownShot-0.3.2-windows-x64-portable.zip","browser_download_url":"https://x/zip"},
                      {"name":"MarkdownShot-0.3.2-windows-x64-setup.exe","browser_download_url":"https://x/setup"}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "0.3.2");
        assert_eq!(r.setup_url.as_deref(), Some("https://x/setup"));
    }
}
