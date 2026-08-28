/* routing_rule_row.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Компонент строки пользовательского правила маршрутизации (VrxxRoutingRuleRow)
//!
//! Отвечает за:
//! - Отображение элемента списка пользовательских правил (`AdwActionRow`)
//! - Динамическую установку иконки в зависимости от типа правила (домен, IP-подсеть, SRS URL)
//! - Форматирование заголовка и информативного подзаголовка
//! - Обработку сигналов редактирования (`request-edit`) и удаления (`request-delete`)

use crate::ui::models::RoutingRuleObject;
use adw::prelude::*;
use gtk::{glib, subclass::prelude::*, CompositeTemplate};
use std::cell::RefCell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    /// Структура CompositeTemplate для виджета VrxxRoutingRuleRow
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/routing_rule_row.ui")]
    pub struct VrxxRoutingRuleRow {
        #[template_child]
        pub action_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub icon_type: TemplateChild<gtk::Image>,
        #[template_child]
        pub btn_edit: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_delete: TemplateChild<gtk::Button>,

        pub item: RefCell<Option<RoutingRuleObject>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxRoutingRuleRow {
        const NAME: &'static str = "VrxxRoutingRuleRow";
        type Type = super::VrxxRoutingRuleRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            adw::ActionRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxRoutingRuleRow {
        /// Регистрация кастомных сигналов GObject для взаимодействия с родительской страницей
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("request-edit").build(),
                    glib::subclass::Signal::builder("request-delete").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_callbacks();
        }
    }

    impl WidgetImpl for VrxxRoutingRuleRow {}
    impl ListBoxRowImpl for VrxxRoutingRuleRow {}
}

glib::wrapper! {
    /// Обертка GObject для строки пользовательского правила маршрутизации
    pub struct VrxxRoutingRuleRow(ObjectSubclass<imp::VrxxRoutingRuleRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl Default for VrxxRoutingRuleRow {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxRoutingRuleRow {
    /// Создает новый экземпляр строки пользовательского правила.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Настраивает обработчики кликов на кнопки редактирования и удаления.
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let row_weak_edit = self.downgrade();
        imp.btn_edit.connect_clicked(move |_| {
            if let Some(row) = row_weak_edit.upgrade() {
                row.emit_by_name::<()>("request-edit", &[]);
            }
        });

        let row_weak_delete = self.downgrade();
        imp.btn_delete.connect_clicked(move |_| {
            if let Some(row) = row_weak_delete.upgrade() {
                row.emit_by_name::<()>("request-delete", &[]);
            }
        });
    }

    /// Привязывает модель данных [`RoutingRuleObject`] к элементам отображения строки.
    pub fn bind(&self, item: &RoutingRuleObject) {
        let imp = self.imp();
        imp.item.replace(Some(item.clone()));

        self.update_display(item);

        // Отслеживание динамических изменений полей объекта
        let row_weak = self.downgrade();
        item.connect_notify_local(None, move |item, _| {
            if let Some(row) = row_weak.upgrade() {
                if let Some(obj) = item.downcast_ref::<RoutingRuleObject>() {
                    row.update_display(obj);
                }
            }
        });
    }

    /// Возвращает связанный объект модели [`RoutingRuleObject`].
    pub fn item(&self) -> Option<RoutingRuleObject> {
        self.imp().item.borrow().clone()
    }

    /// Обновляет визуальные элементы строки на основе переданного объекта.
    fn update_display(&self, item: &RoutingRuleObject) {
        let imp = self.imp();

        let icon_name = match item.rule_type().as_str() {
            "ip" => "network-wired-symbolic",
            "srs_url" => "folder-download-symbolic",
            _ => "web-browser-symbolic",
        };
        imp.icon_type.set_icon_name(Some(icon_name));

        let action_str = match item.action().as_str() {
            "direct" => gettextrs::gettext("DIRECT"),
            "block" => gettextrs::gettext("BLOCK"),
            _ => gettextrs::gettext("PROXY"),
        };

        let type_str = match item.rule_type().as_str() {
            "ip" => gettextrs::gettext("IP"),
            "srs_url" => gettextrs::gettext("RULE-SET"),
            _ => gettextrs::gettext("DOMAIN"),
        };

        let name = item.name();
        let val = item.value();

        if name.trim().is_empty() {
            imp.action_row.set_title(&val);
            imp.action_row
                .set_subtitle(&format!("{} ➔ {}", type_str, action_str));
        } else {
            imp.action_row.set_title(&name);
            imp.action_row
                .set_subtitle(&format!("{} • {} ➔ {}", type_str, val, action_str));
        }
    }
}
