use crate::config::Config;
use crate::i18n::{Language, Strings};
use crate::telemetry::{CrewRole, LocomotionState, MasterMode, TelemetrySnapshot};
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

    pub fn render(&self, lang: Language) -> RenderedPresence {
        self.render_with_telemetry(lang, &Config::default(), None)
    }

    pub fn render_with_telemetry(
        &self,
        lang: Language,
        config: &Config,
        telemetry: Option<&TelemetrySnapshot>,
    ) -> RenderedPresence {
        let tr = lang.strings();
        let active_telemetry = telemetry.filter(|_| config.use_telemetry);
        let env_suffix = match &self.env {
            Some(env) if !env.is_empty() => format!(" ({})", normalize_env_name(env)),
            _ => String::new(),
        };

        let player_prefix = self
            .player_name
            .as_deref()
            .map(|p| format!("{p} • "))
            .unwrap_or_default();

        match &self.mode {
            GameMode::Startup => RenderedPresence {
                details: format!("{}{env_suffix}", tr.startup_details_prefix),
                state: tr.startup_state.to_string(),
                large_image: "starcitizen_white".to_string(),
                large_text: format!("Star Citizen{env_suffix}"),
                small_image: None,
                small_text: None,
                start_timestamp: self.session_start,
            },
            GameMode::MainMenu => RenderedPresence {
                details: format!("{}{env_suffix}", tr.main_menu_details_prefix),
                state: self
                    .player_name
                    .as_deref()
                    .map(|p| format!("{}{p}", tr.main_menu_pilot_prefix))
                    .unwrap_or_else(|| tr.main_menu_ready.to_string()),
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
            GameMode::Loading { target } => {
                let target_label = translate_known_token(target, tr);
                RenderedPresence {
                    details: format!("{}{target_label}{env_suffix}", tr.loading_prefix),
                    state: self
                        .pending_pu_region
                        .as_deref()
                        .map(|r| format!("{}{r}", tr.loading_region_prefix))
                        .unwrap_or_else(|| tr.loading_connecting.to_string()),
                    large_image: "starcitizen_white".to_string(),
                    large_text: format!("Star Citizen{env_suffix}"),
                    small_image: None,
                    small_text: None,
                    start_timestamp: self.mode_start.or(self.session_start),
                }
            }
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
                let mut effective_small_image = manufacturer_key.clone();
                let mut effective_small_text = ship.clone();

                if let Some(tel) = active_telemetry {
                    if config.show_downed_status && tel.is_downed {
                        parts.push(tr.tel_downed.to_string());
                    }

                    match tel.locomotion {
                        LocomotionState::Seated => {
                            if let Some(v) = &tel.vehicle {
                                let v_name = if !v.class_name.trim().is_empty() {
                                    prettify_vehicle_class(&v.class_name)
                                } else if let Some(s) = ship {
                                    s.clone()
                                } else {
                                    v.instance_name.clone()
                                };

                                let role_prefix = if config.show_crew_role {
                                    match tel.crew_role {
                                        CrewRole::Pilot => tr.tel_role_pilot,
                                        CrewRole::Gunner => tr.tel_role_gunner,
                                        CrewRole::Crew => tr.tel_role_crew,
                                        CrewRole::Unknown => "",
                                    }
                                } else {
                                    ""
                                };

                                parts.push(format!("{role_prefix}{v_name}"));
                                effective_small_image = manufacturer_asset_key(&v.manufacturer)
                                    .or_else(|| manufacturer_asset_key(&v.class_name))
                                    .or_else(|| manufacturer_key.clone());
                                effective_small_text = Some(v_name);

                                if config.show_flight_details {
                                    if v.quantum_spooled {
                                        parts.push(tr.tel_quantum_ready.to_string());
                                    } else if v.is_landed {
                                        parts.push(tr.tel_landed.to_string());
                                    } else {
                                        match v.master_mode {
                                            MasterMode::Nav => parts.push("NAV".to_string()),
                                            MasterMode::Scm => parts.push("SCM".to_string()),
                                            MasterMode::Stealth | MasterMode::Unknown => {}
                                        }
                                    }
                                }

                                if config.show_combat_status {
                                    if v.missile_warning {
                                        parts.push(tr.tel_missile_warning.to_string());
                                    } else if v.under_fire {
                                        parts.push(tr.tel_under_fire.to_string());
                                    }
                                }

                                if config.show_ship_health {
                                    if v.shields_level.is_some_and(|s| s <= 0.05) {
                                        parts.push(tr.tel_shields_down.to_string());
                                    } else if v.health > 0.0 && v.health < 0.5 {
                                        let pct = (v.health * 100.0).round() as u32;
                                        parts.push(format!("{} {pct}%", tr.tel_hull_prefix));
                                    }
                                }
                            } else if let Some(ship_name) = ship {
                                parts.push(ship_name.clone());
                            }
                        }
                        LocomotionState::Eva => {
                            effective_small_image = None;
                            effective_small_text = None;
                            if config.show_eva_and_on_foot {
                                parts.push(tr.tel_eva.to_string());
                            }
                        }
                        LocomotionState::OnFoot => {
                            effective_small_image = None;
                            effective_small_text = None;
                            if config.show_eva_and_on_foot {
                                parts.push(tr.tel_on_foot.to_string());
                            }
                        }
                        LocomotionState::Unknown => {
                            if let Some(ship_name) = ship {
                                if let Some(owner) = ship_owner {
                                    parts.push(format!("{ship_name} ({}: {owner})", tr.pu_crew_label));
                                } else {
                                    parts.push(ship_name.clone());
                                }
                            }
                        }
                    }

                    if config.show_vitals_warning {
                        if tel.low_oxygen {
                            parts.push(tr.tel_low_oxygen.to_string());
                        } else if tel.low_health && !tel.is_downed {
                            parts.push(tr.tel_low_health.to_string());
                        }
                    }

                    if config.show_environment
                        && matches!(
                            tel.locomotion,
                            LocomotionState::OnFoot | LocomotionState::Eva
                        )
                    {
                        if !tel.breathable_atmosphere
                            && tel.locomotion == LocomotionState::OnFoot
                        {
                            parts.push(tr.tel_no_air.to_string());
                        } else if tel.ambient_radiation.is_some_and(|r| r >= 1.0) {
                            parts.push(tr.tel_radiation.to_string());
                        } else if tel
                            .external_temperature
                            .is_some_and(|t| t <= -80.0 || t >= 80.0)
                        {
                            parts.push(tr.tel_extreme_temp.to_string());
                        }
                    }

                    if config.show_balance_auec {
                        if let Some(bal) = tel.uec_balance {
                            parts.push(format_auec(bal));
                        }
                    }
                } else if let Some(ship_name) = ship {
                    if let Some(owner) = ship_owner {
                        parts.push(format!("{ship_name} ({}: {owner})", tr.pu_crew_label));
                    } else {
                        parts.push(ship_name.clone());
                    }
                }

                if let Some(loc) = location {
                    parts.push(loc.clone());
                } else if let Some(hangar) = hangar_state {
                    parts.push(translate_known_token(hangar, tr));
                } else if let Some(qt) = quantum_status {
                    parts.push(translate_known_token(qt, tr));
                } else if let Some(jur) = jurisdiction {
                    parts.push(jur.clone());
                }
                if *armistice && parts.len() < 2 {
                    parts.push(tr.pu_armistice_zone.to_string());
                }

                let state = if parts.is_empty() {
                    format!("{player_prefix}{}", tr.pu_in_verse)
                } else {
                    parts.join(" • ")
                };

                let (small_image, small_text) = match (effective_small_image, effective_small_text) {
                    (Some(mkey), Some(sname)) => (Some(mkey), Some(sname)),
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

fn format_auec(amount: i64) -> String {
    if amount >= 1_000_000 {
        format!("{:.1}M aUEC", amount as f64 / 1_000_000.0)
    } else if amount >= 1_000 {
        format!("{:.0}K aUEC", amount as f64 / 1_000.0)
    } else {
        format!("{amount} aUEC")
    }
}

fn translate_known_token(raw: &str, tr: &Strings) -> String {
    match raw {
        "Hauptmenü" => tr.loading_main_menu.to_string(),
        "Wartet auf Hangar" => tr.pu_waiting_for_hangar.to_string(),
        "Im Hangar" => tr.pu_in_hangar.to_string(),
        "Quantum-Ziel erreicht" => tr.pu_quantum_arrived.to_string(),
        other => other.to_string(),
    }
}

pub fn normalize_env_name(raw: &str) -> String {
    let upper = raw.trim().to_ascii_uppercase();
    match upper.as_str() {
        "PUB" | "LIVE" => "LIVE".to_string(),
        "PTU" => "PTU".to_string(),
        "EPTU" => "EPTU".to_string(),
        "TECH-PREVIEW" | "TECH_PREVIEW" | "TECHPREVIEW" | "TECH PREVIEW" => {
            "TECH PREVIEW".to_string()
        }
        _ => upper,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::VehicleTelemetry;

    #[test]
    fn telemetry_overrides_ship_and_handles_eva_and_downed() {
        let mut s = PresenceState::default();
        s.mode = GameMode::PersistentUniverse {
            system: Some("Stanton".to_string()),
            shard: None,
            region: Some("EU".to_string()),
            location: Some("Orison".to_string()),
            jurisdiction: None,
            armistice: false,
            hangar_state: None,
            quantum_status: None,
            ship: Some("Old Log Ship".to_string()),
            ship_owner: None,
            manufacturer_key: None,
        };

        let mut cfg = Config::default();
        let mut snap = TelemetrySnapshot::default();
        snap.locomotion = LocomotionState::Seated;
        snap.crew_role = CrewRole::Gunner;
        snap.vehicle = Some(VehicleTelemetry {
            instance_name: "AEGS_Hammerhead_99".to_string(),
            class_name: "AEGS_Hammerhead".to_string(),
            manufacturer: "AEGS".to_string(),
            is_landed: false,
            master_mode: MasterMode::Scm,
            quantum_spooled: false,
            health: 1.0,
            shields_level: Some(1.0),
            under_fire: false,
            missile_warning: false,
        });

        let r1 = s.render_with_telemetry(Language::De, &cfg, Some(&snap));
        assert_eq!(r1.state, "Geschütz: Aegis Hammerhead • SCM • Orison");
        assert_eq!(r1.small_image.as_deref(), Some("aegis"));

        // EVA + Downed
        snap.locomotion = LocomotionState::Eva;
        snap.vehicle = None;
        snap.is_downed = true;
        let r2 = s.render_with_telemetry(Language::De, &cfg, Some(&snap));
        assert_eq!(r2.state, "⚠ Kampfunfähig (Downed) • EVA im All • Orison");
        assert_eq!(r2.small_image, None);

        // Downed-Anzeige deaktiviert
        cfg.show_downed_status = false;
        let r3 = s.render_with_telemetry(Language::En, &cfg, Some(&snap));
        assert_eq!(r3.state, "EVA in Space • Orison");
    }

    #[test]
    fn telemetry_renders_optional_combat_health_env_vitals_and_balance() {
        let mut s = PresenceState::default();
        s.mode = GameMode::PersistentUniverse {
            system: Some("Pyro".to_string()),
            shard: None,
            region: None,
            location: None,
            jurisdiction: None,
            armistice: false,
            hangar_state: None,
            quantum_status: None,
            ship: None,
            ship_owner: None,
            manufacturer_key: None,
        };

        let mut cfg = Config::default();
        cfg.show_combat_status = true;
        cfg.show_ship_health = true;
        cfg.show_balance_auec = true;

        let mut snap = TelemetrySnapshot::default();
        snap.locomotion = LocomotionState::Seated;
        snap.crew_role = CrewRole::Pilot;
        snap.uec_balance = Some(4_250_000);
        snap.vehicle = Some(VehicleTelemetry {
            instance_name: "DRAK_Corsair_1".to_string(),
            class_name: "DRAK_Corsair".to_string(),
            manufacturer: "DRAK".to_string(),
            is_landed: false,
            master_mode: MasterMode::Scm,
            quantum_spooled: false,
            health: 0.35,
            shields_level: Some(0.0),
            under_fire: true,
            missile_warning: true,
        });

        let r1 = s.render_with_telemetry(Language::De, &cfg, Some(&snap));
        assert_eq!(
            r1.state,
            "Pilot: Drake Corsair • SCM • Raketenwarnung! • Schilde unten! • 4.2M aUEC"
        );

        // Zu Fuß mit Warnungen
        cfg.show_vitals_warning = true;
        cfg.show_environment = true;
        cfg.show_balance_auec = false;
        snap.locomotion = LocomotionState::OnFoot;
        snap.vehicle = None;
        snap.low_oxygen = true;
        snap.breathable_atmosphere = false;

        let r2 = s.render_with_telemetry(Language::Fr, &cfg, Some(&snap));
        assert_eq!(r2.state, "À pied • Oxygène critique ! • Air non respirable");
    }
}
