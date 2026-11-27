#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod config;
mod discord;
mod i18n;
mod log_watcher;
mod parser;
mod settings_ui;
mod state;
mod telemetry;
mod telemetry_setup;
mod updater;
mod wizard;

use crate::config::{resolve_config_path, resolve_writable_config_path, Config};
use crate::discord::DiscordClient;
use crate::i18n::Language;
use crate::log_watcher::{is_star_citizen_running, LogWatcher};
use crate::parser::apply_log_line;
use crate::state::PresenceState;
use crate::telemetry::TelemetryClient;
use crate::updater::UpdateResult;
use image::imageops::FilterType;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const ICON_BYTES: &[u8] = include_bytes!("../images/STARCITIZEN_WHITE.png");
const AUTO_UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

fn load_tray_icon() -> Icon {
    let img = image::load_from_memory(ICON_BYTES)
        .expect("Eingebettetes Tray-Icon konnte nicht geladen werden")
        .resize_exact(32, 32, FilterType::Lanczos3)
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("Tray-Icon RGBA ungültig")
}

struct WorkerState {
    active: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

fn spawn_worker() -> WorkerState {
    let active = Arc::new(AtomicBool::new(true));
    let reload = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));

    let w_active = Arc::clone(&active);
    let w_reload = Arc::clone(&reload);
    let w_shutdown = Arc::clone(&shutdown);

    let handle = thread::spawn(move || {
        let mut config = Config::load(&resolve_config_path());
        let mut watcher = LogWatcher::new();
        let mut state = PresenceState::default();
        let mut discord = DiscordClient::new(config.discord_app_id.clone());
        let telemetry = TelemetryClient::start(config.use_telemetry);

        let mut last_process_check = Instant::now() - Duration::from_secs(10);
        let mut game_running = true;
        let mut was_active = true;

        // Hintergrund-Updatecheck (5s nach Start, dann alle 6h)
        let mut last_update_check = Instant::now() - AUTO_UPDATE_INTERVAL + Duration::from_secs(5);

        while !w_shutdown.load(Ordering::SeqCst) {
            if last_update_check.elapsed() >= AUTO_UPDATE_INTERVAL {
                last_update_check = Instant::now();
                if let Ok(UpdateResult::Updated { .. }) = updater::check_and_update() {
                    discord.close();
                    updater::restart_self();
                }
            }

            if w_reload.swap(false, Ordering::SeqCst) {
                config = Config::load(&resolve_config_path());
                telemetry.set_enabled(config.use_telemetry);
                watcher = LogWatcher::new();
                state.reset();
                discord.close();
                discord = DiscordClient::new(config.discord_app_id.clone());
            }

            let is_active = w_active.load(Ordering::SeqCst);
            if !is_active {
                if was_active {
                    discord.clear();
                    was_active = false;
                }
                thread::sleep(Duration::from_millis(300));
                continue;
            }
            was_active = true;

            if config.require_running_process
                && last_process_check.elapsed() >= Duration::from_secs(5)
            {
                let now_running = is_star_citizen_running();
                if now_running != game_running {
                    game_running = now_running;
                    if !game_running {
                        discord.clear();
                    }
                }
                last_process_check = Instant::now();
            }

            if !config.require_running_process || game_running {
                let (lines, reset) = watcher.poll(&config);
                if reset {
                    state.reset();
                }
                for line in &lines {
                    apply_log_line(&mut state, line);
                }
                if watcher.current_path().is_some() {
                    let lang = Language::from_config_str(&config.language);
                    let snap = if config.use_telemetry {
                        telemetry.latest()
                    } else {
                        None
                    };
                    let rendered = state.render_with_telemetry(lang, &config, snap.as_ref());
                    discord.update(&rendered);
                }
            }

            let sleep_ms = config.poll_interval_ms.max(250);
            thread::sleep(Duration::from_millis(sleep_ms));
        }

        discord.close();
    });

    WorkerState {
        active,
        reload,
        shutdown,
        handle: Some(handle),
    }
}

struct TrayApp {
    _tray: Option<TrayIcon>,
    status_item: MenuItem,
    toggle_item: MenuItem,
    reload_item: MenuItem,
    settings_item: MenuItem,
    pick_dir_item: MenuItem,
    env_submenu: Submenu,
    env_auto_item: CheckMenuItem,
    env_live_item: CheckMenuItem,
    env_ptu_item: CheckMenuItem,
    env_eptu_item: CheckMenuItem,
    env_tech_item: CheckMenuItem,
    require_process_item: CheckMenuItem,
    open_config_item: MenuItem,
    wizard_item: MenuItem,
    autostart_item: CheckMenuItem,
    update_item: MenuItem,
    quit_item: MenuItem,
    worker: Option<WorkerState>,
    updating: Arc<AtomicBool>,
    settings_open: Arc<AtomicBool>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    config_path: PathBuf,
    lang: Language,
}

#[derive(Debug, Clone)]
enum AppEvent {
    Menu(MenuEvent),
    SettingsSaved,
}

impl TrayApp {
    fn new(
        initial_config: &Config,
        config_path: PathBuf,
        proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    ) -> Self {
        let lang = Language::from_config_str(&initial_config.language);
        let tr = lang.strings();

        let status_item = MenuItem::new(
            format!(
                "Star Citizen Presence v{} • {}",
                updater::CURRENT_VERSION,
                tr.tray_active
            ),
            false,
            None,
        );
        let toggle_item = MenuItem::new(tr.tray_stop, true, None);
        let reload_item = MenuItem::new(tr.tray_reload, true, None);
        let settings_item = MenuItem::new(tr.tray_settings, true, None);
        let pick_dir_item = MenuItem::new(tr.tray_pick_dir, true, None);

        let cur_env = initial_config.preferred_env.trim();
        let env_auto_item = CheckMenuItem::new(tr.tray_env_auto, true, cur_env.is_empty(), None);
        let env_live_item = CheckMenuItem::new("LIVE", true, cur_env.eq_ignore_ascii_case("LIVE"), None);
        let env_ptu_item = CheckMenuItem::new("PTU", true, cur_env.eq_ignore_ascii_case("PTU"), None);
        let env_eptu_item = CheckMenuItem::new("EPTU", true, cur_env.eq_ignore_ascii_case("EPTU"), None);
        let env_tech_item = CheckMenuItem::new(
            "TECH PREVIEW",
            true,
            cur_env.eq_ignore_ascii_case("TECH-PREVIEW"),
            None,
        );

        let env_submenu = Submenu::new(tr.tray_env_menu, true);
        let _ = env_submenu.append_items(&[
            &env_auto_item,
            &env_live_item,
            &env_ptu_item,
            &env_eptu_item,
            &env_tech_item,
        ]);

        let require_process_item = CheckMenuItem::new(
            tr.tray_require_process,
            true,
            initial_config.require_running_process,
            None,
        );
        let open_config_item = MenuItem::new(tr.tray_open_config, true, None);
        let wizard_item = MenuItem::new(tr.tray_wizard, true, None);
        let autostart_item = CheckMenuItem::new(
            tr.tray_autostart,
            true,
            autostart::is_enabled(),
            None,
        );
        let update_item = MenuItem::new(tr.tray_check_updates, true, None);
        let quit_item = MenuItem::new(tr.tray_quit, true, None);

        let menu = Menu::new();
        let _ = menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &toggle_item,
            &reload_item,
            &PredefinedMenuItem::separator(),
            &settings_item,
            &pick_dir_item,
            &env_submenu,
            &require_process_item,
            &autostart_item,
            &open_config_item,
            &wizard_item,
            &PredefinedMenuItem::separator(),
            &update_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ]);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Star Citizen Presence")
            .with_icon(load_tray_icon())
            .build()
            .expect("Tray-Icon konnte nicht erstellt werden");

        Self {
            _tray: Some(tray),
            status_item,
            toggle_item,
            reload_item,
            settings_item,
            pick_dir_item,
            env_submenu,
            env_auto_item,
            env_live_item,
            env_ptu_item,
            env_eptu_item,
            env_tech_item,
            require_process_item,
            open_config_item,
            wizard_item,
            autostart_item,
            update_item,
            quit_item,
            worker: Some(spawn_worker()),
            updating: Arc::new(AtomicBool::new(false)),
            settings_open: Arc::new(AtomicBool::new(false)),
            proxy,
            config_path,
            lang,
        }
    }

    fn sync_env_checks(&self, preferred_env: &str) {
        let env = preferred_env.trim();
        self.env_auto_item.set_checked(env.is_empty());
        self.env_live_item.set_checked(env.eq_ignore_ascii_case("LIVE"));
        self.env_ptu_item.set_checked(env.eq_ignore_ascii_case("PTU"));
        self.env_eptu_item.set_checked(env.eq_ignore_ascii_case("EPTU"));
        self.env_tech_item
            .set_checked(env.eq_ignore_ascii_case("TECH-PREVIEW"));
    }

    fn refresh_tray_from_config(&mut self) {
        let cfg = Config::load(&resolve_config_path());
        self.lang = Language::from_config_str(&cfg.language);
        let tr = self.lang.strings();

        let is_active = self
            .worker
            .as_ref()
            .map(|w| w.active.load(Ordering::SeqCst))
            .unwrap_or(true);
        self.status_item.set_text(format!(
            "Star Citizen Presence v{} • {}",
            updater::CURRENT_VERSION,
            if is_active { tr.tray_active } else { tr.tray_paused }
        ));
        self.toggle_item
            .set_text(if is_active { tr.tray_stop } else { tr.tray_start });
        self.reload_item.set_text(tr.tray_reload);
        self.settings_item.set_text(tr.tray_settings);
        self.pick_dir_item.set_text(tr.tray_pick_dir);
        self.env_submenu.set_text(tr.tray_env_menu);
        self.env_auto_item.set_text(tr.tray_env_auto);
        self.require_process_item.set_text(tr.tray_require_process);
        self.open_config_item.set_text(tr.tray_open_config);
        self.wizard_item.set_text(tr.tray_wizard);
        self.autostart_item.set_text(tr.tray_autostart);
        self.update_item.set_text(tr.tray_check_updates);
        self.quit_item.set_text(tr.tray_quit);

        self.sync_env_checks(&cfg.preferred_env);
        self.require_process_item
            .set_checked(cfg.require_running_process);
        self.autostart_item.set_checked(autostart::is_enabled());
    }

    fn update_config_and_reload(&self, mutator: impl FnOnce(&mut Config)) {
        let load_path = if self.config_path.exists() {
            self.config_path.clone()
        } else {
            resolve_config_path()
        };
        let mut cfg = Config::load(&load_path);
        mutator(&mut cfg);
        cfg.setup_completed = true;
        if let Err(err) = cfg.save(&self.config_path) {
            updater::show_message(self.lang.strings().msg_config_error_title, &err);
            return;
        }
        if let Some(worker) = &self.worker {
            worker.reload.store(true, Ordering::SeqCst);
        }
    }

    fn open_config_in_editor(&self) {
        if !self.config_path.exists() {
            let mut cfg = Config::load(&resolve_config_path());
            cfg.setup_completed = true;
            if let Err(err) = cfg.save(&self.config_path) {
                updater::show_message(self.lang.strings().msg_config_error_title, &err);
                return;
            }
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let _ = Command::new("cmd")
                .args(["/C", "start", "", &self.config_path.to_string_lossy()])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    }
}

impl ApplicationHandler<AppEvent> for TrayApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, app_event: AppEvent) {
        let event = match app_event {
            AppEvent::Menu(ev) => ev,
            AppEvent::SettingsSaved => {
                self.refresh_tray_from_config();
                if let Some(worker) = &self.worker {
                    worker.reload.store(true, Ordering::SeqCst);
                }
                return;
            }
        };
        let tr = self.lang.strings();
        if event.id == self.toggle_item.id() {
            if let Some(worker) = &self.worker {
                let prev = worker.active.load(Ordering::SeqCst);
                let next = !prev;
                worker.active.store(next, Ordering::SeqCst);
                if next {
                    self.toggle_item.set_text(tr.tray_stop);
                    self.status_item.set_text(format!(
                        "Star Citizen Presence v{} • {}",
                        updater::CURRENT_VERSION,
                        tr.tray_active
                    ));
                } else {
                    self.toggle_item.set_text(tr.tray_start);
                    self.status_item.set_text(format!(
                        "Star Citizen Presence v{} • {}",
                        updater::CURRENT_VERSION,
                        tr.tray_paused
                    ));
                }
            }
        } else if event.id == self.reload_item.id() {
            if let Some(worker) = &self.worker {
                worker.reload.store(true, Ordering::SeqCst);
            }
        } else if event.id == self.settings_item.id() {
            if self.settings_open.swap(true, Ordering::SeqCst) {
                return;
            }
            let settings_open = Arc::clone(&self.settings_open);
            let proxy = self.proxy.clone();
            thread::spawn(move || {
                if let Ok(exe) = env::current_exe() {
                    if let Ok(mut child) = Command::new(exe).arg("--settings").spawn() {
                        if let Ok(status) = child.wait() {
                            if status.success() {
                                let _ = proxy.send_event(AppEvent::SettingsSaved);
                            }
                        }
                    }
                }
                settings_open.store(false, Ordering::SeqCst);
            });
        } else if event.id == self.pick_dir_item.id() {
            if let Some(picked) = wizard::pick_folder(tr.wizard_folder_picker_title) {
                let normalized = wizard::normalize_sc_base_dir(&picked);
                self.update_config_and_reload(|cfg| {
                    wizard::upsert_sc_base_dir(&mut cfg.sc_base_dirs, &normalized);
                });
                updater::show_message(
                    tr.msg_dir_updated_title,
                    &format!("{}{}", tr.msg_dir_updated_body_prefix, normalized.display()),
                );
            }
        } else if event.id == self.env_auto_item.id() {
            self.sync_env_checks("");
            self.update_config_and_reload(|cfg| cfg.preferred_env.clear());
        } else if event.id == self.env_live_item.id() {
            self.sync_env_checks("LIVE");
            self.update_config_and_reload(|cfg| cfg.preferred_env = "LIVE".to_string());
        } else if event.id == self.env_ptu_item.id() {
            self.sync_env_checks("PTU");
            self.update_config_and_reload(|cfg| cfg.preferred_env = "PTU".to_string());
        } else if event.id == self.env_eptu_item.id() {
            self.sync_env_checks("EPTU");
            self.update_config_and_reload(|cfg| cfg.preferred_env = "EPTU".to_string());
        } else if event.id == self.env_tech_item.id() {
            self.sync_env_checks("TECH-PREVIEW");
            self.update_config_and_reload(|cfg| cfg.preferred_env = "TECH-PREVIEW".to_string());
        } else if event.id == self.require_process_item.id() {
            let desired = self.require_process_item.is_checked();
            self.update_config_and_reload(|cfg| cfg.require_running_process = desired);
        } else if event.id == self.open_config_item.id() {
            self.open_config_in_editor();
        } else if event.id == self.wizard_item.id() {
            let mut cfg = Config::load(&resolve_config_path());
            wizard::run_first_run_wizard(&mut cfg, &self.config_path);
            self.sync_env_checks(&cfg.preferred_env);
            self.require_process_item
                .set_checked(cfg.require_running_process);
            self.autostart_item.set_checked(autostart::is_enabled());
            if let Some(worker) = &self.worker {
                worker.reload.store(true, Ordering::SeqCst);
            }
        } else if event.id == self.autostart_item.id() {
            let desired = self.autostart_item.is_checked();
            if let Err(err) = autostart::set_enabled(desired) {
                self.autostart_item.set_checked(!desired);
                updater::show_message(tr.msg_autostart_error_title, &err);
            }
        } else if event.id == self.update_item.id() {
            if self.updating.swap(true, Ordering::SeqCst) {
                return;
            }
            let updating = Arc::clone(&self.updating);
            let shutdown_flag = self.worker.as_ref().map(|w| Arc::clone(&w.shutdown));
            let lang = self.lang;
            thread::spawn(move || {
                let tr = lang.strings();
                match updater::check_and_update() {
                    Ok(UpdateResult::UpToDate(v)) => {
                        updater::show_message(
                            "Star Citizen Presence",
                            &tr.update_up_to_date(&v),
                        );
                    }
                    Ok(UpdateResult::Updated { from, to }) => {
                        if let Some(flag) = shutdown_flag {
                            flag.store(true, Ordering::SeqCst);
                        }
                        thread::sleep(Duration::from_millis(400));
                        updater::show_message(
                            tr.msg_update_installed_title,
                            &tr.update_installed(&from, &to, lang),
                        );
                        updater::restart_self();
                    }
                    Err(err) => {
                        updater::show_message(tr.msg_update_failed_title, &err);
                    }
                }
                updating.store(false, Ordering::SeqCst);
            });
        } else if event.id == self.quit_item.id() {
            if let Some(mut worker) = self.worker.take() {
                worker.shutdown.store(true, Ordering::SeqCst);
                if let Some(handle) = worker.handle.take() {
                    let _ = handle.join();
                }
            }
            self._tray = None;
            event_loop.exit();
        }
    }
}

fn run_dry_run() {
    let config = Config::load(&resolve_config_path());
    let mut watcher = LogWatcher::new();
    let mut state = PresenceState::default();
    let (lines, _) = watcher.poll(&config);

    if let Some(p) = watcher.current_path() {
        println!("Log-Datei: {}", p.display());
    } else {
        println!("Keine Game.log gefunden.");
        return;
    }

    for line in &lines {
        apply_log_line(&mut state, line);
    }
    let lang = Language::from_config_str(&config.language);
    let rendered = state.render(lang);
    println!(
        "Dry-Run Ergebnis:\n  Details: {}\n  State:   {}\n  Large:   {} ({})\n  Small:   {:?}",
        rendered.details, rendered.state, rendered.large_image, rendered.large_text, rendered.small_image
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--dry-run") {
        run_dry_run();
        return;
    }
    if args.iter().any(|a| a == "--settings") {
        let saved = settings_ui::run_settings_window();
        std::process::exit(if saved { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--install-autostart") {
        if let Err(err) = autostart::set_enabled(true) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--uninstall-autostart") {
        if let Err(err) = autostart::set_enabled(false) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    let force_setup = args.iter().any(|a| a == "--setup");
    let read_path = resolve_config_path();
    let write_path = resolve_writable_config_path();
    let mut initial_config = Config::load(&read_path);

    if force_setup || !initial_config.setup_completed {
        wizard::run_first_run_wizard(&mut initial_config, &write_path);
    }

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("EventLoop konnte nicht erstellt werden");
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(AppEvent::Menu(event));
    }));

    let mut app = TrayApp::new(&initial_config, write_path, proxy);
    let _ = event_loop.run_app(&mut app);
}
