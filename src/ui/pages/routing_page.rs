/* routing_page.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Страница настройки маршрутизации сетевого трафика (VrxxRoutingPage)
//!
//! Предоставляет графический интерфейс для:
//! - Включения/отключения пользовательских правил маршрутизации
//! - Выбора глобального режима (Прямой обход / Проксирование через VPN)
//! - Настройки стратегии разрешения доменных имен (Domain Strategy)
//! - Управления списком пользовательских правил (`VrxxRoutingRuleRow`) с редактированием и удалением
//! - Настройки региональных правил (Россия, Китай, Иран, реестр Антизапрет) по протоколам SRS
//! - Отслеживания диффа несохраненных изменений (Snapshot Diff Tracking)
//! - Управления принудительным отключением протокола IPv6

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use crate::settings::{RoutingRule, SettingsManager};
use crate::ui::components::VrxxRoutingRuleRow;
use crate::ui::models::RoutingRuleObject;
use crate::ui::rule_dialog::VrxxRuleDialog;
use crate::ui::setup_primary_menu;

/// Снимок сохраненного состояния страницы маршрутизации для отслеживания диффа изменений
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingSnapshot {
    enable_routing: bool,
    routing_mode: u32,
    domain_strategy: u32,
    disable_ipv6: bool,
    route_ru_sites: bool,
    route_ru_ips: bool,
    route_cn_sites: bool,
    route_cn_ips: bool,
    route_ir_sites: bool,
    route_ir_ips: bool,
    route_antifilter: bool,
    rules: Vec<RoutingRule>,
}

mod imp {
    use super::*;

    /// Структура CompositeTemplate для страницы маршрутизации VrxxRoutingPage
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/routing_page.ui")]
    pub struct VrxxRoutingPage {
        #[template_child]
        pub btn_apply: TemplateChild<gtk::Button>,
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub enable_routing_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub mode_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub domain_strategy_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub add_rule_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub rules_list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub disable_ipv6_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub regional_rules_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub route_ru_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub route_ru_sites_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_ru_ips_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_cn_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub route_cn_sites_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_cn_ips_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_ir_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub route_ir_sites_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_ir_ips_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_antifilter_expander: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub route_antifilter_row: TemplateChild<adw::SwitchRow>,

        pub model: RefCell<Option<gio::ListStore>>,
        pub snapshot: RefCell<Option<RoutingSnapshot>>,
        pub has_changes: RefCell<bool>,
        pub is_initializing: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxRoutingPage {
        const NAME: &'static str = "VrxxRoutingPage";
        type Type = super::VrxxRoutingPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ActionRow::static_type();
            adw::ComboRow::static_type();
            adw::SwitchRow::static_type();
            adw::ExpanderRow::static_type();
            adw::PreferencesGroup::static_type();
            VrxxRoutingRuleRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxRoutingPage {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_menu();
            obj.setup_rules_list();
            obj.setup_settings();
            obj.update_prr_timestamp();
        }
    }

    impl WidgetImpl for VrxxRoutingPage {}
    impl BinImpl for VrxxRoutingPage {}
}

glib::wrapper! {
    /// Обертка GObject для страницы маршрутизации VrxxRoutingPage
    pub struct VrxxRoutingPage(ObjectSubclass<imp::VrxxRoutingPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxRoutingPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxRoutingPage {
    /// Создает новый экземпляр страницы параметров маршрутизации.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Инициализирует контекстное меню и переключатель темы через `setup_primary_menu`.
    fn setup_menu(&self) {
        setup_primary_menu(&self.imp().primary_menu_btn);
    }

    /// Возвращает флаг наличия несохраненных изменений.
    pub fn has_changes(&self) -> bool {
        *self.imp().has_changes.borrow()
    }

    /// Извлекает текущее состояние элементов интерфейса в структуру `RoutingSnapshot`.
    fn get_current_ui_state(&self) -> RoutingSnapshot {
        let imp = self.imp();
        let mut current_rules = Vec::new();
        if let Some(store) = imp.model.borrow().clone() {
            for i in 0..store.n_items() {
                if let Some(obj) = store.item(i).and_downcast::<RoutingRuleObject>() {
                    current_rules.push(RoutingRule {
                        name: obj.name(),
                        type_: obj.rule_type(),
                        value: obj.value(),
                        action: obj.action(),
                    });
                }
            }
        }

        RoutingSnapshot {
            enable_routing: imp.enable_routing_row.is_active(),
            routing_mode: imp.mode_row.selected(),
            domain_strategy: imp.domain_strategy_row.selected(),
            disable_ipv6: imp.disable_ipv6_row.is_active(),
            route_ru_sites: imp.route_ru_sites_row.is_active(),
            route_ru_ips: imp.route_ru_ips_row.is_active(),
            route_cn_sites: imp.route_cn_sites_row.is_active(),
            route_cn_ips: imp.route_cn_ips_row.is_active(),
            route_ir_sites: imp.route_ir_sites_row.is_active(),
            route_ir_ips: imp.route_ir_ips_row.is_active(),
            route_antifilter: imp.route_antifilter_row.is_active(),
            rules: current_rules,
        }
    }

    /// Проверяет разницу (дифф) между текущим состоянием UI и сохраненным снимком.
    /// Если различий нет — автоматически скрывает кнопку «Применить».
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

    /// Применяет и сохраняет изменения в постоянное хранилище и перезапускает ядро VPN.
    pub fn apply_changes(&self) {
        let imp = self.imp();
        let current = self.get_current_ui_state();

        let manager = SettingsManager::new();
        let mut s = manager.load();

        s.enable_routing = current.enable_routing;
        s.routing_mode = match current.routing_mode {
            1 => "proxy".to_string(),
            _ => "bypass".to_string(),
        };
        s.domain_strategy = match current.domain_strategy {
            1 => "IPIfNonMatch".to_string(),
            2 => "IPOnDemand".to_string(),
            _ => "AsIs".to_string(),
        };
        s.disable_ipv6 = current.disable_ipv6;
        s.route_ru_sites = current.route_ru_sites;
        s.route_ru_ips = current.route_ru_ips;
        s.route_ru = current.route_ru_sites || current.route_ru_ips;

        s.route_cn_sites = current.route_cn_sites;
        s.route_cn_ips = current.route_cn_ips;
        s.route_cn = current.route_cn_sites || current.route_cn_ips;

        s.route_ir_sites = current.route_ir_sites;
        s.route_ir_ips = current.route_ir_ips;
        s.route_ir = current.route_ir_sites || current.route_ir_ips;

        s.route_antifilter = current.route_antifilter;
        s.routing_rules = current.rules.clone();

        manager.save(&s);

        // Обновление снимка сохраненного состояния
        *imp.snapshot.borrow_mut() = Some(current);
        *imp.has_changes.borrow_mut() = false;
        imp.btn_apply.set_visible(false);

        // Перезапуск ядра VPN через единый модуль
        let toast_text = gettextrs::gettext("Routing settings applied and core restarted.");
        crate::ui::change_tracker::apply_and_restart_core(
            &toast_text,
            self.root().and_downcast_ref::<gtk::Window>(),
        );
    }

    /// Отменяет все несохраненные изменения и возвращает значения виджетов к сохраненному снимку.
    pub fn discard_changes(&self) {
        let imp = self.imp();
        if let Some(snapshot) = imp.snapshot.borrow().clone() {
            *imp.is_initializing.borrow_mut() = true;

            imp.enable_routing_row.set_active(snapshot.enable_routing);
            imp.mode_row.set_selected(snapshot.routing_mode);
            imp.domain_strategy_row
                .set_selected(snapshot.domain_strategy);
            imp.disable_ipv6_row.set_active(snapshot.disable_ipv6);

            imp.route_ru_sites_row.set_active(snapshot.route_ru_sites);
            imp.route_ru_ips_row.set_active(snapshot.route_ru_ips);
            imp.route_cn_sites_row.set_active(snapshot.route_cn_sites);
            imp.route_cn_ips_row.set_active(snapshot.route_cn_ips);
            imp.route_ir_sites_row.set_active(snapshot.route_ir_sites);
            imp.route_ir_ips_row.set_active(snapshot.route_ir_ips);
            imp.route_antifilter_row
                .set_active(snapshot.route_antifilter);

            // Восстановление списка правил в модели
            if let Some(store) = imp.model.borrow().clone() {
                store.remove_all();
                for rule in &snapshot.rules {
                    let obj =
                        RoutingRuleObject::new(&rule.name, &rule.type_, &rule.value, &rule.action);
                    store.append(&obj);
                }
            }

            *imp.is_initializing.borrow_mut() = false;
            *imp.has_changes.borrow_mut() = false;
            imp.btn_apply.set_visible(false);
        }
    }

    /// Настраивает список пользовательских правил и фабрику строк `VrxxRoutingRuleRow`.
    fn setup_rules_list(&self) {
        let imp = self.imp();
        let store = gio::ListStore::new::<RoutingRuleObject>();
        imp.model.replace(Some(store.clone()));

        let selection_model = gtk::NoSelection::new(Some(store));

        // Компактный плейсхолдер для пустого списка правил
        let empty_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .build();

        let empty_icon = gtk::Image::builder()
            .icon_name("funnel-symbolic")
            .pixel_size(28)
            .css_classes(["dim-label"])
            .build();

        let empty_title = gtk::Label::builder()
            .label(gettextrs::gettext("No Custom Rules Added"))
            .css_classes(["heading", "dim-label"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();

        let empty_subtitle = gtk::Label::builder()
            .label(gettextrs::gettext(
                "Traffic will follow the default global routing mode",
            ))
            .css_classes(["caption", "dim-label"])
            .wrap(true)
            .max_width_chars(36)
            .justify(gtk::Justification::Center)
            .build();

        empty_box.append(&empty_icon);
        empty_box.append(&empty_title);
        empty_box.append(&empty_subtitle);

        imp.rules_list.set_placeholder(Some(&empty_box));

        let page_weak = self.downgrade();
        imp.rules_list
            .bind_model(Some(&selection_model), move |item| {
                if let Some(obj) = item.downcast_ref::<RoutingRuleObject>() {
                    let row = VrxxRoutingRuleRow::new();
                    row.bind(obj);

                    // Обработчик удаления правила
                    let page_weak_del = page_weak.clone();
                    let obj_del = obj.clone();
                    row.connect_local("request-delete", false, move |_| {
                        if let Some(page) = page_weak_del.upgrade() {
                            if let Some(store) = page.imp().model.borrow().clone() {
                                if let Some(pos) = store.find(&obj_del) {
                                    store.remove(pos);
                                    page.check_changes();
                                }
                            }
                        }
                        None
                    });

                    // Обработчик редактирования правила
                    let page_weak_edit = page_weak.clone();
                    let obj_edit = obj.clone();
                    row.connect_local("request-edit", false, move |_| {
                        if let Some(page) = page_weak_edit.upgrade() {
                            page.show_edit_rule_dialog(&obj_edit);
                        }
                        None
                    });

                    row.upcast::<gtk::Widget>()
                } else {
                    gtk::Box::new(gtk::Orientation::Horizontal, 0).upcast::<gtk::Widget>()
                }
            });

        // Открытие диалога добавления правила
        let page_weak_add = self.downgrade();
        imp.add_rule_row.connect_activated(move |_| {
            if let Some(page) = page_weak_add.upgrade() {
                page.show_add_rule_dialog();
            }
        });
    }

    /// Отображает модальный диалог добавления нового пользовательского правила.
    fn show_add_rule_dialog(&self) {
        if let Some(window) = self.root().and_downcast::<gtk::Window>() {
            let dialog = adw::AlertDialog::builder()
                .heading(gettextrs::gettext("Add Custom Rule"))
                .body(gettextrs::gettext(
                    "Configure domain detour, IP subnet, or remote rule-set",
                ))
                .build();

            dialog.add_response("cancel", &gettextrs::gettext("Cancel"));
            dialog.add_response("add", &gettextrs::gettext("Add"));
            dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("add"));
            dialog.set_close_response("cancel");

            let content = VrxxRuleDialog::new();
            dialog.set_extra_child(Some(&content));

            // По умолчанию кнопка добавления неактивна до ввода валидного значения
            dialog.set_response_enabled("add", false);

            let content_weak = glib::SendWeakRef::from(content.downgrade());
            let dialog_weak = glib::SendWeakRef::from(dialog.downgrade());

            let validate = move || {
                if let (Some(c), Some(d)) = (content_weak.upgrade(), dialog_weak.upgrade()) {
                    match c.validate_input() {
                        Ok(()) => {
                            c.set_error(None);
                            d.set_response_enabled("add", true);
                        }
                        Err(e) => {
                            if !c.value().is_empty() {
                                c.set_error(Some(&e));
                            } else {
                                c.set_error(None);
                            }
                            d.set_response_enabled("add", false);
                        }
                    }
                }
            };

            let v1 = validate.clone();
            content.imp().entry_value.connect_changed(move |_| {
                v1();
            });

            let v2 = validate;
            content.imp().combo_type.connect_selected_notify(move |_| {
                v2();
            });

            let page_weak = self.downgrade();
            glib::MainContext::default().spawn_local(async move {
                let response = dialog.choose_future(&window).await;
                if response == "add" {
                    if let Some(page) = page_weak.upgrade() {
                        if content.validate_input().is_ok() {
                            let name = content.name();
                            let val = content.value();
                            let r_type = content.rule_type();
                            let act = content.action();

                            let obj = RoutingRuleObject::new(&name, &r_type, &val, &act);
                            if let Some(store) = page.imp().model.borrow().clone() {
                                store.append(&obj);
                                page.check_changes();
                            }
                        }
                    }
                }
            });
        }
    }

    /// Отображает модальный диалог редактирования существующего правила.
    fn show_edit_rule_dialog(&self, obj: &RoutingRuleObject) {
        if let Some(window) = self.root().and_downcast::<gtk::Window>() {
            let dialog = adw::AlertDialog::builder()
                .heading(gettextrs::gettext("Edit Custom Rule"))
                .body(gettextrs::gettext(
                    "Modify domain, IP subnet, or detour action",
                ))
                .build();

            dialog.add_response("cancel", &gettextrs::gettext("Cancel"));
            dialog.add_response("save", &gettextrs::gettext("Save"));
            dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");

            let content = VrxxRuleDialog::new();
            content.set_rule(&obj.name(), &obj.rule_type(), &obj.value(), &obj.action());
            dialog.set_extra_child(Some(&content));

            let content_weak = glib::SendWeakRef::from(content.downgrade());
            let dialog_weak = glib::SendWeakRef::from(dialog.downgrade());

            let validate = move || {
                if let (Some(c), Some(d)) = (content_weak.upgrade(), dialog_weak.upgrade()) {
                    match c.validate_input() {
                        Ok(()) => {
                            c.set_error(None);
                            d.set_response_enabled("save", true);
                        }
                        Err(e) => {
                            if !c.value().is_empty() {
                                c.set_error(Some(&e));
                            } else {
                                c.set_error(None);
                            }
                            d.set_response_enabled("save", false);
                        }
                    }
                }
            };

            let v1 = validate.clone();
            content.imp().entry_value.connect_changed(move |_| {
                v1();
            });

            let v2 = validate;
            content.imp().combo_type.connect_selected_notify(move |_| {
                v2();
            });

            let page_weak = self.downgrade();
            let obj_clone = obj.clone();
            glib::MainContext::default().spawn_local(async move {
                let response = dialog.choose_future(&window).await;
                if response == "save" {
                    if let Some(page) = page_weak.upgrade() {
                        if content.validate_input().is_ok() {
                            obj_clone.set_name(content.name());
                            obj_clone.set_value(content.value());
                            obj_clone.set_rule_type(content.rule_type());
                            obj_clone.set_action(content.action());
                            page.check_changes();
                        }
                    }
                }
            });
        }
    }

    /// Инициализирует сохраненные настройки, делает снимок состояния и привязывает сигналы.
    fn setup_settings(&self) {
        let imp = self.imp();
        *imp.is_initializing.borrow_mut() = true;

        let manager = SettingsManager::new();
        let settings = manager.load();

        // Загрузка пользовательских правил в ListStore
        if let Some(store) = imp.model.borrow().clone() {
            store.remove_all();
            for rule in &settings.routing_rules {
                let obj =
                    RoutingRuleObject::new(&rule.name, &rule.type_, &rule.value, &rule.action);
                store.append(&obj);
            }
        }

        // Первичная установка значений виджетов
        imp.enable_routing_row.set_active(settings.enable_routing);

        let mode_idx = match settings.routing_mode.as_str() {
            "proxy" => 1,
            _ => 0,
        };
        imp.mode_row.set_selected(mode_idx);

        let strat_idx = match settings.domain_strategy.as_str() {
            "IPIfNonMatch" => 1,
            "IPOnDemand" => 2,
            _ => 0,
        };
        imp.domain_strategy_row.set_selected(strat_idx);

        imp.disable_ipv6_row.set_active(settings.disable_ipv6);

        imp.route_ru_sites_row.set_active(settings.route_ru_sites);
        imp.route_ru_ips_row.set_active(settings.route_ru_ips);
        imp.route_cn_sites_row.set_active(settings.route_cn_sites);
        imp.route_cn_ips_row.set_active(settings.route_cn_ips);
        imp.route_ir_sites_row.set_active(settings.route_ir_sites);
        imp.route_ir_ips_row.set_active(settings.route_ir_ips);
        imp.route_antifilter_row
            .set_active(settings.route_antifilter);

        // Сохранение исходного снимка состояния
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

        // Подключение отслеживания изменений переключателей
        let connect_switch = |row: &adw::SwitchRow, page: &VrxxRoutingPage| {
            let p_weak = page.downgrade();
            row.connect_active_notify(move |_| {
                if let Some(p) = p_weak.upgrade() {
                    p.check_changes();
                }
            });
        };

        connect_switch(&imp.enable_routing_row, self);
        connect_switch(&imp.disable_ipv6_row, self);
        connect_switch(&imp.route_ru_sites_row, self);
        connect_switch(&imp.route_ru_ips_row, self);
        connect_switch(&imp.route_cn_sites_row, self);
        connect_switch(&imp.route_cn_ips_row, self);
        connect_switch(&imp.route_ir_sites_row, self);
        connect_switch(&imp.route_ir_ips_row, self);
        connect_switch(&imp.route_antifilter_row, self);

        let p_weak_mode = self.downgrade();
        imp.mode_row.connect_selected_notify(move |_| {
            if let Some(p) = p_weak_mode.upgrade() {
                p.check_changes();
            }
        });

        let p_weak_strat = self.downgrade();
        imp.domain_strategy_row.connect_selected_notify(move |_| {
            if let Some(p) = p_weak_strat.upgrade() {
                p.check_changes();
            }
        });

        *imp.is_initializing.borrow_mut() = false;
    }

    /// Обновляет подзаголовок экспандера региональных правил с датой обновления гео-баз.
    fn update_prr_timestamp(&self) {
        let status = crate::services::geo_updater::get_geo_status();
        let prefix = gettextrs::gettext("Precompiled binary rule-sets");
        let updated_lbl = gettextrs::gettext("Last updated");
        self.imp()
            .regional_rules_group
            .set_description(Some(&format!(
                "{}: {} | {}: {}",
                prefix, "RU, CN, IR, Antifilter", updated_lbl, status
            )));
    }
}
