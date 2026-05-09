/* application.rs
 *
 * Copyright 2026 Unknown
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

use crate::config::VERSION;
use crate::window::VrxxWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};

mod imp {
    use super::*;

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
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("vpn.disconnect", &["<control>d"]);
            obj.set_accels_for_action("win.zoom_in", &["<Primary>plus", "<Primary>equal"]);
            obj.set_accels_for_action("win.zoom_out", &["<Primary>minus"]);
            obj.set_accels_for_action("win.zoom_normal", &["<Primary>0"]);
        }
    }

    impl ApplicationImpl for VrxxApplication {
        fn startup(&self) {
            self.parent_startup();

            let manager = adw::StyleManager::default();

            // Загрузка темы при старте
            let settings = crate::settings::SettingsManager::new();
            let app_settings = settings.load();
            match app_settings.theme.as_str() {
                "force-light" => manager.set_color_scheme(adw::ColorScheme::ForceLight),
                "force-dark" => manager.set_color_scheme(adw::ColorScheme::ForceDark),
                _ => manager.set_color_scheme(adw::ColorScheme::Default),
            }

            // Sync the action state so the UI switcher reflects the loaded theme
            use gio::prelude::ActionMapExt;
            if let Some(action) = self.obj().lookup_action("set-color-scheme") {
                if let Some(simple_action) = action.downcast_ref::<gio::SimpleAction>() {
                    simple_action.set_state(&glib::Variant::from(app_settings.theme.as_str()));
                }
            }

            self.obj().setup_icons();
            gtk::Window::set_default_icon_name("ru.mark.vrxx");
        }

        fn activate(&self) {
            let application = self.obj();
            let window = application.active_window().unwrap_or_else(|| {
                let window = VrxxWindow::new(&*application);
                window.upcast()
            });

            // Prevent presenting the window if launched with --hidden
            let is_hidden = std::env::args().any(|arg| arg == "--hidden");
            if !is_hidden {
                window.present();
            }

            // Check if core is installed, if not prompt the user
            crate::ui::components::core_installer::check_and_prompt(
                window.downcast_ref::<gtk::Window>().unwrap(),
            );
        }
    }

    impl GtkApplicationImpl for VrxxApplication {}
    impl AdwApplicationImpl for VrxxApplication {}
}

glib::wrapper! {
    pub struct VrxxApplication(ObjectSubclass<imp::VrxxApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/ru/mark/vrxx")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();

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

                        // Сохраняем тему в настройки
                        let settings_mgr = crate::settings::SettingsManager::new();
                        let mut app_settings = settings_mgr.load();
                        app_settings.theme = scheme;
                        settings_mgr.save(&app_settings);
                    }
                }
            })
            .build();

        let import_config_action = gio::ActionEntry::builder("import_config")
            .activate(move |app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let dialog = gtk::FileDialog::builder().title("Import Settings").build();
                    dialog.open(Some(&window), gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                if let Ok(content) = std::fs::read_to_string(path) {
                                    if let Ok(settings) =
                                        serde_json::from_str::<crate::settings::AppSettings>(
                                            &content,
                                        )
                                    {
                                        crate::settings::SettingsManager::new().save(&settings);
                                        // TODO: reload settings in UI or require restart
                                    }
                                }
                            }
                        }
                    });
                }
            })
            .build();

        let export_config_action = gio::ActionEntry::builder("export_config")
            .activate(move |app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let dialog = gtk::FileDialog::builder()
                        .title("Export Settings")
                        .initial_name("vrxx_config.json")
                        .build();
                    dialog.save(Some(&window), gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                let settings = crate::settings::SettingsManager::new().load();
                                if let Ok(content) = serde_json::to_string_pretty(&settings) {
                                    #[cfg(unix)]
                                    {
                                        use std::io::Write;
                                        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                                        let mut opts = std::fs::OpenOptions::new();
                                        opts.create(true).write(true).truncate(true).mode(0o600);
                                        if let Ok(mut file) = opts.open(&path) {
                                            let _ = file.set_permissions(
                                                std::fs::Permissions::from_mode(0o600),
                                            );
                                            let _ = file.write_all(content.as_bytes());
                                        }
                                    }
                                    #[cfg(not(unix))]
                                    {
                                        let _ = std::fs::write(path, content);
                                    }
                                }
                            }
                        }
                    });
                }
            })
            .build();

        let view_logs_action = gio::ActionEntry::builder("view_logs")
            .activate(move |app: &Self, _, _| {
                // Check if a LogWindow is already open
                for window in app.windows() {
                    if window.is::<crate::ui::components::log_window::VrxxLogWindow>() {
                        window.present();
                        return;
                    }
                }
                let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
                if let Some(parent) = app.active_window() {
                    // Убеждаемся, что мы не пытаемся привязать окно к самому себе
                    if parent.upcast_ref::<gtk::Widget>() != log_window.upcast_ref::<gtk::Widget>()
                    {
                        gtk::prelude::GtkWindowExt::set_transient_for(&log_window, Some(&parent));
                    }
                }
                app.add_window(&log_window);
                gtk::prelude::GtkWindowExt::present(&log_window);
            })
            .build();

        let open_log_dir_action = gio::ActionEntry::builder("open_log_dir")
            .activate(move |_, _, _| {
                let log_dir = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("vrxx")
                    .join("logs");
                std::fs::create_dir_all(&log_dir).ok();
                if let Ok(uri) = glib::filename_to_uri(&log_dir, None) {
                    let _ =
                        gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                }
            })
            .build();

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
                            // Recommend restart
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

    fn setup_icons(&self) {
        if let Some(display) = gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_resource_path("/ru/mark/vrxx/icons");
        }
    }

    fn show_about(&self) {
        let window = match self.active_window() {
            Some(w) => w,
            None => {
                tracing::warn!("Warning: No active window found for about dialog.");
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
            .comments(gettext("A graphical interface for Xray-core designed to simplify VPN and proxy configuration on Linux systems."))
            .build();

        about.present(Some(&window));
    }
}
