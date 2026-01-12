use gtk::{gio, glib, prelude::*, subclass::prelude::*, CompositeTemplate};
use crate::ui::models::VpnKeyObject;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/vpn_key_row.ui")]
    pub struct VrxxVpnKeyRow {
        #[template_child]
        pub header_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub separator: TemplateChild<gtk::Separator>,
        #[template_child]
        pub stats_grid: TemplateChild<gtk::Grid>,

        #[template_child]
        pub lbl_down: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_up: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_time: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_ping: TemplateChild<gtk::Label>,

        pub item: RefCell<Option<VpnKeyObject>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxVpnKeyRow {
        const NAME: &'static str = "VrxxVpnKeyRow";
        type Type = super::VrxxVpnKeyRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxVpnKeyRow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_actions();
        }
    }
    impl WidgetImpl for VrxxVpnKeyRow {}
    impl ListBoxRowImpl for VrxxVpnKeyRow {}
}

glib::wrapper! {
    pub struct VrxxVpnKeyRow(ObjectSubclass<imp::VrxxVpnKeyRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VrxxVpnKeyRow {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn bind(&self, item: &VpnKeyObject) {
        let imp = self.imp();
        imp.item.replace(Some(item.clone()));

        item.bind_property("name", &*imp.header_row, "title")
            .sync_create().build();
        item.bind_property("protocol", &*imp.header_row, "subtitle")
            .sync_create().build();

        item.bind_property("traffic-down", &*imp.lbl_down, "label").sync_create().build();
        item.bind_property("traffic-up", &*imp.lbl_up, "label").sync_create().build();
        item.bind_property("time-connected", &*imp.lbl_time, "label").sync_create().build();
        item.bind_property("ping", &*imp.lbl_ping, "label").sync_create().build();

        let row_weak = self.downgrade();
        item.connect_is_active_notify(move |item| {
            let row = match row_weak.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active());
        });

        self.update_visual_state(item.is_active());
    }

    // Вспомогательный метод для получения объекта
    pub fn item(&self) -> Option<VpnKeyObject> {
        self.imp().item.borrow().clone()
    }

    fn update_visual_state(&self, is_active: bool) {
        let imp = self.imp();
        imp.separator.set_visible(is_active);
        imp.stats_grid.set_visible(is_active);

        if is_active {
            imp.status_icon.set_icon_name(Some("security-high-symbolic"));
            imp.status_icon.add_css_class("success");
            imp.status_icon.remove_css_class("error");
        } else {
            imp.status_icon.set_icon_name(Some("security-low-symbolic"));
            imp.status_icon.add_css_class("error");
            imp.status_icon.remove_css_class("success");
        }
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        let delete_action = gio::SimpleAction::new("delete", None);
        let row_weak = self.downgrade();
        delete_action.connect_activate(move |_, _| {
             let row = match row_weak.upgrade() {
                 Some(r) => r,
                 None => return,
             };
             if let Some(item) = row.item() {
                println!("Request delete for: {}", item.name());
            }
        });
        action_group.add_action(&delete_action);
        self.insert_action_group("row", Some(&action_group));
    }
}
