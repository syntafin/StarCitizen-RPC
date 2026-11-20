use crate::autostart;
use crate::config::{discover_sc_base_dirs, has_any_sc_env, Config};
use crate::updater;
use std::path::{Path, PathBuf};

const ENV_SUBDIRS: &[&str] = &["LIVE", "PTU", "EPTU", "TECH-PREVIEW"];

pub fn normalize_sc_base_dir(picked: &Path) -> PathBuf {
    if let Some(name) = picked.file_name().and_then(|n| n.to_str()) {
        if ENV_SUBDIRS.iter().any(|env| env.eq_ignore_ascii_case(name)) {
            if let Some(parent) = picked.parent() {
                return parent.to_path_buf();
            }
        }
    }
    picked.to_path_buf()
}

pub fn upsert_sc_base_dir(dirs: &mut Vec<String>, new_dir: &Path) {
    let normalized = normalize_sc_base_dir(new_dir);
    let value = normalized.to_string_lossy().to_string();
    dirs.retain(|existing| !existing.eq_ignore_ascii_case(&value));
    dirs.insert(0, value);
}

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

#[cfg(target_os = "windows")]
fn ask_yes_no(title: &str, body: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
    };

    let title_w = to_wide(title);
    let body_w = to_wide(body);
    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    result == IDYES
}

#[cfg(not(target_os = "windows"))]
fn ask_yes_no(_title: &str, _body: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn ask_yes_no_cancel(title: &str, body: &str) -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDNO, IDYES, MB_ICONQUESTION, MB_YESNOCANCEL,
    };

    let title_w = to_wide(title);
    let body_w = to_wide(body);
    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_YESNOCANCEL | MB_ICONQUESTION,
        )
    };
    match result {
        IDYES => Some(true),
        IDNO => Some(false),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn ask_yes_no_cancel(_title: &str, _body: &str) -> Option<bool> {
    None
}

#[cfg(target_os = "windows")]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::MAX_PATH;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };

    let title_w = to_wide(title);
    let mut display_name = vec![0u16; MAX_PATH as usize];

    unsafe {
        let co_hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut bi = BROWSEINFOW {
            hwndOwner: std::ptr::null_mut(),
            pidlRoot: std::ptr::null_mut(),
            pszDisplayName: display_name.as_mut_ptr(),
            lpszTitle: title_w.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            lParam: 0,
            iImage: 0,
        };

        let pidl = SHBrowseForFolderW(&mut bi);
        let mut selected = None;
        if !pidl.is_null() {
            let mut path_buf = vec![0u16; MAX_PATH as usize];
            if SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr()) != 0 {
                let len = path_buf
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(path_buf.len());
                let os_str = OsString::from_wide(&path_buf[..len]);
                selected = Some(PathBuf::from(os_str));
            }
            CoTaskMemFree(pidl as *const core::ffi::c_void);
        }

        if co_hr >= 0 {
            CoUninitialize();
        }
        selected
    }
}

#[cfg(not(target_os = "windows"))]
pub fn pick_folder(_title: &str) -> Option<PathBuf> {
    None
}

pub fn run_first_run_wizard(config: &mut Config, config_path: &Path) {
    let discovered = discover_sc_base_dirs();

    if !discovered.is_empty() {
        let list = discovered.join("\n");
        let msg = format!(
            "Willkommen bei Star Citizen Presence!\n\nFolgende Star Citizen-Ordner wurden automatisch gefunden:\n{list}\n\nMöchtest du diese Ordner verwenden?\n\nJa = Gefundene Ordner übernehmen\nNein = Anderen Ordner manuell auswählen\nAbbrechen = Einrichtung überspringen"
        );
        match ask_yes_no_cancel("Star Citizen Presence - Einrichtung (1/4)", &msg) {
            Some(true) => {
                config.sc_base_dirs = discovered;
            }
            Some(false) => {
                if let Some(picked) = pick_folder(
                    "Star Citizen-Ordner wählen (z. B. Roberts Space Industries\\StarCitizen)",
                ) {
                    upsert_sc_base_dir(&mut config.sc_base_dirs, &picked);
                }
            }
            None => {
                config.setup_completed = true;
                let _ = config.save(config_path);
                return;
            }
        }
    } else {
        let wants_pick = ask_yes_no(
            "Star Citizen Presence - Einrichtung (1/4)",
            "Willkommen bei Star Citizen Presence!\n\nEs wurde kein Star Citizen-Ordner automatisch gefunden.\nMöchtest du deinen Star Citizen-Ordner jetzt manuell auswählen?",
        );
        if wants_pick {
            if let Some(picked) = pick_folder(
                "Star Citizen-Ordner wählen (z. B. Roberts Space Industries\\StarCitizen)",
            ) {
                let normalized = normalize_sc_base_dir(&picked);
                if !has_any_sc_env(&normalized) {
                    updater::show_message(
                        "Hinweis zum Ordner",
                        "In dem gewählten Ordner wurde noch kein LIVE-, PTU-, EPTU- oder TECH-PREVIEW-Unterordner gefunden. Der Pfad wird trotzdem gespeichert.",
                    );
                }
                upsert_sc_base_dir(&mut config.sc_base_dirs, &normalized);
            }
        }
    }

    match ask_yes_no_cancel(
        "Star Citizen Presence - Environment (2/4)",
        "Soll automatisch das neueste Environment (LIVE, PTU, EPTU, TECH-PREVIEW) erkannt werden?\n\nJa = Automatisch erkennen\nNein = Fest auf LIVE setzen\nAbbrechen = Aktuelle Einstellung behalten",
    ) {
        Some(true) => config.preferred_env.clear(),
        Some(false) => config.preferred_env = "LIVE".to_string(),
        None => {}
    }

    let enable_autostart = ask_yes_no(
        "Star Citizen Presence - Autostart (3/4)",
        "Soll Star Citizen Presence automatisch mit Windows starten?",
    );
    if let Err(err) = autostart::set_enabled(enable_autostart) {
        updater::show_message("Autostart-Fehler", &err);
    }

    config.require_running_process = ask_yes_no(
        "Star Citizen Presence - Prozess-Erkennung (4/4)",
        "Soll Discord Rich Presence automatisch geleert werden, wenn StarCitizen.exe nicht läuft?",
    );

    config.setup_completed = true;
    match config.save(config_path) {
        Ok(()) => updater::show_message(
            "Einrichtung abgeschlossen",
            &format!(
                "Einstellungen gespeichert in:\n{}\n\nAlle Optionen kannst du jederzeit über das Tray-Symbol ändern.",
                config_path.display()
            ),
        ),
        Err(err) => updater::show_message("Konfiguration konnte nicht gespeichert werden", &err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_env_subfolder_to_parent() {
        let picked = Path::new(r"E:\Games\StarCitizen\LIVE");
        assert_eq!(normalize_sc_base_dir(picked), Path::new(r"E:\Games\StarCitizen"));

        let picked_base = Path::new(r"E:\Games\StarCitizen");
        assert_eq!(
            normalize_sc_base_dir(picked_base),
            Path::new(r"E:\Games\StarCitizen")
        );
    }

    #[test]
    fn upserts_base_dir_to_front_without_duplicates() {
        let mut dirs = vec![
            r"D:\Roberts Space Industries\StarCitizen".to_string(),
            r"E:\Games\StarCitizen".to_string(),
        ];
        upsert_sc_base_dir(&mut dirs, Path::new(r"E:\Games\StarCitizen\PTU"));
        assert_eq!(
            dirs,
            vec![
                r"E:\Games\StarCitizen".to_string(),
                r"D:\Roberts Space Industries\StarCitizen".to_string(),
            ]
        );
    }
}
