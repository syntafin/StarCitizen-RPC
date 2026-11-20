use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ENV_DIRS: &[&str] = &["LIVE", "PTU", "EPTU", "TECH-PREVIEW"];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_discord_app_id")]
    pub discord_app_id: String,
    #[serde(default = "default_sc_base_dirs")]
    pub sc_base_dirs: Vec<String>,
    #[serde(default)]
    pub preferred_env: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_require_running_process")]
    pub require_running_process: bool,
    #[serde(default)]
    pub setup_completed: bool,
}

fn default_discord_app_id() -> String {
    "1573256731809554483".to_string()
}

fn default_sc_base_dirs() -> Vec<String> {
    vec![
        r"D:\Roberts Space Industries\StarCitizen".to_string(),
        r"C:\Program Files\Roberts Space Industries\StarCitizen".to_string(),
    ]
}

fn default_poll_interval() -> u64 {
    1000
}

fn default_require_running_process() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            discord_app_id: default_discord_app_id(),
            sc_base_dirs: default_sc_base_dirs(),
            preferred_env: String::new(),
            poll_interval_ms: default_poll_interval(),
            require_running_process: default_require_running_process(),
            setup_completed: false,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            match toml::from_str::<Config>(&content) {
                Ok(cfg) => return cfg,
                Err(err) => {
                    eprintln!(
                        "Warnung: Konnte {} nicht parsen ({err}), nutze Defaults.",
                        path.display()
                    );
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Ordner {} konnte nicht erstellt werden: {e}", parent.display()))?;
            }
        }
        let serialized = toml::to_string_pretty(self)
            .map_err(|e| format!("Konfiguration konnte nicht serialisiert werden: {e}"))?;
        fs::write(path, serialized)
            .map_err(|e| format!("Konnte {} nicht speichern: {e}", path.display()))
    }
}

fn appdata_config_path() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("StarCitizenPresence").join("config.toml"))
}

fn exe_config_path() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("config.toml")))
}

fn is_dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".scp_write_test");
    if fs::write(&probe, b"").is_ok() {
        let _ = fs::remove_file(&probe);
        true
    } else {
        false
    }
}

pub fn resolve_config_path() -> PathBuf {
    if let Some(beside_exe) = exe_config_path() {
        if beside_exe.exists() {
            return beside_exe;
        }
    }
    if let Some(appdata) = appdata_config_path() {
        if appdata.exists() {
            return appdata;
        }
    }
    PathBuf::from("config.toml")
}

pub fn resolve_writable_config_path() -> PathBuf {
    if let Some(beside_exe) = exe_config_path() {
        if let Some(dir) = beside_exe.parent() {
            if is_dir_writable(dir) {
                return beside_exe;
            }
        }
    }
    if let Some(appdata) = appdata_config_path() {
        return appdata;
    }
    PathBuf::from("config.toml")
}

pub fn discover_sc_base_dirs() -> Vec<String> {
    let mut found = Vec::new();
    let suffixes = [
        r"Roberts Space Industries\StarCitizen",
        r"Program Files\Roberts Space Industries\StarCitizen",
        r"Program Files (x86)\Roberts Space Industries\StarCitizen",
        r"StarCitizen",
    ];

    for drive in b'C'..=b'Z' {
        let root = format!("{}:\\", drive as char);
        let root_path = Path::new(&root);
        if !root_path.exists() {
            continue;
        }
        for suffix in suffixes {
            let candidate = root_path.join(suffix);
            if has_any_sc_env(&candidate) {
                let s = candidate.to_string_lossy().to_string();
                if !found.contains(&s) {
                    found.push(s);
                }
            }
        }
    }

    found
}

pub fn has_any_sc_env(base_dir: &Path) -> bool {
    if !base_dir.is_dir() {
        return false;
    }
    ENV_DIRS.iter().any(|env| base_dir.join(env).is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_preserves_fields() {
        let tmp = env::temp_dir().join("scp_config_roundtrip_test.toml");
        let cfg = Config {
            discord_app_id: "12345".to_string(),
            sc_base_dirs: vec![r"E:\Games\StarCitizen".to_string()],
            preferred_env: "PTU".to_string(),
            poll_interval_ms: 1500,
            require_running_process: false,
            setup_completed: true,
        };

        cfg.save(&tmp).expect("save should succeed");
        let loaded = Config::load(&tmp);
        let _ = fs::remove_file(&tmp);

        assert_eq!(loaded.discord_app_id, "12345");
        assert_eq!(loaded.sc_base_dirs, vec![r"E:\Games\StarCitizen"]);
        assert_eq!(loaded.preferred_env, "PTU");
        assert_eq!(loaded.poll_interval_ms, 1500);
        assert!(!loaded.require_running_process);
        assert!(loaded.setup_completed);
    }
}
