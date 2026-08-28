/* vpn_key_row.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Виджет строки VPN-ключа (VrxxVpnKeyRow)
//!
//! Отвечает за:
//! - Отображение элемента списка VPN-профилей (`AdwActionRow`)
//! - Управление индикатором статуса подключения (Стек иконок: Inactive / Active / Loading / Error)
//! - Раскрывающийся блок деталей (`details_revealer`): входящий/исходящий трафик, время работы, замер пинга
//! - Обработку сигналов контекстного меню (Edit, Delete, Share, Copy Link, Ping, Activate)

use crate::ui::models::VpnKeyObject;
use adw::prelude::*;
use gtk::{gio, glib, subclass::prelude::*, CompositeTemplate};

mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::sync::OnceLock;

    /// Структура CompositeTemplate для VrxxVpnKeyRow
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/vpn_key_row.ui")]
    pub struct VrxxVpnKeyRow {
        #[template_child]
        pub header_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub icon_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub details_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub lbl_down: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_up: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_time: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_ping: TemplateChild<gtk::Label>,
        #[template_child]
        pub btn_refresh_ping: TemplateChild<gtk::Button>,

        pub item: RefCell<Option<VpnKeyObject>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxVpnKeyRow {
        const NAME: &'static str = "VrxxVpnKeyRow";
        type Type = super::VrxxVpnKeyRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxVpnKeyRow {
        /// Регистрация кастомных сигналов GObject для взаимодействия с родительской страницей
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("request-edit").build(),
                    glib::subclass::Signal::builder("request-info").build(),
                    glib::subclass::Signal::builder("request-delete").build(),
                    glib::subclass::Signal::builder("request-copy-link").build(),
                    glib::subclass::Signal::builder("request-copy-json").build(),
                    glib::subclass::Signal::builder("request-qr-code").build(),
                    glib::subclass::Signal::builder("request-share").build(),
                    glib::subclass::Signal::builder("request-ping").build(),
                    glib::subclass::Signal::builder("request-activate").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
        }
    }
    impl WidgetImpl for VrxxVpnKeyRow {}
    impl ListBoxRowImpl for VrxxVpnKeyRow {}
}

glib::wrapper! {
    /// Обертка GObject для строки VPN-ключа
    pub struct VrxxVpnKeyRow(ObjectSubclass<imp::VrxxVpnKeyRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl Default for VrxxVpnKeyRow {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxVpnKeyRow {
    /// Создает новый экземпляр строки VPN-ключа.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Привязывает реактивную GObject модель [`VpnKeyObject`] к виджетам строки.
    pub fn bind(&self, item: &VpnKeyObject) {
        let imp = self.imp();
        imp.item.replace(Some(item.clone()));

        // Привязка заголовка
        item.bind_property("name", &*imp.header_row, "title")
            .sync_create()
            .build();

        let row_weak_sub = self.downgrade();
        let update_sub = move |item: &VpnKeyObject| {
            if let Some(r) = row_weak_sub.upgrade() {
                r.update_subtitle(item);
            }
        };

        let u_clone1 = update_sub.clone();
        item.connect_protocol_notify(move |item| u_clone1(item));
        let u_clone2 = update_sub.clone();
        item.connect_server_info_notify(move |item| u_clone2(item));
        let u_clone3 = update_sub.clone();
        item.connect_hide_ip_notify(move |item| u_clone3(item));
        let u_clone4 = update_sub;
        item.connect_url_notify(move |item| u_clone4(item));

        self.update_subtitle(item);

        // Привязка телеметрии трафика
        item.bind_property("traffic-down", &*imp.lbl_down, "label")
            .sync_create()
            .build();
        item.bind_property("traffic-up", &*imp.lbl_up, "label")
            .sync_create()
            .build();
        item.bind_property("time-connected", &*imp.lbl_time, "label")
            .sync_create()
            .build();
        item.bind_property("ping", &*imp.lbl_ping, "label")
            .sync_create()
            .build();

        // Отслеживание изменений статуса активности ключа
        let row_weak = self.downgrade();
        item.connect_is_active_notify(move |item| {
            let row = match row_weak.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        let row_weak_loading = self.downgrade();
        item.connect_is_loading_notify(move |item| {
            let row = match row_weak_loading.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        let row_weak_error = self.downgrade();
        item.connect_is_error_notify(move |item| {
            let row = match row_weak_error.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        self.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
    }

    /// Возвращает привязанный GObject элемент данных.
    pub fn item(&self) -> Option<VpnKeyObject> {
        self.imp().item.borrow().clone()
    }

    /// Подключение сигналов клика по строке и кнопки замера пинга.
    fn setup_callbacks(&self) {
        let row_weak = self.downgrade();
        self.imp().header_row.set_activatable(true);
        self.imp().header_row.connect_activated(move |_| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-activate", &[]);
            }
        });

        let row_weak_ping = self.downgrade();
        self.imp().btn_refresh_ping.connect_clicked(move |_| {
            if let Some(row) = row_weak_ping.upgrade() {
                row.emit_by_name::<()>("request-ping", &[]);
            }
        });
    }

    /// Обновляет подзаголовок строки: выводит протокол и хост/IP (с учетом режима стримера).
    fn update_subtitle(&self, item: &VpnKeyObject) {
        let proto = item.protocol();
        let subtitle = if item.hide_ip() {
            format!("{proto} • ***.***.***.***")
        } else {
            let s_info = item.server_info();
            if !s_info.is_empty() && s_info != "0.0.0.0" {
                format!("{proto} • {s_info}")
            } else if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&item.url()) {
                format!("{proto} • {}:{}", parsed.host, parsed.port)
            } else {
                proto
            }
        };
        self.imp().header_row.set_subtitle(&subtitle);
    }

    /// Обновляет визуальное состояние строки: разворачивание блока деталей и иконку статуса.
    fn update_visual_state(&self, is_active: bool, is_loading: bool, is_error: bool) {
        let imp = self.imp();
        imp.details_revealer.set_reveal_child(is_active);

        if is_loading {
            imp.icon_stack.set_visible_child_name("loading");
        } else if is_error {
            imp.icon_stack.set_visible_child_name("error");
        } else if is_active {
            imp.icon_stack.set_visible_child_name("active");
        } else {
            imp.icon_stack.set_visible_child_name("inactive");
        }
    }

    /// Настройка контекстных действий строки (меню действий с профилем).
    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();

        // Действие: Информация о сервере
        let info_action = gio::SimpleAction::new("key_info", None);
        let row_weak_info = self.downgrade();
        info_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_info.upgrade() {
                row.emit_by_name::<()>("request-info", &[]);
            }
        });
        action_group.add_action(&info_action);

        // Действие: Удалить профиль
        let delete_action = gio::SimpleAction::new("delete", None);
        let row_weak = self.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-delete", &[]);
            }
        });
        action_group.add_action(&delete_action);

        // Действие: Редактировать имя профиля
        let edit_action = gio::SimpleAction::new("key_edit", None);
        let row_weak = self.downgrade();
        edit_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-edit", &[]);
            }
        });
        action_group.add_action(&edit_action);

        // Действие: Скопировать ссылку
        let copy_link_action = gio::SimpleAction::new("key_copy_link", None);
        let row_weak = self.downgrade();
        copy_link_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-copy-link", &[]);
            }
        });
        action_group.add_action(&copy_link_action);

        // Действие: Скопировать JSON
        let copy_json_action = gio::SimpleAction::new("key_copy_json", None);
        let row_weak = self.downgrade();
        copy_json_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-copy-json", &[]);
            }
        });
        action_group.add_action(&copy_json_action);

        // Действие: QR-код
        let qr_code_action = gio::SimpleAction::new("key_qr_code", None);
        let row_weak_qr = self.downgrade();
        qr_code_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_qr.upgrade() {
                row.emit_by_name::<()>("request-qr-code", &[]);
            }
        });
        action_group.add_action(&qr_code_action);

        // Действие: Поделиться (открытие единого диалога экспорта и QR)
        let share_action = gio::SimpleAction::new("key_share", None);
        let row_weak_share = self.downgrade();
        share_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_share.upgrade() {
                row.emit_by_name::<()>("request-share", &[]);
            }
        });
        action_group.add_action(&share_action);

        self.insert_action_group("row", Some(&action_group));
    }
}
