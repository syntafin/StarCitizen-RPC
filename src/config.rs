use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub discord_app_id: String,
    pub sc_base_dirs: Vec<String>,
    #[serde(default)]
    pub preferred_env: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_require_running_process")]
    pub require_running_process: bool,
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
            discord_app_id: "1573256731809554483".to_string(),
            sc_base_dirs: vec![
                r"D:\Roberts Space Industries\StarCitizen".to_string(),
                r"C:\Program Files\Roberts Space Industries\StarCitizen".to_string(),
            ],
            preferred_env: String::new(),
            poll_interval_ms: default_poll_interval(),
            require_running_process: default_require_running_process(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            match toml::from_str::<Config>(&content) {
                Ok(cfg) => return cfg,
                Err(err) => {
                    eprintln!("Warnung: Konnte {} nicht parsen ({err}), nutze Defaults.", path.display());
                }
            }
        }
        Self::default()
    }
}
