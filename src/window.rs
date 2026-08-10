use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::ui::pages::{VrxxProxyPage, VrxxSettingsPage, VrxxVpnPage, VrxxWhitelistPage};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/window.ui")]
    pub struct VrxxWindow {
        #[template_child]
        pub navigation_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub view_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub active_connection_btn: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub active_server_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_server_details: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_connection_timer: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_server_traffic: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWindow {
        const NAME: &'static str = "VrxxWindow";
        type Type = super::VrxxWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            VrxxVpnPage::static_type();
            VrxxProxyPage::static_type();
            VrxxWhitelistPage::static_type();
            VrxxSettingsPage::static_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.start_status_polling();

            if let Some(row) = self.navigation_list.row_at_index(0) {
                self.navigation_list.select_row(Some(&row));
                // Принудительная установка начальной страницы
                if let Some(name) = obj.get_page_name_from_row(&row) {
                    self.view_stack.set_visible_child_name(name);
                }
            }
        }
    }
    impl WidgetImpl for VrxxWindow {}
    impl WindowImpl for VrxxWindow {}
    impl ApplicationWindowImpl for VrxxWindow {}
    impl AdwApplicationWindowImpl for VrxxWindow {}
}

glib::wrapper! {
    pub struct VrxxWindow(ObjectSubclass<imp::VrxxWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                   gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl VrxxWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    // Хелпер для извлечения имени страницы из строки
    fn get_page_name_from_row(&self, row: &gtk::ListBoxRow) -> Option<&'static str> {
        match row.index() {
            0 => Some("page_vpn"),
            1 => Some("page_proxy"),
            2 => Some("page_whitelist"),
            3 => Some("page_settings"),
            _ => None,
        }
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        // ИСПОЛЬЗУЕМ connect_row_selected ВМЕСТО connect_row_activated
        let window_weak = self.downgrade();
        imp.navigation_list.connect_row_selected(move |_, row| {
            // row здесь имеет тип Option<&gtk::ListBoxRow>
            if let Some(row) = row {
                let window = match window_weak.upgrade() {
                    Some(w) => w,
                    None => return,
                };
                let imp = window.imp();

                if let Some(page_name) = window.get_page_name_from_row(row) {
                    imp.view_stack.set_visible_child_name(page_name);
                }
            }
        });
    }

    fn start_status_polling(&self) {
        let obj = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(1000), move || {
            obj.update_active_connection_widget();
            glib::ControlFlow::Continue
        });
    }

    fn update_active_connection_widget(&self) {
        use crate::settings::SettingsManager;
        let settings = SettingsManager::new().load();

        let imp = self.imp();

        if let Some(active_key) = settings.keys.iter().find(|k| k.is_active) {
            imp.active_connection_btn.set_visible(true);
            imp.active_server_name.set_label(&active_key.name);
            imp.active_server_details.set_label(&format!(
                "Protocol: {}\nIP: {}",
                active_key.protocol.to_uppercase(),
                active_key.location
            ));
        } else {
            imp.active_connection_btn.set_visible(false);
        }
    }

    pub fn update_stats(&self, time: &str, down: &str, up: &str) {
        let imp = self.imp();
        imp.active_connection_timer.set_label(time);
        imp.active_server_traffic
            .set_label(&format!("↓ {} | ↑ {}", down, up));
    }

    pub fn handle_open_uri(&self, uri: &str) {
        match crate::domain::key_parser::parse_vpn_key(uri) {
            Ok(parsed) => {
                self.present();
                let window_weak = self.downgrade();
                crate::ui::import_dialog::show_import_dialog(
                    self.upcast_ref::<gtk::Window>(),
                    parsed,
                    move |parsed_import| {
                        if let Some(window) = window_weak.upgrade() {
                            window.import_key_to_vpn_page(parsed_import, false);
                        }
                    },
                    {
                        let window_weak = self.downgrade();
                        move |parsed_connect| {
                            if let Some(window) = window_weak.upgrade() {
                                window.import_key_to_vpn_page(parsed_connect, true);
                            }
                        }
                    },
                );
            }
            Err(e) => {
                tracing::error!("Failed to parse URL scheme link '{uri}': {e}");
            }
        }
    }

    fn import_key_to_vpn_page(&self, parsed: crate::domain::key_parser::ParsedKey, connect: bool) {
        let imp = self.imp();
        if let Some(vpn_widget) = imp.view_stack.child_by_name("page_vpn") {
            if let Some(vpn_page) = vpn_widget.downcast_ref::<VrxxVpnPage>() {
                vpn_page.import_key(parsed, connect);
            }
        }
    }
}
