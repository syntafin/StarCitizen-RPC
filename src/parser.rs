use crate::state::{
    manufacturer_asset_key, normalize_env_name, prettify_ac_name, prettify_location,
    prettify_vehicle_class, region_from_shard, GameMode, PresenceState,
};
use chrono::DateTime;
use regex::Regex;
use std::sync::OnceLock;

struct Patterns {
    timestamp: Regex,
    env_tag: Regex,
    build: Regex,
    character: Regex,
    request_mode: Regex,
    loading_closed: Regex,
    join_pu: Regex,
    location_inventory: Regex,
    leaving_landing: Regex,
    notification: Regex,
    ship_channel: Regex,
    ac_vehicle_spawn: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        timestamp: Regex::new(r"^<(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)>").unwrap(),
        env_tag: Regex::new(r"--envtag='([^']+)'").unwrap(),
        build: Regex::new(r#"Build\((\d+)\)"#).unwrap(),
        character: Regex::new(r"<AccountLoginCharacterStatus_Character>.*\bname\s+([^\s]+)\s+-").unwrap(),
        request_mode: Regex::new(r"Requesting game mode\s+([^/\s]+)/([^\s]+)").unwrap(),
        loading_closed: Regex::new(r"Loading screen for\s+([^\s:]+)\s*:\s*([^\s]+)\s+closed").unwrap(),
        join_pu: Regex::new(r"<Join PU>.*shard\[([^\]]+)\]").unwrap(),
        location_inventory: Regex::new(r"<RequestLocationInventory>.*Location\[([^\]]+)\]").unwrap(),
        leaving_landing: Regex::new(r"<Update Inventory Location>.*\bLanding\s+\[\d+\]\s*->\s*\[0\]").unwrap(),
        notification: Regex::new(r#"<SHUDEvent_OnNotification>\s+Added notification\s+"([^"]+)""#).unwrap(),
        ship_channel: Regex::new(r#"You have joined channel '([^']+)'"#).unwrap(),
        ac_vehicle_spawn: Regex::new(r"\[VEHICLE SPAWN\]\s+CPlayerShipRespawnManager::OnVehicleSpawned\s+\d+\s+\(([^)]+)\)").unwrap(),
    })
}

fn parse_timestamp(line: &str) -> Option<i64> {
    let cap = patterns().timestamp.captures(line)?;
    DateTime::parse_from_rfc3339(&cap[1])
        .ok()
        .map(|dt| dt.timestamp())
}

pub fn apply_log_line(state: &mut PresenceState, line: &str) {
    let ts = parse_timestamp(line);
    if state.session_start.is_none() {
        state.session_start = ts;
    }

    let p = patterns();

    if let Some(cap) = p.build.captures(line) {
        if state.build.is_none() {
            state.build = Some(cap[1].to_string());
        }
    }

    if let Some(cap) = p.env_tag.captures(line) {
        state.env = Some(normalize_env_name(&cap[1]));
    }

    if let Some(cap) = p.character.captures(line) {
        state.player_name = Some(cap[1].to_string());
    }

    if let Some(cap) = p.join_pu.captures(line) {
        let shard = cap[1].to_string();
        let region = region_from_shard(&shard);
        state.pending_pu_shard = Some(shard);
        state.pending_pu_region = region;
        state.mode_start = ts;
        state.mode = GameMode::Loading {
            target: "Persistent Universe".to_string(),
        };
        return;
    }

    if let Some(cap) = p.request_mode.captures(line) {
        let map = &cap[1];
        let gamerules = &cap[2];
        state.mode_start = ts;
        if gamerules == "SC_Frontend" {
            state.mode = GameMode::Loading {
                target: "Hauptmenü".to_string(),
            };
        } else {
            state.mode = GameMode::Loading {
                target: format!("{} ({})", prettify_ac_name(gamerules), prettify_ac_name(map)),
            };
        }
        return;
    }

    if let Some(cap) = p.loading_closed.captures(line) {
        let target = &cap[1];
        let gamerules = &cap[2];
        state.mode_start = ts;

        if target == "Frontend_Main" && gamerules == "SC_Frontend" {
            state.pending_pu_shard = None;
            state.pending_pu_region = None;
            state.mode = GameMode::MainMenu;
        } else if gamerules == "SC_Frontend" || gamerules == "SC_Default" {
            state.mode = GameMode::PersistentUniverse {
                system: Some(target.to_string()),
                shard: state.pending_pu_shard.take(),
                region: state.pending_pu_region.take(),
                location: None,
                jurisdiction: None,
                armistice: false,
                hangar_state: None,
                quantum_status: None,
                ship: None,
                ship_owner: None,
                manufacturer_key: None,
            };
        } else {
            state.mode = GameMode::ArenaCommander {
                map: prettify_ac_name(target),
                mode: prettify_ac_name(gamerules),
                ship: None,
                manufacturer_key: None,
            };
        }
        return;
    }

    if let Some(cap) = p.location_inventory.captures(line) {
        if let GameMode::PersistentUniverse {
            location,
            quantum_status,
            ..
        } = &mut state.mode
        {
            *location = Some(prettify_location(&cap[1]));
            *quantum_status = None;
        }
        return;
    }

    if p.leaving_landing.is_match(line) {
        if let GameMode::PersistentUniverse {
            location,
            hangar_state,
            ..
        } = &mut state.mode
        {
            *location = None;
            *hangar_state = None;
        }
        return;
    }

    if line.contains("<Quantum Drive Arrived - Arrived at Final Destination>")
        && line.contains("| AUTH |")
    {
        if let GameMode::PersistentUniverse {
            location,
            hangar_state,
            quantum_status,
            ..
        } = &mut state.mode
        {
            *location = None;
            *hangar_state = None;
            *quantum_status = Some("Quantum-Ziel erreicht".to_string());
        }
        return;
    }

    if let Some(cap) = p.ac_vehicle_spawn.captures(line) {
        let raw_class = &cap[1];
        if raw_class.to_ascii_lowercase().starts_with("default_") {
            return;
        }
        let pretty = prettify_vehicle_class(raw_class);
        let mfr = manufacturer_asset_key(raw_class);
        if let GameMode::ArenaCommander {
            ship,
            manufacturer_key,
            ..
        } = &mut state.mode
        {
            *ship = Some(pretty);
            *manufacturer_key = mfr;
        }
        return;
    }

    // Channel-Join Notification (im Log teils ohne schließendes Anführungszeichen in derselben Zeile)
    if line.contains("<SHUDEvent_OnNotification>") {
        if let Some(cap) = p.ship_channel.captures(line) {
            let channel_name = cap[1].trim();
            // Format: "Kruger L-21 Wolf : Syntafin"
            if let Some((ship_part, owner)) = channel_name.split_once(" : ") {
                let ship_clean = ship_part.trim().to_string();
                let owner_clean = owner.trim();
                let mfr = manufacturer_asset_key(&ship_clean);
                let is_crew = state
                    .player_name
                    .as_deref()
                    .is_some_and(|me| !me.eq_ignore_ascii_case(owner_clean));
                if let GameMode::PersistentUniverse {
                    ship,
                    ship_owner,
                    manufacturer_key,
                    ..
                } = &mut state.mode
                {
                    *ship = Some(ship_clean);
                    *ship_owner = if is_crew {
                        Some(owner_clean.to_string())
                    } else {
                        None
                    };
                    *manufacturer_key = mfr;
                }
            }
            return;
        }

        if let Some(cap) = p.notification.captures(line) {
            let msg = cap[1].trim();
            if let GameMode::PersistentUniverse {
                location,
                jurisdiction,
                armistice,
                hangar_state,
                quantum_status,
                ..
            } = &mut state.mode
            {
                if let Some(rest) = msg.strip_prefix("Entered ") {
                    let clean = rest.trim_end_matches(':').trim();
                    let clean = clean.strip_suffix(" Jurisdiction").unwrap_or(clean);
                    *jurisdiction = Some(clean.to_string());
                    *armistice = false;
                    *quantum_status = None;
                } else if msg.starts_with("Entering Armistice Zone") {
                    *armistice = true;
                    *quantum_status = None;
                } else if msg.starts_with("Leaving Armistice Zone") {
                    *armistice = false;
                    *hangar_state = None;
                    *location = None;
                } else if msg.starts_with("Joined hangar queue") {
                    *hangar_state = Some("Wartet auf Hangar".to_string());
                    *quantum_status = None;
                } else if msg.starts_with("Hangar Request Completed") {
                    *hangar_state = Some("Im Hangar".to_string());
                    *quantum_status = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn parses_pu_and_ship_and_ac_flow() {
        let mut s = PresenceState::default();

        apply_log_line(&mut s, r#"<2026-11-20T08:47:08.389Z> BackupNameAttachment=" Build(13238477) 20 Nov 26 (09 47 00)"  -- used by backup system"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:47:08.392Z>    [Cmdline* ] --envtag='PTU'"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:47:34.905Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1795164453012 - updatedAt 1795164453018 - geid 876941673994 - accountId 6119476 - name Syntafin - state STATE_UNSPECIFIED"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:47:47.439Z> Loading screen for Frontend_Main : SC_Frontend closed after 4.01 seconds"#);

        assert_eq!(s.env.as_deref(), Some("PTU"));
        assert_eq!(s.build.as_deref(), Some("13238477"));
        assert_eq!(s.player_name.as_deref(), Some("Syntafin"));
        assert_eq!(s.mode, GameMode::MainMenu);

        apply_log_line(&mut s, r#"<2026-11-20T08:52:11.135Z> [Notice] <Join PU> address[34.85.161.105] port[64366] shard[ptu_use1b_13238477_010] locationId[1407379178586113]"#);
        assert!(matches!(s.mode, GameMode::Loading { .. }));

        apply_log_line(&mut s, r#"<2026-11-20T08:53:22.817Z> Loading screen for Nyx : SC_Frontend closed after 71.65 seconds"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:53:15.969Z> [Notice] <RequestLocationInventory> Player[Syntafin] requested inventory for Location[Nyx_Levski]"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:56:29.404Z> [Notice] <SHUDEvent_OnNotification> Added notification "You have joined channel 'Kruger L-21 Wolf : Syntafin'."#);

        let rendered = s.render(Language::De);
        assert_eq!(rendered.details, "PU (PTU): Nyx • US-East");
        assert_eq!(rendered.state, "Kruger L-21 Wolf • Levski");
        assert_eq!(rendered.large_image, "starcitizen_white");
        assert_eq!(rendered.small_image.as_deref(), Some("kruger"));

        apply_log_line(&mut s, r#"<2026-11-20T09:06:08.419Z> Requesting game mode EA_AstorsClearing/EA_FreeFlight"#);
        apply_log_line(&mut s, r#"<2026-11-20T09:06:26.084Z> Loading screen for EA_AstorsClearing : EA_FreeFlight closed after 17.65 seconds"#);
        apply_log_line(&mut s, r#"<2026-11-20T09:06:39.974Z> [VEHICLE SPAWN] CPlayerShipRespawnManager::OnVehicleSpawned 200000304500 (ESPR_Talon_200000304500) by player 0"#);
        apply_log_line(&mut s, r#"<2026-11-20T09:06:40.894Z> [VEHICLE SPAWN] CPlayerShipRespawnManager::OnVehicleSpawned 10182 (Default_10182) by player 0"#);

        let rendered_ac = s.render(Language::De);
        assert_eq!(rendered_ac.details, "AC (PTU): Free Flight (Astors Clearing)");
        assert_eq!(rendered_ac.state, "Syntafin • Esperia Talon");
        assert_eq!(rendered_ac.large_image, "starcitizen_white");
        assert_eq!(rendered_ac.small_image.as_deref(), Some("esperia"));
    }

    #[test]
    fn handles_departure_crew_and_auth_quantum() {
        let mut s = PresenceState::default();
        apply_log_line(&mut s, r#"<2026-11-20T08:47:34.905Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 1 - geid 1 - accountId 1 - name Syntafin - state STATE_UNSPECIFIED"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:52:11.135Z> [Notice] <Join PU> address[1.2.3.4] port[64366] shard[ptu_use1b_13238477_010] locationId[1]"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:53:22.817Z> Loading screen for Nyx : SC_Frontend closed after 71.65 seconds"#);
        apply_log_line(&mut s, r#"<2026-11-20T08:53:15.969Z> [Notice] <RequestLocationInventory> Player[Syntafin] requested inventory for Location[Nyx_Levski]"#);
        apply_log_line(&mut s, r#"<2026-11-20T09:20:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification "You have joined channel 'Drake Corsair : Kumpel'."#);

        // NOT AUTH Quantum Travel von anderem Spieler darf Levski nicht löschen
        apply_log_line(&mut s, r#"<2026-11-20T09:21:00.000Z> [Notice] <Quantum Drive Arrived - Arrived at Final Destination> [ItemNavigation][CL][40088] | NOT AUTH | ANVL_Asgard_123[123]|CSCItemNavigation::OnQuantumDriveArrived"#);
        let r1 = s.render(Language::De);
        assert_eq!(r1.state, "Drake Corsair (Crew: Kumpel) • Levski");
        assert_eq!(r1.small_image.as_deref(), Some("drake"));

        // Abflug / Leaving Armistice setzt Landezone zurück
        apply_log_line(&mut s, r#"<2026-11-20T09:30:28.723Z> [Notice] <SHUDEvent_OnNotification> Added notification "Entered Ungoverned Jurisdiction: ""#);
        apply_log_line(&mut s, r#"<2026-11-20T09:30:30.365Z> [Notice] <SHUDEvent_OnNotification> Added notification "Leaving Armistice Zone - Caution Advised: ""#);
        apply_log_line(&mut s, r#"<2026-11-20T09:30:34.550Z> [Notice] <Update Inventory Location> Player [Syntafin] is changing location. Landing [3723364946] -> [0]. Location [3723364946] -> [2627827560]. Pending [0]"#);
        let r2 = s.render(Language::De);
        assert_eq!(r2.state, "Drake Corsair (Crew: Kumpel) • Ungoverned");

        // Eigenes AUTH Quantum Travel
        apply_log_line(&mut s, r#"<2026-11-20T09:35:00.000Z> [Notice] <Quantum Drive Arrived - Arrived at Final Destination> [ItemNavigation][CL][40088] | AUTH | DRAK_Corsair_123[123]|CSCItemNavigation::OnQuantumDriveArrived"#);
        let r3 = s.render(Language::De);
        assert_eq!(r3.state, "Drake Corsair (Crew: Kumpel) • Quantum-Ziel erreicht");

        let r3_en = s.render(Language::En);
        assert_eq!(
            r3_en.state,
            "Drake Corsair (Crew: Kumpel) • Quantum Destination Reached"
        );

        let r3_fr = s.render(Language::Fr);
        assert_eq!(
            r3_fr.state,
            "Drake Corsair (Équipage: Kumpel) • Destination Quantum atteinte"
        );
    }

    #[test]
    fn normalizes_env_tags_for_display() {
        for (raw, expected) in [
            ("PUB", "LIVE"),
            ("LIVE", "LIVE"),
            ("PTU", "PTU"),
            ("EPTU", "EPTU"),
            ("TECH-PREVIEW", "TECH PREVIEW"),
        ] {
            let mut s = PresenceState::default();
            apply_log_line(
                &mut s,
                &format!(r#"<2026-11-20T10:21:17.228Z>    [Cmdline* ] --envtag='{raw}'"#),
            );
            apply_log_line(
                &mut s,
                r#"<2026-11-20T10:22:00.000Z> Loading screen for Nyx : SC_Frontend closed after 5.0 seconds"#,
            );
            assert_eq!(s.env.as_deref(), Some(expected));
            assert_eq!(s.render(Language::De).details, format!("PU ({expected}): Nyx"));
        }
    }
}
