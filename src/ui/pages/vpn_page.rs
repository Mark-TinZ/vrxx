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
            
            // Initialize backend
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

impl VrxxVpnPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_model(&self) {
        let model = gio::ListStore::new::<VpnKeyObject>();

        let settings = SettingsManager::new();
        let saved_keys = settings.load_keys();

        if saved_keys.is_empty() {
            // Init test data
            let key1 = VpnKeyObject::new("Mark-Vless", "VLESS+Reality", true, "vless://uuid@host:443?security=reality");
            key1.set_traffic_down("120.4 MB");
            key1.set_ping("25 ms");
            let key2 = VpnKeyObject::new("Wumt-Vless", "VMess", false, "vmess://...");
            let key3 = VpnKeyObject::new("Eleon-Vless", "VMess", false, "vmess://...");
            key3.set_traffic_down("560.2 MB");
            key3.set_traffic_up("205.9 MB");
            key3.set_time_connected("00:50:25");
            key3.set_ping("105 ms");

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
                    // Give it a short delay to let the UI initialize before connecting
                    let key_clone = key_obj.clone();
                    // Actually, we can just call set_active_key later
                    // Using glib::idle_add_local
                    let page_weak = self.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(page) = page_weak.upgrade() {
                            page.set_active_key(&key_clone);
                        }
                    });
                } else if !auto_connect && k.is_active {
                     // Reset active state if we shouldn't auto connect
                     key_obj.set_is_active(false);
                }
            }
        }

        self.imp().model.replace(Some(model.clone()));

        // Bind model to ListBox
        let page_weak = self.downgrade();
        self.imp().keys_list.bind_model(Some(&model), move |item| {
            let key_obj = item.downcast_ref::<VpnKeyObject>().expect("Item should be a VpnKeyObject");
            let row = VrxxVpnKeyRow::new();
            row.bind(key_obj);

            // === Connect Signals from Row ===

            // Handle Edit
            let page_weak_edit = page_weak.clone();
            let key_obj_edit = key_obj.clone();
            row.connect_local("request-edit", false, move |_| {
                if let Some(page) = page_weak_edit.upgrade() {
                    page.handle_edit_key(&key_obj_edit);
                }
                None
            });

            // Handle Info
            let page_weak_info = page_weak.clone();
            let key_obj_info = key_obj.clone();
            row.connect_local("request-info", false, move |_| {
                if let Some(page) = page_weak_info.upgrade() {
                    page.handle_info_key(&key_obj_info);
                }
                None
            });

            // Handle Duplicate
            let page_weak_dup = page_weak.clone();
            let key_obj_dup = key_obj.clone();
            row.connect_local("request-duplicate", false, move |_| {
                if let Some(page) = page_weak_dup.upgrade() {
                    page.handle_duplicate_key(&key_obj_dup);
                }
                None
            });

            // Handle Copy Link
            let page_weak_cl = page_weak.clone();
            let key_obj_cl = key_obj.clone();
            row.connect_local("request-copy-link", false, move |_| {
                if let Some(page) = page_weak_cl.upgrade() {
                    let clipboard = page.clipboard();
                    clipboard.set_text(&key_obj_cl.url());
                }
                None
            });

            // Handle Copy JSON
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

            // Handle Delete
            let page_weak_del = page_weak.clone();
            let key_obj_del = key_obj.clone();
            row.connect_local("request-delete", false, move |_| {
                if let Some(page) = page_weak_del.upgrade() {
                    page.handle_delete_key(&key_obj_del);
                }
                None
            });

            // Handle Manual Ping
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
                        "-x", &format!("socks5h://127.0.0.1:{}", socks_port),
                        "http://ip-api.com/json/?fields=status,country,timezone,query",
                        "--connect-timeout", "5"
                    ];
                    let args: Vec<&std::ffi::OsStr> = args_raw.iter().map(std::ffi::OsStr::new).collect();

                    let mut success = false;
                    let default_ping_ms = 0;

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
                                                item_clone.set_ping(format!("{} ms", ms));
                                            } else {
                                                item_clone.set_ping(format!("{} ms", default_ping_ms));
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
                    let connected_text = gettext("Connected");
                    let new_subtitle = format!("{} {}", selected_item.name(), connected_text);
                    page.imp().window_title.set_subtitle(&new_subtitle);
                    println!("Selected key: {}", selected_item.name());
                }
            }
        });
    }

    fn setup_dbus_listener(&self) {
        let page_weak = self.downgrade();
        if let Ok(connection) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
            connection.signal_subscribe(
                Some("org.freedesktop.login1"),
                Some("org.freedesktop.login1.Manager"),
                Some("PrepareForSleep"),
                Some("/org/freedesktop/login1"),
                None,
                gio::DBusSignalFlags::NONE,
                move |_conn, _sender, _path, _interface, _signal, parameters| {
                    let is_sleeping = parameters.child_get::<bool>(0);
                    if let Some(page) = page_weak.upgrade() {
                        *page.imp().is_sleeping.borrow_mut() = is_sleeping;
                        if is_sleeping {
                            println!("System is going to sleep! Pausing VPN monitoring.");
                        } else {
                            println!("System woke up! Resuming VPN monitoring.");
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
                    // Update the active item
                    if let Some(model) = imp.model.borrow().as_ref() {
                        for i in 0..model.n_items() {
                            if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                                if item.is_active() {
                                    // Health check: if it's active but process is dead - it's an error
                                    if !imp.backend.borrow().is_running() && !item.is_loading() {
                                        println!("Detected core process crash! Deactivating...");
                                        item.set_is_active(false);
                                        item.set_is_error(true);
                                        imp.start_time.replace(None);
                                        page.update_disconnect_action_state();
                                        page.imp().window_title.set_subtitle(&gettext("Connection Error"));

                                        // Read last few lines of log to show to user
                                        let mut error_details = String::from("Unknown error. Please check System Logs.");
                                        let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
                                        let log_path = log_dir.join("core.log");
                                        if let Ok(content) = std::fs::read_to_string(&log_path) {
                                            let lines: Vec<&str> = content.lines().rev().take(5).collect();
                                            if !lines.is_empty() {
                                                error_details = lines.into_iter().rev().collect::<Vec<&str>>().join("\n");
                                            }
                                        }

                                        let dialog = adw::AlertDialog::builder()
                                            .heading(gettext("Connection Failed"))
                                            .body(&format!("The core process exited unexpectedly. Log details:\n\n{}", error_details))
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
                                    item.set_time_connected(format!("{:02}:{:02}:{:02}", hours, mins, secs));

                                    // Fetch traffic stats from Xray API
                                    let item_clone_stats = item.clone();
                                    let core_bin = crate::settings::SettingsManager::new().load().core;
                                    let bin_name = if core_bin == "sing-box" { "sing-box" } else { "xray" };
                                    
                                    if bin_name == "xray" {
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

                                    // Run real ping and connection verification asynchronously
                                    let is_loading = item.is_loading();
                                    let should_ping = if is_loading {
                                        elapsed == 2 || elapsed == 8 || elapsed == 15
                                    } else {
                                        elapsed > 0 && elapsed % 300 == 0
                                    };

                                    if should_ping {
                                        let item_clone = item.clone();
                                        let page_weak_ping = page_weak.clone();
                                        
                                        glib::spawn_future_local(async move {
                                            let start_ping = std::time::Instant::now();
                                            let socks_port = crate::settings::SettingsManager::new().load().socks_port;
                                            
                                            let fetch_geodata = is_loading && start_ping.elapsed().as_secs() < 10;
                                            let target_url = if fetch_geodata {
                                                "http://ip-api.com/json/?fields=status,country,timezone,query"
                                            } else {
                                                "http://cp.cloudflare.com/generate_204"
                                            };

                                            // Use gio::Subprocess which is fully async and won't block GTK main loop
                                            let args_raw = [
                                                "curl", "-s", "-w", "%{time_total}", 
                                                "-x", &format!("socks5h://127.0.0.1:{}", socks_port),
                                                target_url,
                                                "--connect-timeout", "5"
                                            ];
                                            let args: Vec<&std::ffi::OsStr> = args_raw.iter().map(std::ffi::OsStr::new).collect();
                                            
                                            let mut success = false;
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
                                                                        // Calculate ping
                                                                        if let Ok(time_sec) = time_part.trim().parse::<f64>() {
                                                                            let ms = (time_sec * 1000.0) as u64;
                                                                            item_clone.set_ping(format!("{} ms", ms));
                                                                        } else {
                                                                            item_clone.set_ping(format!("{} ms", default_ping_ms));
                                                                        }

                                                                        // Set Geodata
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
                                                            // Just parsing the time
                                                            if let Ok(time_sec) = stdout_str.trim().parse::<f64>() {
                                                                if time_sec > 0.0 {
                                                                    success = true;
                                                                    let ms = (time_sec * 1000.0) as u64;
                                                                    item_clone.set_ping(format!("{} ms", ms));
                                                                }
                                                            }
                                                            if !success && default_ping_ms > 0 {
                                                                success = true;
                                                                item_clone.set_ping(format!("{} ms", default_ping_ms));
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if success {
                                                item_clone.set_is_loading(false);
                                            } else {
                                                item_clone.set_ping(gettext("timeout"));
                                                if item_clone.is_loading() {
                                                    // If it fails during loading phase, it's likely a broken key. We might want to wait a few tries.
                                                    // But if it's already elapsed > 10, then we disconnect.
                                                    // Actually let's just use an internal check or rely on `start_ping` but wait, `start_ping` is the start of this future.
                                                    // We can't access `elapsed` cleanly. Let's just say if it fails while loading, we disconnect if it's been loading for too long?
                                                    // No, if the FIRST or SECOND ping fails while loading, it means it's unreachable. Let's just mark it error.
                                                    item_clone.set_is_active(false);
                                                    item_clone.set_is_loading(false);
                                                    item_clone.set_is_error(true);
                                                    
                                                    if let Some(page) = page_weak_ping.upgrade() {
                                                        page.imp().start_time.replace(None);
                                                        page.update_disconnect_action_state();
                                                        page.imp().window_title.set_subtitle(&gettext("Connection Failed"));
                                                        
                                                        // Also send notification if not active
                                                        if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
                                                            if let Some(window) = app.active_window() {
                                                                if !window.is_active() {
                                                                    let notification = gio::Notification::new(&gettext("Connection Failed"));
                                                                    notification.set_body(Some(&gettext("Unable to connect to the selected VPN key.")));
                                                                    app.send_notification(Some("vpn_fail"), &notification);
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    // It was already connected, but ping failed. Let's send a background notification.
                                                    if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
                                                        if let Some(window) = app.active_window() {
                                                            if !window.is_active() {
                                                                let notification = gio::Notification::new(&gettext("Connection Unstable"));
                                                                notification.set_body(Some(&gettext("The VPN connection seems to be timing out.")));
                                                                app.send_notification(Some("vpn_unstable"), &notification);
                                                            }
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
            active_item.set_is_error(false); // Clear previous errors
            
            // Sync current streamer mode setting
            let current_settings = SettingsManager::new().load();
            active_item.set_hide_ip(current_settings.streamer_mode);
            
            self.save_current_keys();
            self.update_disconnect_action_state();

            // Reset metrics
            self.imp().start_time.replace(Some(std::time::Instant::now()));
            *self.imp().bytes_down.borrow_mut() = 0;
            *self.imp().bytes_up.borrow_mut() = 0;

            // Setup real configuration
            let app_settings = current_settings; // Use already loaded settings
            
            let config_json = if let Ok(parsed) = crate::key_parser::parse_vpn_key(&active_item.url()) {
                crate::xray_config::build_xray_config(&parsed, &app_settings)
            } else {
                eprintln!("Failed to parse key for config generation");
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                return;
            };

            let backend = self.imp().backend.borrow();
            crate::backend::log_app_event("info", &format!("Connecting to VPN key: {}", active_item.name()));
            if let Err(e) = backend.start(&config_json) {
                crate::backend::log_app_event("error", &format!("Failed to start backend: {}", e));
                active_item.set_is_active(true); // Keep it "expanded" but show error
                active_item.set_is_loading(false);
                active_item.set_is_error(true);
                self.save_current_keys();
                self.update_disconnect_action_state();
                
                let dialog = adw::AlertDialog::builder()
                    .heading(gettext("Connection Error"))
                    .body(&e)
                    .build();
                dialog.add_response("ok", &gettext("OK"));
                if let Some(root) = self.root() {
                    dialog.present(Some(&root));
                }
            } else {
                crate::backend::log_app_event("info", "Backend started successfully");
                // The connection verification in start_metrics_timer will set is_loading(false) 
                // and handle success/failure state transitions.
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

    // Logic for showing key info
    fn handle_info_key(&self, key: &VpnKeyObject) {
        // Sync hide_ip before showing info
        let current_settings = SettingsManager::new().load();
        key.set_hide_ip(current_settings.streamer_mode);
        
        let hide = key.hide_ip();
        let display_ip = if hide { "***.***.***.***".to_string() } else { key.server_info() };
        let display_loc = if hide { "***".to_string() } else { key.location() };
        let display_tz = if hide { "***".to_string() } else { key.timezone() };

        let mut body = format!(
            "<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}\n<b>{}</b>: {}",
            gettext("Server Address"), display_ip,
            gettext("Location"), display_loc,
            gettext("Timezone"), display_tz,
            gettext("Protocol"), key.protocol()
        );

        // Parse key for detailed info
        if let Ok(parsed) = crate::key_parser::parse_vpn_key(&key.url()) {
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
                body.push_str(&format!("\n<b>{}</b>: {}", gettext("Public Key"), display_pbk));
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

    // Logic for editing a key
    fn handle_edit_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_obj_clone = key.clone();
        let key_url = key.url();

        let parsed = match crate::key_parser::parse_vpn_key(&key_url) {
            Ok(p) => p,
            Err(_) => return, // Fail silently or show error
        };

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Edit VPN Key"))
            .build();
        
        let name_row = adw::EntryRow::builder().title(gettext("Name")).text(&parsed.name).build();
        let protocol_row = adw::EntryRow::builder().title(gettext("Protocol")).text(&parsed.protocol).build();
        let host_row = adw::EntryRow::builder().title(gettext("Server Address")).text(&parsed.host).build();
        let port_row = adw::EntryRow::builder().title(gettext("Port")).text(&parsed.port.to_string()).build();
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

    // Logic for duplicating a key
    fn handle_duplicate_key(&self, key: &VpnKeyObject) {
        println!("Logic: Duplicating key '{}'", key.name());

        if let Some(model) = self.imp().model.borrow().as_ref() {
            let new_name = format!("{} (Copy)", key.name());
            let new_protocol = key.protocol();
            // Create a copy (in real app, you'd copy all fields)
            let new_key = VpnKeyObject::new(&new_name, &new_protocol, false, &key.url());

            // Append to model
            model.append(&new_key);
            self.save_current_keys();
        }
    }

    // Logic for deleting a key
    fn handle_delete_key(&self, key: &VpnKeyObject) {
        let page_weak = self.downgrade();
        let key_name = key.name();
        
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete VPN Key"))
            .body(format!("Are you sure you want to delete '{}'?", key_name))
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
                            let was_active = item.map_or(false, |it| it.is_active());
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

        // Action: Add Key (Parameterized with String)
        let add_action = gio::SimpleAction::new("add_key", Some(glib::VariantTy::STRING));
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, parameter| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            // In a real app, this might come from a dialog. For now, if the parameter is a protocol,
            // we could open a dialog. But let's assume it's a URL if it starts with a scheme.
            let input = parameter
                .and_then(|v| v.get::<String>())
                .unwrap_or_else(|| "Key".to_string());

            let _dialog_body = if input.is_empty() || input == "Key" {
                gettext("Paste your VPN link below:")
            } else {
                format!("Paste your {} link below:", input.to_uppercase())
            };

            // We will show a quick dialog to paste the URL
            let dialog = adw::AlertDialog::builder()
                .heading(gettext("Add VPN Key"))
                .build();
            
            let entry_row = adw::EntryRow::builder()
                .title(gettext("VPN Link"))
                .build();

            let list_box = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();
            list_box.append(&entry_row);

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
            let entry_row_clone = entry_row.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "add" {
                    if let Some(page) = page_weak_dialog.upgrade() {
                        let url_str = entry_row_clone.text();
                        match crate::key_parser::parse_vpn_key(&url_str) {
                            Ok(parsed) => {
                                if let Some(model) = page.imp().model.borrow().as_ref() {
                                    let new_key = VpnKeyObject::new(&parsed.name, &parsed.protocol, false, &parsed.raw_url);
                                    model.append(&new_key);
                                    page.save_current_keys();
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse key: {}", e);
                            }
                        }
                    }
                }
            });

            // Need root widget to present the AlertDialog
            if let Some(root) = page.root() {
                dialog.present(Some(&root));
                entry_row.grab_focus();
            }
        });
        action_group.add_action(&add_action);

        // Action: Import from Clipboard
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
                                    eprintln!("Failed to parse key from clipboard: {}", e);
                                }
                            }
                        }
                    }
                });
            }
        });
        action_group.add_action(&import_clip_action);

        // Action: Disconnect
        let disconnect_action = gio::SimpleAction::new("disconnect", None);
        let page_weak = self.downgrade();
        disconnect_action.connect_activate(move |_, _| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            println!("Disconnecting VPN...");

            crate::backend::log_app_event("info", "Disconnecting VPN");
            // Stop the backend
            let backend = page.imp().backend.borrow();
            if let Err(e) = backend.stop() {
                crate::backend::log_app_event("error", &format!("Error stopping backend: {}", e));
                eprintln!("Error stopping backend: {}", e);
            }

            // Deactivate all keys
            if let Some(model) = page.imp().model.borrow().as_ref() {
                for i in 0..model.n_items() {
                    if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                        item.set_is_active(false);
                        item.set_is_loading(false);
                    }
                }
            }
            
            // Update title
            page.imp().window_title.set_subtitle(&gettext("Disconnected"));

            page.imp().start_time.replace(None);

            page.save_current_keys();            page.update_disconnect_action_state();
        });
        action_group.add_action(&disconnect_action);

        self.insert_action_group("vpn", Some(&action_group));
        
        // Initial state update
        self.update_disconnect_action_state();
    }
}

