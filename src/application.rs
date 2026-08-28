/* application.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Главный класс приложения (VrxxApplication)
//!
//! Отвечает за:
//! - Жизненный цикл GTK4 / Libadwaita приложения (`startup`, `activate`, `open`)
//! - Регистрацию глобальных действий GActions (`quit`, `about`, `set-color-scheme`, `export_config` и др.)
//! - Настройку сочетаний горячих клавиш (Ctrl+Q, Ctrl+D, Ctrl+F, Ctrl++/Ctrl+-)
//! - Загрузку CSS стилей из GResource (`/ru/mark/vrxx/style.css`)
//! - Обработку перехвата ссылок протоколов (deep linking: `vless://`, `vmess://` и т.д.) через `ApplicationImpl::open`

use crate::config::VERSION;
use crate::window::VrxxWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};

use crate::backend::VpnCore;
use serde::{Deserialize, Serialize};

/// Структура контейнера для гранулярного экспорта и импорта конфигурации
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    /// Версия формата экспорта
    pub version: u32,
    /// Открытые настройки приложения (если выбраны для экспорта)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<crate::settings::AppSettings>,
    /// Список профилей VPN (если выбраны для экспорта)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<crate::settings::VpnKeyData>>,
}

mod imp {
    use super::*;

    /// Внутренняя реализация GObject для VrxxApplication
    #[derive(Debug, Default)]
    pub struct VrxxApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxApplication {
        const NAME: &'static str = "VrxxApplication";
        type Type = super::VrxxApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for VrxxApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            // Регистрация горячих клавиш приложения
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("win.disconnect", &["<control>d"]);
            obj.set_accels_for_action("win.zoom_in", &["<Primary>plus", "<Primary>equal"]);
            obj.set_accels_for_action("win.zoom_out", &["<Primary>minus"]);
            obj.set_accels_for_action("win.zoom_normal", &["<Primary>0"]);
        }
    }

    impl ApplicationImpl for VrxxApplication {
        /// Первичная инициализация: загрузка CSS, темы, иконок
        fn startup(&self) {
            self.parent_startup();

            self.obj().setup_css();

            let manager = adw::StyleManager::default();

            // Загрузка цветовой схемы из сохраненных настроек
            let settings = crate::settings::SettingsManager::new();
            let app_settings = settings.load();
            match app_settings.theme.as_str() {
                "force-light" => manager.set_color_scheme(adw::ColorScheme::ForceLight),
                "force-dark" => manager.set_color_scheme(adw::ColorScheme::ForceDark),
                _ => manager.set_color_scheme(adw::ColorScheme::Default),
            }

            // Синхронизация состояния действия выбора темы для отображения в меню
            use gio::prelude::ActionMapExt;
            if let Some(action) = self.obj().lookup_action("set-color-scheme") {
                if let Some(simple_action) = action.downcast_ref::<gio::SimpleAction>() {
                    simple_action.set_state(&glib::Variant::from(app_settings.theme.as_str()));
                }
            }

            self.obj().setup_icons();
            gtk::Window::set_default_icon_name("ru.mark.vrxx");
        }

        /// Активация приложения: создание или показ главного окна, логика автоподключения
        fn activate(&self) {
            let application = self.obj();
            let window = application.active_window().unwrap_or_else(|| {
                let window = VrxxWindow::new(&*application);
                window.upcast()
            });

            // Если запуск с аргументом --hidden (например, из автозапуска), окно не отображается
            let is_hidden = std::env::args().any(|arg| arg == "--hidden");
            if !is_hidden {
                window.present();
            }

            // Автоматическое подключение к активному профилю при старте системы
            let app_settings = crate::settings::SettingsManager::new().load();
            if app_settings.connect_on_startup {
                if let Some(active_key) = app_settings.keys.iter().find(|k| k.is_active) {
                    if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&active_key.url) {
                        let config_json = crate::domain::singbox_config::build_singbox_config(
                            &parsed,
                            &app_settings,
                        );
                        let backend = crate::backend::CoreBackend::new();
                        std::thread::spawn(move || {
                            let _ = backend.start(&config_json);
                        });
                    }
                }
            }
        }

        /// Обработка открытия URL-схем протоколов (vless://, vmess:// и т.д.)
        fn open(&self, files: &[gio::File], _hint: &str) {
            let application = self.obj();
            let window = application.active_window().unwrap_or_else(|| {
                let window = VrxxWindow::new(&*application);
                window.upcast()
            });

            if let Some(vrxx_win) = window.downcast_ref::<VrxxWindow>() {
                for file in files {
                    let uri = file.uri();
                    tracing::info!("Received URL via URI scheme handler: {uri}");
                    vrxx_win.handle_open_uri(&uri);
                }
            }
        }
    }

    impl GtkApplicationImpl for VrxxApplication {}
    impl AdwApplicationImpl for VrxxApplication {}
}

glib::wrapper! {
    /// Обертка GObject для приложения VRXX
    pub struct VrxxApplication(ObjectSubclass<imp::VrxxApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxApplication {
    /// Создает новый экземпляр приложения с флагами поддержки открытия URL-схем.
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        let combined_flags = *flags | gio::ApplicationFlags::HANDLES_OPEN;
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", combined_flags)
            .property("resource-base-path", "/ru/mark/vrxx")
            .build()
    }

    /// Регистрация глобальных обработчиков действий (GActions).
    fn setup_gactions(&self) {
        // Действие: Завершение работы
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();

        // Действие: Диалог сведений о программе
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();

        // Действие: Переключение темы оформления
        let color_scheme_action = gio::ActionEntry::builder("set-color-scheme")
            .parameter_type(Some(glib::VariantTy::STRING))
            .state(glib::Variant::from("default"))
            .activate(move |_app: &Self, action, parameter| {
                if let Some(param) = parameter {
                    if let Some(scheme) = param.get::<String>() {
                        let manager = adw::StyleManager::default();
                        match scheme.as_str() {
                            "force-light" => manager.set_color_scheme(adw::ColorScheme::ForceLight),
                            "force-dark" => manager.set_color_scheme(adw::ColorScheme::ForceDark),
                            _ => manager.set_color_scheme(adw::ColorScheme::Default),
                        }
                        action.set_state(param);

                        // Сохраняем тему в персистентные настройки
                        let settings_mgr = crate::settings::SettingsManager::new();
                        let mut app_settings = settings_mgr.load();
                        app_settings.theme = scheme;
                        settings_mgr.save(&app_settings);
                    }
                }
            })
            .build();

        // Действие: Импорт настроек из JSON файла
        let import_config_action = gio::ActionEntry::builder("import_config")
            .activate(move |app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let dialog = gtk::FileDialog::builder()
                        .title(gettext("Import Configuration"))
                        .build();
                    dialog.open(Some(&window), gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let manager = crate::settings::SettingsManager::new();

                                    // 1. Попытка десериализации как ExportBundle (новый формат)
                                    if let Ok(bundle) =
                                        serde_json::from_str::<ExportBundle>(&content)
                                    {
                                        if let Some(imported_settings) = bundle.settings {
                                            manager.save_settings_only(&imported_settings);
                                        }
                                        if let Some(imported_keys) = bundle.keys {
                                            manager.save_keys(&imported_keys);
                                        }
                                        tracing::info!(
                                            "Configuration bundle successfully imported from {:?}",
                                            path
                                        );
                                    }
                                    // 2. Обратная совместимость с сырым AppSettings (старый формат)
                                    else if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(&content)
                                    {
                                        if let Ok(settings) =
                                            serde_json::from_value::<crate::settings::AppSettings>(
                                                val.clone(),
                                            )
                                        {
                                            manager.save_settings_only(&settings);
                                        }
                                        if let Some(keys_val) = val.get("keys") {
                                            if let Ok(legacy_keys) = serde_json::from_value::<
                                                Vec<crate::settings::VpnKeyData>,
                                            >(
                                                keys_val.clone()
                                            ) {
                                                if !legacy_keys.is_empty() {
                                                    manager.save_keys(&legacy_keys);
                                                }
                                            }
                                        }
                                        tracing::info!(
                                            "Legacy configuration successfully imported from {:?}",
                                            path
                                        );
                                    }
                                }
                            }
                        }
                    });
                }
            })
            .build();

        // Действие: Гранулярный экспорт настроек и ключей в JSON файл
        let export_config_action = gio::ActionEntry::builder("export_config")
            .activate(move |app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let win_clone = window.clone();
                    crate::ui::export_dialog::show_export_dialog(
                        &window,
                        move |export_settings, export_keys| {
                            let dialog = gtk::FileDialog::builder()
                                .title(gettext("Export Configuration"))
                                .initial_name("vrxx_backup.json")
                                .build();

                            dialog.save(Some(&win_clone), gio::Cancellable::NONE, move |res| {
                                if let Ok(file) = res {
                                    if let Some(path) = file.path() {
                                        let current_settings =
                                            crate::settings::SettingsManager::new().load();
                                        let bundle = ExportBundle {
                                            version: 1,
                                            settings: if export_settings {
                                                Some(current_settings.clone())
                                            } else {
                                                None
                                            },
                                            keys: if export_keys {
                                                Some(current_settings.keys.clone())
                                            } else {
                                                None
                                            },
                                        };

                                        if let Ok(content) = serde_json::to_string_pretty(&bundle) {
                                            #[cfg(unix)]
                                            {
                                                use std::io::Write;
                                                use std::os::unix::fs::{
                                                    OpenOptionsExt, PermissionsExt,
                                                };
                                                let mut opts = std::fs::OpenOptions::new();
                                                opts.create(true)
                                                    .write(true)
                                                    .truncate(true)
                                                    .mode(0o600);
                                                if let Ok(mut file) = opts.open(&path) {
                                                    let _ = file.set_permissions(
                                                        std::fs::Permissions::from_mode(0o600),
                                                    );
                                                    let _ = file.write_all(content.as_bytes());
                                                }
                                            }
                                            #[cfg(not(unix))]
                                            {
                                                let _ = std::fs::write(&path, content);
                                            }
                                            tracing::info!(
                                                "Configuration export successfully saved to {:?}",
                                                path
                                            );
                                        }
                                    }
                                }
                            });
                        },
                    );
                }
            })
            .build();

        // Действие: Открытие окна системных логов
        let view_logs_action = gio::ActionEntry::builder("view_logs")
            .activate(move |app: &Self, _, _| {
                // Если окно логов уже открыто — выносим его на передний план
                for window in app.windows() {
                    if window.is::<crate::ui::components::log_window::VrxxLogWindow>() {
                        window.present();
                        return;
                    }
                }
                let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
                if let Some(parent) = app.active_window() {
                    if parent.upcast_ref::<gtk::Widget>() != log_window.upcast_ref::<gtk::Widget>()
                    {
                        gtk::prelude::GtkWindowExt::set_transient_for(&log_window, Some(&parent));
                        gtk::prelude::GtkWindowExt::set_destroy_with_parent(&log_window, true);
                    }
                }
                app.add_window(&log_window);
                gtk::prelude::GtkWindowExt::present(&log_window);
            })
            .build();

        // Действие: Открытие системной папки логов
        let open_log_dir_action = gio::ActionEntry::builder("open_log_dir")
            .activate(move |_, _, _| {
                let log_dir = crate::ui::components::log_window::get_log_dir();
                std::fs::create_dir_all(&log_dir).ok();
                if let Ok(uri) = glib::filename_to_uri(&log_dir, None) {
                    let _ =
                        gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                }
            })
            .build();

        // Действие: Сброс настроек к заводским
        let reset_settings_action = gio::ActionEntry::builder("reset_settings")
            .activate(move |app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let dialog = adw::AlertDialog::builder()
                        .heading("Reset Settings")
                        .body("Are you sure you want to reset all settings to defaults?")
                        .build();
                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("reset", "Reset");
                    dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);

                    dialog.connect_response(None, move |_, response| {
                        if response == "reset" {
                            let manager = crate::settings::SettingsManager::new();
                            let default_settings = crate::settings::AppSettings::new();
                            manager.save(&default_settings);
                        }
                    });
                    dialog.present(Some(&window));
                }
            })
            .build();

        self.add_action_entries([
            quit_action,
            about_action,
            color_scheme_action,
            import_config_action,
            export_config_action,
            view_logs_action,
            open_log_dir_action,
            reset_settings_action,
        ]);
    }

    /// Загружает CSS таблицу стилей из GResource
    fn setup_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/ru/mark/vrxx/style.css");
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    /// Подключает каталог иконок из ресурсов приложения
    fn setup_icons(&self) {
        if let Some(display) = gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_resource_path("/ru/mark/vrxx/icons");
        }
    }

    /// Отображает модальный диалог «О программе» (AdwAboutDialog)
    fn show_about(&self) {
        let window = match self.active_window() {
            Some(w) => w,
            None => {
                tracing::warn!("No active window found to display About dialog.");
                return;
            }
        };
        let about = adw::AboutDialog::builder()
            .application_name("vrxx")
            .application_icon("ru.mark.vrxx")
            .developer_name("Mark")
            .version(VERSION)
            .developers(vec!["Mark"])
            .artists(vec!["GNOME Design Team"])
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 Mark")
            .license_type(gtk::License::Mpl20)
            .website("https://github.com/Mark-TinZ/vrxx")
            .issue_url("https://github.com/Mark-TinZ/vrxx/issues")
            .comments(gettext("A graphical interface for sing-box designed to simplify VPN and proxy configuration on Linux systems."))
            .build();

        about.present(Some(&window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppSettings, VpnKeyData};

    #[test]
    fn test_export_bundle_serialization_roundtrip() {
        let mut settings = AppSettings::new();
        settings.theme = "force-dark".to_string();
        settings.socks_port = 10888;

        let keys = vec![VpnKeyData {
            name: "Export Test".to_string(),
            protocol: "VLESS".to_string(),
            is_active: false,
            traffic_down: "0 MB".to_string(),
            traffic_up: "0 MB".to_string(),
            time_connected: "00:00:00".to_string(),
            ping: "15 ms".to_string(),
            location: "SE".to_string(),
            timezone: "UTC+1".to_string(),
            url: "vless://test-export@1.1.1.1:443#Sweden".to_string(),
        }];

        // 1. Полный экспорт (настройки + ключи)
        let full_bundle = ExportBundle {
            version: 1,
            settings: Some(settings.clone()),
            keys: Some(keys.clone()),
        };
        let json_full = serde_json::to_string_pretty(&full_bundle).unwrap();
        let parsed_full: ExportBundle = serde_json::from_str(&json_full).unwrap();
        assert_eq!(parsed_full.version, 1);
        assert!(parsed_full.settings.is_some());
        assert!(parsed_full.keys.is_some());
        assert_eq!(parsed_full.settings.unwrap().socks_port, 10888);
        assert_eq!(parsed_full.keys.unwrap()[0].name, "Export Test");

        // 2. Экспорт только настроек (без ключей)
        let settings_only_bundle = ExportBundle {
            version: 1,
            settings: Some(settings),
            keys: None,
        };
        let json_settings = serde_json::to_string_pretty(&settings_only_bundle).unwrap();
        assert!(!json_settings.contains("\"keys\""));
        let parsed_settings: ExportBundle = serde_json::from_str(&json_settings).unwrap();
        assert!(parsed_settings.settings.is_some());
        assert!(parsed_settings.keys.is_none());

        // 3. Экспорт только ключей (без настроек)
        let keys_only_bundle = ExportBundle {
            version: 1,
            settings: None,
            keys: Some(keys),
        };
        let json_keys = serde_json::to_string_pretty(&keys_only_bundle).unwrap();
        assert!(!json_keys.contains("\"settings\""));
        let parsed_keys: ExportBundle = serde_json::from_str(&json_keys).unwrap();
        assert!(parsed_keys.settings.is_none());
        assert!(parsed_keys.keys.is_some());
        assert_eq!(parsed_keys.keys.unwrap().len(), 1);
    }
}
