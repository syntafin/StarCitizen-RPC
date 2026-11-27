use std::fs;
use std::path::{Path, PathBuf};

const ENV_VAR_NAME: &str = "GRPC_CLIENT_SERVER_ENABLED";
const ENV_SUBDIRS: &[&str] = &["LIVE", "PTU", "EPTU", "TECH-PREVIEW"];
const KEY_GRPC: &str = "grpc.client.server.enabled";
const KEY_TELEMETRY: &str = "server.game.telemetry.enabled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelCfgStatus {
    pub label: String,
    pub user_cfg_path: PathBuf,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySetupStatus {
    pub env_var_set: bool,
    pub channels: Vec<ChannelCfgStatus>,
}

impl TelemetrySetupStatus {
    pub fn is_fully_ready(&self) -> bool {
        self.env_var_set
            && !self.channels.is_empty()
            && self.channels.iter().all(|c| c.configured)
    }
}

pub fn inspect_telemetry_setup(sc_base_dirs: &[String]) -> TelemetrySetupStatus {
    let env_var_set = is_env_var_enabled();
    let mut channels = Vec::new();
    let mut seen_paths = Vec::new();

    for base in sc_base_dirs {
        let base_path = Path::new(base);
        if !base_path.is_dir() {
            continue;
        }
        for env in ENV_SUBDIRS {
            let channel_dir = base_path.join(env);
            if !channel_dir.is_dir() {
                continue;
            }
            let cfg_path = channel_dir.join("user.cfg");
            if seen_paths.iter().any(|p: &PathBuf| p == &cfg_path) {
                continue;
            }
            seen_paths.push(cfg_path.clone());

            let configured = fs::read_to_string(&cfg_path)
                .map(|content| is_user_cfg_configured(&content))
                .unwrap_or(false);

            channels.push(ChannelCfgStatus {
                label: (*env).to_string(),
                user_cfg_path: cfg_path,
                configured,
            });
        }
    }

    TelemetrySetupStatus {
        env_var_set,
        channels,
    }
}

pub fn apply_telemetry_setup(sc_base_dirs: &[String]) -> Result<TelemetrySetupStatus, String> {
    set_user_env_var_enabled()?;

    let status = inspect_telemetry_setup(sc_base_dirs);
    for ch in &status.channels {
        let existing = fs::read_to_string(&ch.user_cfg_path).unwrap_or_default();
        let updated = upsert_user_cfg_content(&existing);
        if existing != updated {
            fs::write(&ch.user_cfg_path, updated).map_err(|e| {
                format!(
                    "Konnte {} nicht schreiben: {e}",
                    ch.user_cfg_path.display()
                )
            })?;
        }
    }

    Ok(inspect_telemetry_setup(sc_base_dirs))
}

pub fn is_user_cfg_configured(content: &str) -> bool {
    let mut has_grpc = false;
    let mut has_telemetry = false;

    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if key.eq_ignore_ascii_case(KEY_GRPC) && val == "1" {
                has_grpc = true;
            } else if key.eq_ignore_ascii_case(KEY_TELEMETRY) && val == "1" {
                has_telemetry = true;
            }
        }
    }

    has_grpc && has_telemetry
}

pub fn upsert_user_cfg_content(existing: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut found_grpc = false;
    let mut found_telemetry = false;

    for raw_line in existing.lines() {
        let code_part = raw_line.split('#').next().unwrap_or("").trim();
        if let Some((k, _)) = code_part.split_once('=') {
            let key = k.trim();
            if key.eq_ignore_ascii_case(KEY_GRPC) {
                if !found_grpc {
                    lines.push(format!("{KEY_GRPC} = 1"));
                    found_grpc = true;
                }
                continue;
            }
            if key.eq_ignore_ascii_case(KEY_TELEMETRY) {
                if !found_telemetry {
                    lines.push(format!("{KEY_TELEMETRY} = 1"));
                    found_telemetry = true;
                }
                continue;
            }
        }
        lines.push(raw_line.to_string());
    }

    if !found_grpc {
        lines.push(format!("{KEY_GRPC} = 1"));
    }
    if !found_telemetry {
        lines.push(format!("{KEY_TELEMETRY} = 1"));
    }

    let mut out = lines.join("\r\n");
    if !out.is_empty() {
        out.push_str("\r\n");
    }
    out
}

#[cfg(target_os = "windows")]
fn is_env_var_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment") {
        if let Ok(val) = hkcu.get_value::<String, _>(ENV_VAR_NAME) {
            let norm = val.trim().to_ascii_lowercase();
            if norm == "true" || norm == "1" {
                return true;
            }
        }
    }

    std::env::var(ENV_VAR_NAME)
        .map(|v| {
            let norm = v.trim().to_ascii_lowercase();
            norm == "true" || norm == "1"
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn is_env_var_enabled() -> bool {
    std::env::var(ENV_VAR_NAME)
        .map(|v| {
            let norm = v.trim().to_ascii_lowercase();
            norm == "true" || norm == "1"
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn set_user_env_var_enabled() -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu
        .create_subkey("Environment")
        .map_err(|e| format!("Konnte HKCU\\Environment nicht öffnen: {e}"))?;
    env_key
        .set_value(ENV_VAR_NAME, &"true")
        .map_err(|e| format!("Konnte {ENV_VAR_NAME} nicht setzen: {e}"))?;

    let env_wide: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut _result = 0usize;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env_wide.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            500,
            &mut _result,
        );
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_user_env_var_enabled() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upserts_user_cfg_without_clobbering_existing_settings() {
        let initial = "r_DisplayInfo = 3\r\ngrpc.client.server.enabled = 0\r\n";
        let updated = upsert_user_cfg_content(initial);
        assert!(updated.contains("r_DisplayInfo = 3"));
        assert!(is_user_cfg_configured(&updated));
    }

    #[test]
    fn detects_missing_keys_in_user_cfg() {
        assert!(!is_user_cfg_configured("grpc.client.server.enabled = 1\n"));
        assert!(is_user_cfg_configured(
            "grpc.client.server.enabled = 1\nserver.game.telemetry.enabled = 1\n"
        ));
    }
}
