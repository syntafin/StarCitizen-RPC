use crate::autostart;
use crate::config::{resolve_config_path, resolve_writable_config_path, Config};
use crate::i18n::{Language, Strings};
use crate::telemetry_setup::{self, TelemetrySetupStatus};
use crate::wizard;
use eframe::egui;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Telemetry,
    Privacy,
}

struct SettingsApp {
    config: Config,
    autostart_enabled: bool,
    write_path: PathBuf,
    tab: SettingsTab,
    saved: bool,
    error_msg: Option<String>,
    telemetry_status: TelemetrySetupStatus,
    telemetry_setup_applied: bool,
}

impl SettingsApp {
    fn new(config: Config, write_path: PathBuf) -> Self {
        let telemetry_status = telemetry_setup::inspect_telemetry_setup(&config.sc_base_dirs);
        Self {
            config,
            autostart_enabled: autostart::is_enabled(),
            write_path,
            tab: SettingsTab::General,
            saved: false,
            error_msg: None,
            telemetry_status,
            telemetry_setup_applied: false,
        }
    }

    fn active_language(&self) -> Language {
        Language::from_config_str(&self.config.language)
    }

    fn build_preview(&self, lang: Language) -> (String, String) {
        let tr = lang.strings();
        let env_label = if self.config.preferred_env.trim().is_empty() {
            "LIVE"
        } else {
            self.config.preferred_env.trim()
        };
        let details = format!("PU ({env_label}): Stanton • EU");

        if !self.config.use_telemetry {
            return (details, format!("Anvil Carrack • Orison"));
        }

        let mut parts: Vec<String> = Vec::new();
        if self.config.show_downed_status {
            let downed_label = match lang {
                Language::De => "⚠ Kampfunfähig (Downed)",
                Language::En => "⚠ Incapacitated (Downed)",
                Language::Fr => "⚠ À terre (Inconscient)",
            };
            parts.push(downed_label.to_string());
        } else if self.config.show_eva_and_on_foot {
            let role = if self.config.show_crew_role {
                match lang {
                    Language::De => "Pilot: Anvil Carrack",
                    Language::En => "Pilot: Anvil Carrack",
                    Language::Fr => "Pilote : Anvil Carrack",
                }
            } else {
                "Anvil Carrack"
            };
            parts.push(role.to_string());
        } else {
            parts.push("Anvil Carrack".to_string());
        }

        if self.config.show_flight_details && !self.config.show_downed_status {
            let flight = match lang {
                Language::De => "NAV • Quantum bereit",
                Language::En => "NAV • Quantum Ready",
                Language::Fr => "NAV • Quantum prêt",
            };
            parts.push(flight.to_string());
        }

        if self.config.show_combat_status {
            let combat = match lang {
                Language::De => "Im Gefecht",
                Language::En => "In Combat",
                Language::Fr => "Au combat",
            };
            parts.push(combat.to_string());
        }

        if self.config.show_ship_health {
            let health = match lang {
                Language::De => "Schilde 42%",
                Language::En => "Shields 42%",
                Language::Fr => "Boucliers 42%",
            };
            parts.push(health.to_string());
        }

        if self.config.show_balance_auec {
            parts.push("4.2M aUEC".to_string());
        }

        if parts.is_empty() {
            parts.push(tr.pu_in_verse.to_string());
        }

        (details, parts.join(" • "))
    }
}

fn option_toggle(ui: &mut egui::Ui, value: &mut bool, label: &str, description: &str) {
    ui.checkbox(value, egui::RichText::new(label).strong());
    ui.indent(label, |ui| {
        ui.label(
            egui::RichText::new(description)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
    ui.add_space(6.0);
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let lang = self.active_language();
        let tr = lang.strings();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            tr.settings_window_title.to_string(),
        ));

        egui::TopBottomPanel::bottom("settings_footer").show(ctx, |ui| {
            ui.add_space(6.0);
            if let Some(err) = &self.error_msg {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                if ui.button(tr.settings_reset).clicked() {
                    let setup_done = self.config.setup_completed;
                    let dirs = self.config.sc_base_dirs.clone();
                    self.config = Config::default();
                    self.config.setup_completed = setup_done;
                    self.config.sc_base_dirs = dirs;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(tr.settings_save).strong(),
                            )
                            .fill(egui::Color32::from_rgb(45, 115, 210)),
                        )
                        .clicked()
                    {
                        self.config.setup_completed = true;
                        match self.config.save(&self.write_path) {
                            Ok(()) => {
                                if let Err(err) = autostart::set_enabled(self.autostart_enabled) {
                                    self.error_msg = Some(err);
                                } else {
                                    self.saved = true;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            }
                            Err(err) => {
                                self.error_msg = Some(err);
                            }
                        }
                    }

                    if ui.button(tr.settings_cancel).clicked() {
                        self.saved = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Live-Vorschau oben
            let (preview_details, preview_state) = self.build_preview(lang);
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(30, 31, 34))
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(tr.settings_preview_title)
                            .small()
                            .strong()
                            .color(egui::Color32::from_rgb(148, 155, 164)),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(tr.settings_preview_playing)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new(preview_details)
                            .color(egui::Color32::from_rgb(219, 222, 225)),
                    );
                    ui.label(
                        egui::RichText::new(preview_state)
                            .small()
                            .color(egui::Color32::from_rgb(181, 186, 193)),
                    );
                });

            ui.add_space(10.0);

            // Tabs
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, SettingsTab::General, tr.settings_tab_general);
                ui.selectable_value(
                    &mut self.tab,
                    SettingsTab::Telemetry,
                    tr.settings_tab_telemetry,
                );
                ui.selectable_value(&mut self.tab, SettingsTab::Privacy, tr.settings_tab_privacy);
            });
            ui.separator();
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                SettingsTab::General => self.render_general_tab(ui, tr),
                SettingsTab::Telemetry => self.render_telemetry_tab(ui, tr),
                SettingsTab::Privacy => self.render_privacy_tab(ui, tr),
            });
        });
    }
}

impl SettingsApp {
    fn render_general_tab(&mut self, ui: &mut egui::Ui, tr: &Strings) {
        // Sprache
        ui.label(egui::RichText::new(tr.settings_lang_label).strong());
        let selected_lang_text = match self.config.language.trim().to_ascii_lowercase().as_str() {
            "de" => "Deutsch",
            "en" => "English",
            "fr" => "Français",
            _ => tr.settings_lang_auto,
        };
        egui::ComboBox::from_id_salt("language_combo")
            .selected_text(selected_lang_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.config.language,
                    "auto".to_string(),
                    tr.settings_lang_auto,
                );
                ui.selectable_value(&mut self.config.language, "de".to_string(), "Deutsch");
                ui.selectable_value(&mut self.config.language, "en".to_string(), "English");
                ui.selectable_value(&mut self.config.language, "fr".to_string(), "Français");
            });
        ui.label(
            egui::RichText::new(tr.settings_lang_desc)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(10.0);

        // Environment
        ui.label(egui::RichText::new(tr.settings_env_label).strong());
        let env_display = if self.config.preferred_env.trim().is_empty() {
            tr.tray_env_auto
        } else {
            self.config.preferred_env.as_str()
        };
        egui::ComboBox::from_id_salt("env_combo")
            .selected_text(env_display)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.config.preferred_env,
                    String::new(),
                    tr.tray_env_auto,
                );
                for env_name in ["LIVE", "PTU", "EPTU", "TECH-PREVIEW"] {
                    ui.selectable_value(
                        &mut self.config.preferred_env,
                        env_name.to_string(),
                        env_name,
                    );
                }
            });
        ui.label(
            egui::RichText::new(tr.settings_env_desc)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(10.0);

        // Star Citizen Ordner
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr.settings_dirs_label).strong());
            if ui.button(tr.settings_dirs_add).clicked() {
                if let Some(picked) = wizard::pick_folder(tr.wizard_folder_picker_title) {
                    wizard::upsert_sc_base_dir(&mut self.config.sc_base_dirs, &picked);
                    self.telemetry_status =
                        telemetry_setup::inspect_telemetry_setup(&self.config.sc_base_dirs);
                }
            }
        });
        ui.label(
            egui::RichText::new(tr.settings_dirs_desc)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        if self.config.sc_base_dirs.is_empty() {
            ui.label(
                egui::RichText::new(tr.settings_dirs_empty)
                    .italics()
                    .color(ui.visuals().weak_text_color()),
            );
        } else {
            let mut remove_idx: Option<usize> = None;
            for (idx, dir) in self.config.sc_base_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                    ui.monospace(dir);
                });
            }
            if let Some(idx) = remove_idx {
                self.config.sc_base_dirs.remove(idx);
                self.telemetry_status =
                    telemetry_setup::inspect_telemetry_setup(&self.config.sc_base_dirs);
            }
        }
        ui.add_space(10.0);

        option_toggle(
            ui,
            &mut self.config.require_running_process,
            tr.tray_require_process,
            tr.settings_require_process_desc,
        );
        option_toggle(
            ui,
            &mut self.autostart_enabled,
            tr.tray_autostart,
            tr.settings_autostart_desc,
        );
    }

    fn render_telemetry_tab(&mut self, ui: &mut egui::Ui, tr: &Strings) {
        option_toggle(
            ui,
            &mut self.config.use_telemetry,
            tr.settings_use_telemetry_label,
            tr.settings_use_telemetry_desc,
        );

        ui.add_enabled_ui(self.config.use_telemetry, |ui| {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(tr.tel_setup_title).strong());
                    ui.add_space(4.0);

                    if self.telemetry_status.env_var_set {
                        ui.colored_label(
                            egui::Color32::from_rgb(87, 242, 135),
                            tr.tel_setup_env_ok,
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(250, 166, 26),
                            tr.tel_setup_env_missing,
                        );
                    }

                    if self.telemetry_status.channels.is_empty() {
                        ui.label(
                            egui::RichText::new(tr.tel_setup_no_channels)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else {
                        for ch in &self.telemetry_status.channels {
                            if ch.configured {
                                ui.colored_label(
                                    egui::Color32::from_rgb(87, 242, 135),
                                    format!("✓ {}: {}", ch.label, tr.tel_setup_cfg_ok),
                                );
                            } else {
                                ui.colored_label(
                                    egui::Color32::from_rgb(250, 166, 26),
                                    format!("✗ {}: {}", ch.label, tr.tel_setup_cfg_missing),
                                );
                            }
                        }
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if !self.telemetry_status.is_fully_ready()
                            && ui.button(tr.tel_setup_apply_btn).clicked()
                        {
                            match telemetry_setup::apply_telemetry_setup(&self.config.sc_base_dirs)
                            {
                                Ok(new_status) => {
                                    self.telemetry_status = new_status;
                                    self.telemetry_setup_applied = true;
                                    self.error_msg = None;
                                }
                                Err(err) => {
                                    self.error_msg = Some(err);
                                }
                            }
                        }
                        if ui.button(tr.tel_setup_refresh_btn).clicked() {
                            self.telemetry_status =
                                telemetry_setup::inspect_telemetry_setup(&self.config.sc_base_dirs);
                        }
                    });

                    if self.telemetry_setup_applied {
                        ui.add_space(4.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(87, 242, 135),
                            egui::RichText::new(tr.tel_setup_applied_hint).small(),
                        );
                    }
                });
            ui.add_space(8.0);

            option_toggle(
                ui,
                &mut self.config.show_eva_and_on_foot,
                tr.settings_eva_label,
                tr.settings_eva_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_crew_role,
                tr.settings_crew_label,
                tr.settings_crew_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_flight_details,
                tr.settings_flight_label,
                tr.settings_flight_desc,
            );
        });
    }

    fn render_privacy_tab(&mut self, ui: &mut egui::Ui, tr: &Strings) {
        ui.add_enabled_ui(self.config.use_telemetry, |ui| {
            option_toggle(
                ui,
                &mut self.config.show_downed_status,
                tr.settings_downed_label,
                tr.settings_downed_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_combat_status,
                tr.settings_combat_label,
                tr.settings_combat_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_ship_health,
                tr.settings_ship_health_label,
                tr.settings_ship_health_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_environment,
                tr.settings_environment_label,
                tr.settings_environment_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_vitals_warning,
                tr.settings_vitals_label,
                tr.settings_vitals_desc,
            );
            option_toggle(
                ui,
                &mut self.config.show_balance_auec,
                tr.settings_balance_label,
                tr.settings_balance_desc,
            );
        });
    }
}

/// Öffnet das Einstellungsfenster. Gibt `true` zurück, wenn gespeichert wurde.
pub fn run_settings_window() -> bool {
    let read_path = resolve_config_path();
    let write_path = resolve_writable_config_path();
    let config = Config::load(&read_path);
    let initial_title = Language::from_config_str(&config.language)
        .strings()
        .settings_window_title
        .to_string();

    let saved_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saved_flag_clone = std::sync::Arc::clone(&saved_flag);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 580.0])
            .with_min_inner_size([460.0, 460.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        &initial_title,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let app = SettingsApp::new(config, write_path);
            Ok(Box::new(SavedTrackingApp {
                inner: app,
                saved_flag: saved_flag_clone,
            }))
        }),
    );

    saved_flag.load(std::sync::atomic::Ordering::SeqCst)
}

struct SavedTrackingApp {
    inner: SettingsApp,
    saved_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl eframe::App for SavedTrackingApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        if self.inner.saved {
            self.saved_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
