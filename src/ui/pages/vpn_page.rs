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

use crate::settings::{SettingsManager, VpnKeyData};
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
        pub keys_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,

        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        pub model: RefCell<Option<gio::ListStore>>,
        pub backend: RefCell<crate::backend::CoreBackend>,
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
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            VrxxVpnKeyRow::static_type();
            adw::WindowTitle::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxVpnPage {
        fn constructed(&self) {
            self.parent_constructed();

            // Инициализация бэкенда ядра
            self.backend.replace(crate::backend::CoreBackend::new());

            self.obj().setup_model();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
            self.obj().start_metrics_timer();
            self.obj().setup_dbus_listener();
            self.obj().setup_daemon_listener();

            setup_primary_menu(&self.primary_menu_btn.get());
        }
    }

    impl WidgetImpl for VrxxVpnPage {}
    impl BinImpl for VrxxVpnPage {}
}

glib::wrapper! {
    /// Обертка GObject для страницы VPN
    pub struct VrxxVpnPage(ObjectSubclass<imp::VrxxVpnPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxVpnPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxVpnPage {
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

            let key_obj = VpnKeyObject::new(&k.name, &k.protocol, is_key_active, &k.url);
            if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&k.url) {
                key_obj.set_server_info(parsed.host);
            }
            key_obj.set_traffic_down(k.traffic_down);
            key_obj.set_traffic_up(k.traffic_up);
            key_obj.set_time_connected(k.time_connected);
            key_obj.set_ping(k.ping);
            key_obj.set_location(k.location);
            key_obj.set_timezone(k.timezone);
            key_obj.set_hide_ip(streamer_mode);
            model.append(&key_obj);

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

        // Устанавливаем заглушку (empty state) для пустого списка по HIG
        let status_page = adw::StatusPage::builder()
            .icon_name("network-vpn-symbolic")
            .title(gettext("No VPN Connections"))
            .description(gettext(
                "Add a new connection using the buttons above to get started.",
            ))
            .build();
        self.imp().keys_list.set_placeholder(Some(&status_page));

        // Привязываем модель к ListBox
        let page_weak = self.downgrade();
        self.imp().keys_list.bind_model(Some(&model), move |item| {
            let Some(key_obj) = item.downcast_ref::<VpnKeyObject>() else {
                return gtk::ListBoxRow::new().upcast::<gtk::Widget>();
            };
            let row = VrxxVpnKeyRow::new();
            row.bind(key_obj);

            // Обработчики сигналов из строки ключа:

            // Действие: Редактировать
            let page_weak_edit = page_weak.clone();
            let key_obj_edit = key_obj.clone();
            row.connect_local("request-edit", false, move |_| {
                if let Some(page) = page_weak_edit.upgrade() {
                    page.handle_edit_key(&key_obj_edit);
                }
                None
            });

            // Действие: Информация
            let page_weak_info = page_weak.clone();
            let key_obj_info = key_obj.clone();
            row.connect_local("request-info", false, move |_| {
                if let Some(page) = page_weak_info.upgrade() {
                    page.handle_info_key(&key_obj_info);
                }
                None
            });

            // Действие: Скопировать ссылку
            let page_weak_cl = page_weak.clone();
            let key_obj_cl = key_obj.clone();
            row.connect_local("request-copy-link", false, move |_| {
                if let Some(page) = page_weak_cl.upgrade() {
                    let clipboard = page.clipboard();
                    clipboard.set_text(&key_obj_cl.url());
                }
                None
            });

            // Действие: Скопировать JSON
            let page_weak_cj = page_weak.clone();
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
            let page_weak_del = page_weak.clone();
            let key_obj_del = key_obj.clone();
            row.connect_local("request-delete", false, move |_| {
                if let Some(page) = page_weak_del.upgrade() {
                    page.handle_delete_key(&key_obj_del);
                }
                None
            });

            // Действие: QR Код и Поделиться
            let page_weak_qr = page_weak.clone();
            let key_obj_qr = key_obj.clone();
            row.connect_local("request-qr-code", false, move |_| {
                if let Some(page) = page_weak_qr.upgrade() {
                    page.handle_qr_code_key(&key_obj_qr);
                }
                None
            });

            let page_weak_share = page_weak.clone();
            let key_obj_share = key_obj.clone();
            row.connect_local("request-share", false, move |_| {
                if let Some(page) = page_weak_share.upgrade() {
                    page.handle_qr_code_key(&key_obj_share);
                }
                None
            });

            // Действие: Ручной замер пинга
            let key_obj_ping = key_obj.clone();
            let page_weak_ping = page_weak.clone();
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

            row.upcast::<gtk::Widget>()
        });
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

            // Выбор строки и переключение активного профиля
            if let Some(key_row) = row.downcast_ref::<VrxxVpnKeyRow>() {
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
                    page.handle_daemon_status_change(&status);
                }
            }

            let events = client.subscribe_events();
            while let Ok(event) = events.recv().await {
                if let crate::daemon::DaemonEvent::StatusChanged(status) = event {
                    if let Some(page) = page_weak.upgrade() {
                        page.handle_daemon_status_change(&status);
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
    fn handle_daemon_status_change(&self, status: &str) {
        let imp = self.imp();
        let is_conn = status == "Connected";
        imp.is_connected.set(is_conn);

        match status {
            "Connected" => {
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
            "Disconnected" => {
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
            "Connecting" => {
                imp.window_title.set_subtitle(&gettext("Connecting..."));
                if let Some(w) = self.root().and_downcast::<crate::window::VrxxWindow>() {
                    w.update_status_state("Connecting", None);
                }
            }
            "Disconnecting" => {
                imp.window_title.set_subtitle(&gettext("Disconnecting..."));
            }
            "Error" => {
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
            _ => {}
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

            let config_json =
                crate::domain::singbox_config::build_singbox_config(&parsed, &app_settings);

            let core_type = "sing-box".to_string();
            let tun_mode = app_settings.tun_mode;
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
                tracing::info!("Connecting to VPN key via REST API: {}", item_clone.name());
                if let Err(e) = proxy.start_proxy(core_type, config_json, tun_mode).await {
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

                            // Автоматически включаем системный прокси GNOME, если не используется TUN
                            if !tun_mode {
                                let current_settings =
                                    crate::settings::SettingsManager::new().load();
                                if current_settings.set_system_proxy {
                                    tracing::info!("Auto-activating GNOME system proxy...");
                                    crate::backend::CoreBackend::update_system_proxy(true);
                                }
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

    /// Инициирует асинхронный замер задержки (ping) для указанного ключа.
    pub fn trigger_ping_key(&self, item_clone: &VpnKeyObject) {
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
                        timezone: item.timezone(),
                        url: item.url(),
                    });
                }
            }
            SettingsManager::new().save_keys(&data);
        }
    }

    /// Добавляет распарсенный ключ в модель и при необходимости инициирует подключение.
    pub fn import_key(&self, parsed: crate::domain::key_parser::ParsedKey, connect: bool) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let new_url = crate::domain::key_parser::build_vpn_key(&parsed);
            let raw_url = if new_url.is_empty() {
                parsed.raw_url
            } else {
                new_url
            };
            let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &raw_url);
            model.append(&new_key);
            self.save_current_keys();

            if connect {
                self.set_active_key_internal(&new_key, true);
            }
        }
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

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Edit VPN key"))
            .build();

        let name_row = adw::EntryRow::builder()
            .title(gettext("Name"))
            .text(&parsed.name)
            .build();
        let protocol_row = adw::EntryRow::builder()
            .title(gettext("Protocol"))
            .text(&parsed.protocol)
            .build();
        let host_row = adw::EntryRow::builder()
            .title(gettext("Server address"))
            .text(&parsed.host)
            .build();
        let port_row = adw::EntryRow::builder()
            .title(gettext("Port"))
            .text(parsed.port.to_string())
            .build();
        let uuid_row = adw::EntryRow::builder()
            .title(gettext("UUID / Password"))
            .text(&parsed.uuid)
            .build();

        let group_general = adw::PreferencesGroup::builder()
            .title(gettext("General"))
            .build();
        group_general.add(&name_row);

        let group_connection = adw::PreferencesGroup::builder()
            .title(gettext("Connection"))
            .build();
        group_connection.add(&protocol_row);
        group_connection.add(&host_row);
        group_connection.add(&port_row);
        group_connection.add(&uuid_row);

        let pref_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        pref_page.append(&group_general);
        pref_page.append(&group_connection);

        let clamp = adw::Clamp::builder()
            .maximum_size(580)
            .tightening_threshold(460)
            .child(&pref_page)
            .build();

        clamp.set_margin_top(18);
        clamp.set_margin_bottom(18);
        clamp.set_margin_start(12);
        clamp.set_margin_end(12);

        dialog.set_extra_child(Some(&clamp));
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("save", &gettext("Save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let parsed_clone = parsed.clone();
        let name_row_clone = name_row.clone();
        let protocol_row_clone = protocol_row.clone();
        let host_row_clone = host_row.clone();
        let port_row_clone = port_row.clone();
        let uuid_row_clone = uuid_row.clone();

        dialog.connect_response(None, move |_, response| {
            if response == "save" {
                if let Some(page) = page_weak.upgrade() {
                    let mut p = parsed_clone.clone();
                    p.name = name_row_clone.text().to_string();
                    p.protocol = protocol_row_clone.text().to_string();
                    p.host = host_row_clone.text().to_string();
                    p.port = port_row_clone.text().parse().unwrap_or(p.port);
                    p.uuid = uuid_row_clone.text().to_string();

                    let new_url = crate::domain::key_parser::build_vpn_key(&p);
                    key_obj_clone.set_name(p.name);
                    key_obj_clone.set_protocol(p.protocol);
                    key_obj_clone.set_url(new_url);
                    page.save_current_keys();
                }
            }
        });

        if let Some(root) = self.root() {
            dialog.present(Some(&root));
            name_row.grab_focus();
        }
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
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            let dialog = adw::AlertDialog::builder()
                .heading(gettext("Add VPN Profile"))
                .build();

            // Группа: Быстрый импорт по ссылке
            let group_quick = adw::PreferencesGroup::builder()
                .title(gettext("Quick Import from Link"))
                .build();

            let link_entry = adw::EntryRow::builder()
                .title(gettext("VPN Link (vless://, vmess://, etc.)"))
                .build();
            group_quick.add(&link_entry);

            // Группа: Основные параметры
            let group_general = adw::PreferencesGroup::builder()
                .title(gettext("General"))
                .build();

            let name_row = adw::EntryRow::builder()
                .title(gettext("Profile Name"))
                .text("New VPN Profile")
                .build();
            group_general.add(&name_row);

            let proto_dropdown = gtk::DropDown::from_strings(&[
                "VLESS",
                "VMess",
                "Trojan",
                "Shadowsocks",
                "Hysteria2",
                "TUIC",
                "WireGuard",
            ]);
            proto_dropdown.set_valign(gtk::Align::Center);

            let proto_row = adw::ActionRow::builder().title(gettext("Protocol")).build();
            proto_row.add_suffix(&proto_dropdown);
            group_general.add(&proto_row);

            // Группа: Параметры сервера
            let group_conn = adw::PreferencesGroup::builder()
                .title(gettext("Server Connection"))
                .build();

            let host_row = adw::EntryRow::builder()
                .title(gettext("Server address / Host"))
                .build();
            let port_row = adw::EntryRow::builder()
                .title(gettext("Port"))
                .text("443")
                .build();
            let uuid_row = adw::EntryRow::builder()
                .title(gettext("UUID / Password / Key"))
                .build();

            group_conn.add(&host_row);
            group_conn.add(&port_row);
            group_conn.add(&uuid_row);

            // Строка предварительного замера задержки (TCP ping)
            let latency_spinner = gtk::Spinner::builder()
                .spinning(false)
                .visible(false)
                .valign(gtk::Align::Center)
                .build();

            let btn_ping = gtk::Button::builder()
                .icon_name("network-transmit-receive-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Test Connection"))
                .build();

            let latency_row = adw::ActionRow::builder()
                .title(gettext("Latency Check"))
                .subtitle(gettext("Ready"))
                .build();
            latency_row.add_suffix(&latency_spinner);
            latency_row.add_suffix(&btn_ping);
            group_conn.add(&latency_row);

            // Группа: Параметры безопасности и TLS
            let group_sec = adw::PreferencesGroup::builder()
                .title(gettext("Security & Parameters"))
                .build();

            let sec_row = adw::EntryRow::builder()
                .title(gettext("Security Mode (reality/tls/none)"))
                .build();
            let sni_row = adw::EntryRow::builder()
                .title(gettext("SNI / Server Name"))
                .build();
            let pbk_row = adw::EntryRow::builder()
                .title(gettext("Reality Public Key (pbk)"))
                .build();
            let fp_row = adw::EntryRow::builder()
                .title(gettext("Fingerprint (uTLS)"))
                .build();
            let flow_row = adw::EntryRow::builder()
                .title(gettext("Flow (e.g. xtls-rprx-vision)"))
                .build();

            group_sec.add(&sec_row);
            group_sec.add(&sni_row);
            group_sec.add(&pbk_row);
            group_sec.add(&fp_row);
            group_sec.add(&flow_row);

            // Сборка контейнера
            let content_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(12)
                .build();
            content_box.append(&group_quick);
            content_box.append(&group_general);
            content_box.append(&group_conn);
            content_box.append(&group_sec);

            let clamp = adw::Clamp::builder()
                .maximum_size(580)
                .tightening_threshold(460)
                .child(&content_box)
                .build();
            clamp.set_margin_top(12);
            clamp.set_margin_bottom(12);
            clamp.set_margin_start(12);
            clamp.set_margin_end(12);

            dialog.set_extra_child(Some(&clamp));

            // Автозаполнение формы при вставке ссылки
            let name_r = name_row.clone();
            let proto_d = proto_dropdown.clone();
            let host_r = host_row.clone();
            let port_r = port_row.clone();
            let uuid_r = uuid_row.clone();
            let sec_r = sec_row.clone();
            let sni_r = sni_row.clone();
            let pbk_r = pbk_row.clone();
            let fp_r = fp_row.clone();
            let flow_r = flow_row.clone();

            link_entry.connect_changed(move |entry| {
                let text = entry.text();
                if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&text) {
                    name_r.set_text(&parsed.name);
                    let proto_idx = match parsed.protocol.to_uppercase().as_str() {
                        "VLESS" => 0,
                        "VMESS" => 1,
                        "TROJAN" => 2,
                        "SHADOWSOCKS" | "SS" => 3,
                        "HYSTERIA2" | "HY2" => 4,
                        "TUIC" => 5,
                        "WIREGUARD" | "WG" => 6,
                        _ => 0,
                    };
                    proto_d.set_selected(proto_idx);
                    host_r.set_text(&parsed.host);
                    port_r.set_text(&parsed.port.to_string());
                    uuid_r.set_text(&parsed.uuid);
                    sec_r.set_text(
                        parsed
                            .query_params
                            .get("security")
                            .map(|s| s.as_str())
                            .unwrap_or(""),
                    );
                    sni_r.set_text(
                        parsed
                            .query_params
                            .get("sni")
                            .map(|s| s.as_str())
                            .unwrap_or(""),
                    );
                    pbk_r.set_text(
                        parsed
                            .query_params
                            .get("pbk")
                            .map(|s| s.as_str())
                            .unwrap_or(""),
                    );
                    fp_r.set_text(
                        parsed
                            .query_params
                            .get("fp")
                            .map(|s| s.as_str())
                            .unwrap_or(""),
                    );
                    flow_r.set_text(
                        parsed
                            .query_params
                            .get("flow")
                            .map(|s| s.as_str())
                            .unwrap_or(""),
                    );
                }
            });

            // Обработчик кнопки пинга
            let host_r_p = host_row.clone();
            let port_r_p = port_row.clone();
            let latency_r_p = latency_row.clone();
            let latency_sp_p = latency_spinner.clone();

            btn_ping.connect_clicked(move |_| {
                let host = host_r_p.text().to_string();
                let port = port_r_p.text().parse::<u16>().unwrap_or(443);
                if host.trim().is_empty() {
                    latency_r_p.set_subtitle(&gettext("Enter host address first"));
                    return;
                }
                latency_sp_p.set_visible(true);
                latency_sp_p.set_spinning(true);
                latency_r_p.set_subtitle(&gettext("Measuring ping..."));

                let (tx, rx) = async_channel::unbounded::<(bool, u128)>();
                let lat_r = latency_r_p.clone();
                let lat_sp = latency_sp_p.clone();

                glib::spawn_future_local(async move {
                    if let Ok((success, ms)) = rx.recv().await {
                        lat_sp.set_spinning(false);
                        lat_sp.set_visible(false);
                        if success {
                            lat_r.set_subtitle(&format!("{ms} ms"));
                        } else {
                            lat_r.set_subtitle(&gettext("Connection timeout"));
                        }
                    }
                });

                std::thread::spawn(move || {
                    use std::net::{TcpStream, ToSocketAddrs};
                    use std::time::{Duration, Instant};
                    let start = Instant::now();
                    let mut ok = false;
                    let addr = format!("{host}:{port}");
                    if let Ok(mut addrs) = addr.to_socket_addrs() {
                        if let Some(sock) = addrs.next() {
                            if let Ok(stream) =
                                TcpStream::connect_timeout(&sock, Duration::from_secs(3))
                            {
                                ok = true;
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                            }
                        }
                    }
                    let elapsed = start.elapsed().as_millis();
                    let _ = tx.send_blocking((ok, elapsed));
                });
            });

            // Кнопки действий
            dialog.add_response("cancel", &gettext("Cancel"));
            dialog.add_response("add", &gettext("Add Profile"));
            dialog.add_response("connect", &gettext("Add and Connect"));
            dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("connect"));
            dialog.set_close_response("cancel");

            let page_weak_dialog = page_weak.clone();
            let name_r_save = name_row.clone();
            let proto_d_save = proto_dropdown.clone();
            let host_r_save = host_row.clone();
            let port_r_save = port_row.clone();
            let uuid_r_save = uuid_row.clone();
            let sec_r_save = sec_row.clone();
            let sni_r_save = sni_row.clone();
            let pbk_r_save = pbk_row.clone();
            let fp_r_save = fp_row.clone();
            let flow_r_save = flow_row.clone();
            let link_e_save = link_entry.clone();

            dialog.connect_response(None, move |_, response| {
                if response == "add" || response == "connect" {
                    if let Some(page) = page_weak_dialog.upgrade() {
                        let proto_str = match proto_d_save.selected() {
                            0 => "VLESS",
                            1 => "VMess",
                            2 => "Trojan",
                            3 => "Shadowsocks",
                            4 => "Hysteria2",
                            5 => "TUIC",
                            6 => "WireGuard",
                            _ => "VLESS",
                        };

                        let mut query_params = std::collections::HashMap::new();
                        let sec = sec_r_save.text().trim().to_string();
                        if !sec.is_empty() {
                            query_params.insert("security".to_string(), sec);
                        }
                        let sni = sni_r_save.text().trim().to_string();
                        if !sni.is_empty() {
                            query_params.insert("sni".to_string(), sni);
                        }
                        let pbk = pbk_r_save.text().trim().to_string();
                        if !pbk.is_empty() {
                            query_params.insert("pbk".to_string(), pbk);
                        }
                        let fp = fp_r_save.text().trim().to_string();
                        if !fp.is_empty() {
                            query_params.insert("fp".to_string(), fp);
                        }
                        let flow = flow_r_save.text().trim().to_string();
                        if !flow.is_empty() {
                            query_params.insert("flow".to_string(), flow);
                        }

                        let host = host_r_save.text().trim().to_string();
                        let port = port_r_save.text().trim().parse::<u16>().unwrap_or(443);
                        let uuid = uuid_r_save.text().trim().to_string();
                        let name = {
                            let t = name_r_save.text().trim().to_string();
                            if t.is_empty() {
                                format!("{host}:{port}")
                            } else {
                                t
                            }
                        };

                        let mut parsed = crate::domain::key_parser::ParsedKey {
                            protocol: proto_str.to_string(),
                            name,
                            host,
                            port,
                            uuid,
                            query_params,
                            raw_url: String::new(),
                        };
                        let built = crate::domain::key_parser::build_vpn_key(&parsed);
                        let raw = link_e_save.text().trim().to_string();
                        parsed.raw_url = if !raw.is_empty() { raw } else { built };

                        let connect_now = response == "connect";
                        page.import_key(parsed, connect_now);
                    }
                }
            });

            if let Some(root) = page.root() {
                dialog.present(Some(&root));
                link_entry.grab_focus();
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

        // Действие: Отключение
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        let page_weak_disconnect = self.downgrade();
        disconnect_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_disconnect.upgrade() {
                page.disconnect();
            }
        });
        action_group.add_action(&disconnect_action);

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
                page.handle_daemon_status_change("Disconnected");
                crate::backend::CoreBackend::update_system_proxy(false);
            }
        });
    }
}
