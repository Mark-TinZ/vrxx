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

//! # Диалог создания и настройки правила маршрутизации (VrxxRuleDialog)
//!
//! Отвечает за:
//! - Декларативное отображение полей ввода правила в стиле GNOME HIG (Libadwaita)
//! - Управление параметрами: описание (опциональное), тип (Domain, IP, SRS URL), значение и действие (Direct, Proxy, Block)
//! - Строгую валидацию введенных данных, защиту от инъекций и динамические контекстные подсказки
//! - Инкапсуляцию пользовательского ввода без императивного создания виджетов в коде

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use std::net::IpAddr;
use std::str::FromStr;

mod imp {
    use super::*;

    /// Структура CompositeTemplate для виджета содержимого диалога правила маршрутизации
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/rule_dialog.ui")]
    pub struct VrxxRuleDialog {
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxRuleDialog {
        const NAME: &'static str = "VrxxRuleDialog";
        type Type = super::VrxxRuleDialog;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::Clamp::static_type();
            adw::PreferencesGroup::static_type();
            adw::ExpanderRow::static_type();
            adw::ActionRow::static_type();
            adw::EntryRow::static_type();
            adw::ComboRow::static_type();
            gtk::Label::static_type();

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
            obj.update_hint();

            // Динамическое обновление подсказки и плейсхолдера при смене типа правила
            self.combo_type.connect_selected_notify(glib::clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_hint();
                }
            ));
        }
    }

    impl WidgetImpl for VrxxRuleDialog {}
    impl BinImpl for VrxxRuleDialog {}
}

glib::wrapper! {
    /// Обертка GObject для виджета параметров правила маршрутизации
    pub struct VrxxRuleDialog(ObjectSubclass<imp::VrxxRuleDialog>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxRuleDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxRuleDialog {
    /// Создает новый экземпляр виджета содержимого диалога правила.
    pub fn new() -> Self {
        glib::Object::builder().build()
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
                imp.lbl_hint.set_text(&gettextrs::gettext(
                    "Matches exact IP address or CIDR subnet (e.g. 192.168.1.50 or 10.0.0.0/24)",
                ));
                imp.entry_value
                    .set_title(&gettextrs::gettext("IP Address or CIDR Subnet"));
            }
            2 => {
                imp.lbl_hint.set_text(&gettextrs::gettext(
                    "Remote binary compiled rule-set URL (e.g. https://example.com/ruleset.srs)",
                ));
                imp.entry_value
                    .set_title(&gettextrs::gettext("Rule-Set URL (.srs)"));
            }
            _ => {
                imp.lbl_hint.set_text(&gettextrs::gettext(
                    "Matches domain and all its subdomains (e.g. example.com matches example.com and sub.example.com; .org matches all .org domains)",
                ));
                imp.entry_value
                    .set_title(&gettextrs::gettext("Domain Name or Suffix"));
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
    }

    /// Проверяет корректность ввода, защищает от инъекций и возвращает ошибку в случае невалидности.
    pub fn validate_input(&self) -> Result<(), String> {
        let val = self.value();

        if val.is_empty() {
            return Err(gettextrs::gettext("Please enter a rule value"));
        }

        // Проверка на опасные управляющие символы и инъекции JSON
        if val.contains('\n')
            || val.contains('\r')
            || val.contains('\t')
            || val.contains('"')
            || val.contains('\\')
            || val.contains('\0')
        {
            return Err(gettextrs::gettext(
                "Value contains forbidden control characters or quotes",
            ));
        }

        let r_type = self.rule_type();
        match r_type.as_str() {
            "domain" => {
                // Домен не должен содержать пробелы, слеши, протоколы или двоеточия
                if val.contains(' ') || val.contains('/') || val.contains(':') || val.contains("..")
                {
                    return Err(gettextrs::gettext(
                        "Invalid domain format. Enter domain name without protocol or path (e.g. example.com or .org)",
                    ));
                }
                // Проверка допустимых символов домена
                let is_valid_domain = val
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
                if !is_valid_domain || val == "." {
                    return Err(gettextrs::gettext(
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
                                return Err(gettextrs::gettext(
                                    "CIDR subnet prefix length exceeds valid range (0-32 for IPv4, 0-128 for IPv6)",
                                ));
                            }
                        } else {
                            return Err(gettextrs::gettext("Invalid CIDR prefix format"));
                        }
                    } else {
                        return Err(gettextrs::gettext("Invalid IP address in CIDR expression"));
                    }
                } else if IpAddr::from_str(&val).is_err() {
                    return Err(gettextrs::gettext(
                        "Invalid IP address format (e.g. 192.168.1.1 or 10.0.0.0/24)",
                    ));
                }
            }
            "srs_url" => {
                if !val.starts_with("http://") && !val.starts_with("https://") {
                    return Err(gettextrs::gettext(
                        "Rule-Set URL must start with http:// or https://",
                    ));
                }
                if val.contains(' ') {
                    return Err(gettextrs::gettext("URL must not contain spaces"));
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
}
