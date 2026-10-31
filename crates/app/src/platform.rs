use std::path::Path;

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) != 0 {
            return Some((p.x, p.y));
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn foreground_title() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
        let h = GetForegroundWindow();
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..n as usize]);
        if s.trim().is_empty() { None } else { Some(s) }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn open(target: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let op = wide("open");
        let t = wide(target);
        ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), t.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

pub fn open_path(p: &Path) {
    open(&p.to_string_lossy());
}

pub fn open_in_obsidian(p: &Path) {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut enc = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/:".contains(&b) {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    open(&format!("obsidian://open?path={enc}"));
}

pub fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn single_instance() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name = wide("Local\\MarkdownShotInstance");
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() {
            return true;
        }
        GetLastError() != ERROR_ALREADY_EXISTS
    }
    #[cfg(not(windows))]
    {
        true
    }
}

pub fn message_box(title: &str, text: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
        let t = wide(title);
        let m = wide(text);
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
    }
    #[cfg(not(windows))]
    {
        eprintln!("{title} : {text}");
    }
}

#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[cfg(windows)]
const RUN_NAME: &str = "MarkdownShot";

#[cfg(windows)]
fn run_value_exists(name: &str) -> bool {
    unsafe {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
        let key = wide(RUN_KEY);
        let name = wide(name);
        let mut size = 0u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        ) == 0
    }
}

#[cfg(windows)]
fn run_value_set(name: &str, on: bool) -> bool {
    unsafe {
        use windows_sys::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
        };
        let key = wide(RUN_KEY);
        let name = wide(name);
        let mut h: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_SET_VALUE, &mut h) != 0 {
            return false;
        }
        let ok = if on {
            let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            let val = wide(&format!("\"{exe}\""));
            RegSetValueExW(h, name.as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, (val.len() * 2) as u32) == 0
        } else {
            RegDeleteValueW(h, name.as_ptr()) == 0
        };
        RegCloseKey(h);
        ok
    }
}

pub fn autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        run_value_exists(RUN_NAME)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn set_autostart(on: bool) -> bool {
    #[cfg(windows)]
    {
        run_value_set(RUN_NAME, on)
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        false
    }
}

