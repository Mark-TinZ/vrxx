use adw::subclass::prelude::*;
use adw::prelude::*;
use gtk::{glib, CompositeTemplate};
use crate::ui::setup_primary_menu;
use crate::settings::SettingsManager;
use std::cell::RefCell;
use gettextrs::gettext;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/proxy_page.ui")]
    pub struct VrxxProxyPage {
        #[template_child]
        pub btn_apply: TemplateChild<gtk::Button>,
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub system_proxy_switch: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub tun_mode_switch: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub socks_port_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub http_port_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub allow_lan_switch: TemplateChild<adw::SwitchRow>,

        pub has_changes: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxProxyPage {
        const NAME: &'static str = "VrxxProxyPage";
        type Type = super::VrxxProxyPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::SwitchRow::static_type();
            adw::SpinRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxProxyPage {
        fn constructed(&self) {
            self.parent_constructed();
            setup_primary_menu(&self.primary_menu_btn.get());
            self.obj().setup_settings();
        }
    }
    impl WidgetImpl for VrxxProxyPage {}
    impl BinImpl for VrxxProxyPage {}
}

glib::wrapper! {
    pub struct VrxxProxyPage(ObjectSubclass<imp::VrxxProxyPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxProxyPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxProxyPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn mark_changed(&self) {
        let imp = self.imp();
        *imp.has_changes.borrow_mut() = true;
        imp.btn_apply.set_visible(true);
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.allow_lan_switch.set_title(&gettext("Allow connections from LAN"));
        imp.allow_lan_switch.set_subtitle(&gettext("Proxy will be available for other devices in your local network"));

        imp.btn_apply.set_visible(false);

        imp.btn_apply.connect_clicked(glib::clone!(
            #[weak(rename_to = page)] self,
            move |btn| {
                let _ = crate::settings::core_restart_channel().0.send_blocking(());
                if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>() {
                    let notification = gtk::gio::Notification::new(&gettext("Settings applied"));
                    notification.set_body(Some(&gettext("Core was restarted to apply new settings.")));
                    app.send_notification(Some("settings_applied"), &notification);
                }
                *page.imp().has_changes.borrow_mut() = false;
                btn.set_visible(false);
            }
        ));

        // Load values
        imp.system_proxy_switch.set_active(settings.set_system_proxy);
        imp.tun_mode_switch.set_active(settings.tun_mode);
        imp.socks_port_row.set_value(settings.socks_port as f64);
        imp.http_port_row.set_value(settings.http_port as f64);
        imp.allow_lan_switch.set_active(settings.allow_lan);

        // Bind signals to save
        imp.system_proxy_switch.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.set_system_proxy = row.is_active();
                manager.save(&s);
                crate::backend::CoreBackend::new().update_system_proxy(s.set_system_proxy);
                page.mark_changed();
            }
        ));

        imp.tun_mode_switch.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.tun_mode = row.is_active();
                manager.save(&s);
                page.mark_changed();
            }
        ));

        imp.socks_port_row.connect_value_notify(glib::clone!(
            #[weak(rename_to = page)] self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.socks_port = row.value() as u16;
                manager.save(&s);
                page.mark_changed();
            }
        ));

        imp.http_port_row.connect_value_notify(glib::clone!(
            #[weak(rename_to = page)] self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.http_port = row.value() as u16;
                manager.save(&s);
                page.mark_changed();
            }
        ));

        imp.allow_lan_switch.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self,
            move |row| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                s.allow_lan = row.is_active();
                manager.save(&s);
                page.mark_changed();
            }
        ));
    }
}
