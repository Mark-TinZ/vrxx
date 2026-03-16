use adw::subclass::prelude::*;
use adw::prelude::*;
use gtk::{glib, CompositeTemplate};
use crate::ui::setup_primary_menu;
use crate::settings::SettingsManager;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/settings_page.ui")]
    pub struct VrxxSettingsPage {
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub core_selector: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub core_info_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub language_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub autostart_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub connect_startup_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub notifications_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub streamer_mode_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub tun_mode_row: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub log_level_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub sniffing_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub domain_strategy_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub bypass_lan_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub fake_dns_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub mux_row: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub mux_concurrency_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub fragment_row: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxSettingsPage {
        const NAME: &'static str = "VrxxSettingsPage";
        type Type = super::VrxxSettingsPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ComboRow::static_type();
            adw::SwitchRow::static_type();
            adw::ExpanderRow::static_type();
            adw::SpinRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxSettingsPage {
        fn constructed(&self) {
            self.parent_constructed();

            // Connect the shared menu with theme switcher
            setup_primary_menu(&self.primary_menu_btn.get());

            self.obj().setup_settings();
        }
    }
    impl WidgetImpl for VrxxSettingsPage {}
    impl BinImpl for VrxxSettingsPage {}
}

glib::wrapper! {
    pub struct VrxxSettingsPage(ObjectSubclass<imp::VrxxSettingsPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VrxxSettingsPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        // Load core
        let selected_idx = match settings.core.as_str() {
            "sing-box" => 1,
            _ => 0, // xray default
        };
        imp.core_selector.set_selected(selected_idx);

        // Load language
        let lang_idx = match settings.language.as_str() {
            "en" => 1,
            "ru" => 2,
            _ => 0, // system
        };
        imp.language_row.set_selected(lang_idx);

        // Load switches
        imp.autostart_row.set_active(settings.autostart);
        imp.connect_startup_row.set_active(settings.connect_on_startup);
        imp.notifications_row.set_active(settings.notifications);
        imp.streamer_mode_row.set_active(settings.streamer_mode);
        imp.tun_mode_row.set_enable_expansion(settings.tun_mode);

        imp.sniffing_row.set_active(settings.enable_sniffing);
        imp.bypass_lan_row.set_active(settings.bypass_lan);
        imp.fake_dns_row.set_active(settings.enable_fake_dns);
        imp.mux_row.set_enable_expansion(settings.enable_mux);
        imp.mux_concurrency_row.set_value(settings.mux_concurrency as f64);
        imp.fragment_row.set_active(settings.enable_fragment);

        let strategy_idx = match settings.domain_strategy.as_str() {
            "IPIfNonMatch" => 1,
            "IPOnDemand" => 2,
            _ => 0, // AsIs
        };
        imp.domain_strategy_row.set_selected(strategy_idx);

        // Load log level
        let log_idx = match settings.log_level.as_str() {
            "error" => 0,
            "warning" => 1,
            "debug" => 3,
            _ => 2, // info default
        };
        imp.log_level_row.set_selected(log_idx);

        // Bind signals
        let self_weak = self.downgrade();
        imp.core_selector.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            let old_core = s.core.clone();
            s.core = if row.selected() == 1 { "sing-box".to_string() } else { "xray".to_string() };
            if old_core != s.core {
                crate::backend::log_app_event("info", &format!("Core changed from {} to {}", old_core, s.core));
            }
            manager.save(&s);
            if let Some(page) = self_weak.upgrade() {
                page.update_core_info();
            }
        });

        self.update_core_info();

        let lang_row_clone = imp.language_row.clone();
        imp.language_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            let old_lang = s.language.clone();
            s.language = match row.selected() {
                1 => "en".to_string(),
                2 => "ru".to_string(),
                _ => "system".to_string(),
            };
            if old_lang != s.language {
                crate::backend::log_app_event("info", &format!("Language changed from {} to {}", old_lang, s.language));
            }
            manager.save(&s);
            
            // Show toast for restart
            if let Some(window) = lang_row_clone.root().and_then(|r| r.downcast::<adw::Window>().ok()) {
                let toast = adw::Toast::new("Требуется перезапуск приложения для применения языка");
                if let Some(toast_overlay) = window.child().and_then(|c| c.downcast::<adw::ToastOverlay>().ok()) {
                    toast_overlay.add_toast(toast);
                }
            }
        });

        imp.autostart_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.autostart = row.is_active();
            crate::backend::log_app_event("info", &format!("Autostart toggled to {}", s.autostart));
            manager.save(&s);
            
            // Apply autostart
            let autostart_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("autostart");
            let desktop_file = autostart_dir.join("ru.mark.vrxx.desktop");
            
            if s.autostart {
                std::fs::create_dir_all(&autostart_dir).ok();
                let content = "[Desktop Entry]\nName=Vrxx\nComment=Advanced Xray/Sing-box Client\nExec=vrxx\nIcon=ru.mark.vrxx\nTerminal=false\nType=Application\nCategories=Network;VPN;\nStartupWMClass=vrxx\nX-GNOME-Autostart-enabled=true\n";
                let _ = std::fs::write(desktop_file, content);
            } else {
                let _ = std::fs::remove_file(desktop_file);
            }
        });

        imp.connect_startup_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.connect_on_startup = row.is_active();
            crate::backend::log_app_event("info", &format!("Connect on startup toggled to {}", s.connect_on_startup));
            manager.save(&s);
        });

        imp.notifications_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.notifications = row.is_active();
            crate::backend::log_app_event("info", &format!("Notifications toggled to {}", s.notifications));
            manager.save(&s);
        });

        imp.streamer_mode_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.streamer_mode = row.is_active();
            crate::backend::log_app_event("info", &format!("Streamer mode toggled to {}", s.streamer_mode));
            manager.save(&s);
        });

        imp.tun_mode_row.connect_enable_expansion_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.tun_mode = row.enables_expansion();
            crate::backend::log_app_event("info", &format!("TUN mode toggled to {}", s.tun_mode));
            manager.save(&s);
        });

        imp.sniffing_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_sniffing = row.is_active();
            crate::backend::log_app_event("info", &format!("Sniffing toggled to {}", s.enable_sniffing));
            manager.save(&s);
        });

        imp.bypass_lan_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.bypass_lan = row.is_active();
            crate::backend::log_app_event("info", &format!("Bypass LAN toggled to {}", s.bypass_lan));
            manager.save(&s);
        });

        imp.fake_dns_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_fake_dns = row.is_active();
            crate::backend::log_app_event("info", &format!("Fake DNS toggled to {}", s.enable_fake_dns));
            manager.save(&s);
        });

        imp.fragment_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_fragment = row.is_active();
            crate::backend::log_app_event("info", &format!("Fragment toggled to {}", s.enable_fragment));
            manager.save(&s);
        });

        imp.mux_row.connect_enable_expansion_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_mux = row.enables_expansion();
            crate::backend::log_app_event("info", &format!("MUX toggled to {}", s.enable_mux));
            manager.save(&s);
        });

        imp.mux_concurrency_row.connect_value_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.mux_concurrency = row.value() as i32;
            crate::backend::log_app_event("info", &format!("MUX concurrency set to {}", s.mux_concurrency));
            manager.save(&s);
        });

        imp.domain_strategy_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            let old_strategy = s.domain_strategy.clone();
            s.domain_strategy = match row.selected() {
                1 => "IPIfNonMatch".to_string(),
                2 => "IPOnDemand".to_string(),
                _ => "AsIs".to_string(),
            };
            if old_strategy != s.domain_strategy {
                crate::backend::log_app_event("info", &format!("Domain strategy changed from {} to {}", old_strategy, s.domain_strategy));
            }
            manager.save(&s);
        });

        imp.log_level_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            let old_level = s.log_level.clone();
            s.log_level = match row.selected() {
                0 => "error".to_string(),
                1 => "warning".to_string(),
                3 => "debug".to_string(),
                _ => "info".to_string(),
            };
            if old_level != s.log_level {
                crate::backend::log_app_event("info", &format!("Log level changed from {} to {}", old_level, s.log_level));
            }
            manager.save(&s);
        });
    }

    fn update_core_info(&self) {
        let settings = SettingsManager::new().load();
        let bin_name = if settings.core == "sing-box" { "sing-box" } else { "xray" };
        
        // Execute version command
        let output = std::process::Command::new(bin_name)
            .arg("version")
            .output();

        let version_str = match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                // Extract version from first line
                s.lines().next().unwrap_or("Unknown Version").to_string()
            }
            Err(_) => format!("{} не найден", bin_name),
        };

        self.imp().core_info_row.set_subtitle(&version_str);
    }
}
