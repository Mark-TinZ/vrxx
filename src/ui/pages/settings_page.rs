use crate::settings::SettingsManager;
use crate::ui::setup_primary_menu;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/settings_page.ui")]
    pub struct VrxxSettingsPage {
        #[template_child]
        pub btn_apply: TemplateChild<gtk::Button>,
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub core_info_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub language_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub autostart_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub connect_startup_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub notifications_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub streamer_mode_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub log_level_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub sniffing_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub domain_strategy_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub update_geo_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub btn_update_geo: TemplateChild<gtk::Button>,
        #[template_child]
        pub geo_update_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub geo_update_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub tun_mode_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub bypass_lan_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub fake_dns_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub mux_row: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub mux_concurrency_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub fragment_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub ping_algorithm_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub ping_target_url_row: TemplateChild<adw::EntryRow>,

        pub has_changes: RefCell<bool>,
        pub has_lang_changed: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxSettingsPage {
        const NAME: &'static str = "VrxxSettingsPage";
        type Type = super::VrxxSettingsPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ComboRow::static_type();
            adw::EntryRow::static_type();
            adw::SwitchRow::static_type();
            adw::ExpanderRow::static_type();
            adw::SpinRow::static_type();
            adw::ActionRow::static_type();
            gtk::Stack::static_type();
            gtk::Spinner::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxSettingsPage {
        fn constructed(&self) {
            self.parent_constructed();
            setup_primary_menu(&self.primary_menu_btn.get());
            self.obj().setup_settings();
        }
    }
    impl WidgetImpl for VrxxSettingsPage {}
    impl BinImpl for VrxxSettingsPage {}
}

glib::wrapper! {
    pub struct VrxxSettingsPage(ObjectSubclass<imp::VrxxSettingsPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxSettingsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxSettingsPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    // --- Раздел: Обработка изменений ---
    fn mark_changed(&self, is_lang: bool) {
        let imp = self.imp();
        *imp.has_changes.borrow_mut() = true;
        if is_lang {
            *imp.has_lang_changed.borrow_mut() = true;
        }
        imp.btn_apply.set_visible(true);
    }

    fn apply_changes(&self) {
        let imp = self.imp();
        let lang_changed = *imp.has_lang_changed.borrow();

        // REFACTOR: Сохранение настроек сейчас происходит при каждом изменении (connect_active_notify и т.д.)
        // Кнопка Apply по сути просто скрывается и инициирует перезапуск ядра или приложения.
        // Стоит перенести логику сохранения именно сюда для атомарности.

        *imp.has_changes.borrow_mut() = false;
        *imp.has_lang_changed.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        if lang_changed {
            if let Some(window) = self.root().and_downcast::<gtk::Window>() {
                let dialog = adw::AlertDialog::builder()
                    .heading(gettextrs::gettext("Restart Required"))
                    .body(gettextrs::gettext("You have changed the language. The application needs to restart to apply the new language. Restart now?"))
                    .build();

                dialog.add_response("cancel", &gettextrs::gettext("Cancel"));
                dialog.add_response("restart", &gettextrs::gettext("Restart"));
                dialog.set_response_appearance("restart", adw::ResponseAppearance::Destructive);

                gtk::glib::MainContext::default().spawn_local(async move {
                    let response = dialog.choose_future(&window).await;
                    if response == "restart" {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe).spawn();
                            std::process::exit(0);
                        }
                    }
                });
            }
        } else {
            self.show_restart_core_toast();
        }
    }
    // ================================

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.btn_apply.set_visible(false);

        imp.btn_apply.connect_clicked(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                page.apply_changes();
            }
        ));

        imp.btn_update_geo.connect_clicked(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                page.update_geo_data();
            }
        ));

        let lang_idx = match settings.language.as_str() {
            "en" => 1,
            "ru" => 2,
            _ => 0, // system
        };
        imp.language_row.set_selected(lang_idx);

        imp.autostart_row.set_active(settings.autostart);
        imp.connect_startup_row
            .set_active(settings.connect_on_startup);
        imp.notifications_row.set_active(settings.notifications);
        imp.streamer_mode_row.set_active(settings.streamer_mode);

        imp.tun_mode_row.set_active(settings.tun_mode);
        imp.sniffing_row.set_active(settings.enable_sniffing);
        imp.bypass_lan_row.set_active(settings.bypass_lan);
        imp.fake_dns_row.set_active(settings.enable_fake_dns);
        imp.mux_row.set_enable_expansion(settings.enable_mux);
        imp.mux_concurrency_row
            .set_value(settings.mux_concurrency as f64);
        imp.fragment_row.set_active(settings.enable_fragment);

        let strategy_idx = match settings.domain_strategy.as_str() {
            "IPIfNonMatch" => 1,
            "IPOnDemand" => 2,
            _ => 0, // AsIs
        };
        imp.domain_strategy_row.set_selected(strategy_idx);

        let log_idx = match settings.log_level.as_str() {
            "error" => 0,
            "warning" => 1,
            "debug" => 3,
            _ => 2, // info default
        };
        imp.log_level_row.set_selected(log_idx);

        let ping_algo_idx = match settings.ping_algorithm.as_str() {
            "icmp_ping" => 1,
            "via_proxy_get" => 2,
            "via_proxy_head" => 3,
            _ => 0, // tcp_handshake
        };
        imp.ping_algorithm_row.set_selected(ping_algo_idx);
        imp.ping_target_url_row.set_text(&settings.ping_target_url);

        self.update_core_info(None);
        self.refresh_geo_status();

        // Connect signals
        imp.ping_algorithm_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.ping_algorithm = match row.selected() {
                    1 => "icmp_ping".to_string(),
                    2 => "via_proxy_get".to_string(),
                    3 => "via_proxy_head".to_string(),
                    _ => "tcp_handshake".to_string(),
                };
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.ping_target_url_row.connect_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                let text = row.text().to_string();
                if !text.trim().is_empty() {
                    s.ping_target_url = text;
                    manager.save(&s);
                    page.mark_changed(false);
                }
            }
        ));

        imp.language_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.language = match row.selected() {
                    1 => "en".to_string(),
                    2 => "ru".to_string(),
                    _ => "system".to_string(),
                };
                manager.save(&s);
                page.mark_changed(true);
            }
        ));

        // --- Раздел: Системные настройки ---
        imp.autostart_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.autostart = row.is_active();
                manager.save(&s);

                // NOTE: Настройка автозагрузки через создание .desktop файла
                let autostart_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("autostart");
                std::fs::create_dir_all(&autostart_dir).ok();
                let desktop_file_path = autostart_dir.join("ru.mark.vrxx.desktop");

                if s.autostart {
                    let exe_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("vrxx"));
                    let exec_cmd = if std::env::var("FLATPAK_ID").is_ok() {
                        "flatpak run ru.mark.vrxx --hidden".to_string()
                    } else {
                        format!("{} --hidden", exe_path.display())
                    };

                    let desktop_content = format!("[Desktop Entry]\nType=Application\nName=Vrxx\nExec={exec_cmd}\nIcon=ru.mark.vrxx\nComment=VPN Client\nTerminal=false\nCategories=Network;\n");
                    let _ = std::fs::write(&desktop_file_path, desktop_content);
                } else {
                    let _ = std::fs::remove_file(&desktop_file_path);
                }
                page.mark_changed(false);
            }
        ));
        // ================================

        imp.connect_startup_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.connect_on_startup = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.notifications_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.notifications = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.streamer_mode_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.streamer_mode = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.tun_mode_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.tun_mode = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.sniffing_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.enable_sniffing = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.bypass_lan_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.bypass_lan = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.fake_dns_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.enable_fake_dns = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.fragment_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.enable_fragment = row.is_active();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.mux_row.connect_enable_expansion_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.enable_mux = row.enables_expansion();
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.mux_concurrency_row.connect_value_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.mux_concurrency = row.value() as i32;
                manager.save(&s);
                page.mark_changed(false);
            }
        ));

        imp.domain_strategy_row
            .connect_selected_notify(glib::clone!(
                #[weak(rename_to = page)]
                self,
                move |row| {
                    let manager = SettingsManager::new();
                    let mut s = manager.load();
                    s.domain_strategy = match row.selected() {
                        1 => "IPIfNonMatch".to_string(),
                        2 => "IPOnDemand".to_string(),
                        _ => "AsIs".to_string(),
                    };
                    manager.save(&s);
                    page.mark_changed(false);
                }
            ));

        imp.log_level_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.log_level = match row.selected() {
                    0 => "error".to_string(),
                    1 => "warning".to_string(),
                    3 => "debug".to_string(),
                    _ => "info".to_string(),
                };
                manager.save(&s);
                page.mark_changed(false);
            }
        ));
    }

    fn show_restart_core_toast(&self) {
        let _ = crate::settings::core_restart_channel().0.send_blocking(());
        if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>() {
            let notification = gtk::gio::Notification::new(&gettextrs::gettext("Settings applied"));
            notification.set_body(Some(&gettextrs::gettext(
                "Core was restarted to apply new settings.",
            )));
            app.send_notification(Some("settings_applied"), &notification);
        }
    }

    // --- Раздел: Диагностика ---
    fn refresh_geo_status(&self) {
        let status = crate::services::geo_updater::get_geo_status();
        self.imp().update_geo_row.set_subtitle(&format!(
            "{}: {}",
            gettextrs::gettext("Last update"),
            status
        ));
    }

    fn update_geo_data(&self) {
        let imp = self.imp();

        // Переключаем на спиннер и меняем подзаголовок
        imp.geo_update_stack.set_visible_child_name("spinner_page");
        imp.update_geo_row
            .set_subtitle(&gettextrs::gettext("Downloading..."));

        gtk::glib::MainContext::default().spawn_local(glib::clone!(
            #[weak(rename_to = page)]
            self,
            async move {
                // Выполняем обновление без передачи канала прогресса (он больше не нужен)
                let _ = crate::services::geo_updater::update_geo_databases(true, None).await;

                // Возвращаем UI в исходное состояние
                page.refresh_geo_status();
                page.imp()
                    .geo_update_stack
                    .set_visible_child_name("button_page");

                if let Some(app) =
                    gtk::gio::Application::default().and_downcast::<gtk::Application>()
                {
                    let notification =
                        gtk::gio::Notification::new(&gettextrs::gettext("Geo Data Updated"));
                    notification.set_body(Some(&gettextrs::gettext(
                        "Latest geo-databases downloaded.",
                    )));
                    app.send_notification(Some("geo_updated"), &notification);
                }
            }
        ));
    }

    fn update_core_info(&self, name: Option<&str>) {
        // --- Раздел: Диагностика ядер ---
        // XXX: Мы всегда используем sing-box теперь
        let bin_name = name.unwrap_or("sing-box");

        // TODO: Использовать асинхронный вызов команды для получения версии
        let output = std::process::Command::new(bin_name).arg("version").output();

        let version_str = match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.lines()
                    .next()
                    .unwrap_or(&gettextrs::gettext("Unknown Version"))
                    .to_string()
            }
            Err(_) => format!("{} {}", bin_name, gettextrs::gettext("not found")),
        };

        self.imp().core_info_row.set_subtitle(&version_str);
        self.imp().fragment_row.set_visible(false);
    }
    // ================================
}
