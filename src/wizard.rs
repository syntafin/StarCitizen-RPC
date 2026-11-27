use crate::autostart;
use crate::config::{discover_sc_base_dirs, has_any_sc_env, Config};
use crate::i18n::Language;
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
    let tr = Language::from_config_str(&config.language).strings();
    let discovered = discover_sc_base_dirs();

    if !discovered.is_empty() {
        let list = discovered.join("\n");
        let msg = format!(
            "{}{list}{}",
            tr.wizard_step1_found_intro, tr.wizard_step1_found_question
        );
        match ask_yes_no_cancel(tr.wizard_step1_title, &msg) {
            Some(true) => {
                config.sc_base_dirs = discovered;
            }
            Some(false) => {
                if let Some(picked) = pick_folder(tr.wizard_folder_picker_title) {
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
        let wants_pick = ask_yes_no(tr.wizard_step1_title, tr.wizard_step1_missing_body);
        if wants_pick {
            if let Some(picked) = pick_folder(tr.wizard_folder_picker_title) {
                let normalized = normalize_sc_base_dir(&picked);
                if !has_any_sc_env(&normalized) {
                    updater::show_message(tr.wizard_dir_hint_title, tr.wizard_dir_hint_body);
                }
                upsert_sc_base_dir(&mut config.sc_base_dirs, &normalized);
            }
        }
    }

    match ask_yes_no_cancel(tr.wizard_step2_title, tr.wizard_step2_body) {
        Some(true) => config.preferred_env.clear(),
        Some(false) => config.preferred_env = "LIVE".to_string(),
        None => {}
    }

    let enable_autostart = ask_yes_no(tr.wizard_step3_title, tr.wizard_step3_body);
    if let Err(err) = autostart::set_enabled(enable_autostart) {
        updater::show_message(tr.msg_autostart_error_title, &err);
    }

    config.require_running_process = ask_yes_no(tr.wizard_step4_title, tr.wizard_step4_body);

    config.setup_completed = true;
    match config.save(config_path) {
        Ok(()) => updater::show_message(
            tr.wizard_done_title,
            &format!(
                "{}{}{}",
                tr.wizard_done_saved_prefix,
                config_path.display(),
                tr.wizard_done_hint
            ),
        ),
        Err(err) => updater::show_message(tr.wizard_save_error_title, &err),
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
