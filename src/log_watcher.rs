use crate::config::Config;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const ENV_DIRS: &[&str] = &["LIVE", "PTU", "EPTU", "TECH-PREVIEW"];

pub struct LogWatcher {
    current_path: Option<PathBuf>,
    offset: u64,
}

impl LogWatcher {
    pub fn new() -> Self {
        Self {
            current_path: None,
            offset: 0,
        }
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    /// Sucht die aktivste `Game.log` und gibt `(neue_zeilen, wurde_neu_gestartet)` zurück.
    pub fn poll(&mut self, config: &Config) -> (Vec<String>, bool) {
        let latest = find_latest_log(config);
        let Some(path) = latest else {
            return (Vec::new(), false);
        };

        let mut reset = false;
        if self.current_path.as_ref() != Some(&path) {
            self.current_path = Some(path.clone());
            self.offset = 0;
            reset = true;
        }

        let Ok(meta) = fs::metadata(&path) else {
            return (Vec::new(), reset);
        };
        let len = meta.len();

        // Spiel wurde neu gestartet -> Game.log wurde gekürzt/neu angelegt
        if len < self.offset {
            self.offset = 0;
            reset = true;
        }

        if len == self.offset {
            return (Vec::new(), reset);
        }

        let Ok(file) = File::open(&path) else {
            return (Vec::new(), reset);
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return (Vec::new(), reset);
        }

        let mut lines = Vec::new();
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // Nur vollständige Zeilen verarbeiten (falls SC gerade mitten in einer Zeile schreibt)
                    if buf.ends_with(b"\n") {
                        self.offset += n as u64;
                        let line = String::from_utf8_lossy(&buf);
                        let trimmed = line.trim_end_matches(&['\r', '\n'][..]).to_string();
                        if !trimmed.is_empty() {
                            lines.push(trimmed);
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        (lines, reset)
    }
}

pub fn find_latest_log(config: &Config) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;

    for base in &config.sc_base_dirs {
        let base_path = Path::new(base);
        if !base_path.exists() {
            continue;
        }

        let envs: Vec<&str> = if !config.preferred_env.trim().is_empty() {
            vec![config.preferred_env.trim()]
        } else {
            ENV_DIRS.to_vec()
        };

        for env in envs {
            let candidate = base_path.join(env).join("Game.log");
            if let Ok(meta) = fs::metadata(&candidate) {
                if let Ok(modified) = meta.modified() {
                    let is_newer = best
                        .as_ref()
                        .is_none_or(|(best_time, _)| modified > *best_time);
                    if is_newer {
                        best = Some((modified, candidate));
                    }
                }
            }
        }
    }

    best.map(|(_, path)| path)
}

#[cfg(target_os = "windows")]
pub fn is_star_citizen_running() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq StarCitizen.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .map(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("StarCitizen.exe")
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_star_citizen_running() -> bool {
    true
}
