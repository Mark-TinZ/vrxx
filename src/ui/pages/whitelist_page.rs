use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use crate::ui::setup_primary_menu;
use crate::settings::SettingsManager;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/whitelist_page.ui")]
    pub struct VrxxWhitelistPage {
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub enable_routing_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub mode_row: TemplateChild<adw::ComboRow>,

        #[template_child]
        pub route_ru_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_cn_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_ir_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub route_antifilter_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub disable_ipv6_row: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWhitelistPage {
        const NAME: &'static str = "VrxxWhitelistPage";
        type Type = super::VrxxWhitelistPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ComboRow::static_type();
            adw::SwitchRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxWhitelistPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_settings();
            setup_primary_menu(&self.primary_menu_btn.get());
        }
    }
    impl WidgetImpl for VrxxWhitelistPage {}
    impl BinImpl for VrxxWhitelistPage {}
}

glib::wrapper! {
    pub struct VrxxWhitelistPage(ObjectSubclass<imp::VrxxWhitelistPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxWhitelistPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxWhitelistPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.enable_routing_row.set_active(settings.enable_routing);
        
        let mode_idx = match settings.routing_mode.as_str() {
            "proxy" => 1,
            _ => 0, // bypass
        };
        imp.mode_row.set_selected(mode_idx);

        imp.route_ru_row.set_active(settings.route_ru);
        imp.route_cn_row.set_active(settings.route_cn);
        imp.route_ir_row.set_active(settings.route_ir);
        imp.route_antifilter_row.set_active(settings.route_antifilter);
        imp.disable_ipv6_row.set_active(settings.disable_ipv6);

        imp.enable_routing_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_routing = row.is_active();
            manager.save(&s);
        });

        imp.mode_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.routing_mode = match row.selected() {
                1 => "proxy".to_string(),
                _ => "bypass".to_string(),
            };
            manager.save(&s);
        });

        imp.route_ru_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.route_ru = row.is_active();
            manager.save(&s);
        });

        imp.route_cn_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.route_cn = row.is_active();
            manager.save(&s);
        });

        imp.route_ir_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.route_ir = row.is_active();
            manager.save(&s);
        });

        imp.route_antifilter_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.route_antifilter = row.is_active();
            manager.save(&s);
        });

        imp.disable_ipv6_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.disable_ipv6 = row.is_active();
            manager.save(&s);
        });
    }
}
