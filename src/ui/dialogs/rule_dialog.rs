/* rule_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Диалоговое окно создания и редактирования правила маршрутизации (VrxxRuleDialog)
//!
//! Отвечает за:
//! - Декларативное отображение параметров правила маршрутизации по стандартам Libadwaita 1.5+ (`AdwDialog`)
//! - Адаптивную компоновку с точками останова (`AdwBreakpoint`) для комфортной работы на экранах от 360px
//! - Четкое центрированное отображение заголовка через `AdwWindowTitle`
//! - Управление параметрами правила: описание, тип (Domain, IP/CIDR, Rule-Set URL), значение и действие
//! - Непрерывную валидацию ввода с защитой от инъекций и выводом контекстных подсказок
//! - Предоставление безопасных точек входа `present_add` и `present_edit`

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use std::net::IpAddr;
use std::str::FromStr;

use crate::ui::models::RoutingRuleObject;

/// Режим работы диалога правила маршрутизации
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleDialogMode {
    /// Режим добавления нового пользовательского правила
    #[default]
    Add,
    /// Режим редактирования существующего правила
    Edit,
}

mod imp {
    use super::*;
    use std::cell::Cell;

    /// Составной шаблон виджета диалога правила маршрутизации
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/dialogs/rule_dialog.ui")]
    pub struct VrxxRuleDialog {
        #[template_child]
        pub dialog_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub btn_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_save: TemplateChild<gtk::Button>,

        #[template_child]
        pub help_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub entry_name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub combo_type: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub entry_value: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub combo_action: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub lbl_hint: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_error: TemplateChild<gtk::Label>,

        pub mode: Cell<RuleDialogMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxRuleDialog {
        const NAME: &'static str = "VrxxRuleDialog";
        type Type = super::VrxxRuleDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            adw::ToolbarView::static_type();
            adw::WindowTitle::static_type();
            adw::Breakpoint::static_type();
            adw::PreferencesPage::static_type();
            adw::PreferencesGroup::static_type();
            adw::ExpanderRow::static_type();
            adw::ActionRow::static_type();
            adw::EntryRow::static_type();
            adw::ComboRow::static_type();
            gtk::Label::static_type();
            gtk::Button::static_type();
            gtk::Image::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxRuleDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_width_request(360);
            obj.set_height_request(360);
            obj.setup_signals();
            obj.update_hint();
        }
    }

    impl WidgetImpl for VrxxRuleDialog {}
    impl adw::subclass::dialog::AdwDialogImpl for VrxxRuleDialog {}
}

glib::wrapper! {
    /// Обертка GObject для модального диалога создания и настройки правил маршрутизации
    pub struct VrxxRuleDialog(ObjectSubclass<imp::VrxxRuleDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxRuleDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxRuleDialog {
    /// Создает новый экземпляр диалога правила маршрутизации.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Устанавливает режим работы диалога (`Add` или `Edit`).
    pub fn set_mode(&self, mode: RuleDialogMode) {
        let imp = self.imp();
        imp.mode.set(mode);

        match mode {
            RuleDialogMode::Add => {
                let title = gettext("Add Custom Rule");
                let subtitle = gettext("Configure domain, IP, or rule-set detour");
                self.set_title(&title);
                imp.dialog_title.set_title(&title);
                imp.dialog_title.set_subtitle(&subtitle);
                imp.btn_save.set_label(&gettext("Add"));
            }
            RuleDialogMode::Edit => {
                let title = gettext("Edit Custom Rule");
                let subtitle = gettext("Modify domain, IP subnet, or detour action");
                self.set_title(&title);
                imp.dialog_title.set_title(&title);
                imp.dialog_title.set_subtitle(&subtitle);
                imp.btn_save.set_label(&gettext("Save"));
            }
        }
        self.revalidate();
    }

    /// Настройка сигналов виджетов диалога (закрытие, автовалидация, подсказки).
    fn setup_signals(&self) {
        let imp = self.imp();

        // 1. Кнопка «Отмена» — закрытие диалога
        let dialog_weak_cancel = self.downgrade();
        imp.btn_cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak_cancel.upgrade() {
                dialog.close();
            }
        });

        // 2. Реактивная валидация ввода значения правила
        let dialog_weak_val = self.downgrade();
        imp.entry_value.connect_changed(move |_| {
            if let Some(dialog) = dialog_weak_val.upgrade() {
                dialog.revalidate();
            }
        });

        // 3. Обновление подсказки и валидации при смене типа правила
        let dialog_weak_type = self.downgrade();
        imp.combo_type.connect_selected_notify(move |_| {
            if let Some(dialog) = dialog_weak_type.upgrade() {
                dialog.update_hint();
                dialog.revalidate();
            }
        });
    }

    /// Выполняет повторную валидацию текущих значений формы и обновляет состояние кнопки подтверждения.
    pub fn revalidate(&self) {
        let imp = self.imp();
        match self.validate_input() {
            Ok(()) => {
                self.set_error(None);
                imp.btn_save.set_sensitive(true);
            }
            Err(e) => {
                if !self.value().is_empty() {
                    self.set_error(Some(&e));
                } else {
                    self.set_error(None);
                }
                imp.btn_save.set_sensitive(false);
            }
        }
    }

    /// Возвращает введенное описание правила (может быть пустым).
    pub fn name(&self) -> String {
        self.imp().entry_name.text().trim().to_string()
    }

    /// Возвращает строковый идентификатор типа правила ("domain", "ip", "srs_url").
    pub fn rule_type(&self) -> String {
        match self.imp().combo_type.selected() {
            1 => "ip".to_string(),
            2 => "srs_url".to_string(),
            _ => "domain".to_string(),
        }
    }

    /// Возвращает введенное значение правила (домен, CIDR или URL).
    pub fn value(&self) -> String {
        self.imp().entry_value.text().trim().to_string()
    }

    /// Возвращает выбранное действие маршрутизации ("direct", "proxy", "block").
    pub fn action(&self) -> String {
        match self.imp().combo_action.selected() {
            0 => "direct".to_string(),
            1 => "proxy".to_string(),
            _ => "block".to_string(),
        }
    }

    /// Обновляет контекстную подсказку и плейсхолдер в зависимости от выбранного типа правила.
    pub fn update_hint(&self) {
        let imp = self.imp();
        match imp.combo_type.selected() {
            1 => {
                imp.lbl_hint.set_text(&gettext(
                    "Matches exact IP address or CIDR subnet (e.g. 192.168.1.50 or 10.0.0.0/24)",
                ));
                imp.entry_value
                    .set_title(&gettext("IP Address or CIDR Subnet"));
            }
            2 => {
                imp.lbl_hint.set_text(&gettext(
                    "Remote binary compiled rule-set URL (e.g. https://example.com/ruleset.srs)",
                ));
                imp.entry_value.set_title(&gettext("Rule-Set URL (.srs)"));
            }
            _ => {
                imp.lbl_hint.set_text(&gettext(
                    "Matches domain and all its subdomains (e.g. example.com matches example.com and sub.example.com; .org matches all .org domains)",
                ));
                imp.entry_value.set_title(&gettext("Domain Name or Suffix"));
            }
        }
    }

    /// Устанавливает начальные значения полей правила при редактировании.
    pub fn set_rule(&self, name: &str, rule_type: &str, value: &str, action: &str) {
        let imp = self.imp();
        imp.entry_name.set_text(name);
        imp.entry_value.set_text(value);

        let type_idx = match rule_type {
            "ip" => 1,
            "srs_url" => 2,
            _ => 0,
        };
        imp.combo_type.set_selected(type_idx);

        let act_idx = match action {
            "direct" => 0,
            "block" => 2,
            _ => 1, // proxy
        };
        imp.combo_action.set_selected(act_idx);
        self.update_hint();
        self.revalidate();
    }

    /// Очищает введенные поля диалога.
    pub fn clear(&self) {
        let imp = self.imp();
        imp.entry_name.set_text("");
        imp.entry_value.set_text("");
        imp.combo_type.set_selected(0);
        imp.combo_action.set_selected(0);
        self.update_hint();
        self.set_error(None);
        self.revalidate();
    }

    /// Проверяет корректность ввода, защищает от инъекций и возвращает ошибку в случае невалидности.
    pub fn validate_input(&self) -> Result<(), String> {
        let val = self.value();

        if val.is_empty() {
            return Err(gettext("Please enter a rule value"));
        }

        // Проверка на опасные управляющие символы и инъекции JSON
        if val.contains('\n')
            || val.contains('\r')
            || val.contains('\t')
            || val.contains('"')
            || val.contains('\\')
            || val.contains('\0')
        {
            return Err(gettext(
                "Value contains forbidden control characters or quotes",
            ));
        }

        let r_type = self.rule_type();
        match r_type.as_str() {
            "domain" => {
                // Домен не должен содержать пробелы, слеши, протоколы или двоеточия
                if val.contains(' ') || val.contains('/') || val.contains(':') || val.contains("..")
                {
                    return Err(gettext(
                        "Invalid domain format. Enter domain name without protocol or path (e.g. example.com or .org)",
                    ));
                }
                // Проверка допустимых символов домена
                let is_valid_domain = val
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
                if !is_valid_domain || val == "." {
                    return Err(gettext(
                        "Domain contains invalid characters. Use letters, numbers, dots and hyphens",
                    ));
                }
            }
            "ip" => {
                // Проверка IP или CIDR
                if let Some((ip_part, mask_part)) = val.split_once('/') {
                    if let Ok(ip) = IpAddr::from_str(ip_part) {
                        if let Ok(mask) = mask_part.parse::<u8>() {
                            let max_mask = if ip.is_ipv4() { 32 } else { 128 };
                            if mask > max_mask {
                                return Err(gettext(
                                    "CIDR subnet prefix length exceeds valid range (0-32 for IPv4, 0-128 for IPv6)",
                                ));
                            }
                        } else {
                            return Err(gettext("Invalid CIDR prefix format"));
                        }
                    } else {
                        return Err(gettext("Invalid IP address in CIDR expression"));
                    }
                } else if IpAddr::from_str(&val).is_err() {
                    return Err(gettext(
                        "Invalid IP address format (e.g. 192.168.1.1 or 10.0.0.0/24)",
                    ));
                }
            }
            "srs_url" => {
                if !val.starts_with("http://") && !val.starts_with("https://") {
                    return Err(gettext("Rule-Set URL must start with http:// or https://"));
                }
                if val.contains(' ') {
                    return Err(gettext("URL must not contain spaces"));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Отображает или скрывает сообщение об ошибке валидации.
    pub fn set_error(&self, msg: Option<&str>) {
        let imp = self.imp();
        if let Some(error) = msg {
            imp.lbl_error.set_text(error);
            imp.lbl_error.set_visible(true);
        } else {
            imp.lbl_error.set_text("");
            imp.lbl_error.set_visible(false);
        }
    }

    /// Отображает модальный диалог создания нового правила маршрутизации (`Add`).
    pub fn present_add<F>(parent: &impl IsA<gtk::Widget>, on_add: F)
    where
        F: Fn(RoutingRuleObject) + 'static,
    {
        let dialog = Self::new();
        dialog.set_mode(RuleDialogMode::Add);

        let dialog_weak = dialog.downgrade();
        dialog.imp().btn_save.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                if d.validate_input().is_ok() {
                    let name = d.name();
                    let val = d.value();
                    let r_type = d.rule_type();
                    let act = d.action();

                    let obj = RoutingRuleObject::new(&name, &r_type, &val, &act);
                    on_add(obj);
                    d.close();
                }
            }
        });

        dialog.present(Some(parent));
        dialog.imp().entry_value.grab_focus();
    }

    /// Отображает модальный диалог редактирования существующего правила маршрутизации (`Edit`).
    pub fn present_edit<F>(parent: &impl IsA<gtk::Widget>, obj: &RoutingRuleObject, on_save: F)
    where
        F: Fn(String, String, String, String) + 'static,
    {
        let dialog = Self::new();
        dialog.set_mode(RuleDialogMode::Edit);
        dialog.set_rule(&obj.name(), &obj.rule_type(), &obj.value(), &obj.action());

        let dialog_weak = dialog.downgrade();
        dialog.imp().btn_save.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                if d.validate_input().is_ok() {
                    let name = d.name();
                    let val = d.value();
                    let r_type = d.rule_type();
                    let act = d.action();

                    on_save(name, r_type, val, act);
                    d.close();
                }
            }
        });

        dialog.present(Some(parent));
        dialog.imp().entry_value.grab_focus();
    }
}
