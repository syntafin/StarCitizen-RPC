#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod config;
mod discord;
mod log_watcher;
mod parser;
mod state;
mod updater;
mod wizard;

use crate::config::{resolve_config_path, resolve_writable_config_path, Config};
use crate::discord::DiscordClient;
use crate::log_watcher::{is_star_citizen_running, LogWatcher};
use crate::parser::apply_log_line;
use crate::state::PresenceState;
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
                    let rendered = state.render();
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
    pick_dir_item: MenuItem,
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
    config_path: PathBuf,
}

impl TrayApp {
    fn new(initial_config: &Config, config_path: PathBuf) -> Self {
        let status_item = MenuItem::new(
            format!("Star Citizen Presence v{} • Aktiv", updater::CURRENT_VERSION),
            false,
            None,
        );
        let toggle_item = MenuItem::new("Stop", true, None);
        let reload_item = MenuItem::new("Neu laden", true, None);
        let pick_dir_item = MenuItem::new("Star Citizen-Ordner wählen...", true, None);

        let cur_env = initial_config.preferred_env.trim();
        let env_auto_item = CheckMenuItem::new("Auto (Neueste Log)", true, cur_env.is_empty(), None);
        let env_live_item = CheckMenuItem::new("LIVE", true, cur_env.eq_ignore_ascii_case("LIVE"), None);
        let env_ptu_item = CheckMenuItem::new("PTU", true, cur_env.eq_ignore_ascii_case("PTU"), None);
        let env_eptu_item = CheckMenuItem::new("EPTU", true, cur_env.eq_ignore_ascii_case("EPTU"), None);
        let env_tech_item = CheckMenuItem::new(
            "TECH PREVIEW",
            true,
            cur_env.eq_ignore_ascii_case("TECH-PREVIEW"),
            None,
        );

        let env_submenu = Submenu::new("Environment", true);
        let _ = env_submenu.append_items(&[
            &env_auto_item,
            &env_live_item,
            &env_ptu_item,
            &env_eptu_item,
            &env_tech_item,
        ]);

        let require_process_item = CheckMenuItem::new(
            "Nur bei laufendem Spiel",
            true,
            initial_config.require_running_process,
            None,
        );
        let open_config_item = MenuItem::new("config.toml öffnen", true, None);
        let wizard_item = MenuItem::new("Setup-Assistent starten", true, None);
        let autostart_item = CheckMenuItem::new(
            "Mit Windows starten",
            true,
            autostart::is_enabled(),
            None,
        );
        let update_item = MenuItem::new("Nach Updates suchen", true, None);
        let quit_item = MenuItem::new("Beenden", true, None);

        let menu = Menu::new();
        let _ = menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &toggle_item,
            &reload_item,
            &PredefinedMenuItem::separator(),
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
            pick_dir_item,
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
            config_path,
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
            updater::show_message("Konfigurations-Fehler", &err);
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
                updater::show_message("Konfigurations-Fehler", &err);
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

impl ApplicationHandler<MenuEvent> for TrayApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MenuEvent) {
        if event.id == self.toggle_item.id() {
            if let Some(worker) = &self.worker {
                let prev = worker.active.load(Ordering::SeqCst);
                let next = !prev;
                worker.active.store(next, Ordering::SeqCst);
                if next {
                    self.toggle_item.set_text("Stop");
                    self.status_item.set_text(format!(
                        "Star Citizen Presence v{} • Aktiv",
                        updater::CURRENT_VERSION
                    ));
                } else {
                    self.toggle_item.set_text("Start");
                    self.status_item.set_text(format!(
                        "Star Citizen Presence v{} • Pausiert",
                        updater::CURRENT_VERSION
                    ));
                }
            }
        } else if event.id == self.reload_item.id() {
            if let Some(worker) = &self.worker {
                worker.reload.store(true, Ordering::SeqCst);
            }
        } else if event.id == self.pick_dir_item.id() {
            if let Some(picked) = wizard::pick_folder(
                "Star Citizen-Ordner wählen (z. B. Roberts Space Industries\\StarCitizen)",
            ) {
                let normalized = wizard::normalize_sc_base_dir(&picked);
                self.update_config_and_reload(|cfg| {
                    wizard::upsert_sc_base_dir(&mut cfg.sc_base_dirs, &normalized);
                });
                updater::show_message(
                    "Star Citizen-Ordner aktualisiert",
                    &format!("Neuer Suchpfad:\n{}", normalized.display()),
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
                updater::show_message("Autostart-Fehler", &err);
            }
        } else if event.id == self.update_item.id() {
            if self.updating.swap(true, Ordering::SeqCst) {
                return;
            }
            let updating = Arc::clone(&self.updating);
            let shutdown_flag = self.worker.as_ref().map(|w| Arc::clone(&w.shutdown));
            thread::spawn(move || {
                match updater::check_and_update() {
                    Ok(UpdateResult::UpToDate(v)) => {
                        updater::show_message(
                            "Star Citizen Presence",
                            &format!("Du nutzt bereits die neueste Version (v{v})."),
                        );
                    }
                    Ok(UpdateResult::Updated { from, to }) => {
                        if let Some(flag) = shutdown_flag {
                            flag.store(true, Ordering::SeqCst);
                        }
                        thread::sleep(Duration::from_millis(400));
                        updater::show_message(
                            "Update installiert",
                            &format!("Aktualisiert von v{from} auf {to}. Die Anwendung startet neu."),
                        );
                        updater::restart_self();
                    }
                    Err(err) => {
                        updater::show_message("Update-Prüfung fehlgeschlagen", &err);
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
    let rendered = state.render();
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

    let event_loop = EventLoop::<MenuEvent>::with_user_event()
        .build()
        .expect("EventLoop konnte nicht erstellt werden");
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(event);
    }));

    let mut app = TrayApp::new(&initial_config, write_path);
    let _ = event_loop.run_app(&mut app);
}
