/* proxy_page.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Страница конфигурации прокси-серверов (VrxxProxyPage)
//!
//! Отвечает за:
//! - Управление системным прокси окружения GNOME/FreeDesktop (`gsettings`)
//! - Настройку локальных портов входящих соединений SOCKS5 и HTTP
//! - Переключатель общего доступа из локальной сети (Allow LAN)
//! - Кнопку немедленного перезапуска ядра для применения изменений конфигурации
//! - Отслеживание диффа изменений состояния и поддержку навигационного стража

use crate::settings::SettingsManager;
use crate::ui::setup_primary_menu;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub struct ProxySnapshot {
    set_system_proxy: bool,
    socks_port: u16,
    http_port: u16,
    allow_lan: bool,
}

mod imp {
    use super::*;

    /// Структура CompositeTemplate для страницы прокси VrxxProxyPage
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/proxy_page.ui")]
    pub struct VrxxProxyPage {
        #[template_child]
        pub btn_apply: TemplateChild<gtk::Button>,
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub system_proxy_switch: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub socks_port_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub http_port_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub allow_lan_switch: TemplateChild<adw::SwitchRow>,

        pub snapshot: RefCell<Option<ProxySnapshot>>,
        pub has_changes: RefCell<bool>,
        pub is_initializing: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxProxyPage {
        const NAME: &'static str = "VrxxProxyPage";
        type Type = super::VrxxProxyPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::SwitchRow::static_type();
            adw::SpinRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxProxyPage {
        fn constructed(&self) {
            self.parent_constructed();
            setup_primary_menu(&self.primary_menu_btn.get());
            self.obj().setup_settings();
        }
    }
    impl WidgetImpl for VrxxProxyPage {}
    impl BinImpl for VrxxProxyPage {}
}

glib::wrapper! {
    /// Обертка GObject для страницы управления прокси
    pub struct VrxxProxyPage(ObjectSubclass<imp::VrxxProxyPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxProxyPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxProxyPage {
    /// Создает новый экземпляр страницы настроек прокси.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Возвращает флаг наличия несохраненных изменений.
    pub fn has_changes(&self) -> bool {
        *self.imp().has_changes.borrow()
    }

    /// Считывает текущее состояние элементов интерфейса страницы прокси.
    fn get_current_ui_state(&self) -> ProxySnapshot {
        let imp = self.imp();
        ProxySnapshot {
            set_system_proxy: imp.system_proxy_switch.is_active(),
            socks_port: imp.socks_port_row.value() as u16,
            http_port: imp.http_port_row.value() as u16,
            allow_lan: imp.allow_lan_switch.is_active(),
        }
    }

    /// Проверяет дифф между текущим состоянием и сохраненным снимком.
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

    /// Применяет и сохраняет настройки прокси, перезапускает ядро и обновляет системный прокси.
    pub fn apply_changes(&self) {
        let imp = self.imp();
        let current = self.get_current_ui_state();

        let manager = SettingsManager::new();
        let mut s = manager.load();
        s.set_system_proxy = current.set_system_proxy;
        s.socks_port = current.socks_port;
        s.http_port = current.http_port;
        s.allow_lan = current.allow_lan;
        manager.save(&s);

        // Обновляем переменные окружения процесса и системный прокси
        crate::backend::set_process_proxy_env(s.http_port, s.set_system_proxy);
        let result = crate::backend::CoreBackend::update_system_proxy(s.set_system_proxy);

        // Если GSettings недоступен на данном DE
        if s.set_system_proxy {
            if let crate::backend::SystemProxyResult::SchemaUnavailable { desktop } = result {
                if let Some(window) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                    let msg = gettext(format!(
                        "GNOME proxy GSettings scheme is unavailable on {}. Use TUN mode for system-wide routing.",
                        desktop
                    ));
                    let toast = adw::Toast::new(&msg);
                    toast.set_button_label(Some(&gettext("Switch to TUN")));

                    let win_weak = window.downgrade();
                    toast.connect_button_clicked(move |_| {
                        let settings_mgr = SettingsManager::new();
                        let mut app_settings = settings_mgr.load();
                        app_settings.tun_mode = true;
                        settings_mgr.save(&app_settings);

                        let _ = crate::settings::core_restart_channel().0.send_blocking(());

                        if let Some(w) = win_weak.upgrade() {
                            w.add_toast(adw::Toast::new(&gettext(
                                "TUN mode enabled and core restarted.",
                            )));
                        }
                    });

                    window.add_toast(toast);
                }
            }
        }

        *imp.snapshot.borrow_mut() = Some(current);
        *imp.has_changes.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        let toast_text = gettext("Proxy settings applied and core restarted.");
        crate::ui::change_tracker::apply_and_restart_core(
            &toast_text,
            self.root().and_downcast_ref::<gtk::Window>(),
        );
    }

    /// Откатывает виджеты страницы к сохраненному состоянию.
    pub fn discard_changes(&self) {
        let imp = self.imp();
        if let Some(snapshot) = imp.snapshot.borrow().clone() {
            *imp.is_initializing.borrow_mut() = true;

            imp.system_proxy_switch
                .set_active(snapshot.set_system_proxy);
            imp.socks_port_row.set_value(snapshot.socks_port as f64);
            imp.http_port_row.set_value(snapshot.http_port as f64);
            imp.allow_lan_switch.set_active(snapshot.allow_lan);

            *imp.is_initializing.borrow_mut() = false;
            *imp.has_changes.borrow_mut() = false;
            imp.btn_apply.set_visible(false);
        }
    }

    /// Инициализирует значения элементов управления и привязывает обработчики событий.
    fn setup_settings(&self) {
        let imp = self.imp();
        *imp.is_initializing.borrow_mut() = true;

        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.allow_lan_switch
            .set_title(&gettext("Allow connections from LAN"));
        imp.allow_lan_switch.set_subtitle(&gettext(
            "Proxy will be available for other devices in your local network",
        ));

        imp.system_proxy_switch
            .set_active(settings.set_system_proxy);
        imp.socks_port_row.set_value(settings.socks_port as f64);
        imp.http_port_row.set_value(settings.http_port as f64);
        imp.allow_lan_switch.set_active(settings.allow_lan);

        let initial_snapshot = self.get_current_ui_state();
        *imp.snapshot.borrow_mut() = Some(initial_snapshot);
        *imp.has_changes.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        let page_weak_apply = self.downgrade();
        imp.btn_apply.connect_clicked(move |_| {
            if let Some(page) = page_weak_apply.upgrade() {
                page.apply_changes();
            }
        });

        let p_weak1 = self.downgrade();
        imp.system_proxy_switch.connect_active_notify(move |_| {
            if let Some(p) = p_weak1.upgrade() {
                p.check_changes();
            }
        });

        let p_weak2 = self.downgrade();
        imp.socks_port_row.connect_value_notify(move |_| {
            if let Some(p) = p_weak2.upgrade() {
                p.check_changes();
            }
        });

        let p_weak3 = self.downgrade();
        imp.http_port_row.connect_value_notify(move |_| {
            if let Some(p) = p_weak3.upgrade() {
                p.check_changes();
            }
        });

        let p_weak4 = self.downgrade();
        imp.allow_lan_switch.connect_active_notify(move |_| {
            if let Some(p) = p_weak4.upgrade() {
                p.check_changes();
            }
        });

        *imp.is_initializing.borrow_mut() = false;
    }
}
