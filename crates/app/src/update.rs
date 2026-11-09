use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const REPO: &str = "MrSacre/MarkdownShot";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub setup_url: Option<String>,
    pub zip_url: Option<String>,
    pub page: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    Installer,
    Portable,
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
    let asset = |suffix: &str| r.assets.iter().find(|a| a.name.ends_with(suffix)).map(|a| a.browser_download_url.clone());
    let setup_url = asset("-setup.exe");
    let zip_url = asset("-portable.zip");
    Ok(Release { version: r.tag_name.trim_start_matches('v').to_string(), setup_url, zip_url, page: r.html_url })
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

fn app_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(".markdownshot-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn method(rel: &Release) -> Option<Method> {
    if installed_with_setup() {
        return rel.setup_url.as_ref().map(|_| Method::Installer);
    }
    let dir = app_dir()?;
    (cfg!(windows) && rel.zip_url.is_some() && writable(&dir)).then_some(Method::Portable)
}

pub fn fetch(rel: &Release, method: Method) -> Result<PathBuf> {
    let (url, name) = match method {
        Method::Installer => (&rel.setup_url, format!("MarkdownShot-{}-windows-x64-setup.exe", rel.version)),
        Method::Portable => (&rel.zip_url, format!("MarkdownShot-{}-windows-x64-portable.zip", rel.version)),
    };
    let Some(url) = url else { bail!("no file to download in release {}", rel.version) };
    let dest = std::env::temp_dir().join(name);
    download(url, &dest)?;
    Ok(dest)
}

pub fn apply(method: Method, file: &Path) -> Result<()> {
    match method {
        Method::Installer => {
            Command::new(file)
                .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH"])
                .spawn()
                .with_context(|| format!("starting {}", file.display()))?;
        }
        Method::Portable => {
            let dir = app_dir().context("application folder not found")?;
            let script = std::env::temp_dir().join(format!("markdownshot-update-{}.ps1", std::process::id()));
            std::fs::write(&script, portable_script(std::process::id(), file, &dir))
                .with_context(|| format!("writing {}", script.display()))?;
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]).arg(&script);
            hide_window(&mut cmd);
            cmd.spawn().context("starting the update script")?;
        }
    }
    Ok(())
}

fn portable_script(pid: u32, zip: &Path, dir: &Path) -> String {
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $zip = '{zip}'\r\n\
         $dest = '{dest}'\r\n\
         $work = Join-Path $env:TEMP ('markdownshot-update-' + [guid]::NewGuid())\r\n\
         try {{ Wait-Process -Id {pid} -Timeout 30 }} catch {{}}\r\n\
         try {{\r\n\
         \x20 Expand-Archive -LiteralPath $zip -DestinationPath $work -Force\r\n\
         \x20 $src = Join-Path $work 'MarkdownShot'\r\n\
         \x20 if (-not (Test-Path -LiteralPath $src)) {{ $src = $work }}\r\n\
         \x20 for ($i = 0; $i -lt 40; $i++) {{\r\n\
         \x20   try {{ Copy-Item -Path (Join-Path $src '*') -Destination $dest -Recurse -Force; break }}\r\n\
         \x20   catch {{ Start-Sleep -Milliseconds 500 }}\r\n\
         \x20 }}\r\n\
         }} finally {{\r\n\
         \x20 Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue\r\n\
         \x20 Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue\r\n\
         \x20 Start-Process -FilePath (Join-Path $dest 'markdownshot.exe')\r\n\
         \x20 Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\r\n\
         }}\r\n",
        zip = q(zip),
        dest = q(dir),
    )
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
        assert_eq!(r.zip_url.as_deref(), Some("https://x/zip"));
    }

    #[test]
    fn portable_script_quotes_paths() {
        let s = portable_script(42, Path::new("C:/Temp/it's.zip"), Path::new("C:/Apps/MarkdownShot"));
        assert!(s.contains("$zip = 'C:/Temp/it''s.zip'"));
        assert!(s.contains("Wait-Process -Id 42"));
        assert!(s.contains("Start-Process -FilePath (Join-Path $dest 'markdownshot.exe')"));
    }
}
