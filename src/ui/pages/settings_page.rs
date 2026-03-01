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
        @extends gtk::Widget, adw::Bin;
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

        // Load log level
        let log_idx = match settings.log_level.as_str() {
            "error" => 0,
            "warning" => 1,
            "debug" => 3,
            _ => 2, // info default
        };
        imp.log_level_row.set_selected(log_idx);

        // Bind signals
        imp.core_selector.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.core = if row.selected() == 1 { "sing-box".to_string() } else { "xray".to_string() };
            manager.save(&s);
        });

        imp.language_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.language = match row.selected() {
                1 => "en".to_string(),
                2 => "ru".to_string(),
                _ => "system".to_string(),
            };
            manager.save(&s);
        });

        imp.autostart_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.autostart = row.is_active();
            manager.save(&s);
        });

        imp.connect_startup_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.connect_on_startup = row.is_active();
            manager.save(&s);
        });

        imp.notifications_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.notifications = row.is_active();
            manager.save(&s);
        });

        imp.streamer_mode_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.streamer_mode = row.is_active();
            manager.save(&s);
            // In real app, we would notify other pages here
        });

        imp.tun_mode_row.connect_enable_expansion_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.tun_mode = row.enables_expansion();
            manager.save(&s);
        });

        imp.log_level_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.log_level = match row.selected() {
                0 => "error".to_string(),
                1 => "warning".to_string(),
                3 => "debug".to_string(),
                _ => "info".to_string(),
            };
            manager.save(&s);
        });
    }
}
