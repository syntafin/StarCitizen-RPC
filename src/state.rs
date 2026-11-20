use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameMode {
    Startup,
    MainMenu,
    Loading {
        target: String,
    },
    PersistentUniverse {
        system: Option<String>,
        shard: Option<String>,
        region: Option<String>,
        location: Option<String>,
        jurisdiction: Option<String>,
        armistice: bool,
        hangar_state: Option<String>,
        quantum_status: Option<String>,
        ship: Option<String>,
        ship_owner: Option<String>,
        manufacturer_key: Option<String>,
    },
    ArenaCommander {
        map: String,
        mode: String,
        ship: Option<String>,
        manufacturer_key: Option<String>,
    },
}

impl Default for GameMode {
    fn default() -> Self {
        Self::Startup
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PresenceState {
    pub env: Option<String>,
    pub build: Option<String>,
    pub player_name: Option<String>,
    pub session_start: Option<i64>,
    pub mode_start: Option<i64>,
    pub mode: GameMode,
    pub pending_pu_shard: Option<String>,
    pub pending_pu_region: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPresence {
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    pub start_timestamp: Option<i64>,
}

impl PresenceState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn render(&self) -> RenderedPresence {
        let env_suffix = match &self.env {
            Some(env) if !env.is_empty() && env != "LIVE" => format!(" ({env})"),
            _ => String::new(),
        };

        let player_prefix = self
            .player_name
            .as_deref()
            .map(|p| format!("{p} • "))
            .unwrap_or_default();

        match &self.mode {
            GameMode::Startup => RenderedPresence {
                details: format!("Startet Star Citizen{env_suffix}"),
                state: "Initialisierung...".to_string(),
                large_image: "starcitizen_white".to_string(),
                large_text: format!("Star Citizen{env_suffix}"),
                small_image: None,
                small_text: None,
                start_timestamp: self.session_start,
            },
            GameMode::MainMenu => RenderedPresence {
                details: format!("Im Hauptmenü{env_suffix}"),
                state: self
                    .player_name
                    .as_deref()
                    .map(|p| format!("Pilot: {p}"))
                    .unwrap_or_else(|| "Bereit zum Start".to_string()),
                large_image: "starcitizen_white".to_string(),
                large_text: self
                    .build
                    .as_deref()
                    .map(|b| format!("Build {b}"))
                    .unwrap_or_else(|| format!("Star Citizen{env_suffix}")),
                small_image: None,
                small_text: None,
                start_timestamp: self.mode_start.or(self.session_start),
            },
            GameMode::Loading { target } => RenderedPresence {
                details: format!("Lädt {target}{env_suffix}"),
                state: self
                    .pending_pu_region
                    .as_deref()
                    .map(|r| format!("Region: {r}"))
                    .unwrap_or_else(|| "Verbinde...".to_string()),
                large_image: "starcitizen_white".to_string(),
                large_text: format!("Star Citizen{env_suffix}"),
                small_image: None,
                small_text: None,
                start_timestamp: self.mode_start.or(self.session_start),
            },
            GameMode::PersistentUniverse {
                system,
                region,
                location,
                jurisdiction,
                armistice,
                hangar_state,
                quantum_status,
                ship,
                ship_owner,
                manufacturer_key,
                ..
            } => {
                let sys_text = system.as_deref().unwrap_or("Persistent Universe");
                let region_text = region
                    .as_deref()
                    .map(|r| format!(" • {r}"))
                    .unwrap_or_default();
                let details = format!("PU{env_suffix}: {sys_text}{region_text}");

                let mut parts: Vec<String> = Vec::new();
                if let Some(ship_name) = ship {
                    if let Some(owner) = ship_owner {
                        parts.push(format!("{ship_name} (Crew: {owner})"));
                    } else {
                        parts.push(ship_name.clone());
                    }
                }
                if let Some(loc) = location {
                    parts.push(loc.clone());
                } else if let Some(hangar) = hangar_state {
                    parts.push(hangar.clone());
                } else if let Some(qt) = quantum_status {
                    parts.push(qt.clone());
                } else if let Some(jur) = jurisdiction {
                    parts.push(jur.clone());
                }
                if *armistice && parts.len() < 2 {
                    parts.push("Armistice Zone".to_string());
                }

                let state = if parts.is_empty() {
                    format!("{player_prefix}Im Verse")
                } else {
                    parts.join(" • ")
                };

                let (small_image, small_text) = match (manufacturer_key, ship) {
                    (Some(mkey), Some(sname)) => (Some(mkey.clone()), Some(sname.clone())),
                    _ => (None, None),
                };

                RenderedPresence {
                    details,
                    state,
                    large_image: "starcitizen_white".to_string(),
                    large_text: format!("Persistent Universe{env_suffix}"),
                    small_image,
                    small_text,
                    start_timestamp: self.mode_start.or(self.session_start),
                }
            }
            GameMode::ArenaCommander {
                map,
                mode,
                ship,
                manufacturer_key,
            } => {
                let details = format!("AC{env_suffix}: {mode} ({map})");
                let state = ship
                    .as_deref()
                    .map(|s| format!("{player_prefix}{s}"))
                    .unwrap_or_else(|| format!("{player_prefix}Arena Commander"));

                let (small_image, small_text) = match (manufacturer_key, ship) {
                    (Some(mkey), Some(sname)) => (Some(mkey.clone()), Some(sname.clone())),
                    _ => (None, None),
                };

                RenderedPresence {
                    details,
                    state,
                    large_image: "starcitizen_white".to_string(),
                    large_text: format!("Arena Commander{env_suffix}"),
                    small_image,
                    small_text,
                    start_timestamp: self.mode_start.or(self.session_start),
                }
            }
        }
    }
}

impl fmt::Display for RenderedPresence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{} | {}] (img: {})",
            self.details, self.state, self.large_image
        )
    }
}

pub fn region_from_shard(shard: &str) -> Option<String> {
    let lower = shard.to_ascii_lowercase();
    if lower.contains("_use1") || lower.contains("_us1") {
        Some("US-East".to_string())
    } else if lower.contains("_usw") {
        Some("US-West".to_string())
    } else if lower.contains("_euw") || lower.contains("_eu1") {
        Some("EU".to_string())
    } else if lower.contains("_apse") || lower.contains("_au") {
        Some("AUS".to_string())
    } else if lower.contains("_ap") {
        Some("APAC".to_string())
    } else {
        None
    }
}

/// Mappt Hersteller-Namen oder Fahrzeug-Klassenpräfixe auf die Asset-Keys aus `images/manufactures/*.png`.
pub fn manufacturer_asset_key(name_or_class: &str) -> Option<String> {
    let upper = name_or_class.trim().to_ascii_uppercase();
    let code = upper.split(['_', ' ']).next().unwrap_or("");

    let key = match code {
        "AEGS" | "AEGIS" => "aegis",
        "ANVL" | "ANVIL" => "anvil",
        "AOPO" | "AOPOA" | "XIAN" => "aopoa",
        "ARGO" => "argo",
        "BANU" => "banu",
        "CNOU" | "CONSOLIDATED" => "cnou",
        "CRSD" | "CRUS" | "CRUSADER" => "crusader",
        "DRAK" | "DRAKE" => "drake",
        "ESPR" | "ESPERIA" => "esperia",
        "GATC" | "GATAC" => "gatac",
        "KRIG" | "KRUGER" => "kruger",
        "MIRA" | "MIRAI" => "mirai",
        "MISC" => "misc",
        "ORIG" | "ORIGIN" => "origin",
        "RSI" => "rsi",
        "TMBL" | "TUMBRIL" => "tumbril",
        _ => return None,
    };

    Some(key.to_string())
}

pub fn prettify_vehicle_class(raw: &str) -> String {
    // Beispiel: "ESPR_Talon_200000304500" -> "Esperia Talon"
    let parts: Vec<&str> = raw
        .split('_')
        .filter(|p| !p.is_empty() && !p.chars().all(|c| c.is_ascii_digit()))
        .collect();

    if parts.is_empty() {
        return raw.to_string();
    }

    let mfr_upper = parts[0].to_ascii_uppercase();
    let mfr = match mfr_upper.as_str() {
        "AEGS" => "Aegis",
        "ANVL" => "Anvil",
        "AOPO" => "Aopoa",
        "ARGO" => "ARGO",
        "BANU" => "Banu",
        "CNOU" => "Consolidated Outland",
        "CRSD" | "CRUS" => "Crusader",
        "DRAK" => "Drake",
        "ESPR" => "Esperia",
        "GATC" => "Gatac",
        "KRIG" => "Kruger",
        "MIRA" => "Mirai",
        "MISC" => "MISC",
        "ORIG" => "Origin",
        "RSI" => "RSI",
        "TMBL" => "Tumbril",
        other => other,
    };

    if parts.len() == 1 {
        mfr.to_string()
    } else {
        format!("{mfr} {}", parts[1..].join(" "))
    }
}

pub fn prettify_location(raw: &str) -> String {
    // Beispiel: "Nyx_Levski" -> "Levski", "Stanton4_AstorsClearing" -> "Astors Clearing"
    let tail = raw.split('_').last().unwrap_or(raw);
    split_pascal_case(tail)
}

pub fn prettify_ac_name(raw: &str) -> String {
    let trimmed = raw.strip_prefix("EA_").unwrap_or(raw);
    split_pascal_case(trimmed)
}

fn split_pascal_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    let chars: Vec<char> = input.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if i > 0 && ch.is_uppercase() {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if prev.is_lowercase() || (prev.is_uppercase() && next_is_lower) {
                out.push(' ');
            }
        }
        out.push(ch);
    }
    out
}
