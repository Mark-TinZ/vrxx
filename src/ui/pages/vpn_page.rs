/* vpn_page.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Главная страница управления VPN-ключами (VrxxVpnPage)
//!
//! Основной контроллер интерфейса, отвечающий за:
//! - Отображение списка сохраненных профилей подключений (`VrxxVpnKeyRow`)
//! - Управление подключением/отключением через REST API демона `DaemonClient`
//! - Замер задержки (Ping) и E2E Warm-Up верификацию соединения после запуска
//! - Сбор и обновление телеметрии трафика в реальном времени (входящий/исходящий)
//! - Обработку сигналов питания через D-Bus (`PrepareForSleep`) для корректного засыпания
//! - Диалоги создания, редактирования, удаления и детальной информации о профилях

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib, CompositeTemplate};

use crate::settings::{SettingsManager, VpnGroupData, VpnKeyData, VpnSubscriptionData};
use crate::ui::components::vpn_key_row::VrxxVpnKeyRow;
use crate::ui::models::VpnKeyObject;
use crate::ui::setup_primary_menu;

mod imp {
    use super::*;
    use std::cell::RefCell;

    /// Структура CompositeTemplate для страницы VPN
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/vpn_page.ui")]
    pub struct VrxxVpnPage {
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub keys_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,

        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub header_actions_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub btn_add_compact_menu: TemplateChild<gtk::MenuButton>,

        pub model: RefCell<Option<gio::ListStore>>,
        pub action_group: RefCell<Option<gio::SimpleActionGroup>>,

        pub start_time: RefCell<Option<std::time::Instant>>,
        pub last_key_switch: RefCell<Option<std::time::Instant>>,
        pub last_disconnect: RefCell<Option<std::time::Instant>>,
        pub last_ping: RefCell<Option<std::time::Instant>>,
        pub connecting_target_url: RefCell<Option<String>>,
        pub bytes_down: RefCell<u64>,
        pub bytes_up: RefCell<u64>,
        pub is_sleeping: RefCell<bool>,
        pub is_connected: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxVpnPage {
        const NAME: &'static str = "VrxxVpnPage";
        type Type = super::VrxxVpnPage;
        type ParentType = adw::BreakpointBin;

        fn class_init(klass: &mut Self::Class) {
            VrxxVpnKeyRow::static_type();
            adw::WindowTitle::static_type();
            adw::ExpanderRow::static_type();
            adw::Breakpoint::static_type();
            adw::BreakpointBin::static_type();
            gtk::Stack::static_type();
            gtk::StackPage::static_type();
            adw::StatusPage::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxVpnPage {
        fn constructed(&self) {
            self.parent_constructed();

            // Явное задание минимальных размеров для AdwBreakpointBin во избежание предупреждений Libadwaita
            self.obj().set_width_request(360);
            self.obj().set_height_request(200);

            self.obj().setup_model();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
            self.obj().start_metrics_timer();
            self.obj().setup_dbus_listener();
            self.obj().setup_daemon_listener();
            self.obj().setup_auto_ping();

            setup_primary_menu(&self.primary_menu_btn.get());
        }
    }

    impl WidgetImpl for VrxxVpnPage {}
    impl adw::subclass::prelude::BreakpointBinImpl for VrxxVpnPage {}
}

glib::wrapper! {
    /// Обертка GObject для страницы VPN
    pub struct VrxxVpnPage(ObjectSubclass<imp::VrxxVpnPage>)
        @extends gtk::Widget, adw::BreakpointBin,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxVpnPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxVpnPage {
    /// Возвращает статус активности текущего VPN подключения.
    pub fn is_connected(&self) -> bool {
        self.imp().is_connected.get()
    }

    /// Создает новый экземпляр страницы VPN.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    // =========================================================================
    // 1. РАБОТА С ДАННЫМИ И МОДЕЛЬЮ СПИСКА (MODEL)
    // =========================================================================
    fn setup_model(&self) {
        let model = gio::ListStore::new::<VpnKeyObject>();

        let settings = SettingsManager::new();
        let saved_keys = settings.load_keys();

        let loaded_settings = settings.load();
        let streamer_mode = loaded_settings.streamer_mode;
        let auto_connect = loaded_settings.connect_on_startup;

        let mut active_found = false;
        for k in saved_keys {
            let is_key_active = if k.is_active && !active_found {
                active_found = true;
                true
            } else {
                false
            };

            let key_obj =
                VpnKeyObject::new_with_group(&k.name, &k.protocol, is_key_active, &k.url, &k.group);
            if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&k.url) {
                key_obj.set_server_info(parsed.host);
            }
            key_obj.set_traffic_down(k.traffic_down);
            key_obj.set_traffic_up(k.traffic_up);
            key_obj.set_time_connected(k.time_connected);
            key_obj.set_ping(k.ping);
            key_obj.set_location(k.location.clone());
            key_obj.set_country_code(k.country_code);
            key_obj.set_city(k.city);
            key_obj.set_isp(k.isp);
            key_obj.set_timezone(k.timezone);
            key_obj.set_hide_ip(streamer_mode);
            model.append(&key_obj);

            if let Some(from_name) = crate::services::geoip::parse_location_from_name(&k.name) {
                key_obj.set_location(from_name.format_display());
                key_obj.set_country_code(from_name.country_code);
                if !from_name.city.is_empty() {
                    key_obj.set_city(from_name.city);
                }
            } else if k.location.is_empty() || k.location == "Unknown" {
                self.resolve_location_for_key(&key_obj);
            }

            if auto_connect && k.is_active {
                let key_clone = key_obj.clone();
                let page_weak = self.downgrade();

                self.connect_map(move |_| {
                    if let Some(page) = page_weak.upgrade() {
                        // Однократное подключение при отображении, если еще не подключены
                        if !page.imp().is_connected.get() {
                            page.set_active_key_internal(&key_clone, true);
                        }
                    }
                });
            } else if !auto_connect && k.is_active {
                // Сбрасываем активное состояние, если автоподключение выключено
                key_obj.set_is_active(false);
            }
        }

        self.imp().model.replace(Some(model.clone()));

        // Первичное построение дерева ключей, групп и подписок
        self.rebuild_ui();
    }

    /// Создает виджет строки ключа [`VrxxVpnKeyRow`] и привязывает обработчики сигналов.
    fn create_key_row(&self, key_obj: &VpnKeyObject) -> VrxxVpnKeyRow {
        let row = VrxxVpnKeyRow::new();
        row.bind(key_obj);

        // Обработчики сигналов из строки ключа:

        // Действие: Клик по строке для активации
        let page_weak_act = self.downgrade();
        let key_obj_act = key_obj.clone();
        row.connect_local("request-activate", false, move |_| {
            if let Some(page) = page_weak_act.upgrade() {
                page.set_active_key(&key_obj_act);
            }
            None
        });

        // Действие: Редактировать
        let page_weak_edit = self.downgrade();
        let key_obj_edit = key_obj.clone();
        row.connect_local("request-edit", false, move |_| {
            if let Some(page) = page_weak_edit.upgrade() {
                page.handle_edit_key(&key_obj_edit);
            }
            None
        });

        // Действие: Информация
        let page_weak_info = self.downgrade();
        let key_obj_info = key_obj.clone();
        row.connect_local("request-info", false, move |_| {
            if let Some(page) = page_weak_info.upgrade() {
                page.handle_info_key(&key_obj_info);
            }
            None
        });

        // Действие: Скопировать ссылку
        let page_weak_cl = self.downgrade();
        let key_obj_cl = key_obj.clone();
        row.connect_local("request-copy-link", false, move |_| {
            if let Some(page) = page_weak_cl.upgrade() {
                let clipboard = page.clipboard();
                clipboard.set_text(&key_obj_cl.url());
            }
            None
        });

        // Действие: Скопировать JSON
        let page_weak_cj = self.downgrade();
        let key_obj_cj = key_obj.clone();
        row.connect_local("request-copy-json", false, move |_| {
            if let Some(page) = page_weak_cj.upgrade() {
                let url = key_obj_cj.url();
                if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&url) {
                    if let Ok(json_str) = serde_json::to_string_pretty(&parsed) {
                        let clipboard = page.clipboard();
                        clipboard.set_text(&json_str);
                    }
                }
            }
            None
        });

        // Действие: Удалить
        let page_weak_del = self.downgrade();
        let key_obj_del = key_obj.clone();
        row.connect_local("request-delete", false, move |_| {
            if let Some(page) = page_weak_del.upgrade() {
                page.handle_delete_key(&key_obj_del);
            }
            None
        });

        // Действие: QR Код и Поделиться
        let page_weak_qr = self.downgrade();
        let key_obj_qr = key_obj.clone();
        row.connect_local("request-qr-code", false, move |_| {
            if let Some(page) = page_weak_qr.upgrade() {
                page.handle_qr_code_key(&key_obj_qr);
            }
            None
        });

        let page_weak_share = self.downgrade();
        let key_obj_share = key_obj.clone();
        row.connect_local("request-share", false, move |_| {
            if let Some(page) = page_weak_share.upgrade() {
                page.handle_qr_code_key(&key_obj_share);
            }
            None
        });

        // Действие: Ручной замер пинга
        let key_obj_ping = key_obj.clone();
        let page_weak_ping = self.downgrade();
        row.connect_local("request-ping", false, move |_| {
            if key_obj_ping.is_loading() {
                return None;
            }
            if let Some(page) = page_weak_ping.upgrade() {
                if let Some(last) = *page.imp().last_ping.borrow() {
                    if last.elapsed() < std::time::Duration::from_millis(1500) {
                        page.show_toast(&gettext("Please wait before pinging again"));
                        return None;
                    }
                }
                page.imp()
                    .last_ping
                    .replace(Some(std::time::Instant::now()));

                key_obj_ping.set_ping(gettext("pinging..."));
                key_obj_ping.set_is_loading(true);
                page.trigger_ping_key(&key_obj_ping);
            }
            None
        });

        // Действие: Переместить в группу
        let key_obj_move = key_obj.clone();
        let page_weak_move = self.downgrade();
        row.connect_local("request-move-group", false, move |_| {
            if let Some(page) = page_weak_move.upgrade() {
                page.handle_move_key_to_group(&key_obj_move);
            }
            None
        });

        row
    }

    /// Перестраивает иерархический интерфейс списка ключей, групп и подписок.
    pub fn rebuild_ui(&self) {
        let imp = self.imp();

        // Очищаем текущие элементы в ListBox
        while let Some(child) = imp.keys_list.first_child() {
            imp.keys_list.remove(&child);
        }

        let model = match imp.model.borrow().as_ref() {
            Some(m) => m.clone(),
            None => {
                imp.main_stack.set_visible_child_name("empty");
                return;
            }
        };

        let total_items = model.n_items();
        let settings_mgr = SettingsManager::new();
        let groups = settings_mgr.load_groups();
        let subscriptions = settings_mgr.load_subscriptions();

        if total_items == 0 && groups.is_empty() && subscriptions.is_empty() {
            imp.main_stack.set_visible_child_name("empty");
            return;
        }

        imp.main_stack.set_visible_child_name("list");

        // Собираем все объекты ключей из модели
        let mut all_keys: Vec<VpnKeyObject> = Vec::with_capacity(total_items as usize);
        for i in 0..total_items {
            if let Some(item) = model
                .item(i)
                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
            {
                all_keys.push(item);
            }
        }

        let has_groups = !groups.is_empty() || !subscriptions.is_empty();

        // 1. Отображение подписок (Subscriptions)
        for sub in &subscriptions {
            let expander = adw::ExpanderRow::builder()
                .title(&sub.name)
                .expanded(sub.is_expanded)
                .build();

            let icon = gtk::Image::from_icon_name("cloud-download-symbolic");
            expander.add_prefix(&icon);

            let sub_keys: Vec<&VpnKeyObject> =
                all_keys.iter().filter(|k| k.group() == sub.id).collect();
            let count = sub_keys.len();

            let subtitle = if sub.last_updated.is_empty() {
                format!(
                    "{} • {} {}",
                    gettext("Subscription"),
                    count,
                    gettext("keys")
                )
            } else {
                format!("{} • {} {}", sub.last_updated, count, gettext("keys"))
            };
            expander.set_subtitle(&subtitle);

            // Кнопка обновления подписки
            let btn_refresh = gtk::Button::builder()
                .icon_name("view-refresh-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Update subscription keys"))
                .css_classes(["flat", "circular"])
                .build();
            let page_weak_ref = self.downgrade();
            let sub_id_ref = sub.id.clone();
            btn_refresh.connect_clicked(move |_| {
                if let Some(page) = page_weak_ref.upgrade() {
                    page.update_subscription(&sub_id_ref);
                }
            });
            expander.add_suffix(&btn_refresh);

            // Меню подписки
            let btn_menu = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build();

            let popover = gtk::Popover::new();
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
            vbox.set_margin_top(6);
            vbox.set_margin_bottom(6);
            vbox.set_margin_start(6);
            vbox.set_margin_end(6);

            let btn_pop_update = gtk::Button::builder()
                .label(gettext("Update"))
                .has_frame(false)
                .build();
            let p_ref2 = self.downgrade();
            let s_id2 = sub.id.clone();
            let pop_weak1 = popover.downgrade();
            btn_pop_update.connect_clicked(move |_| {
                if let Some(pop) = pop_weak1.upgrade() {
                    pop.popdown();
                }
                if let Some(page) = p_ref2.upgrade() {
                    page.update_subscription(&s_id2);
                }
            });
            vbox.append(&btn_pop_update);

            let btn_pop_edit = gtk::Button::builder()
                .label(gettext("Edit"))
                .has_frame(false)
                .build();
            let p_edit = self.downgrade();
            let s_id_edit = sub.id.clone();
            let pop_weak2 = popover.downgrade();
            btn_pop_edit.connect_clicked(move |_| {
                if let Some(pop) = pop_weak2.upgrade() {
                    pop.popdown();
                }
                if let Some(page) = p_edit.upgrade() {
                    page.prompt_edit_subscription(&s_id_edit);
                }
            });
            vbox.append(&btn_pop_edit);

            let btn_pop_del = gtk::Button::builder()
                .label(gettext("Delete"))
                .has_frame(false)
                .css_classes(["destructive-action"])
                .build();
            let p_del = self.downgrade();
            let s_id_del = sub.id.clone();
            let pop_weak3 = popover.downgrade();
            btn_pop_del.connect_clicked(move |_| {
                if let Some(pop) = pop_weak3.upgrade() {
                    pop.popdown();
                }
                if let Some(page) = p_del.upgrade() {
                    page.delete_subscription(&s_id_del);
                }
            });
            vbox.append(&btn_pop_del);

            popover.set_child(Some(&vbox));
            btn_menu.set_popover(Some(&popover));
            expander.add_suffix(&btn_menu);

            // Вложенные строки ключей
            for key_obj in sub_keys {
                let row = self.create_key_row(key_obj);
                expander.add_row(&row);
            }

            // Отслеживание сворачивания
            let s_id_exp = sub.id.clone();
            expander.connect_expanded_notify(move |exp| {
                let is_exp = exp.is_expanded();
                let mgr = SettingsManager::new();
                let mut all_s = mgr.load_subscriptions();
                if let Some(s) = all_s.iter_mut().find(|s| s.id == s_id_exp) {
                    s.is_expanded = is_exp;
                }
                mgr.save_subscriptions(&all_s);
            });

            imp.keys_list.append(&expander);
        }

        // 2. Отображение пользовательских групп (Groups)
        for group in &groups {
            let expander = adw::ExpanderRow::builder()
                .title(&group.name)
                .expanded(group.is_expanded)
                .build();

            let icon = gtk::Image::from_icon_name("folder-symbolic");
            expander.add_prefix(&icon);

            let group_keys: Vec<&VpnKeyObject> =
                all_keys.iter().filter(|k| k.group() == group.id).collect();
            let count = group_keys.len();
            expander.set_subtitle(&format!("{} {}", count, gettext("keys")));

            // Кнопка быстрого добавления ключа в группу
            let btn_add_key = gtk::Button::builder()
                .icon_name("list-add-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Add key to this group"))
                .css_classes(["flat", "circular"])
                .build();
            let page_weak_add = self.downgrade();
            let g_id_add = group.id.clone();
            btn_add_key.connect_clicked(move |_| {
                if let Some(page) = page_weak_add.upgrade() {
                    page.add_key_to_group(&g_id_add);
                }
            });
            expander.add_suffix(&btn_add_key);

            // Меню группы: Переименовать, Удалить
            let btn_menu = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build();

            let popover = gtk::Popover::new();
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
            vbox.set_margin_top(6);
            vbox.set_margin_bottom(6);
            vbox.set_margin_start(6);
            vbox.set_margin_end(6);

            let btn_pop_rename = gtk::Button::builder()
                .label(gettext("Rename"))
                .has_frame(false)
                .build();
            let p_ren = self.downgrade();
            let g_id_ren = group.id.clone();
            let pop_weak1 = popover.downgrade();
            btn_pop_rename.connect_clicked(move |_| {
                if let Some(pop) = pop_weak1.upgrade() {
                    pop.popdown();
                }
                if let Some(page) = p_ren.upgrade() {
                    page.prompt_rename_group(&g_id_ren);
                }
            });
            vbox.append(&btn_pop_rename);

            let btn_pop_del = gtk::Button::builder()
                .label(gettext("Delete"))
                .has_frame(false)
                .css_classes(["destructive-action"])
                .build();
            let p_del = self.downgrade();
            let g_id_del = group.id.clone();
            let pop_weak2 = popover.downgrade();
            btn_pop_del.connect_clicked(move |_| {
                if let Some(pop) = pop_weak2.upgrade() {
                    pop.popdown();
                }
                if let Some(page) = p_del.upgrade() {
                    page.delete_group(&g_id_del);
                }
            });
            vbox.append(&btn_pop_del);

            popover.set_child(Some(&vbox));
            btn_menu.set_popover(Some(&popover));
            expander.add_suffix(&btn_menu);

            // Вложенные строки ключей
            for key_obj in group_keys {
                let row = self.create_key_row(key_obj);
                expander.add_row(&row);
            }

            // Отслеживание сворачивания
            let g_id_exp = group.id.clone();
            expander.connect_expanded_notify(move |exp| {
                let is_exp = exp.is_expanded();
                let mgr = SettingsManager::new();
                let mut all_g = mgr.load_groups();
                if let Some(g) = all_g.iter_mut().find(|g| g.id == g_id_exp) {
                    g.is_expanded = is_exp;
                }
                mgr.save_groups(&all_g);
            });

            imp.keys_list.append(&expander);
        }

        // 3. Отображение ключей без группы (Ungrouped)
        let group_ids: Vec<String> = groups.iter().map(|g| g.id.clone()).collect();
        let sub_ids: Vec<String> = subscriptions.iter().map(|s| s.id.clone()).collect();

        let ungrouped_keys: Vec<&VpnKeyObject> = all_keys
            .iter()
            .filter(|k| {
                let g = k.group();
                g.is_empty() || (!group_ids.contains(&g) && !sub_ids.contains(&g))
            })
            .collect();

        if !ungrouped_keys.is_empty() {
            if has_groups {
                let expander = adw::ExpanderRow::builder()
                    .title(gettext("Ungrouped"))
                    .subtitle(format!("{} {}", ungrouped_keys.len(), gettext("keys")))
                    .expanded(true)
                    .build();

                let icon = gtk::Image::from_icon_name("network-vpn-symbolic");
                expander.add_prefix(&icon);

                for key_obj in ungrouped_keys {
                    let row = self.create_key_row(key_obj);
                    expander.add_row(&row);
                }

                imp.keys_list.append(&expander);
            } else {
                for key_obj in ungrouped_keys {
                    let row = self.create_key_row(key_obj);
                    imp.keys_list.append(&row);
                }
            }
        }
    }

    // =========================================================================
    // 2. СИГНАЛЫ И КОЛБЭКИ (CALLBACKS & EVENTS)
    // =========================================================================
    fn setup_callbacks(&self) {
        let page_weak = self.downgrade();
        self.imp().keys_list.connect_row_activated(move |_, row| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            // Выбор строки и переключение активного профиля (поддерживаем как прямой VrxxVpnKeyRow, так и вложенный AdwActionRow)
            let key_row_opt = row.downcast_ref::<VrxxVpnKeyRow>().cloned().or_else(|| {
                row.ancestor(VrxxVpnKeyRow::static_type())
                    .and_then(|w| w.downcast::<VrxxVpnKeyRow>().ok())
            });
            if let Some(key_row) = key_row_opt {
                if let Some(selected_item) = key_row.item() {
                    page.set_active_key(&selected_item);
                }
            }
        });

        // Отслеживание запросов на перезапуск ядра из настроек
        let page_weak_restart = self.downgrade();
        glib::spawn_future_local(async move {
            let (_, receiver) = crate::settings::core_restart_channel();
            while receiver.recv().await.is_ok() {
                if let Some(page) = page_weak_restart.upgrade() {
                    if let Some(model) = page.imp().model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                if item.is_active() {
                                    page.set_active_key_internal(&item, true);
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    tracing::debug!("VPN page destroyed, terminating core restart listener");
                    break;
                }
            }
        });
    }

    /// Настройка D-Bus слушателя системных событий перехода в ждущий/спящий режим (systemd-logind).
    fn setup_dbus_listener(&self) {
        let page_weak = self.downgrade();
        if let Ok(connection) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
            #[allow(deprecated)]
            connection.signal_subscribe(
                Some("org.freedesktop.login1"),
                Some("org.freedesktop.login1.Manager"),
                Some("PrepareForSleep"),
                Some("/org/freedesktop/login1"),
                None,
                gio::DBusSignalFlags::empty(),
                move |_, _, _, _, _, params| {
                    let is_sleeping = params.child_get::<bool>(0);
                    if let Some(page) = page_weak.upgrade() {
                        *page.imp().is_sleeping.borrow_mut() = is_sleeping;
                        if is_sleeping {
                            tracing::info!("System is suspending. Pausing VPN monitoring.");
                        } else {
                            tracing::info!("System resumed. Resuming VPN monitoring.");
                        }
                    }
                },
            );
        }
    }

    /// Подписка на события смены статуса от фонового демона.
    fn setup_daemon_listener(&self) {
        let page_weak = self.downgrade();
        glib::spawn_future_local(async move {
            let client = crate::ipc::DaemonClient::new();
            if let Ok(status) = client.status().await {
                if let Some(page) = page_weak.upgrade() {
                    page.handle_daemon_status_change(status);
                }
            }

            let events = client.subscribe_events();
            while let Ok(event) = events.recv().await {
                if let crate::daemon::DaemonEvent::StatusChanged(status) = event {
                    if let Some(page) = page_weak.upgrade() {
                        page.handle_daemon_status_change(status);
                    } else {
                        tracing::debug!("VPN page destroyed, terminating daemon event listener");
                        break;
                    }
                }
            }
        });
    }

    // =========================================================================
    // 3. УПРАВЛЕНИЕ СОСТОЯНИЕМ СОЕДИНЕНИЯ (STATE MANAGEMENT)
    // =========================================================================
    fn handle_daemon_status_change(&self, status: crate::protocol::DaemonStatus) {
        use crate::protocol::DaemonStatus;
        let imp = self.imp();
        let is_conn = status == DaemonStatus::Connected;
        imp.is_connected.set(is_conn);

        match status {
            DaemonStatus::Connected => {
                // Если идет процедура E2E-верификации для конкретного ключа,
                // не вмешиваемся в состояние ключей до завершения проверки
                if imp.connecting_target_url.borrow().is_some() {
                    return;
                }
                let mut active_name = String::new();
                if let Some(model) = imp.model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model
                            .item(i)
                            .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                        {
                            if item.is_active() {
                                active_name = item.name();
                                item.set_is_loading(false);
                                item.set_is_error(false);
                            }
                        }
                    }
                    if active_name.is_empty() {
                        let saved = crate::settings::SettingsManager::new().load();
                        if let Some(saved_active) = saved.keys.iter().find(|k| k.is_active) {
                            for i in 0..model.n_items() {
                                if let Some(item) = model
                                    .item(i)
                                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                                {
                                    if item.url() == saved_active.url {
                                        item.set_is_active(true);
                                        item.set_is_loading(false);
                                        item.set_is_error(false);
                                        active_name = item.name();
                                    } else {
                                        item.set_is_active(false);
                                        item.set_is_loading(false);
                                    }
                                }
                            }
                        }
                    }
                }
                // Восстанавливаем время старта подключения
                if imp.start_time.borrow().is_none() {
                    imp.start_time.replace(Some(std::time::Instant::now()));
                }
                self.save_current_keys();
                self.update_disconnect_action_state();
                let subtitle = if active_name.is_empty() {
                    gettext("Connected")
                } else {
                    format!("{active_name} {}", gettext("Connected"))
                };
                imp.window_title.set_subtitle(&subtitle);
                if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                    w.update_status_state("Connected", Some(&active_name));
                }
            }
            DaemonStatus::Disconnected => {
                // Проверяем, находимся ли мы в процессе подключения или переключения профиля
                let is_connecting_target = imp.connecting_target_url.borrow().is_some();
                let is_switching_loading = imp.model.borrow().as_ref().is_some_and(|model| {
                    (0..model.n_items()).any(|i| {
                        model
                            .item(i)
                            .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            .map(|item| item.is_loading())
                            .unwrap_or(false)
                    })
                });

                if is_connecting_target || is_switching_loading {
                    // При переключении: демон остановил старый прокси, новый запускается
                    tracing::debug!("Ignoring Disconnected event during active key transition");
                } else {
                    // Истинное отключение: сбрасываем визуальный статус и таймеры
                    imp.connecting_target_url.replace(None);
                    imp.window_title.set_subtitle(&gettext("Disconnected"));
                    imp.start_time.replace(None);

                    if let Some(model) = imp.model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                item.set_is_active(false);
                                item.set_is_loading(false);
                                item.set_is_error(false);
                            }
                        }
                    }
                    self.save_current_keys();
                    self.update_disconnect_action_state();
                    if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                        w.update_status_state("Disconnected", None);
                    }
                    crate::backend::CoreBackend::update_system_proxy(false);
                }
            }
            DaemonStatus::Connecting => {
                imp.window_title.set_subtitle(&gettext("Connecting..."));
                if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                    w.update_status_state("Connecting", None);
                }
            }
            DaemonStatus::Disconnecting => {
                imp.window_title.set_subtitle(&gettext("Disconnecting..."));
            }
            DaemonStatus::Error => {
                imp.connecting_target_url.replace(None);
                imp.window_title.set_subtitle(&gettext("Connection error"));
                if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                    w.update_status_state("Error", None);
                }
                crate::backend::CoreBackend::update_system_proxy(false);
                if let Some(model) = imp.model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model
                            .item(i)
                            .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                        {
                            if item.is_active() || item.is_loading() {
                                item.set_is_loading(false);
                                item.set_is_error(true);
                                item.set_is_active(false);
                            }
                        }
                    }
                }
                let mut error_details = gettext("Unknown error. Please check System logs.");
                let log_dir = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("vrxx")
                    .join("logs");
                let log_path = log_dir.join("core.log");
                if let Ok(content) = std::fs::read_to_string(&log_path) {
                    let lines: Vec<&str> = content.lines().rev().take(5).collect();
                    if !lines.is_empty() {
                        error_details = lines.into_iter().rev().collect::<Vec<&str>>().join("\n");
                    }
                }

                let dialog = adw::AlertDialog::builder()
                    .heading(gettext("Connection failure"))
                    .body(format!(
                        "{}:\n\n{error_details}",
                        gettext("Core process unexpectedly terminated. Log details")
                    ))
                    .build();
                dialog.add_response("ok", &gettext("OK"));
                if let Some(root) = self.root().and_downcast::<gtk::Window>() {
                    dialog.present(Some(&root));
                }
            }
        }

        self.update_disconnect_action_state();
    }

    /// Отображает всплывающее уведомление `AdwToast`.
    fn show_toast(&self, message: &str) {
        if let Some(window) = self.root().and_downcast::<crate::window::VrxxWindow>() {
            let toast = adw::Toast::new(message);
            toast.set_timeout(2);
            window.add_toast(toast);
        }
    }

    /// Запускает ежесекундный таймер обновления метрик активности и объема переданного трафика.
    fn start_metrics_timer(&self) {
        let page_weak = self.downgrade();
        glib::timeout_add_seconds_local(1, move || {
            if let Some(page) = page_weak.upgrade() {
                let imp = page.imp();

                if *imp.is_sleeping.borrow() {
                    return glib::ControlFlow::Continue;
                }

                let start_time = *imp.start_time.borrow();

                if let Some(start) = start_time {
                    if let Some(model) = imp.model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                if item.is_active() {
                                    let elapsed = start.elapsed().as_secs();
                                    let hours = elapsed / 3600;
                                    let mins = (elapsed % 3600) / 60;
                                    let secs = elapsed % 60;
                                    item.set_time_connected(format!(
                                        "{hours:02}:{mins:02}:{secs:02}"
                                    ));

                                    if imp.is_connected.get() {
                                        if let Some(w) =
                                            page.root().and_downcast::<crate::window::VrxxWindow>()
                                        {
                                            w.update_stats(
                                                &item.time_connected(),
                                                &item.traffic_down(),
                                                &item.traffic_up(),
                                                &item.ping(),
                                            );
                                        }

                                        let item_clone_stats = item.clone();
                                        let page_weak_stats = page.downgrade();

                                        glib::spawn_future_local(async move {
                                            let client = match reqwest::Client::builder()
                                                .timeout(std::time::Duration::from_secs(2))
                                                .build()
                                            {
                                                Ok(c) => c,
                                                Err(_) => return,
                                            };

                                            let res = client
                                                .get("http://127.0.0.1:9090/connections")
                                                .send()
                                                .await;
                                            if let Ok(resp) = res {
                                                if let Ok(json) =
                                                    resp.json::<serde_json::Value>().await
                                                {
                                                    let total_down = json
                                                        .get("downloadTotal")
                                                        .or_else(|| json.get("download_total"))
                                                        .and_then(|v| v.as_u64())
                                                        .unwrap_or(0);
                                                    let total_up = json
                                                        .get("uploadTotal")
                                                        .or_else(|| json.get("upload_total"))
                                                        .and_then(|v| v.as_u64())
                                                        .unwrap_or(0);

                                                    let format_bytes = |b: u64| -> String {
                                                        let tb = 1_099_511_627_776_f64;
                                                        let gb = 1_073_741_824_f64;
                                                        let mb = 1_048_576_f64;
                                                        let kb = 1_024_f64;
                                                        let bf = b as f64;

                                                        if bf >= tb {
                                                            format!("{:.2} TB", bf / tb)
                                                        } else if bf >= gb {
                                                            format!("{:.2} GB", bf / gb)
                                                        } else if bf >= mb {
                                                            format!("{:.1} MB", bf / mb)
                                                        } else if bf >= kb {
                                                            format!("{:.0} KB", bf / kb)
                                                        } else {
                                                            format!("{b} B")
                                                        }
                                                    };
                                                    let down_str = format_bytes(total_down);
                                                    let up_str = format_bytes(total_up);
                                                    item_clone_stats
                                                        .set_traffic_down(down_str.clone());
                                                    item_clone_stats.set_traffic_up(up_str.clone());

                                                    if let Some(p) = page_weak_stats.upgrade() {
                                                        *p.imp().bytes_down.borrow_mut() =
                                                            total_down;
                                                        *p.imp().bytes_up.borrow_mut() = total_up;

                                                        if let Some(w) = p
                                                            .root()
                                                            .and_downcast::<crate::window::VrxxWindow>(
                                                        ) {
                                                            w.update_stats(
                                                                &item_clone_stats.time_connected(),
                                                                &down_str,
                                                                &up_str,
                                                                &item_clone_stats.ping(),
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // =========================================================================
    // 4. УПРАВЛЕНИЕ КЛЮЧАМИ И КОНФИГУРАЦИЕЙ (KEY OPERATIONS)
    // =========================================================================
    fn set_active_key(&self, active_item: &VpnKeyObject) {
        self.set_active_key_internal(active_item, false);
    }

    fn set_active_key_internal(&self, active_item: &VpnKeyObject, force: bool) {
        if !force {
            if let Some(last_time) = *self.imp().last_key_switch.borrow() {
                let elapsed = last_time.elapsed();
                // Защита от дребезга при сверхбыстрых кликах (< 400мс)
                if elapsed < std::time::Duration::from_millis(400) {
                    return;
                }
                // Кулдаун 3.5 сек для завершения цикла переподключения
                let cooldown = std::time::Duration::from_millis(3500);
                if elapsed < cooldown {
                    self.show_toast(&gettext("Please wait before switching keys"));
                    tracing::info!("Key switch throttled by cooldown timeout");
                    return;
                }
            }
        }

        self.imp()
            .last_key_switch
            .replace(Some(std::time::Instant::now()));

        self.imp()
            .connecting_target_url
            .replace(Some(active_item.url()));

        if let Some(model) = self.imp().model.borrow().as_ref() {
            for i in 0..model.n_items() {
                if let Some(item) = model
                    .item(i)
                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                {
                    if item.url() != active_item.url() {
                        item.set_is_active(false);
                        item.set_is_loading(false);
                    }
                }
            }
            active_item.set_is_active(false);
            active_item.set_is_loading(true);
            active_item.set_is_error(false);

            // Синхронизация режима стримера
            let current_settings = SettingsManager::new().load();
            active_item.set_hide_ip(current_settings.streamer_mode);

            self.save_current_keys();
            self.update_disconnect_action_state();

            // Сброс метрик
            self.imp()
                .start_time
                .replace(Some(std::time::Instant::now()));
            *self.imp().bytes_down.borrow_mut() = 0;
            *self.imp().bytes_up.borrow_mut() = 0;

            self.imp()
                .window_title
                .set_subtitle(&gettext("Connecting..."));

            let app_settings = current_settings;

            let parsed = match crate::domain::key_parser::parse_vpn_key(&active_item.url()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to parse key for config generation: {}", e);
                    self.imp().connecting_target_url.replace(None);
                    active_item.set_is_loading(false);
                    active_item.set_is_error(true);
                    self.imp()
                        .window_title
                        .set_subtitle(&gettext("Configuration error"));
                    let root_widget = self.root();
                    crate::ui::error_dialog::show_error_dialog(
                        root_widget.as_ref(),
                        Some(&gettext("Invalid Key Format")),
                        &format!("{}: {}", gettext("Failed to parse VPN key URL"), e),
                        &e,
                    );
                    return;
                }
            };

            // Статическая валидация параметров ключа
            if let Err(val_err) = parsed.validate() {
                tracing::error!("Key validation error: {}", val_err);
                self.imp().connecting_target_url.replace(None);
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                self.imp()
                    .window_title
                    .set_subtitle(&gettext("Invalid Key"));
                let err_msg = val_err.to_string();
                let root_widget = self.root();
                crate::ui::error_dialog::show_error_dialog(
                    root_widget.as_ref(),
                    Some(&gettext("Key Validation Error")),
                    &gettext("The VPN key parameters are invalid or corrupted (e.g. invalid UUID, missing Reality public key or SNI)."),
                    &err_msg,
                );
                return;
            }

            let options = crate::protocol::DaemonConfigOptions::from_settings(
                &app_settings,
                app_settings.tun_mode,
            );

            let page_weak = self.downgrade();
            let item_clone = active_item.clone();
            let socks_port = app_settings.socks_port;
            let ping_target_url = if app_settings.ping_target_url.is_empty() {
                "https://www.gstatic.com/generate_204".to_string()
            } else {
                app_settings.ping_target_url.clone()
            };

            glib::spawn_future_local(async move {
                let proxy = crate::ipc::DaemonClient::new();

                // Проверяем доступность системного демона перед попыткой подключения
                if !proxy.is_alive().await {
                    tracing::warn!("Cannot connect to VPN: vrxx-daemon is not running");
                    if let Some(page) = page_weak.upgrade() {
                        if page.imp().connecting_target_url.borrow().as_deref()
                            != Some(&item_clone.url())
                        {
                            return;
                        }
                        page.imp().connecting_target_url.replace(None);
                        item_clone.set_is_active(false);
                        item_clone.set_is_loading(false);
                        item_clone.set_is_error(false);
                        page.imp().start_time.replace(None);
                        page.update_disconnect_action_state();

                        // Показываем предупреждение и раскрываем баннер в главном окне
                        if let Some(w) = page.root().and_downcast::<crate::window::VrxxWindow>() {
                            w.set_daemon_banner_revealed(true);
                            w.add_toast(adw::Toast::new(&gettext(
                                "VRXX system service is not running. Please start vrxx-daemon.",
                            )));
                        } else {
                            let root_widget = page.root();
                            crate::ui::error_dialog::show_error_dialog(
                                root_widget.as_ref(),
                                Some(&gettext("Service Unavailable")),
                                &gettext(
                                    "VRXX system daemon is not running. Please start vrxx-daemon to enable VPN.",
                                ),
                                "vrxx-daemon not reachable at /run/vrxx/daemon.sock",
                            );
                        }
                    }
                    return;
                }

                tracing::info!("Connecting to VPN key via REST API: {}", item_clone.name());
                if let Err(e) = proxy.start_proxy(&parsed, &options).await {
                    tracing::error!("Failed to start core via REST API: {}", e);
                    if let Some(page) = page_weak.upgrade() {
                        if page.imp().connecting_target_url.borrow().as_deref()
                            != Some(&item_clone.url())
                        {
                            tracing::debug!(
                                "Ignoring launch error for stale connection: {}",
                                item_clone.url()
                            );
                            return;
                        }
                        page.imp().connecting_target_url.replace(None);
                        item_clone.set_is_active(false);
                        item_clone.set_is_loading(false);
                        item_clone.set_is_error(true);
                        page.imp().start_time.replace(None);
                        page.imp()
                            .window_title
                            .set_subtitle(&gettext("Core startup error"));
                        page.save_current_keys();
                        page.update_disconnect_action_state();

                        let raw_err = e.to_string();
                        let human_msg = crate::ui::error_dialog::format_human_error(&raw_err);
                        let tech_log = format!(
                            "--- Technical Log & Error Stack ---\nTimestamp: {}\nError: {}\nRaw Trace: {:?}",
                            chrono::Local::now().to_rfc3339(),
                            human_msg,
                            e
                        );

                        let root_widget = page.root();
                        crate::ui::error_dialog::show_error_dialog(
                            root_widget.as_ref(),
                            Some(&gettext("Failed to connect to VPN")),
                            &human_msg,
                            &tech_log,
                        );
                    }
                } else {
                    tracing::info!("Core started. Running E2E Warm-Up connectivity check...");

                    // Сквозная проверка доступности сети через прокси (до 8 секунд)
                    let probe_res = crate::services::ping::verify_proxy_connectivity(
                        socks_port,
                        &ping_target_url,
                        std::time::Duration::from_secs(8),
                    )
                    .await;

                    match probe_res {
                        Ok(latency_ms) => {
                            tracing::info!(
                                "E2E Warm-Up check succeeded! Latency: {} ms",
                                latency_ms
                            );

                            // Управление системным прокси GNOME: включаем только если не используется TUN
                            if !options.tun_mode {
                                let current_settings =
                                    crate::settings::SettingsManager::new().load();
                                if current_settings.set_system_proxy {
                                    tracing::info!("Auto-activating GNOME system proxy...");
                                    crate::backend::CoreBackend::update_system_proxy(true);
                                }
                            } else {
                                tracing::info!(
                                    "TUN mode active: ensuring GNOME system proxy is set to 'none'"
                                );
                                crate::backend::CoreBackend::update_system_proxy(false);
                            }

                            if let Some(page) = page_weak.upgrade() {
                                if page.imp().connecting_target_url.borrow().as_deref()
                                    != Some(&item_clone.url())
                                {
                                    tracing::debug!(
                                        "Ignoring success result for stale connection: {}",
                                        item_clone.url()
                                    );
                                    return;
                                }
                                page.imp().connecting_target_url.replace(None);
                                item_clone.set_is_loading(false);
                                item_clone.set_is_active(true);
                                item_clone.set_is_error(false);
                                item_clone.set_ping(format!("{latency_ms} ms"));
                                page.save_current_keys();
                                page.resolve_live_egress_location(&item_clone, socks_port);
                                page.update_disconnect_action_state();
                                let subtitle =
                                    format!("{} {}", item_clone.name(), gettext("Connected"));
                                page.imp().window_title.set_subtitle(&subtitle);
                                if let Some(w) =
                                    page.root().and_downcast::<crate::window::VrxxWindow>()
                                {
                                    w.update_status_state("Connected", Some(&item_clone.name()));
                                    w.update_active_connection_widget();
                                }
                            }
                        }
                        Err(probe_err) => {
                            tracing::warn!(
                                "E2E Warm-Up check failed: {}. Stopping core...",
                                probe_err
                            );
                            crate::backend::CoreBackend::update_system_proxy(false);
                            if let Some(page) = page_weak.upgrade() {
                                if page.imp().connecting_target_url.borrow().as_deref()
                                    != Some(&item_clone.url())
                                {
                                    tracing::debug!(
                                        "Ignoring check error for stale connection: {}",
                                        item_clone.url()
                                    );
                                    return;
                                }
                                let _ = proxy.stop_proxy().await;
                                page.imp().connecting_target_url.replace(None);
                                item_clone.set_is_active(false);
                                item_clone.set_is_loading(false);
                                item_clone.set_is_error(true);
                                item_clone.set_ping(gettext("error"));
                                page.imp().start_time.replace(None);
                                page.imp()
                                    .window_title
                                    .set_subtitle(&gettext("Connection failed"));
                                page.save_current_keys();
                                page.update_disconnect_action_state();

                                let human_msg = match &probe_err {
                                    crate::services::ping::ConnectivityError::HandshakeFailed(msg) => {
                                        format!("{}: {}", gettext("Server rejected authentication or handshake failed (invalid UUID or wrong password)"), msg)
                                    }
                                    crate::services::ping::ConnectivityError::Timeout(_) => {
                                        gettext("Server connection timed out. The remote server is unreachable, dead or blocked.")
                                    }
                                    crate::services::ping::ConnectivityError::ProxyError(msg) => {
                                        format!("{}: {}", gettext("Local proxy error"), msg)
                                    }
                                    crate::services::ping::ConnectivityError::RequestFailed(msg) => {
                                        format!("{}: {}", gettext("Test request through proxy failed"), msg)
                                    }
                                };

                                let tech_log = format!(
                                    "--- Technical Log & Connectivity Stack ---\nTimestamp: {}\nError: {}\nProbe Error: {:?}",
                                    chrono::Local::now().to_rfc3339(),
                                    human_msg,
                                    probe_err
                                );

                                let root_widget = page.root();
                                crate::ui::error_dialog::show_error_dialog(
                                    root_widget.as_ref(),
                                    Some(&gettext("Failed to connect to VPN")),
                                    &human_msg,
                                    &tech_log,
                                );
                            }
                        }
                    }
                }
            });
        }
    }

    /// Инициирует замер задержки (ping) для всех сохраненных профилей в списке.
    pub fn ping_all(&self) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let n = model.n_items();
            if n == 0 {
                return;
            }
            tracing::info!(target: "vrxx::ping", total_keys = n, "Initiating ping for all VPN profiles");
            for i in 0..n {
                if let Some(item) = model
                    .item(i)
                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                {
                    if !item.is_loading() {
                        item.set_ping(gettext("pinging..."));
                        item.set_is_loading(true);
                        self.trigger_ping_key(&item);
                    }
                }
            }
        }
    }

    /// Инициирует замер задержки для выбранного или первого активного профиля.
    pub fn ping_selected(&self) {
        let imp = self.imp();
        if let Some(row) = imp.keys_list.selected_row() {
            let idx = row.index();
            if idx >= 0 {
                if let Some(model) = imp.model.borrow().as_ref() {
                    if let Some(item) = model
                        .item(idx as u32)
                        .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                    {
                        if !item.is_loading() {
                            item.set_ping(gettext("pinging..."));
                            item.set_is_loading(true);
                            self.trigger_ping_key(&item);
                            return;
                        }
                    }
                }
            }
        }

        // Если строка не выбрана явно в ListBox, пингуем активный профиль
        if let Some(model) = imp.model.borrow().as_ref() {
            for i in 0..model.n_items() {
                if let Some(item) = model
                    .item(i)
                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                {
                    if item.is_active() && !item.is_loading() {
                        item.set_ping(gettext("pinging..."));
                        item.set_is_loading(true);
                        self.trigger_ping_key(&item);
                        return;
                    }
                }
            }
        }
    }

    /// Настраивает автопинг при запуске и периодический опрос по кулдауну.
    fn setup_auto_ping(&self) {
        let settings = SettingsManager::new().load();

        if settings.auto_ping_on_startup {
            let page_weak = self.downgrade();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if let Some(page) = page_weak.upgrade() {
                    tracing::info!(target: "vrxx::ping", "Running startup latency check");
                    page.ping_all();
                }
            });
        }

        if settings.auto_ping_interval > 0 {
            let page_weak = self.downgrade();
            let interval = std::time::Duration::from_secs(settings.auto_ping_interval);
            glib::timeout_add_local(interval, move || {
                if let Some(page) = page_weak.upgrade() {
                    let cur_settings = SettingsManager::new().load();
                    if cur_settings.auto_ping_interval > 0 {
                        tracing::info!(
                            target: "vrxx::ping",
                            interval_secs = cur_settings.auto_ping_interval,
                            "Running periodic background latency check"
                        );
                        page.ping_all();
                        glib::ControlFlow::Continue
                    } else {
                        glib::ControlFlow::Break
                    }
                } else {
                    glib::ControlFlow::Break
                }
            });
        }
    }

    /// Инициирует асинхронный замер задержки (ping) для указанного ключа.
    pub fn trigger_ping_key(&self, item_clone: &VpnKeyObject) {
        if item_clone.location().is_empty() || item_clone.location() == "Unknown" {
            self.resolve_location_for_key(item_clone);
        }

        let raw_url = item_clone.url();
        let (sender, receiver) = async_channel::unbounded::<crate::services::ping::PingResult>();
        let item_ui = item_clone.clone();

        let page_weak_ping = self.downgrade();
        glib::spawn_future_local(async move {
            if let Ok(res) = receiver.recv().await {
                item_ui.set_is_loading(false);
                let ping_val = match res {
                    crate::services::ping::PingResult::Success(ms) => format!("{ms} ms"),
                    crate::services::ping::PingResult::Timeout => gettext("timeout"),
                    crate::services::ping::PingResult::Error(e) => {
                        let e_lower = e.to_lowercase();
                        if e_lower.contains("handshake") || e_lower.contains("auth") {
                            gettext("auth error")
                        } else {
                            gettext("error")
                        }
                    }
                };
                item_ui.set_ping(ping_val);

                if let Some(page) = page_weak_ping.upgrade() {
                    page.save_current_keys();
                    if let Some(w) = page.root().and_downcast::<crate::window::VrxxWindow>() {
                        w.update_active_connection_widget();
                    }
                }
            }
        });

        let raw_url_bg = raw_url;
        let is_connected = self.imp().is_connected.get();
        let is_key_active = item_clone.is_active();

        std::thread::spawn(move || {
            let parsed = crate::domain::key_parser::parse_vpn_key(&raw_url_bg).unwrap_or_default();
            if parsed.validate().is_err() {
                let _ = sender.send_blocking(crate::services::ping::PingResult::Error(
                    "Невалидный ключ".to_string(),
                ));
                return;
            }

            let target = crate::services::ping::PingTarget {
                id: parsed.name.clone(),
                host: parsed.host.clone(),
                port: if parsed.port == 0 { 443 } else { parsed.port },
                raw_url: raw_url_bg,
            };

            let settings = crate::settings::SettingsManager::new().load();
            let algorithm = crate::services::ping::PingAlgorithm::parse(&settings.ping_algorithm);

            // Прокси передается ТОЛЬКО если данный конкретный ключ активен И демон подключен
            let proxy_url = if is_connected && is_key_active {
                Some(format!("socks5://127.0.0.1:{}", settings.socks_port))
            } else {
                None
            };

            let options = crate::services::ping::PingOptions {
                algorithm,
                target_url: settings.ping_target_url,
                timeout: std::time::Duration::from_secs(4),
                proxy_url,
                concurrency_limit: 1,
            };

            let ping_res = if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(crate::services::ping::ping_target(&target, &options))
            } else {
                crate::services::ping::PingResult::Timeout
            };

            let _ = sender.send_blocking(ping_res);
        });
    }

    /// Сохраняет текущее состояние списка ключей в персистентное хранилище `SettingsManager`.
    fn save_current_keys(&self) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let mut data = Vec::new();
            for i in 0..model.n_items() {
                if let Some(item) = model
                    .item(i)
                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                {
                    data.push(VpnKeyData {
                        name: item.name(),
                        protocol: item.protocol(),
                        is_active: item.is_active(),
                        traffic_down: item.traffic_down(),
                        traffic_up: item.traffic_up(),
                        time_connected: item.time_connected(),
                        ping: item.ping(),
                        location: item.location(),
                        country_code: item.country_code(),
                        city: item.city(),
                        isp: item.isp(),
                        timezone: item.timezone(),
                        url: item.url(),
                        group: item.group(),
                    });
                }
            }
            SettingsManager::new().save_keys(&data);
        }
    }

    /// Добавляет распарсенный ключ в модель и при необходимости инициирует подключение.
    pub fn import_key(&self, parsed: crate::domain::key_parser::ParsedKey, connect: bool) {
        self.import_key_with_group(parsed, connect, String::new());
    }

    /// Добавляет распарсенный ключ в модель с привязкой к группе и при необходимости инициирует подключение.
    pub fn import_key_with_group(
        &self,
        parsed: crate::domain::key_parser::ParsedKey,
        connect: bool,
        group: String,
    ) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let new_url = crate::domain::key_parser::build_vpn_key(&parsed);
            let raw_url = if new_url.is_empty() {
                parsed.raw_url
            } else {
                new_url
            };
            let new_key = VpnKeyObject::new_with_group(
                &parsed.name,
                &parsed.protocol,
                false,
                &raw_url,
                &group,
            );
            model.append(&new_key);
            self.save_current_keys();
            self.resolve_location_for_key(&new_key);
            self.rebuild_ui();

            if connect {
                self.set_active_key_internal(&new_key, true);
            }
        }
    }

    /// Разрешает геолокацию сервера: в приоритете эвристический парсинг названия
    /// (включая цепочки серверов и флаги), а при его отсутствии — GeoIP по входному хосту.
    pub fn resolve_location_for_key(&self, key_obj: &VpnKeyObject) {
        let name = key_obj.name();

        // 1. Приоритет: парсинг названия (защищает от Anycast и релейных цепочек вроде "Россия - Нидерланды")
        if let Some(info) = crate::services::geoip::parse_location_from_name(&name) {
            key_obj.set_location(info.format_display());
            key_obj.set_country_code(info.country_code);
            key_obj.set_city(info.city);
            self.save_current_keys();
            if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                w.update_active_connection_widget();
            }
            return;
        }

        // 2. Резерв: асинхронный GeoIP-запрос по входному хосту
        let host = key_obj.server_info();
        if host.is_empty() || host == "0.0.0.0" {
            return;
        }

        let key_weak = key_obj.downgrade();
        let page_weak = self.downgrade();

        glib::spawn_future_local(async move {
            if let Some(info) = crate::services::geoip::resolve_host_location(&host).await {
                if let Some(key) = key_weak.upgrade() {
                    key.set_location(info.format_display());
                    key.set_country_code(info.country_code);
                    key.set_city(info.city);
                    key.set_isp(info.isp);
                    if !info.timezone.is_empty() {
                        key.set_timezone(info.timezone);
                    }

                    if let Some(page) = page_weak.upgrade() {
                        page.save_current_keys();
                        if let Some(w) = page.root().and_downcast::<crate::window::VrxxWindow>() {
                            w.update_active_connection_widget();
                        }
                    }
                }
            }
        });
    }

    /// Асинхронно определяет истинную точку выхода (Egress IP) через активный туннель прокси
    /// и обновляет метаданные ключа 100% подтвержденными данными.
    pub fn resolve_live_egress_location(&self, key_obj: &VpnKeyObject, socks_port: u16) {
        let key_weak = key_obj.downgrade();
        let page_weak = self.downgrade();

        glib::spawn_future_local(async move {
            if let Some(info) = crate::services::geoip::resolve_egress_location(socks_port).await {
                if let Some(key) = key_weak.upgrade() {
                    tracing::info!(
                        "Live egress location verified for key '{}': {} ({})",
                        key.name(),
                        info.format_display(),
                        info.country_code
                    );
                    key.set_location(info.format_display());
                    key.set_country_code(info.country_code);
                    key.set_city(info.city);
                    key.set_isp(info.isp);
                    if !info.timezone.is_empty() {
                        key.set_timezone(info.timezone);
                    }

                    if let Some(page) = page_weak.upgrade() {
                        page.save_current_keys();
                        if let Some(w) = page.root().and_downcast::<crate::window::VrxxWindow>() {
                            w.update_active_connection_widget();
                        }
                    }
                }
            }
        });
    }

    // =========================================================================
    // 5. МОДАЛЬНЫЕ ДИАЛОГИ (UI DIALOGS)
    // =========================================================================

    /// Диалог отображения сведений о профиле (IP, Порт, Протокол, SNI, Публичный ключ)
    fn handle_info_key(&self, key: &VpnKeyObject) {
        let current_settings = SettingsManager::new().load();
        key.set_hide_ip(current_settings.streamer_mode);

        let parsed_opt = crate::domain::key_parser::parse_vpn_key(&key.url()).ok();
        let s_info = key.server_info();
        let raw_host = if !s_info.is_empty() && s_info != "0.0.0.0" {
            s_info
        } else if let Some(ref p) = parsed_opt {
            p.host.clone()
        } else {
            "0.0.0.0".to_string()
        };

        let hide = key.hide_ip();
        let display_ip = if hide {
            "***.***.***.***".to_string()
        } else {
            raw_host
        };
        let display_loc = if hide {
            "***".to_string()
        } else {
            key.location()
        };
        let display_tz = if hide {
            "***".to_string()
        } else {
            key.timezone()
        };

        let mut body = format!(
            "<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}",
            gettext("Server address"),
            display_ip,
            gettext("Location"),
            display_loc,
            gettext("Timezone"),
            display_tz,
            gettext("Protocol"),
            key.protocol()
        );

        if !key.isp().is_empty() {
            let display_isp = if hide { "***".to_string() } else { key.isp() };
            body.push_str(&format!(
                "\n<b>{}</b>: {}",
                gettext("ISP / Host"),
                display_isp
            ));
        }

        if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&key.url()) {
            let display_port = if hide {
                "***".to_string()
            } else {
                parsed.port.to_string()
            };
            body.push_str(&format!("\n<b>{}</b>: {}", gettext("Port"), display_port));

            if let Some(net) = parsed.query_params.get("type") {
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Network"), net));
            }
            if let Some(sec) = parsed.query_params.get("security") {
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Security"), sec));
            }
            if let Some(sni) = parsed.query_params.get("sni") {
                let display_sni = if hide { "***".to_string() } else { sni.clone() };
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("SNI"), display_sni));
            }
            if let Some(fp) = parsed.query_params.get("fp") {
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Fingerprint"), fp));
            }
            if let Some(pbk) = parsed.query_params.get("pbk") {
                let display_pbk = if hide { "***".to_string() } else { pbk.clone() };
                body.push_str(&format!(
                    "\n<b>{}</b>: {}",
                    gettext("Public key"),
                    display_pbk
                ));
            }
            if let Some(flow) = parsed.query_params.get("flow") {
                if !flow.is_empty() {
                    body.push_str(&format!("\n<b>{}</b>: {}", gettext("Flow"), flow));
                }
            }
        }

        let label = gtk::Label::builder()
            .label(&body)
            .use_markup(true)
            .halign(gtk::Align::Start)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let clamp = adw::Clamp::builder()
            .maximum_size(580)
            .child(&label)
            .build();

        let dialog = adw::AlertDialog::builder()
            .heading(key.name())
            .extra_child(&clamp)
            .build();

        dialog.add_response("close", &gettext("Close"));
        dialog.set_close_response("close");

        if let Some(root) = self.root().and_downcast::<gtk::Window>() {
            dialog.present(Some(&root));
        }
    }

    /// Отображение диалога QR-кода профиля
    fn handle_qr_code_key(&self, key: &VpnKeyObject) {
        if let Some(root) = self.root().and_downcast::<gtk::Window>() {
            crate::ui::qr_dialog::show_qr_dialog(&root, &key.name(), &key.url());
        }
    }

    /// Диалог редактирования параметров ключа (название, хост, порт, пароль/UUID)
    fn handle_edit_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_obj_clone = key.clone();
        let key_url = key.url();

        let parsed = match crate::domain::key_parser::parse_vpn_key(&key_url) {
            Ok(p) => p,
            Err(_) => return,
        };

        crate::ui::key_dialog::VrxxKeyDialog::present_edit(self, &parsed, move |updated_key| {
            if let Some(page) = page_weak.upgrade() {
                let new_url = if !updated_key.raw_url.is_empty() {
                    updated_key.raw_url.clone()
                } else {
                    crate::domain::key_parser::build_vpn_key(&updated_key)
                };
                key_obj_clone.set_name(updated_key.name);
                key_obj_clone.set_protocol(updated_key.protocol);
                key_obj_clone.set_url(new_url);
                page.save_current_keys();
                page.rebuild_ui();
            }
        });
    }

    /// Диалог подтверждения удаления VPN-ключа
    fn handle_delete_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_name = key.name();

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete VPN key"))
            .body(format!(
                "{} '{}'?",
                gettext("Are you sure you want to delete"),
                key_name
            ))
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("delete", &gettext("Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let key_name_str = key_name.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                if let Some(page) = page_weak.upgrade() {
                    if let Some(model) = page.imp().model.borrow().as_ref() {
                        let mut target_index = None;
                        for i in 0..model.n_items() {
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                if item.name() == key_name_str {
                                    target_index = Some(i);
                                    break;
                                }
                            }
                        }
                        if let Some(index) = target_index {
                            let item = model
                                .item(index)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok());
                            let was_active = item.is_some_and(|it| it.is_active());
                            model.remove(index);
                            page.save_current_keys();
                            page.rebuild_ui();
                            if was_active {
                                page.update_disconnect_action_state();
                            }
                        }
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Обновляет доступность действия Disconnect в зависимости от наличия активного ключа.
    fn update_disconnect_action_state(&self) {
        use gio::prelude::ActionMapExt;

        if let Some(group) = self.imp().action_group.borrow().as_ref() {
            if let Some(action) = group
                .lookup_action("disconnect")
                .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            {
                let mut has_active = false;
                if let Some(model) = self.imp().model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model
                            .item(i)
                            .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                        {
                            if item.is_active() {
                                has_active = true;
                                break;
                            }
                        }
                    }
                }
                action.set_enabled(has_active);
            }
        }
    }

    // =========================================================================
    // 6. РЕГИСТРАЦИЯ ДЕЙСТВИЙ (GACTIONS)
    // =========================================================================
    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        self.imp().action_group.replace(Some(action_group.clone()));

        // Действие: Добавить профиль VPN (расширенный диалог ручной настройки и быстрого импорта)
        let add_action = gio::SimpleAction::new("add_key", None);
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak.upgrade() {
                let page_weak_add = page.downgrade();
                let page_weak_conn = page.downgrade();

                crate::ui::key_dialog::VrxxKeyDialog::present_add(
                    &page,
                    move |parsed| {
                        if let Some(p) = page_weak_add.upgrade() {
                            p.import_key(parsed, false);
                        }
                    },
                    move |parsed| {
                        if let Some(p) = page_weak_conn.upgrade() {
                            p.import_key(parsed, true);
                        }
                    },
                );
            }
        });
        action_group.add_action(&add_action);

        // Действие: Импорт из буфера обмена
        let import_clip_action = gio::SimpleAction::new("import_clipboard", None);
        let page_weak_clip = self.downgrade();
        import_clip_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_clip.upgrade() {
                let Some(display) = gdk::Display::default() else {
                    tracing::warn!("GDK display unavailable for reading clipboard");
                    return;
                };
                let clipboard = display.clipboard();

                let p_weak = page.downgrade();
                clipboard.read_text_async(gio::Cancellable::NONE, move |res| {
                    if let Ok(Some(text)) = res {
                        if let Some(p) = p_weak.upgrade() {
                            match crate::domain::key_parser::parse_vpn_key(&text) {
                                Ok(parsed) => {
                                    if let Some(model) = p.imp().model.borrow().as_ref() {
                                        let new_key = VpnKeyObject::new(
                                            &parsed.name,
                                            &parsed.protocol,
                                            false,
                                            &parsed.raw_url,
                                        );
                                        model.append(&new_key);
                                        p.save_current_keys();
                                        p.resolve_location_for_key(&new_key);
                                        p.rebuild_ui();
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse key from clipboard: {e}");
                                }
                            }
                        }
                    }
                });
            }
        });
        action_group.add_action(&import_clip_action);

        // Действие: Добавить группу
        let add_group_action = gio::SimpleAction::new("add_group", None);
        let page_weak_group = self.downgrade();
        add_group_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_group.upgrade() {
                page.prompt_add_group();
            }
        });
        action_group.add_action(&add_group_action);

        // Действие: Добавить подписку
        let add_sub_action = gio::SimpleAction::new("add_subscription", None);
        let page_weak_sub = self.downgrade();
        add_sub_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_sub.upgrade() {
                page.prompt_add_subscription();
            }
        });
        action_group.add_action(&add_sub_action);

        // Действие: Отключение
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        let page_weak_disconnect = self.downgrade();
        disconnect_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_disconnect.upgrade() {
                page.disconnect();
            }
        });
        action_group.add_action(&disconnect_action);

        // Действие: Пинговать выбранный ключ
        let ping_selected_action = gio::SimpleAction::new("ping_selected", None);
        let page_weak_selected = self.downgrade();
        ping_selected_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_selected.upgrade() {
                page.ping_selected();
            }
        });
        action_group.add_action(&ping_selected_action);

        self.insert_action_group("vpn", Some(&action_group));

        self.update_disconnect_action_state();
    }

    /// Выполняет отключение активного VPN соединения.
    pub fn disconnect(&self) {
        if let Some(last) = *self.imp().last_disconnect.borrow() {
            if last.elapsed() < std::time::Duration::from_millis(2500) {
                self.show_toast(&gettext("Please wait before disconnecting again"));
                return;
            }
        }
        self.imp()
            .last_disconnect
            .replace(Some(std::time::Instant::now()));

        tracing::info!("Disconnecting VPN via REST API");
        let page_weak = self.downgrade();
        glib::spawn_future_local(async move {
            if let Some(page) = page_weak.upgrade() {
                page.imp().connecting_target_url.replace(None);
            }
            let proxy = crate::ipc::DaemonClient::new();
            if let Err(e) = proxy.stop_proxy().await {
                tracing::error!("Failed to stop backend via REST API: {}", e);
            }
            if let Some(page) = page_weak.upgrade() {
                page.handle_daemon_status_change(crate::protocol::DaemonStatus::Disconnected);
                crate::backend::CoreBackend::update_system_proxy(false);
            }
        });
    }

    // =========================================================================
    // 7. УПРАВЛЕНИЕ ГРУППАМИ И ПОДПИСКАМИ (GROUPS & SUBSCRIPTIONS)
    // =========================================================================

    /// Добавление ключа напрямую в указанную группу.
    pub fn add_key_to_group(&self, group_id: &str) {
        let page_weak_add = self.downgrade();
        let page_weak_conn = self.downgrade();
        let g_id = group_id.to_string();
        let g_id2 = group_id.to_string();

        crate::ui::key_dialog::VrxxKeyDialog::present_add(
            self,
            move |parsed| {
                if let Some(p) = page_weak_add.upgrade() {
                    p.import_key_with_group(parsed, false, g_id.clone());
                }
            },
            move |parsed| {
                if let Some(p) = page_weak_conn.upgrade() {
                    p.import_key_with_group(parsed, true, g_id2.clone());
                }
            },
        );
    }

    /// Отображает диалог создания новой группы ключей.
    pub fn prompt_add_group(&self) {
        let page_weak = self.downgrade();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);

        let entry = gtk::Entry::builder()
            .placeholder_text(gettext("Group Name"))
            .hexpand(true)
            .activates_default(true)
            .build();
        vbox.append(&entry);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Add Group"))
            .body(gettext("Enter a name for the new key group:"))
            .extra_child(&vbox)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("add", &gettext("Add"));
        dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("add"));
        dialog.set_close_response("cancel");

        let entry_clone = entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "add" {
                let name = entry_clone.text().trim().to_string();
                if !name.is_empty() {
                    if let Some(page) = page_weak.upgrade() {
                        let new_id = format!("grp-{}", glib::uuid_string_random());
                        let mgr = SettingsManager::new();
                        let mut groups = mgr.load_groups();
                        groups.push(VpnGroupData {
                            id: new_id,
                            name,
                            is_expanded: true,
                        });
                        mgr.save_groups(&groups);
                        page.rebuild_ui();
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Отображает диалог переименования группы.
    pub fn prompt_rename_group(&self, group_id: &str) {
        let mgr = SettingsManager::new();
        let groups = mgr.load_groups();
        let current_name = groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.name.clone())
            .unwrap_or_default();

        let page_weak = self.downgrade();
        let g_id = group_id.to_string();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);

        let entry = gtk::Entry::builder()
            .text(&current_name)
            .hexpand(true)
            .activates_default(true)
            .build();
        vbox.append(&entry);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Rename Group"))
            .body(gettext("Enter a new name for the group:"))
            .extra_child(&vbox)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("rename", &gettext("Rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");

        let entry_clone = entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "rename" {
                let new_name = entry_clone.text().trim().to_string();
                if !new_name.is_empty() {
                    if let Some(page) = page_weak.upgrade() {
                        let mgr = SettingsManager::new();
                        let mut groups = mgr.load_groups();
                        if let Some(g) = groups.iter_mut().find(|g| g.id == g_id) {
                            g.name = new_name;
                        }
                        mgr.save_groups(&groups);
                        page.rebuild_ui();
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Удаление группы. Ключи внутри группы не удаляются, а переводятся в состояние без группы.
    pub fn delete_group(&self, group_id: &str) {
        let mgr = SettingsManager::new();
        let groups = mgr.load_groups();
        let group_name = groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| gettext("Group"));

        let page_weak = self.downgrade();
        let g_id = group_id.to_string();

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete Group"))
            .body(format!(
                "{} '{}'? {}",
                gettext("Are you sure you want to delete"),
                group_name,
                gettext("Keys in this group will not be deleted, but moved to ungrouped.")
            ))
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("delete", &gettext("Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");

        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                if let Some(page) = page_weak.upgrade() {
                    // Удаляем группу из настроек
                    let mgr = SettingsManager::new();
                    let mut groups = mgr.load_groups();
                    groups.retain(|g| g.id != g_id);
                    mgr.save_groups(&groups);

                    // Сбрасываем group для ключей, которые были в этой группе
                    if let Some(model) = page.imp().model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                if item.group() == g_id {
                                    item.set_group(String::new());
                                }
                            }
                        }
                    }
                    page.save_current_keys();
                    page.rebuild_ui();
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Отображает диалог добавления новой подписки по URL.
    pub fn prompt_add_subscription(&self) {
        let page_weak = self.downgrade();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);

        let name_entry = gtk::Entry::builder()
            .placeholder_text(gettext("Subscription Name"))
            .hexpand(true)
            .build();
        vbox.append(&name_entry);

        let url_entry = gtk::Entry::builder()
            .placeholder_text("https://example.com/sub/...")
            .hexpand(true)
            .activates_default(true)
            .build();
        vbox.append(&url_entry);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Add Subscription"))
            .body(gettext("Enter a subscription name and URL:"))
            .extra_child(&vbox)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("add", &gettext("Add"));
        dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("add"));
        dialog.set_close_response("cancel");

        let name_clone = name_entry.clone();
        let url_clone = url_entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "add" {
                let name = name_clone.text().trim().to_string();
                let url = url_clone.text().trim().to_string();
                if !name.is_empty() && !url.is_empty() {
                    if let Some(page) = page_weak.upgrade() {
                        let new_id = format!("sub-{}", glib::uuid_string_random());
                        let mgr = SettingsManager::new();
                        let mut subs = mgr.load_subscriptions();
                        subs.push(VpnSubscriptionData {
                            id: new_id.clone(),
                            name,
                            url,
                            last_updated: String::new(),
                            is_expanded: true,
                        });
                        mgr.save_subscriptions(&subs);
                        page.rebuild_ui();
                        page.update_subscription(&new_id);
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Отображает диалог редактирования параметров подписки.
    pub fn prompt_edit_subscription(&self, sub_id: &str) {
        let mgr = SettingsManager::new();
        let subs = mgr.load_subscriptions();
        let Some(existing_sub) = subs.iter().find(|s| s.id == sub_id).cloned() else {
            return;
        };

        let page_weak = self.downgrade();
        let s_id = sub_id.to_string();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);

        let name_entry = gtk::Entry::builder()
            .text(&existing_sub.name)
            .placeholder_text(gettext("Subscription Name"))
            .hexpand(true)
            .build();
        vbox.append(&name_entry);

        let url_entry = gtk::Entry::builder()
            .text(&existing_sub.url)
            .placeholder_text("https://example.com/sub/...")
            .hexpand(true)
            .activates_default(true)
            .build();
        vbox.append(&url_entry);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Edit Subscription"))
            .body(gettext("Modify the subscription parameters:"))
            .extra_child(&vbox)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("save", &gettext("Save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let name_clone = name_entry.clone();
        let url_clone = url_entry.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "save" {
                let name = name_clone.text().trim().to_string();
                let url = url_clone.text().trim().to_string();
                if !name.is_empty() && !url.is_empty() {
                    if let Some(page) = page_weak.upgrade() {
                        let mgr = SettingsManager::new();
                        let mut subs = mgr.load_subscriptions();
                        if let Some(sub) = subs.iter_mut().find(|s| s.id == s_id) {
                            sub.name = name;
                            sub.url = url;
                        }
                        mgr.save_subscriptions(&subs);
                        page.rebuild_ui();
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Загружает и обновляет ключи подписки по сети.
    pub fn update_subscription(&self, sub_id: &str) {
        let mgr = SettingsManager::new();
        let subs = mgr.load_subscriptions();
        let Some(sub) = subs.iter().find(|s| s.id == sub_id).cloned() else {
            return;
        };

        self.show_toast(&format!(
            "{} '{}'...",
            gettext("Updating subscription"),
            sub.name
        ));

        let page_weak = self.downgrade();
        let s_id = sub_id.to_string();
        let sub_name = sub.name.clone();
        let sub_url = sub.url.clone();

        glib::spawn_future_local(async move {
            match crate::domain::subscription::fetch_subscription(&sub_url).await {
                Ok(keys) => {
                    let count = keys.len();
                    tracing::info!("Fetched {} keys for subscription '{}'", count, sub_name);

                    if let Some(page) = page_weak.upgrade() {
                        if let Some(model) = page.imp().model.borrow().as_ref() {
                            // 1. Удаляем старые ключи этой подписки из модели
                            let mut i = model.n_items();
                            while i > 0 {
                                i -= 1;
                                if let Some(item) = model
                                    .item(i)
                                    .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                                {
                                    if item.group() == s_id {
                                        model.remove(i);
                                    }
                                }
                            }

                            // 2. Добавляем новые ключи
                            for k in keys {
                                let new_key = VpnKeyObject::new_with_group(
                                    &k.name,
                                    &k.protocol,
                                    false,
                                    &k.raw_url,
                                    &s_id,
                                );
                                model.append(&new_key);
                                page.resolve_location_for_key(&new_key);
                            }
                        }

                        // 3. Обновляем время последней синхронизации подписки
                        let mgr = SettingsManager::new();
                        let mut all_subs = mgr.load_subscriptions();
                        if let Some(sub_entry) = all_subs.iter_mut().find(|s| s.id == s_id) {
                            sub_entry.last_updated =
                                chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
                        }
                        mgr.save_subscriptions(&all_subs);

                        page.save_current_keys();
                        page.rebuild_ui();
                        page.show_toast(&format!(
                            "{}: {} {} {}",
                            sub_name,
                            gettext("updated"),
                            count,
                            gettext("keys")
                        ));
                    }
                }
                Err(err) => {
                    tracing::error!("Failed to update subscription '{}': {}", sub_name, err);
                    if let Some(page) = page_weak.upgrade() {
                        page.show_toast(&format!(
                            "{}: {} ({})",
                            sub_name,
                            gettext("failed to update subscription"),
                            err
                        ));
                    }
                }
            }
        });
    }

    /// Удаление подписки и связанных с ней ключей.
    pub fn delete_subscription(&self, sub_id: &str) {
        let mgr = SettingsManager::new();
        let subs = mgr.load_subscriptions();
        let sub_name = subs
            .iter()
            .find(|s| s.id == sub_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| gettext("Subscription"));

        let page_weak = self.downgrade();
        let s_id = sub_id.to_string();

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete Subscription"))
            .body(format!(
                "{} '{}'? {}",
                gettext("Are you sure you want to delete"),
                sub_name,
                gettext("All keys imported from this subscription will also be deleted.")
            ))
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("delete", &gettext("Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");

        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                if let Some(page) = page_weak.upgrade() {
                    // Удаляем саму подписку из настроек
                    let mgr = SettingsManager::new();
                    let mut subs = mgr.load_subscriptions();
                    subs.retain(|s| s.id != s_id);
                    mgr.save_subscriptions(&subs);

                    // Удаляем все ключи этой подписки из модели
                    let mut was_active = false;
                    if let Some(model) = page.imp().model.borrow().as_ref() {
                        let mut i = model.n_items();
                        while i > 0 {
                            i -= 1;
                            if let Some(item) = model
                                .item(i)
                                .and_then(|obj| obj.downcast::<VpnKeyObject>().ok())
                            {
                                if item.group() == s_id {
                                    if item.is_active() {
                                        was_active = true;
                                    }
                                    model.remove(i);
                                }
                            }
                        }
                    }

                    page.save_current_keys();
                    page.rebuild_ui();
                    if was_active {
                        page.update_disconnect_action_state();
                    }
                }
            }
        });

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }

    /// Перемещение ключа в другую группу или удаление из группы.
    pub fn handle_move_key_to_group(&self, key: &VpnKeyObject) {
        let mgr = SettingsManager::new();
        let groups = mgr.load_groups();

        let page_weak = self.downgrade();
        let key_clone = key.clone();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        vbox.set_margin_start(12);
        vbox.set_margin_end(12);
        vbox.set_margin_top(6);
        vbox.set_margin_bottom(6);

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Move to Group"))
            .body(format!(
                "{}: '{}'",
                gettext("Select a group for key"),
                key.name()
            ))
            .extra_child(&vbox)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.set_close_response("cancel");

        // Опция 1: Без группы (Ungrouped)
        let row_none = adw::ActionRow::builder()
            .title(gettext("None (Ungrouped)"))
            .activatable(true)
            .build();
        if key.group().is_empty() {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            row_none.add_suffix(&check);
        }

        let p_weak = page_weak.clone();
        let k_c = key_clone.clone();
        let d_weak = dialog.downgrade();
        row_none.connect_activated(move |_| {
            k_c.set_group(String::new());
            if let Some(p) = p_weak.upgrade() {
                p.save_current_keys();
                p.rebuild_ui();
            }
            if let Some(d) = d_weak.upgrade() {
                d.force_close();
            }
        });
        list_box.append(&row_none);

        // Пользовательские группы
        for g in &groups {
            let row = adw::ActionRow::builder()
                .title(&g.name)
                .activatable(true)
                .build();

            if key.group() == g.id {
                let check = gtk::Image::from_icon_name("object-select-symbolic");
                row.add_suffix(&check);
            }

            let p_w = page_weak.clone();
            let k_c2 = key_clone.clone();
            let d_w = dialog.downgrade();
            let g_id = g.id.clone();
            row.connect_activated(move |_| {
                k_c2.set_group(g_id.clone());
                if let Some(p) = p_w.upgrade() {
                    p.save_current_keys();
                    p.rebuild_ui();
                }
                if let Some(d) = d_w.upgrade() {
                    d.force_close();
                }
            });
            list_box.append(&row);
        }

        vbox.append(&list_box);

        if let Some(root) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            dialog.present(Some(&root));
        }
    }
}
