use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::ui::pages::{VrxxVpnPage, VrxxProxyPage, VrxxWhitelistPage, VrxxSettingsPage};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/window.ui")]
    pub struct VrxxWindow {
        #[template_child]
        pub navigation_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub view_stack: TemplateChild<gtk::Stack>,
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
        imp.navigation_list.connect_row_selected(
            move |_, row| {
                // row здесь имеет тип Option<&gtk::ListBoxRow>
                if let Some(row) = row {
                    let window = match window_weak.upgrade() {
                        Some(w) => w,
                        None => return,
                    };
                    let imp = window.imp();

                    if let Some(page_name) = window.get_page_name_from_row(row) {
                        // crate::backend::log_app_event("info", &format!("DEBUG: Switching to page '{}'", page_name));
                        imp.view_stack.set_visible_child_name(page_name);
                    } else {
                        // crate::backend::log_app_event("warn", "DEBUG: Could not determine page name for row");
                    }
                }
            },
        );
    }
}

