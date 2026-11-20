#[cfg(target_os = "windows")]
mod imp {
    use std::env;
    use winreg::enums::*;
    use winreg::RegKey;

    const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "StarCitizenPresence";

    pub fn is_enabled() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY_PATH, KEY_READ) else {
            return false;
        };
        key.get_value::<String, _>(VALUE_NAME).is_ok()
    }

    pub fn set_enabled(enable: bool) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(RUN_KEY_PATH)
            .map_err(|e| format!("Registry-Schlüssel konnte nicht geöffnet werden: {e}"))?;

        if enable {
            let exe = env::current_exe()
                .map_err(|e| format!("Exe-Pfad konnte nicht ermittelt werden: {e}"))?;
            let quoted = format!("\"{}\"", exe.display());
            key.set_value(VALUE_NAME, &quoted)
                .map_err(|e| format!("Autostart-Eintrag fehlgeschlagen: {e}"))?;
        } else {
            match key.delete_value(VALUE_NAME) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Autostart-Eintrag konnte nicht entfernt werden: {e}")),
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enable: bool) -> Result<(), String> {
        Err("Autostart wird nur unter Windows unterstützt.".to_string())
    }
}

pub use imp::{is_enabled, set_enabled};
