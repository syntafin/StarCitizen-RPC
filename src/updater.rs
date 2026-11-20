use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io;
use std::process::Command;

const GITHUB_REPO: &str = "syntafin/starcitizen-rpc";
pub const CURRENT_VERSION: &str = match option_env!("APP_VERSION") {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
pub enum UpdateResult {
    UpToDate(String),
    Updated { from: String, to: String },
}

fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches('v');
    let core = trimmed.split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

pub fn check_and_update() -> Result<UpdateResult, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let resp = ureq::get(&url)
        .set("User-Agent", "star-citizen-presence")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub-Abfrage fehlgeschlagen: {e}"))?;

    let release: GitHubRelease = resp
        .into_json()
        .map_err(|e| format!("Ungültige GitHub-Antwort: {e}"))?;

    let remote_ver = parse_semver(&release.tag_name)
        .ok_or_else(|| format!("Ungültiges Release-Tag: {}", release.tag_name))?;
    let local_ver = parse_semver(CURRENT_VERSION).unwrap_or((0, 0, 0));

    if remote_ver <= local_ver {
        return Ok(UpdateResult::UpToDate(CURRENT_VERSION.to_string()));
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("star-citizen-presence.exe"))
        .or_else(|| {
            release.assets.iter().find(|a| {
                let lower = a.name.to_ascii_lowercase();
                lower.ends_with(".exe") && !lower.contains("setup")
            })
        })
        .ok_or_else(|| "Kein .exe-Asset im neuesten GitHub Release gefunden.".to_string())?;

    let dl_resp = ureq::get(&asset.browser_download_url)
        .set("User-Agent", "star-citizen-presence")
        .call()
        .map_err(|e| format!("Download fehlgeschlagen: {e}"))?;

    let tmp_path = env::temp_dir().join("star-citizen-presence-update.exe");
    {
        let mut reader = dl_resp.into_reader();
        let mut out = File::create(&tmp_path)
            .map_err(|e| format!("Temporäre Datei konnte nicht erstellt werden: {e}"))?;
        io::copy(&mut reader, &mut out)
            .map_err(|e| format!("Fehler beim Speichern des Updates: {e}"))?;
    }

    if is_same_as_current_exe(&tmp_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Ok(UpdateResult::UpToDate(CURRENT_VERSION.to_string()));
    }

    self_replace::self_replace(&tmp_path)
        .map_err(|e| format!("Binary-Austausch fehlgeschlagen: {e}"))?;
    let _ = std::fs::remove_file(&tmp_path);

    Ok(UpdateResult::Updated {
        from: CURRENT_VERSION.to_string(),
        to: release.tag_name,
    })
}

fn is_same_as_current_exe(candidate: &std::path::Path) -> bool {
    let Ok(cur_exe) = env::current_exe() else {
        return false;
    };
    let (Ok(cur_meta), Ok(new_meta)) = (std::fs::metadata(&cur_exe), std::fs::metadata(candidate))
    else {
        return false;
    };
    if cur_meta.len() != new_meta.len() {
        return false;
    }
    match (std::fs::read(&cur_exe), std::fs::read(candidate)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn restart_self() -> ! {
    if let Ok(exe) = env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
    std::process::exit(0);
}

#[cfg(target_os = "windows")]
pub fn show_message(title: &str, body: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let title_w: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let body_w: Vec<u16> = OsStr::new(body).encode_wide().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_message(title: &str, body: &str) {
    println!("[{title}] {body}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_tags() {
        assert_eq!(parse_semver("v0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_semver("0.2.1"), Some((0, 2, 1)));
        assert!(parse_semver("v0.2.0").unwrap() > parse_semver("0.1.9").unwrap());
    }
}
