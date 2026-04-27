use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use gettextrs::gettext;

use crate::ui::models::VpnKeyObject;
use crate::ui::components::vpn_key_row::VrxxVpnKeyRow;
use crate::ui::setup_primary_menu;
use crate::settings::{SettingsManager, VpnKeyData};
use crate::backend::VpnCore;

mod imp {
    use super::*;
    use std::cell::RefCell;

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
        pub bytes_down: RefCell<u64>,
        pub bytes_up: RefCell<u64>,
        pub is_sleeping: RefCell<bool>,
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

            // Инициализация бэкенда
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
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    // --- Раздел: Работа с данными (Model) ---
    fn setup_model(&self) {
        let model = gio::ListStore::new::<VpnKeyObject>();

        let settings = SettingsManager::new();
        let saved_keys = settings.load_keys();

        let loaded_settings = settings.load();
        let streamer_mode = loaded_settings.streamer_mode;
        let auto_connect = loaded_settings.connect_on_startup;
        
        for k in saved_keys {
            let key_obj = VpnKeyObject::new(&k.name, &k.protocol, k.is_active, &k.url);
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
                        // Run only once by checking if we are already connected
                        if !page.imp().backend.borrow().is_running() {
                            page.set_active_key(&key_clone);
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
            .description(gettext("Add a new connection using the buttons above to get started."))
            .build();
        self.imp().keys_list.set_placeholder(Some(&status_page));

        // Привязываем модель к ListBox
        let page_weak = self.downgrade();
        self.imp().keys_list.bind_model(Some(&model), move |item| {
            let key_obj = item.downcast_ref::<VpnKeyObject>().expect("Item should be a VpnKeyObject");
            let row = VrxxVpnKeyRow::new();
            row.bind(key_obj);

            // Обработчики сигналов из строки ключа

            // Изменить
            let page_weak_edit = page_weak.clone();
            let key_obj_edit = key_obj.clone();
            row.connect_local("request-edit", false, move |_| {
                if let Some(page) = page_weak_edit.upgrade() {
                    page.handle_edit_key(&key_obj_edit);
                }
                None
            });

            // Информация
            let page_weak_info = page_weak.clone();
            let key_obj_info = key_obj.clone();
            row.connect_local("request-info", false, move |_| {
                if let Some(page) = page_weak_info.upgrade() {
                    page.handle_info_key(&key_obj_info);
                }
                None
            });

            // Скопировать ссылку
            let page_weak_cl = page_weak.clone();
            let key_obj_cl = key_obj.clone();
            row.connect_local("request-copy-link", false, move |_| {
                if let Some(page) = page_weak_cl.upgrade() {
                    let clipboard = page.clipboard();
                    clipboard.set_text(&key_obj_cl.url());
                }
                None
            });

            // Скопировать JSON
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

            // Delete
            let page_weak_del = page_weak.clone();
            let key_obj_del = key_obj.clone();
            row.connect_local("request-delete", false, move |_| {
                if let Some(page) = page_weak_del.upgrade() {
                    page.handle_delete_key(&key_obj_del);
                }
                None
            });

            // Ручной пинг (ИСПРАВЛЕНО: Явное указание типов)
            let key_obj_ping = key_obj.clone();
            row.connect_local("request-ping", false, move |_| {
                let item_clone = key_obj_ping.clone();
                item_clone.set_ping(gettext("pinging..."));
                item_clone.set_is_loading(true);
                
                // XXX: Извлекаем хост и порт из ключа для прямого TCP-пинга
                let raw_url = item_clone.url();
                let parsed = crate::domain::key_parser::parse_vpn_key(&raw_url).unwrap_or_else(|_| crate::domain::key_parser::ParsedKey {
                    protocol: "".to_string(), name: "".to_string(), host: "127.0.0.1".to_string(), port: 0, uuid: "".to_string(), query_params: std::collections::HashMap::new(), raw_url: "".to_string()
                });
                let target_host = parsed.host.clone();
                let target_port = parsed.port;

                // 1. Создаем канал с ЯВНЫМ указанием типов <(bool, u128)>
                let (sender, receiver) = async_channel::unbounded::<(bool, u128)>();

                // 2. Получатель работает в главном UI потоке
                let item_clone_ui = item_clone.clone();
                glib::spawn_future_local(async move {
                    if let Ok((success, ms)) = receiver.recv().await {
                        item_clone_ui.set_is_loading(false);
                        if success {
                            item_clone_ui.set_ping(format!("{ms} ms"));
                        } else {
                            item_clone_ui.set_ping(gettext("timeout"));
                        }
                    }
                });

                // 3. Отправитель работает в фоне (БЕЗ объектов UI)
                std::thread::spawn(move || {
                    use std::net::{TcpStream, ToSocketAddrs};
                    use std::time::{Instant, Duration};

                    let start_ping = Instant::now();
                    let mut success = false;
                    let timeout = Duration::from_secs(2);

                    // Разрешаем доменное имя в IP, если это не IP
                    let addr = format!("{target_host}:{target_port}");
                    if let Ok(mut addrs) = addr.to_socket_addrs() {
                        if let Some(socket_addr) = addrs.next() {
                            if let Ok(stream) = TcpStream::connect_timeout(&socket_addr, timeout) {
                                success = true;
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                            }
                        }
                    }
                    
                    let _ = sender.send_blocking((success, start_ping.elapsed().as_millis()));
                });
                None
            });

            row.upcast::<gtk::Widget>()
        });
    }

    // ================================

    // --- Раздел: Сигналы и Колбэки ---
    fn setup_callbacks(&self) {
        let imp = self.imp();
        let page_weak = self.downgrade();

        imp.keys_list.connect_row_activated(move |_, row| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            if let Ok(key_row) = row.clone().downcast::<VrxxVpnKeyRow>() {
                if let Some(selected_item) = key_row.item() {
                    page.set_active_key(&selected_item);
                }
            }
        });

        // Listen for core restart requests
        let page_weak_restart = self.downgrade();
        glib::spawn_future_local(async move {
            let (_, receiver) = crate::settings::core_restart_channel();
            while let Ok(_) = receiver.recv().await {
                if let Some(page) = page_weak_restart.upgrade() {
                    // Find active key and reconnect
                    if let Some(model) = page.imp().model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                                if item.is_active() {
                                    page.set_active_key(&item);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

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
                            tracing::info!("System is going to sleep! Suspending VPN monitoring.");
                        } else {
                            tracing::info!("System woke up! Resuming VPN monitoring.");
                        }
                    }
                },
            );
        }
    }

    // ================================

    // --- Раздел: Мониторинг демона и метрики ---
    fn setup_daemon_listener(&self) {
        let page_weak = self.downgrade();
        glib::spawn_future_local(async move {
            match crate::ipc::get_system_connection().await {
                Ok(conn) => {
                    match crate::ipc::DaemonProxy::new(&conn).await {
                        Ok(proxy) => {
                            use futures_util::StreamExt;
                            // Initial status
                            if let Ok(status) = proxy.status().await {
                                if let Some(page) = page_weak.upgrade() {
                                    page.handle_daemon_status_change(&status);
                                }
                            }

                            // Watch for changes
                            let mut status_changes = proxy.receive_status_changed().await;

                            while let Some(_) = status_changes.next().await {
                                if let Ok(status) = proxy.status().await {
                                    if let Some(page) = page_weak.upgrade() {
                                        page.handle_daemon_status_change(&status);
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::error!("Failed to create DaemonProxy: {}", e),
                    }
                }
                Err(e) => tracing::error!("Failed to connect to D-Bus System Bus: {}", e),
            }
        });
    }

    // ================================

    // --- Раздел: Управление состоянием соединения ---
    fn handle_daemon_status_change(&self, status: &str) {
        let imp = self.imp();
        
        match status {
            "Connected" => {
                imp.window_title.set_subtitle(&gettext("Connected"));
                // Find and update the active item's time metrics
                if let Some(model) = imp.model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                            if item.is_active() {
                                item.set_is_loading(false);
                                item.set_is_error(false);
                                break;
                            }
                        }
                    }
                }
                imp.start_time.replace(Some(std::time::Instant::now()));
            }
            "Disconnected" => {
                imp.window_title.set_subtitle(&gettext("Disconnected"));
                imp.start_time.replace(None);
                
                // Reset all items
                if let Some(model) = imp.model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                            item.set_is_active(false);
                            item.set_is_loading(false);
                        }
                    }
                }
            }
            "Connecting" => {
                imp.window_title.set_subtitle(&gettext("Connecting..."));
            }
            "Disconnecting" => {
                imp.window_title.set_subtitle(&gettext("Disconnecting..."));
            }
            "Error" => {
                imp.window_title.set_subtitle(&gettext("Connection error"));
                // Handle error notification or dialog
            }
            _ => {}
        }
        
        self.update_disconnect_action_state();
    }

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
                    // Обновляем активный элемент
                    if let Some(model) = imp.model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                                if item.is_active() {
                                    // Проверка здоровья: если активно, но процесс мертв - это ошибка
                                    if !imp.backend.borrow().is_running() && !item.is_loading() {
                                        tracing::error!("Core process crash detected! Disconnecting...");
                                        item.set_is_active(false);
                                        item.set_is_error(true);
                                        imp.start_time.replace(None);
                                        page.update_disconnect_action_state();
                                        page.imp().window_title.set_subtitle(&gettext("Connection error"));

                                        // Читаем последние строки лога для отображения пользователю
                                        let mut error_details = String::from("Unknown error. Please check System logs.");
                                        let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx").join("logs");
                                        let log_path = log_dir.join("core.log");
                                        if let Ok(content) = std::fs::read_to_string(&log_path) {
                                            let lines: Vec<&str> = content.lines().rev().take(5).collect();
                                            if !lines.is_empty() {
                                                error_details = lines.into_iter().rev().collect::<Vec<&str>>().join("\n");
                                            }
                                        }

                                        let dialog = adw::AlertDialog::builder()
                                            .heading(gettext("Connection failure"))
                                            .body(format!("Core process unexpectedly terminated. Log details:\n\n{error_details}"))
                                            .build();
                                        dialog.add_response("ok", &gettext("OK"));
                                        if let Some(root) = page.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                                            dialog.present(Some(&root));
                                        }

                                        break;
                                    }

                                    let elapsed = start.elapsed().as_secs();
                                    let hours = elapsed / 3600;
                                    let mins = (elapsed % 3600) / 60;
                                    let secs = elapsed % 60;
                                    item.set_time_connected(format!("{hours:02}:{mins:02}:{secs:02}"));

                                    // Получение статистики трафика из Xray API (каждые 3 секунды)
                                    let item_clone_stats = item.clone();
                                    let core_bin = crate::settings::SettingsManager::new().load().core;
                                    let bin_name = if core_bin == "sing-box" { "sing-box" } else { "xray" };
                                    
                                    let should_stats = elapsed % 3 == 0;
                                    let page_clone = page.clone();

                                    if should_stats && imp.backend.borrow().is_running() {
                                        glib::spawn_future_local(async move {
                                            let mut args: Vec<&std::ffi::OsStr> = Vec::new();
                                            if bin_name == "xray" {
                                                args.push(std::ffi::OsStr::new("xray"));
                                                args.push(std::ffi::OsStr::new("api"));
                                                args.push(std::ffi::OsStr::new("statsquery"));
                                                args.push(std::ffi::OsStr::new("-server=127.0.0.1:10085"));
                                            } else {
                                                args.push(std::ffi::OsStr::new("curl"));
                                                args.push(std::ffi::OsStr::new("-s"));
                                                args.push(std::ffi::OsStr::new("http://127.0.0.1:9090/connections"));
                                            }
                                            
                                            if let Ok(subprocess) = gio::Subprocess::newv(&args, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE) {
                                                if let Ok((Some(stdout_bytes), _)) = subprocess.communicate_future(None).await {
                                                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout_str) {
                                                        let mut total_down: u64 = 0;
                                                        let mut total_up: u64 = 0;
                                                        
                                                        if bin_name == "xray" {
                                                            if let Some(stats) = json.get("stat").and_then(|s| s.as_array()) {
                                                                for stat in stats {
                                                                    if let (Some(name), Some(value)) = (stat.get("name").and_then(|n| n.as_str()), stat.get("value").and_then(|v| {
                                                                        if v.is_string() { Some(v.as_str().unwrap_or("0").to_string()) }
                                                                        else if v.is_number() { Some(v.to_string()) }
                                                                        else { None }
                                                                    })) {
                                                                        if let Ok(val) = value.parse::<u64>() {
                                                                            if name.ends_with("downlink") {
                                                                                total_down += val;
                                                                            } else if name.ends_with("uplink") {
                                                                                total_up += val;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            total_down = json.get("downloadTotal").and_then(|v| v.as_u64()).unwrap_or(0);
                                                            total_up = json.get("uploadTotal").and_then(|v| v.as_u64()).unwrap_or(0);
                                                        }
                                                        
                                                        if total_down > 0 || total_up > 0 {
                                                            let format_bytes = |b: u64| -> String {
                                                                let tb = 1_099_511_627_776_f64;
                                                                let gb = 1_073_741_824_f64;
                                                                let mb = 1_048_576_f64;
                                                                let kb = 1_024_f64;
                                                                let bf = b as f64;
                                                                
                                                                if bf >= tb { format!("{:.2} TB", bf / tb) }
                                                                else if bf >= gb { format!("{:.2} GB", bf / gb) }
                                                                else if bf >= mb { format!("{:.1} MB", bf / mb) }
                                                                else if bf >= kb { format!("{:.0} KB", bf / kb) }
                                                                else { format!("{b} B") }
                                                            };
                                                            let down_str = format_bytes(total_down);
                                                            let up_str = format_bytes(total_up);
                                                            item_clone_stats.set_traffic_down(down_str.clone());
                                                            item_clone_stats.set_traffic_up(up_str.clone());
                                                            
                                                            if let Some(w) = page_clone.root().and_downcast::<crate::window::VrxxWindow>() {
                                                                w.update_stats(
                                                                    &item_clone_stats.time_connected(),
                                                                    &down_str,
                                                                    &up_str
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    }

                                    // Пинг и проверка соединения асинхронно
                                    let is_loading = item.is_loading();
                                    
                                    // Улучшенная логика пинга: частые проверки при загрузке, редкие при работе
                                    let should_ping = if is_loading {
                                        // Пингуем на 2, 4, 6, 8, 10 секунде
                                        elapsed > 0 && elapsed <= 10 && elapsed % 2 == 0
                                    } else {
                                        elapsed > 0 && elapsed % 60 == 0 // Пингуем раз в минуту в фоне
                                    };

                                    // Если загрузка идет больше 12 секунд и нет успеха - прерываем соединение
                                    if is_loading && elapsed > 12 {
                                        tracing::warn!("Connection timeout (over 12 sec).");
                                        item.set_is_active(false);
                                        item.set_is_loading(false);
                                        item.set_is_error(true);
                                        
                                        imp.start_time.replace(None);
                                        page.update_disconnect_action_state();
                                        page.imp().window_title.set_subtitle(&gettext("Connection failure"));
                                        
                                        if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
                                            if let Some(window) = app.active_window() {
                                                if !window.is_active() {
                                                    let notification = gio::Notification::new(&gettext("Connection failure"));
                                                    notification.set_body(Some(&gettext("Failed to connect to the selected VPN key.")));
                                                    app.send_notification(Some("vpn_fail"), &notification);
                                                }
                                            }
                                        }
                                        break;
                                    }

                                    if should_ping {
                                        let item_clone = item.clone();
                                        let page_weak_ping = page_weak.clone();
                                        let is_currently_loading = item_clone.is_loading();
                                        let socks_port = crate::settings::SettingsManager::new().load().socks_port;
                                        
                                        // 1. Создаем канал (ИСПРАВЛЕНО: Явное указание типов)
                                        let (sender, receiver) = async_channel::unbounded::<(bool, u128, String, String, String)>();

                                        // 2. UI Поток ловит результаты и обновляет интерфейс
                                        let item_clone_ui = item_clone.clone();
                                        let page_weak_ui = page_weak_ping.clone();
                                        glib::spawn_future_local(async move {
                                            if let Ok((success, ms, ip, country, tz)) = receiver.recv().await {
                                                if success {
                                                    item_clone_ui.set_ping(format!("{ms} ms"));
                                                    if !ip.is_empty() { item_clone_ui.set_server_info(ip); }
                                                    if !country.is_empty() { item_clone_ui.set_location(country); }
                                                    if !tz.is_empty() { item_clone_ui.set_timezone(tz); }

                                                    item_clone_ui.set_is_loading(false);
                                                    item_clone_ui.set_is_error(false);
                                                    if let Some(page) = page_weak_ui.upgrade() {
                                                        let new_subtitle = format!("{} {}", item_clone_ui.name(), gettext("Connected"));
                                                        page.imp().window_title.set_subtitle(&new_subtitle);
                                                    }
                                                } else {
                                                    item_clone_ui.set_ping(gettext("timeout"));
                                                    item_clone_ui.set_is_loading(false);
                                                    item_clone_ui.set_is_error(true);
                                                    if is_currently_loading {
                                                        if let Some(page) = page_weak_ui.upgrade() {
                                                            page.imp().window_title.set_subtitle(&gettext("Connection timeout"));
                                                        }
                                                    }
                                                }
                                            }
                                        });

                                        // 3. Фоновый поток (только сетевые запросы)
                                        let raw_url_bg = item.url();
                                        std::thread::spawn(move || {
                                            let start_ping = std::time::Instant::now();
                                            let mut success = false;
                                            let mut ms = 0;
                                            let mut ip = String::new();
                                            let mut country = String::new();
                                            let mut tz = String::new();

                                            if is_currently_loading {
                                                let proxy_url = format!("socks5://127.0.0.1:{socks_port}");
                                                let agent = match ureq::Proxy::new(&proxy_url) {
                                                    Ok(proxy) => ureq::builder().proxy(proxy).timeout(std::time::Duration::from_secs(4)).build(),
                                                    Err(_) => return,
                                                };
                                                if let Ok(resp) = agent.get("http://ip-api.com/json/?fields=status,country,timezone,query").call() {
                                                    if let Ok(json) = resp.into_json::<serde_json::Value>() {
                                                        if json.get("status").and_then(|s| s.as_str()) == Some("success") {
                                                            success = true;
                                                            ms = start_ping.elapsed().as_millis();
                                                            ip = json.get("query").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                                            country = json.get("country").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                                            tz = json.get("timezone").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                                        }
                                                    }
                                                }
                                            } else {
                                                use std::net::{TcpStream, ToSocketAddrs};
                                                let parsed = crate::domain::key_parser::parse_vpn_key(&raw_url_bg).unwrap_or_else(|_| crate::domain::key_parser::ParsedKey {
                                                    protocol: "".to_string(), name: "".to_string(), host: "127.0.0.1".to_string(), port: 0, uuid: "".to_string(), query_params: std::collections::HashMap::new(), raw_url: "".to_string()
                                                });
                                                
                                                let timeout = std::time::Duration::from_secs(2);
                                                let addr = format!("{}:{}", parsed.host, parsed.port);
                                                if let Ok(mut addrs) = addr.to_socket_addrs() {
                                                    if let Some(socket_addr) = addrs.next() {
                                                        if let Ok(stream) = TcpStream::connect_timeout(&socket_addr, timeout) {
                                                            success = true;
                                                            ms = start_ping.elapsed().as_millis();
                                                            let _ = stream.shutdown(std::net::Shutdown::Both);
                                                        }
                                                    }
                                                }
                                            }

                                            // Безопасно отправляем простые типы данных
                                            let _ = sender.send_blocking((success, ms, ip, country, tz));
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

    // ================================

    // --- Раздел: Управление ключами и конфигурацией ---
    fn set_active_key(&self, active_item: &VpnKeyObject) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                    if item.is_active() && item.name() != active_item.name() {
                        item.set_is_active(false);
                    }
                }
            }
            active_item.set_is_active(true);
            active_item.set_is_loading(true);
            active_item.set_is_error(false);
            
            // Синхронизация режима стримера
            let current_settings = SettingsManager::new().load();
            active_item.set_hide_ip(current_settings.streamer_mode);
            
            self.save_current_keys();
            self.update_disconnect_action_state();

            // Сброс метрик
            self.imp().start_time.replace(Some(std::time::Instant::now()));
            *self.imp().bytes_down.borrow_mut() = 0;
            *self.imp().bytes_up.borrow_mut() = 0;
            
            self.imp().window_title.set_subtitle(&gettext("Connecting..."));

            let app_settings = current_settings;
            
            let config_json = if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&active_item.url()) {
                if app_settings.core == "sing-box" {
                    crate::domain::singbox_config::build_singbox_config(&parsed, &app_settings)
                } else {
                    crate::domain::xray_config::build_xray_config(&parsed, &app_settings)
                }
            } else {
                tracing::error!("Failed to parse key for configuration generation");
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                self.imp().window_title.set_subtitle(&gettext("Configuration error"));
                return;
            };

            let core_type = app_settings.core.clone();
            let tun_mode = app_settings.tun_mode;
            let page_weak = self.downgrade();
            let item_clone = active_item.clone();

            glib::spawn_future_local(async move {
                match crate::ipc::get_system_connection().await {
                    Ok(conn) => {
                        match crate::ipc::DaemonProxy::new(&conn).await {
                            Ok(proxy) => {
                                tracing::info!("Connecting to VPN key via D-Bus: {}", item_clone.name());
                                if let Err(e) = proxy.start_proxy(core_type, config_json, tun_mode).await {
                                    tracing::error!("Failed to start backend via D-Bus: {}", e);
                                    if let Some(page) = page_weak.upgrade() {
                                        item_clone.set_is_active(false);
                                        item_clone.set_is_loading(false);
                                        item_clone.set_is_error(true);
                                        page.imp().start_time.replace(None);
                                        page.imp().window_title.set_subtitle(&gettext("Core startup error"));
                                        page.save_current_keys();
                                        page.update_disconnect_action_state();
                                        
                                        let dialog = adw::AlertDialog::builder()
                                            .heading(gettext("Connection error"))
                                            .body(e.to_string())
                                            .build();
                                        dialog.add_response("ok", &gettext("OK"));
                                        if let Some(root) = page.root() {
                                            dialog.present(Some(&root));
                                        }
                                    }
                                } else {
                                    tracing::info!("Backend successfully started via D-Bus");
                                }
                            }
                            Err(e) => tracing::error!("Failed to create DaemonProxy: {}", e),
                        }
                    }
                    Err(e) => tracing::error!("Failed to connect to D-Bus System Bus: {}", e),
                }
            });
        }
    }

    fn save_current_keys(&self) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let mut data = Vec::new();
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
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

    // Логика отображения информации о ключе
    // ================================

    // --- Раздел: UI Диалоги ---
    fn handle_info_key(&self, key: &VpnKeyObject) {
        let current_settings = SettingsManager::new().load();
        key.set_hide_ip(current_settings.streamer_mode);
        
        let hide = key.hide_ip();
        let display_ip = if hide { "***.***.***.***".to_string() } else { key.server_info() };
        let display_loc = if hide { "***".to_string() } else { key.location() };
        let display_tz = if hide { "***".to_string() } else { key.timezone() };

        let mut body = format!(
            "<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}",
            gettext("Server address"), display_ip,
            gettext("Location"), display_loc,
            gettext("Timezone"), display_tz,
            gettext("Protocol"), key.protocol()
        );

        if let Ok(parsed) = crate::domain::key_parser::parse_vpn_key(&key.url()) {
            let display_port = if hide { "***".to_string() } else { parsed.port.to_string() };
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
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Public key"), display_pbk));
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
            .maximum_size(400)
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

    // Логика редактирования ключа
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
        
        let name_row = adw::EntryRow::builder().title(gettext("Name")).text(&parsed.name).build();
        let protocol_row = adw::EntryRow::builder().title(gettext("Protocol")).text(&parsed.protocol).build();
        let host_row = adw::EntryRow::builder().title(gettext("Server address")).text(&parsed.host).build();
        let port_row = adw::EntryRow::builder().title(gettext("Port")).text(parsed.port.to_string()).build();
        let uuid_row = adw::EntryRow::builder().title(gettext("UUID / Password")).text(&parsed.uuid).build();

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
            .maximum_size(450)
            .tightening_threshold(300)
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

    // Логика удаления ключа
    fn handle_delete_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_name = key.name();
        
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete VPN key"))
            .body(format!("Are you sure you want to delete '{key_name}'?"))
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
                            if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                                if item.name() == key_name_str {
                                    target_index = Some(i);
                                    break;
                                }
                            }
                        }
                        if let Some(index) = target_index {
                            let item = model.item(index).and_then(|obj| obj.downcast::<VpnKeyObject>().ok());
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

    fn update_disconnect_action_state(&self) {
        use gio::prelude::ActionMapExt;
        
        if let Some(group) = self.imp().action_group.borrow().as_ref() {
            if let Some(action) = group.lookup_action("disconnect").and_then(|a| a.downcast::<gio::SimpleAction>().ok()) {
                let mut has_active = false;
                if let Some(model) = self.imp().model.borrow().as_ref() {
                    for i in 0..model.n_items() {
                        if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
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

    // ================================

    // --- Раздел: GActions ---
    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        self.imp().action_group.replace(Some(action_group.clone()));

        // Действие: Add ключ
        let add_action = gio::SimpleAction::new("add_key", Some(glib::VariantTy::STRING));
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, parameter| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            let _input = parameter
                .and_then(|v| v.get::<String>())
                .unwrap_or_else(|| "Key".to_string());

            let dialog = adw::AlertDialog::builder()
                .heading(gettext("Add VPN key"))
                .build();
            
            let entry = gtk::Entry::builder()
                .placeholder_text(gettext("VPN Link"))
                .activates_default(true)
                .build();

            let list_box = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();
            
            let row = gtk::ListBoxRow::builder().child(&entry).activatable(false).build();
            list_box.append(&row);

            let clamp = adw::Clamp::builder()
                .maximum_size(450)
                .tightening_threshold(300)
                .child(&list_box)
                .build();
            
            clamp.set_margin_top(12);
            clamp.set_margin_bottom(12);
            clamp.set_margin_start(12);
            clamp.set_margin_end(12);

            dialog.set_extra_child(Some(&clamp));
            dialog.add_response("cancel", &gettext("Cancel"));
            dialog.add_response("add", &gettext("Add"));
            dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("add"));
            dialog.set_close_response("cancel");
            
            let page_weak_dialog = page_weak.clone();
            let entry_clone = entry.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "add" {
                    if let Some(page) = page_weak_dialog.upgrade() {
                        let url_str = entry_clone.text();
                        match crate::domain::key_parser::parse_vpn_key(&url_str) {
                            Ok(parsed) => {
                                if let Some(model) = page.imp().model.borrow().as_ref() {
                                    let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &parsed.raw_url);
                                    model.append(&new_key);
                                    page.save_current_keys();
                                }
                            }
                            Err(e) => {
                                tracing::error!("{}", &format!("Key parsing error: {e}"));
                            }
                        }
                    }
                }
            });

            if let Some(root) = page.root() {
                dialog.present(Some(&root));
                entry.grab_focus();
            }
        });
        action_group.add_action(&add_action);

        // Действие: Импорт из буфера обмена
        let import_clip_action = gio::SimpleAction::new("import_clipboard", None);
        let page_weak_clip = self.downgrade();
        import_clip_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak_clip.upgrade() {
                let display = gdk::Display::default().expect("No display");
                let clipboard = display.clipboard();
                
                let p_weak = page.downgrade();
                clipboard.read_text_async(gio::Cancellable::NONE, move |res| {
                    if let Ok(Some(text)) = res {
                        if let Some(p) = p_weak.upgrade() {
                            match crate::domain::key_parser::parse_vpn_key(&text) {
                                Ok(parsed) => {
                                    if let Some(model) = p.imp().model.borrow().as_ref() {
                                        let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &parsed.raw_url);
                                        model.append(&new_key);
                                        p.save_current_keys();
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("{}", &format!("Key parsing error from buffer: {e}"));
                                }
                            }
                        }
                    }
                });
            }
        });
        action_group.add_action(&import_clip_action);

        // Действие: Disconnect
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        disconnect_action.connect_activate(move |_, _| {
            tracing::info!("Disconnecting VPN via D-Bus");
            glib::spawn_future_local(async move {
                match crate::ipc::get_system_connection().await {
                    Ok(conn) => {
                        match crate::ipc::DaemonProxy::new(&conn).await {
                            Ok(proxy) => {
                                if let Err(e) = proxy.stop_proxy().await {
                                    tracing::error!("Failed to stop backend via D-Bus: {}", e);
                                }
                            }
                            Err(e) => tracing::error!("Failed to create DaemonProxy: {}", e),
                        }
                    }
                    Err(e) => tracing::error!("Failed to connect to D-Bus System Bus: {}", e),
                }
            });
        });
        action_group.add_action(&disconnect_action);

        self.insert_action_group("vpn", Some(&action_group));
        
        self.update_disconnect_action_state();
    }
}
