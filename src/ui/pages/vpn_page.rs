use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use gettextrs::gettext;

use crate::ui::models::VpnKeyObject;
use crate::ui::components::vpn_key_row::VrxxVpnKeyRow;
use crate::ui::setup_primary_menu;
use crate::settings::{SettingsManager, VpnKeyData};

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
        pub backend: RefCell<crate::backend::XrayBackend>,
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
            self.backend.replace(crate::backend::XrayBackend::new());

            self.obj().setup_model();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
            self.obj().start_metrics_timer();
            self.obj().setup_dbus_listener();

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

    fn setup_model(&self) {
        let model = gio::ListStore::new::<VpnKeyObject>();

        let settings = SettingsManager::new();
        let saved_keys = settings.load_keys();

        if saved_keys.is_empty() {
            // Инициализация тестовых данных, если ключей нет
            let key1 = VpnKeyObject::new("Mark-Vless", "VLESS+Reality", false, "vless://uuid@host:443?security=reality");
            let key2 = VpnKeyObject::new("Wumt-Vless", "VMess", false, "vmess://...");
            let key3 = VpnKeyObject::new("Eleon-Vless", "VMess", false, "vmess://...");

            model.append(&key1);
            model.append(&key2);
            model.append(&key3);
        } else {
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
                    // Задержка перед подключением, чтобы UI успел загрузиться
                    let key_clone = key_obj.clone();
                    let page_weak = self.downgrade();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                        if let Some(page) = page_weak.upgrade() {
                            page.set_active_key(&key_clone);
                        }
                    });
                } else if !auto_connect && k.is_active {
                     // Сбрасываем активное состояние, если автоподключение выключено
                     key_obj.set_is_active(false);
                }
            }
        }

        self.imp().model.replace(Some(model.clone()));

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

            // Дублировать
            let page_weak_dup = page_weak.clone();
            let key_obj_dup = key_obj.clone();
            row.connect_local("request-duplicate", false, move |_| {
                if let Some(page) = page_weak_dup.upgrade() {
                    page.handle_duplicate_key(&key_obj_dup);
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
                    if let Ok(parsed) = crate::key_parser::parse_vpn_key(&url) {
                        if let Ok(json_str) = serde_json::to_string_pretty(&parsed) {
                            let clipboard = page.clipboard();
                            clipboard.set_text(&json_str);
                        }
                    }
                }
                None
            });

            // Удалить
            let page_weak_del = page_weak.clone();
            let key_obj_del = key_obj.clone();
            row.connect_local("request-delete", false, move |_| {
                if let Some(page) = page_weak_del.upgrade() {
                    page.handle_delete_key(&key_obj_del);
                }
                None
            });

            // Ручной пинг
            let key_obj_ping = key_obj.clone();
            row.connect_local("request-ping", false, move |_| {
                let item_clone = key_obj_ping.clone();
                item_clone.set_ping(gettext("pinging..."));
                item_clone.set_is_loading(true);
                
                glib::spawn_future_local(async move {
                    let start_ping = std::time::Instant::now();
                    let socks_port = crate::settings::SettingsManager::new().load().socks_port;
                    
                    let args_raw = [
                        "curl", "-s", "-w", "%{time_total}", 
                        "-x", &format!("socks5h://127.0.0.1:{socks_port}"),
                        "http://ip-api.com/json/?fields=status,country,timezone,query",
                        "--connect-timeout", "5"
                    ];
                    let args: Vec<&std::ffi::OsStr> = args_raw.iter().map(std::ffi::OsStr::new).collect();

                    let mut success = false;

                    if let Ok(subprocess) = gio::Subprocess::newv(&args, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE) {
                        if let Ok((Some(stdout_bytes), _)) = subprocess.communicate_future(None).await {
                            let is_cmd_successful = subprocess.is_successful();
                            let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                            let default_ping_ms = start_ping.elapsed().as_millis();

                            if is_cmd_successful {
                                if let Some(brace_idx) = stdout_str.rfind('}') {
                                    let json_part = &stdout_str[..=brace_idx];
                                    let time_part = &stdout_str[brace_idx+1..];

                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_part) {
                                        if json.get("status").and_then(|s| s.as_str()) == Some("success") {
                                            success = true;

                                            if let Ok(time_sec) = time_part.trim().parse::<f64>() {
                                                let ms = (time_sec * 1000.0) as u64;
                                                item_clone.set_ping(format!("{ms} ms"));
                                            } else {
                                                item_clone.set_ping(format!("{default_ping_ms} ms"));
                                            }

                                            if let Some(ip) = json.get("query").and_then(|s| s.as_str()) {
                                                item_clone.set_server_info(ip.to_string());
                                            }
                                            if let Some(country) = json.get("country").and_then(|s| s.as_str()) {
                                                item_clone.set_location(country.to_string());
                                            }
                                            if let Some(tz) = json.get("timezone").and_then(|s| s.as_str()) {
                                                item_clone.set_timezone(tz.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    item_clone.set_is_loading(false);
                    if !success {
                        item_clone.set_ping(gettext("timeout"));
                    }
                });
                None
            });

            row.upcast::<gtk::Widget>()
        });
    }

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
                            crate::backend::log_app_event("info", "Система переходит в спящий режим! Приостановка мониторинга VPN.");
                        } else {
                            crate::backend::log_app_event("info", "Система проснулась! Возобновление мониторинга VPN.");
                        }
                    }
                },
            );
        }
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
                                        crate::backend::log_app_event("error", "Обнаружено падение процесса ядра! Отключение...");
                                        item.set_is_active(false);
                                        item.set_is_error(true);
                                        imp.start_time.replace(None);
                                        page.update_disconnect_action_state();
                                        page.imp().window_title.set_subtitle(&gettext("Ошибка соединения"));

                                        // Читаем последние строки лога для отображения пользователю
                                        let mut error_details = String::from("Неизвестная ошибка. Пожалуйста, проверьте Системные логи.");
                                        let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
                                        let log_path = log_dir.join("core.log");
                                        if let Ok(content) = std::fs::read_to_string(&log_path) {
                                            let lines: Vec<&str> = content.lines().rev().take(5).collect();
                                            if !lines.is_empty() {
                                                error_details = lines.into_iter().rev().collect::<Vec<&str>>().join("\n");
                                            }
                                        }

                                        let dialog = adw::AlertDialog::builder()
                                            .heading(gettext("Сбой подключения"))
                                            .body(format!("Процесс ядра неожиданно завершился. Детали лога:\n\n{error_details}"))
                                            .build();
                                        dialog.add_response("ok", &gettext("ОК"));
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
                                    
                                    if bin_name == "xray" && should_stats && imp.backend.borrow().is_running() {
                                        glib::spawn_future_local(async move {
                                            let args = [
                                                std::ffi::OsStr::new("xray"), 
                                                std::ffi::OsStr::new("api"), 
                                                std::ffi::OsStr::new("statsquery"), 
                                                std::ffi::OsStr::new("-server=127.0.0.1:10085")
                                            ];
                                            if let Ok(subprocess) = gio::Subprocess::newv(&args, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE) {
                                                if let Ok((Some(stdout_bytes), _)) = subprocess.communicate_future(None).await {
                                                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout_str) {
                                                        let mut total_down: u64 = 0;
                                                        let mut total_up: u64 = 0;
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
                                                            if total_down > 0 || total_up > 0 {
                                                                item_clone_stats.set_traffic_down(format!("{:.1} MB", total_down as f64 / 1_048_576.0));
                                                                item_clone_stats.set_traffic_up(format!("{:.1} MB", total_up as f64 / 1_048_576.0));
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
                                        crate::backend::log_app_event("warn", "Таймаут подключения (более 12 сек).");
                                        item.set_is_active(false);
                                        item.set_is_loading(false);
                                        item.set_is_error(true);
                                        
                                        imp.start_time.replace(None);
                                        page.update_disconnect_action_state();
                                        page.imp().window_title.set_subtitle(&gettext("Сбой подключения"));
                                        
                                        if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
                                            if let Some(window) = app.active_window() {
                                                if !window.is_active() {
                                                    let notification = gio::Notification::new(&gettext("Сбой подключения"));
                                                    notification.set_body(Some(&gettext("Не удалось подключиться к выбранному VPN ключу.")));
                                                    app.send_notification(Some("vpn_fail"), &notification);
                                                }
                                            }
                                        }
                                        break;
                                    }

                                    if should_ping {
                                        let item_clone = item.clone();
                                        let page_weak_ping = page_weak.clone();
                                        
                                        glib::spawn_future_local(async move {
                                            let start_ping = std::time::Instant::now();
                                            let socks_port = crate::settings::SettingsManager::new().load().socks_port;
                                            
                                            // Загружаем геоданные только в начале
                                            let fetch_geodata = is_loading;
                                            let target_url = if fetch_geodata {
                                                "http://ip-api.com/json/?fields=status,country,timezone,query"
                                            } else {
                                                "http://cp.cloudflare.com/generate_204"
                                            };

                                            let args_raw = [
                                                "curl", "-s", "-w", "%{time_total}", 
                                                "-x", &format!("socks5h://127.0.0.1:{socks_port}"),
                                                target_url,
                                                "--connect-timeout", "4"
                                            ];
                                            let args: Vec<&std::ffi::OsStr> = args_raw.iter().map(std::ffi::OsStr::new).collect();
                                            
                                            let mut success = false;
                                            #[allow(unused_assignments)]
                                            let mut default_ping_ms = 0;

                                            if let Ok(subprocess) = gio::Subprocess::newv(&args, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE) {
                                                if let Ok((Some(stdout_bytes), _)) = subprocess.communicate_future(None).await {
                                                    let is_cmd_successful = subprocess.is_successful();
                                                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                                                    default_ping_ms = start_ping.elapsed().as_millis();
                                                    
                                                    if is_cmd_successful {
                                                        if fetch_geodata {
                                                            if let Some(brace_idx) = stdout_str.rfind('}') {
                                                                let json_part = &stdout_str[..=brace_idx];
                                                                let time_part = &stdout_str[brace_idx+1..];

                                                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_part) {
                                                                    if json.get("status").and_then(|s| s.as_str()) == Some("success") {
                                                                        success = true;
                                                                        if let Ok(time_sec) = time_part.trim().parse::<f64>() {
                                                                            let ms = (time_sec * 1000.0) as u64;
                                                                            item_clone.set_ping(format!("{ms} ms"));
                                                                        } else {
                                                                            item_clone.set_ping(format!("{default_ping_ms} ms"));
                                                                        }

                                                                        if let Some(ip) = json.get("query").and_then(|s| s.as_str()) {
                                                                            item_clone.set_server_info(ip.to_string());
                                                                        }
                                                                        if let Some(country) = json.get("country").and_then(|s| s.as_str()) {
                                                                            item_clone.set_location(country.to_string());
                                                                        }
                                                                        if let Some(tz) = json.get("timezone").and_then(|s| s.as_str()) {
                                                                            item_clone.set_timezone(tz.to_string());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            // Обычный пинг до cloudflare
                                                            if let Ok(time_sec) = stdout_str.trim().parse::<f64>() {
                                                                if time_sec > 0.0 {
                                                                    success = true;
                                                                    let ms = (time_sec * 1000.0) as u64;
                                                                    item_clone.set_ping(format!("{ms} ms"));
                                                                }
                                                            }
                                                            if !success && default_ping_ms > 0 {
                                                                success = true;
                                                                item_clone.set_ping(format!("{default_ping_ms} ms"));
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if success {
                                                item_clone.set_is_loading(false);
                                                item_clone.set_is_error(false);
                                                
                                                if let Some(page) = page_weak_ping.upgrade() {
                                                    let connected_text = gettext("Подключено");
                                                    let new_subtitle = format!("{} {}", item_clone.name(), connected_text);
                                                    page.imp().window_title.set_subtitle(&new_subtitle);
                                                }
                                            } else {
                                                item_clone.set_ping(gettext("timeout"));
                                                if !item_clone.is_loading() {
                                                    // Соединение было установлено, но пинг пропал
                                                    item_clone.set_is_error(true);
                                                    if let Some(page) = page_weak_ping.upgrade() {
                                                        page.imp().window_title.set_subtitle(&gettext("Соединение нестабильно"));
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
            
            self.imp().window_title.set_subtitle(&gettext("Подключение..."));

            let app_settings = current_settings;
            
            let config_json = if let Ok(parsed) = crate::key_parser::parse_vpn_key(&active_item.url()) {
                if app_settings.core == "sing-box" {
                    crate::singbox_config::build_singbox_config(&parsed, &app_settings)
                } else {
                    crate::xray_config::build_xray_config(&parsed, &app_settings)
                }
            } else {
                crate::backend::log_app_event("error", "Не удалось распарсить ключ для генерации конфигурации");
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                self.imp().window_title.set_subtitle(&gettext("Ошибка конфигурации"));
                return;
            };

            let backend = self.imp().backend.borrow();
            crate::backend::log_app_event("info", &format!("Подключение к VPN ключу: {}", active_item.name()));
            
            if let Err(e) = backend.start(&config_json) {
                crate::backend::log_app_event("error", &format!("Не удалось запустить бэкенд: {e}"));
                active_item.set_is_active(true); 
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                
                self.imp().start_time.replace(None);
                self.imp().window_title.set_subtitle(&gettext("Ошибка запуска ядра"));
                
                self.save_current_keys();
                self.update_disconnect_action_state();
                
                let dialog = adw::AlertDialog::builder()
                    .heading(gettext("Ошибка соединения"))
                    .body(e.to_string())
                    .build();
                dialog.add_response("ok", &gettext("ОК"));
                if let Some(root) = self.root() {
                    dialog.present(Some(&root));
                }
            } else {
                crate::backend::log_app_event("info", "Бэкенд успешно запущен");
            }
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
    fn handle_info_key(&self, key: &VpnKeyObject) {
        let current_settings = SettingsManager::new().load();
        key.set_hide_ip(current_settings.streamer_mode);
        
        let hide = key.hide_ip();
        let display_ip = if hide { "***.***.***.***".to_string() } else { key.server_info() };
        let display_loc = if hide { "***".to_string() } else { key.location() };
        let display_tz = if hide { "***".to_string() } else { key.timezone() };

        let mut body = format!(
            "<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}",
            gettext("Адрес сервера"), display_ip,
            gettext("Локация"), display_loc,
            gettext("Часовой пояс"), display_tz,
            gettext("Протокол"), key.protocol()
        );

        if let Ok(parsed) = crate::key_parser::parse_vpn_key(&key.url()) {
            let display_port = if hide { "***".to_string() } else { parsed.port.to_string() };
            body.push_str(&format!("\n<b>{}</b>: {}", gettext("Порт"), display_port));
            
            if let Some(net) = parsed.query_params.get("type") {
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Сеть"), net));
            }
            if let Some(sec) = parsed.query_params.get("security") {
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Безопасность"), sec));
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
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Публичный ключ"), display_pbk));
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
        
        dialog.add_response("close", &gettext("Закрыть"));
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

        let parsed = match crate::key_parser::parse_vpn_key(&key_url) {
            Ok(p) => p,
            Err(_) => return,
        };

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Редактировать VPN ключ"))
            .build();
        
        let name_row = adw::EntryRow::builder().title(gettext("Имя")).text(&parsed.name).build();
        let protocol_row = adw::EntryRow::builder().title(gettext("Протокол")).text(&parsed.protocol).build();
        let host_row = adw::EntryRow::builder().title(gettext("Адрес сервера")).text(&parsed.host).build();
        let port_row = adw::EntryRow::builder().title(gettext("Порт")).text(parsed.port.to_string()).build();
        let uuid_row = adw::EntryRow::builder().title(gettext("UUID / Пароль")).text(&parsed.uuid).build();

        let group_general = adw::PreferencesGroup::builder()
            .title(gettext("Общие"))
            .build();
        group_general.add(&name_row);

        let group_connection = adw::PreferencesGroup::builder()
            .title(gettext("Соединение"))
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
        dialog.add_response("cancel", &gettext("Отмена"));
        dialog.add_response("save", &gettext("Сохранить"));
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

                    let new_url = crate::key_parser::build_vpn_key(&p);
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

    // Логика дублирования ключа
    fn handle_duplicate_key(&self, key: &VpnKeyObject) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let new_name = format!("{} (Копия)", key.name());
            let new_protocol = key.protocol();
            let new_key = VpnKeyObject::new(&new_name, &new_protocol, false, &key.url());

            model.append(&new_key);
            self.save_current_keys();
        }
    }

    // Логика удаления ключа
    fn handle_delete_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_name = key.name();
        
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Удалить VPN ключ"))
            .body(format!("Вы уверены, что хотите удалить '{key_name}'?"))
            .build();
            
        dialog.add_response("cancel", &gettext("Отмена"));
        dialog.add_response("delete", &gettext("Удалить"));
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

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        self.imp().action_group.replace(Some(action_group.clone()));

        // Действие: Добавить ключ
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
                .heading(gettext("Добавить VPN ключ"))
                .build();
            
            let entry = gtk::Entry::builder()
                .placeholder_text(gettext("VPN Ссылка"))
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
            dialog.add_response("cancel", &gettext("Отмена"));
            dialog.add_response("add", &gettext("Добавить"));
            dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("add"));
            dialog.set_close_response("cancel");
            
            let page_weak_dialog = page_weak.clone();
            let entry_clone = entry.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "add" {
                    if let Some(page) = page_weak_dialog.upgrade() {
                        let url_str = entry_clone.text();
                        match crate::key_parser::parse_vpn_key(&url_str) {
                            Ok(parsed) => {
                                if let Some(model) = page.imp().model.borrow().as_ref() {
                                    let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &parsed.raw_url);
                                    model.append(&new_key);
                                    page.save_current_keys();
                                }
                            }
                            Err(e) => {
                                crate::backend::log_app_event("error", &format!("Ошибка парсинга ключа: {e}"));
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
                            match crate::key_parser::parse_vpn_key(&text) {
                                Ok(parsed) => {
                                    if let Some(model) = p.imp().model.borrow().as_ref() {
                                        let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &parsed.raw_url);
                                        model.append(&new_key);
                                        p.save_current_keys();
                                    }
                                }
                                Err(e) => {
                                    crate::backend::log_app_event("error", &format!("Ошибка парсинга ключа из буфера: {e}"));
                                }
                            }
                        }
                    }
                });
            }
        });
        action_group.add_action(&import_clip_action);

        // Действие: Отключить
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        let page_weak = self.downgrade();
        disconnect_action.connect_activate(move |_, _| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            crate::backend::log_app_event("info", "Отключение VPN");
            // Остановка бэкенда
            let backend = page.imp().backend.borrow();
            if let Err(e) = backend.stop() {
                crate::backend::log_app_event("error", &format!("Ошибка остановки бэкенда: {e}"));
            }

            // Деактивация всех ключей
            if let Some(model) = page.imp().model.borrow().as_ref() {
                for i in 0..model.n_items() {
                    if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                        item.set_is_active(false);
                        item.set_is_loading(false);
                    }
                }
            }
            
            page.imp().window_title.set_subtitle(&gettext("Отключено"));
            page.imp().start_time.replace(None);

            page.save_current_keys();
            page.update_disconnect_action_state();
        });
        action_group.add_action(&disconnect_action);

        self.insert_action_group("vpn", Some(&action_group));
        
        self.update_disconnect_action_state();
    }
}
