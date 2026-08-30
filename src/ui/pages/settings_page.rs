/* settings_page.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Страница настроек приложения и сетевого ядра (VrxxSettingsPage)
//!
//! Отвечает за:
//! - Общие параметры приложения (язык интерфейса, автозапуск GNOME, автоподключение, уведомления, режим стримера)
//! - Конфигурацию ядра sing-box (уровень логирования, Sniffing протоколов, блокировка QUIC/HTTP3, TUN-режим, FakeDNS, Multiplex)
//! - Параметры замера задержки (TCP Handshake, ICMP Ping, HTTP GET / HEAD, кастомный URL)
//! - Обновление баз данных GeoIP и GeoSite с индикацией загрузки
//! - Диагностику установленной версии бинарника `sing-box`
//! - Отслеживание диффа настроек (Snapshot Diff) и поддержку навигационного стража

use crate::settings::SettingsManager;
use crate::ui::setup_primary_menu;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    language: u32,
    autostart: bool,
    connect_on_startup: bool,
    notifications: bool,
    streamer_mode: bool,
    tun_mode: bool,
    enable_sniffing: bool,
    block_quic: bool,
    bypass_lan: bool,
    enable_fake_dns: bool,
    enable_mux: bool,
    mux_concurrency: i32,
    log_level: u32,
    ping_algorithm: u32,
    ping_target_url: String,
}

mod imp {
    use super::*;

    /// Структура CompositeTemplate для страницы настроек VrxxSettingsPage
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
        pub block_quic_row: TemplateChild<adw::SwitchRow>,
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
        pub ping_algorithm_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub ping_target_url_row: TemplateChild<adw::EntryRow>,

        pub snapshot: RefCell<Option<SettingsSnapshot>>,
        pub has_changes: RefCell<bool>,
        pub is_initializing: RefCell<bool>,
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
    /// Обертка GObject для страницы настроек
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
    /// Создает новый экземпляр страницы настроек.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Возвращает флаг наличия несохраненных изменений.
    pub fn has_changes(&self) -> bool {
        *self.imp().has_changes.borrow()
    }

    /// Считывает текущее состояние элементов интерфейса в структуру `SettingsSnapshot`.
    fn get_current_ui_state(&self) -> SettingsSnapshot {
        let imp = self.imp();
        SettingsSnapshot {
            language: imp.language_row.selected(),
            autostart: imp.autostart_row.is_active(),
            connect_on_startup: imp.connect_startup_row.is_active(),
            notifications: imp.notifications_row.is_active(),
            streamer_mode: imp.streamer_mode_row.is_active(),
            tun_mode: imp.tun_mode_row.is_active(),
            enable_sniffing: imp.sniffing_row.is_active(),
            block_quic: imp.block_quic_row.is_active(),
            bypass_lan: imp.bypass_lan_row.is_active(),
            enable_fake_dns: imp.fake_dns_row.is_active(),
            enable_mux: imp.mux_row.enables_expansion(),
            mux_concurrency: imp.mux_concurrency_row.value() as i32,
            log_level: imp.log_level_row.selected(),
            ping_algorithm: imp.ping_algorithm_row.selected(),
            ping_target_url: imp.ping_target_url_row.text().trim().to_string(),
        }
    }

    /// Проверяет дифф между текущим состоянием элементов UI и сохраненным снимком.
    pub fn check_changes(&self) {
        if *self.imp().is_initializing.borrow() {
            return;
        }

        let imp = self.imp();
        let current = self.get_current_ui_state();
        let saved = imp.snapshot.borrow().clone();

        let changed = match saved {
            Some(ref s) => s != &current,
            None => false,
        };

        *imp.has_changes.borrow_mut() = changed;
        imp.btn_apply.set_visible(changed);
    }

    /// Применяет накопленные изменения настроек и при необходимости инициирует перезапуск ядра/приложения.
    pub fn apply_changes(&self) {
        let imp = self.imp();
        let current = self.get_current_ui_state();
        let initial_lang = imp
            .snapshot
            .borrow()
            .as_ref()
            .map(|s| s.language)
            .unwrap_or(0);
        let lang_changed = initial_lang != current.language;

        let manager = SettingsManager::new();
        let mut s = manager.load();

        s.language = match current.language {
            1 => "en".to_string(),
            2 => "ru".to_string(),
            _ => "system".to_string(),
        };

        s.autostart = current.autostart;
        s.connect_on_startup = current.connect_on_startup;
        s.notifications = current.notifications;
        s.streamer_mode = current.streamer_mode;
        s.tun_mode = current.tun_mode;
        s.enable_sniffing = current.enable_sniffing;
        s.block_quic = current.block_quic;
        s.bypass_lan = current.bypass_lan;
        s.enable_fake_dns = current.enable_fake_dns;
        s.enable_mux = current.enable_mux;
        s.mux_concurrency = current.mux_concurrency.clamp(1, 128);

        s.log_level = match current.log_level {
            0 => "error".to_string(),
            1 => "warning".to_string(),
            3 => "debug".to_string(),
            _ => "info".to_string(),
        };

        s.ping_algorithm = match current.ping_algorithm {
            1 => "icmp_ping".to_string(),
            2 => "via_proxy_get".to_string(),
            3 => "via_proxy_head".to_string(),
            _ => "tcp_handshake".to_string(),
        };

        if !current.ping_target_url.is_empty() {
            s.ping_target_url = current.ping_target_url.clone();
        }

        manager.save(&s);

        // Настройка автозапуска GNOME
        let autostart_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("autostart");
        std::fs::create_dir_all(&autostart_dir).ok();
        let desktop_file_path = autostart_dir.join("ru.mark.vrxx.desktop");

        if s.autostart {
            let exe_path =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("vrxx"));
            let exec_cmd = if std::env::var("FLATPAK_ID").is_ok() {
                "flatpak run ru.mark.vrxx --hidden".to_string()
            } else {
                format!("{} --hidden", exe_path.display())
            };

            let desktop_content = format!(
                "[Desktop Entry]\nType=Application\nName=Vrxx\nExec={exec_cmd}\nIcon=ru.mark.vrxx\nComment=VPN Client\nTerminal=false\nCategories=Network;\n"
            );
            let _ = std::fs::write(&desktop_file_path, desktop_content);
        } else {
            let _ = std::fs::remove_file(&desktop_file_path);
        }

        *imp.snapshot.borrow_mut() = Some(current);
        *imp.has_changes.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        if lang_changed {
            if let Some(window) = self.root().and_downcast::<gtk::Window>() {
                let dialog = adw::AlertDialog::builder()
                    .heading(gettextrs::gettext("Restart Required"))
                    .body(gettextrs::gettext(
                        "You have changed the language. The application needs to restart to apply the new language. Restart now?",
                    ))
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
            let toast_text = gettextrs::gettext("Application settings applied and core restarted.");
            crate::ui::change_tracker::apply_and_restart_core(
                &toast_text,
                self.root().and_downcast_ref::<gtk::Window>(),
            );
        }
    }

    /// Откатывает виджеты страницы настроек к сохраненному снимку.
    pub fn discard_changes(&self) {
        let imp = self.imp();
        if let Some(snapshot) = imp.snapshot.borrow().clone() {
            *imp.is_initializing.borrow_mut() = true;

            imp.language_row.set_selected(snapshot.language);
            imp.autostart_row.set_active(snapshot.autostart);
            imp.connect_startup_row
                .set_active(snapshot.connect_on_startup);
            imp.notifications_row.set_active(snapshot.notifications);
            imp.streamer_mode_row.set_active(snapshot.streamer_mode);
            imp.tun_mode_row.set_active(snapshot.tun_mode);
            imp.sniffing_row.set_active(snapshot.enable_sniffing);
            imp.block_quic_row.set_active(snapshot.block_quic);
            imp.bypass_lan_row.set_active(snapshot.bypass_lan);
            imp.fake_dns_row.set_active(snapshot.enable_fake_dns);
            imp.mux_row.set_enable_expansion(snapshot.enable_mux);
            imp.mux_concurrency_row
                .set_value(snapshot.mux_concurrency as f64);
            imp.log_level_row.set_selected(snapshot.log_level);
            imp.ping_algorithm_row.set_selected(snapshot.ping_algorithm);
            imp.ping_target_url_row.set_text(&snapshot.ping_target_url);

            *imp.is_initializing.borrow_mut() = false;
            *imp.has_changes.borrow_mut() = false;
            imp.btn_apply.set_visible(false);
        }
    }

    /// Инициализирует элементы управления начальными значениями из конфигурации и привязывает сигналы.
    fn setup_settings(&self) {
        let imp = self.imp();
        *imp.is_initializing.borrow_mut() = true;

        let manager = SettingsManager::new();
        let settings = manager.load();

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
        imp.block_quic_row.set_active(settings.block_quic);
        imp.bypass_lan_row.set_active(settings.bypass_lan);
        imp.fake_dns_row.set_active(settings.enable_fake_dns);
        imp.mux_row.set_enable_expansion(settings.enable_mux);
        imp.mux_concurrency_row
            .set_value(settings.mux_concurrency as f64);

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

        let initial_snapshot = self.get_current_ui_state();
        *imp.snapshot.borrow_mut() = Some(initial_snapshot);
        *imp.has_changes.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        // Обработчик кнопки «Применить»
        let page_weak_apply = self.downgrade();
        imp.btn_apply.connect_clicked(move |_| {
            if let Some(page) = page_weak_apply.upgrade() {
                page.apply_changes();
            }
        });

        // Кнопка обновления баз гео-данных
        let page_weak_geo = self.downgrade();
        imp.btn_update_geo.connect_clicked(move |_| {
            if let Some(page) = page_weak_geo.upgrade() {
                page.update_geo_data();
            }
        });

        // Подключение отслеживания изменений переключателей и полей
        let connect_switch = |row: &adw::SwitchRow, page: &VrxxSettingsPage| {
            let p_weak = page.downgrade();
            row.connect_active_notify(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    p.check_changes();
                }
            });
        };

        connect_switch(&imp.autostart_row, self);
        connect_switch(&imp.connect_startup_row, self);
        connect_switch(&imp.notifications_row, self);
        connect_switch(&imp.streamer_mode_row, self);
        connect_switch(&imp.tun_mode_row, self);
        connect_switch(&imp.sniffing_row, self);
        connect_switch(&imp.block_quic_row, self);
        connect_switch(&imp.bypass_lan_row, self);
        connect_switch(&imp.fake_dns_row, self);

        let p_weak_lang = self.downgrade();
        imp.language_row.connect_selected_notify(move |_| {
            if let Some(p) = p_weak_lang.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_log = self.downgrade();
        imp.log_level_row.connect_selected_notify(move |_| {
            if let Some(p) = p_weak_log.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_ping = self.downgrade();
        imp.ping_algorithm_row.connect_selected_notify(move |_| {
            if let Some(p) = p_weak_ping.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_url = self.downgrade();
        imp.ping_target_url_row.connect_changed(move |_| {
            if let Some(p) = p_weak_url.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_mux_exp = self.downgrade();
        imp.mux_row.connect_enable_expansion_notify(move |_| {
            if let Some(p) = p_weak_mux_exp.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_mux_conc = self.downgrade();
        imp.mux_concurrency_row.connect_value_notify(move |_| {
            if let Some(p) = p_weak_mux_conc.upgrade() {
                p.check_changes();
            }
        });

        *imp.is_initializing.borrow_mut() = false;
    }

    /// Обновляет текстовое поле статуса и даты последнего обновления гео-баз.
    fn refresh_geo_status(&self) {
        let status = crate::services::geo_updater::get_geo_status();
        self.imp().update_geo_row.set_subtitle(&format!(
            "{}: {}",
            gettextrs::gettext("Last update"),
            status
        ));
    }

    /// Запускает процесс ручного скачивания и обновления баз GeoIP и GeoSite.
    fn update_geo_data(&self) {
        let imp = self.imp();

        // Переключаем стек на спиннер и меняем подзаголовок
        imp.geo_update_stack.set_visible_child_name("spinner_page");
        imp.update_geo_row
            .set_subtitle(&gettextrs::gettext("Downloading..."));

        gtk::glib::MainContext::default().spawn_local(glib::clone!(
            #[weak(rename_to = page)]
            self,
            async move {
                let _ = crate::services::geo_updater::update_geo_databases(true, None).await;

                // Возвращаем интерфейс в исходное состояние
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

    /// Запрашивает и отображает версию установленного бинарника `sing-box`.
    fn update_core_info(&self, _name: Option<&str>) {
        let bin_path = crate::daemon::updater::find_singbox_binary()
            .unwrap_or_else(|| std::path::PathBuf::from("sing-box"));

        let output = std::process::Command::new(&bin_path)
            .arg("version")
            .output();

        let version_str = match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.lines()
                    .next()
                    .unwrap_or(&gettextrs::gettext("Unknown Version"))
                    .to_string()
            }
            Err(_) => format!("sing-box {}", gettextrs::gettext("not found")),
        };

        self.imp().core_info_row.set_subtitle(&version_str);
    }
}
