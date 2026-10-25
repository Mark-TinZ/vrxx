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
        pub lbl_location: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_row_ping: TemplateChild<gtk::Label>,

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
                    glib::subclass::Signal::builder("request-move-group").build(),
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

        // Привязка локации сервера с учетом режима стримера
        let row_weak_loc = self.downgrade();
        let update_loc = move |item: &VpnKeyObject| {
            if let Some(r) = row_weak_loc.upgrade() {
                r.update_location_label(item);
            }
        };

        let loc_clone1 = update_loc.clone();
        item.connect_location_notify(move |item| loc_clone1(item));
        let loc_clone2 = update_loc;
        item.connect_hide_ip_notify(move |item| loc_clone2(item));

        self.update_location_label(item);

        // Отслеживание изменений задержки для отображения пинга в заголовке строки (GNOME HIG)
        let row_weak_ping = self.downgrade();
        item.connect_ping_notify(move |item| {
            if let Some(r) = row_weak_ping.upgrade() {
                r.update_row_ping(&item.ping());
            }
        });
        self.update_row_ping(&item.ping());

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
        // 1. Активация строки через клавиатуру (Enter / Пробел)
        let row_weak_act = self.downgrade();
        self.connect_activate(move |_| {
            if let Some(row) = row_weak_act.upgrade() {
                row.emit_by_name::<()>("request-activate", &[]);
            }
        });

        // 2. Активация кликом мыши по заголовку строки
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        let row_weak_click = self.downgrade();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            // Если клик пришелся на контекстное меню (GtkMenuButton) или его дочерние элементы,
            // не активируем профиль, давая меню открыться
            if let Some(widget) = gesture.widget() {
                if let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT) {
                    if picked.ancestor(gtk::MenuButton::static_type()).is_some() {
                        gesture.set_state(gtk::EventSequenceState::Denied);
                        return;
                    }
                }
            }

            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(row) = row_weak_click.upgrade() {
                row.emit_by_name::<()>("request-activate", &[]);
            }
        });
        self.imp().header_row.add_controller(click);

        // 3. Поддержка штатного activated AdwActionRow для стилей ховера и совместимости
        self.imp().header_row.set_activatable(true);
        let row_weak_hdr = self.downgrade();
        self.imp().header_row.connect_activated(move |_| {
            if let Some(row) = row_weak_hdr.upgrade() {
                row.emit_by_name::<()>("request-activate", &[]);
            }
        });

    }

    /// Обновляет отображение локации сервера с учетом маскирования в режиме стримера.
    fn update_location_label(&self, item: &VpnKeyObject) {
        let text = if item.hide_ip() {
            "***".to_string()
        } else {
            let loc = item.location();
            if loc.trim().is_empty() {
                "Unknown".to_string()
            } else {
                loc
            }
        };
        self.imp().lbl_location.set_label(&text);
    }

    /// Обновляет подзаголовок строки: выводит только наименование протокола по стандартам GNOME HIG.
    fn update_subtitle(&self, item: &VpnKeyObject) {
        let proto = item.protocol();
        self.imp().header_row.set_subtitle(&proto);
    }

    /// Обновляет отображение индикатора задержки (пинга) в заголовке строки по стандарту GNOME HIG.
    fn update_row_ping(&self, ping: &str) {
        let imp = self.imp();
        let trimmed = ping.trim();

        if trimmed.is_empty() || trimmed == "0 ms" || trimmed == "-" {
            imp.lbl_row_ping.set_visible(false);
            return;
        }

        // Очищаем предыдущие цветовые классы
        imp.lbl_row_ping.remove_css_class("success");
        imp.lbl_row_ping.remove_css_class("warning");
        imp.lbl_row_ping.remove_css_class("error");
        imp.lbl_row_ping.remove_css_class("dim-label");

        // Применяем семантические цвета по правилам GNOME HIG
        if let Some(ms_str) = trimmed.strip_suffix(" ms") {
            if let Ok(ms) = ms_str.trim().parse::<u64>() {
                if ms < 150 {
                    imp.lbl_row_ping.add_css_class("success");
                } else if ms < 300 {
                    imp.lbl_row_ping.add_css_class("warning");
                } else {
                    imp.lbl_row_ping.add_css_class("error");
                }
            } else {
                imp.lbl_row_ping.add_css_class("dim-label");
            }
        } else if trimmed.eq_ignore_ascii_case("timeout")
            || trimmed.eq_ignore_ascii_case("error")
            || trimmed.contains("error")
            || trimmed.contains("ошибка")
            || trimmed.contains("таймаут")
        {
            imp.lbl_row_ping.add_css_class("error");
        } else {
            imp.lbl_row_ping.add_css_class("dim-label");
        }

        imp.lbl_row_ping.set_label(trimmed);
        imp.lbl_row_ping.set_visible(true);
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

        // Действие: Замерить пинг профиля
        let ping_action = gio::SimpleAction::new("key_ping", None);
        let row_weak_ping = self.downgrade();
        ping_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_ping.upgrade() {
                row.emit_by_name::<()>("request-ping", &[]);
            }
        });
        action_group.add_action(&ping_action);

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

        // Действие: Переместить в группу
        let move_group_action = gio::SimpleAction::new("key_move_group", None);
        let row_weak_move = self.downgrade();
        move_group_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_move.upgrade() {
                row.emit_by_name::<()>("request-move-group", &[]);
            }
        });
        action_group.add_action(&move_group_action);

        self.insert_action_group("row", Some(&action_group));
    }
}
