use crate::ui::models::VpnKeyObject;
use gtk::{gio, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::sync::OnceLock;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/vpn_key_row.ui")]
    pub struct VrxxVpnKeyRow {
        #[template_child]
        pub header_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub icon_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub details_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub lbl_down: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_up: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_time: TemplateChild<gtk::Label>,
        #[template_child]
        pub lbl_ping: TemplateChild<gtk::Label>,
        #[template_child]
        pub btn_refresh_ping: TemplateChild<gtk::Button>,

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
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("request-edit").build(),
                    glib::subclass::Signal::builder("request-info").build(),
                    glib::subclass::Signal::builder("request-delete").build(),
                    glib::subclass::Signal::builder("request-copy-link").build(),
                    glib::subclass::Signal::builder("request-copy-json").build(),
                    glib::subclass::Signal::builder("request-ping").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
        }
    }
    impl WidgetImpl for VrxxVpnKeyRow {}
    impl ListBoxRowImpl for VrxxVpnKeyRow {}
}

glib::wrapper! {
    pub struct VrxxVpnKeyRow(ObjectSubclass<imp::VrxxVpnKeyRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl Default for VrxxVpnKeyRow {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxVpnKeyRow {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn bind(&self, item: &VpnKeyObject) {
        let imp = self.imp();
        imp.item.replace(Some(item.clone()));

        item.bind_property("name", &*imp.header_row, "title")
            .sync_create()
            .build();
        item.bind_property("protocol", &*imp.header_row, "subtitle")
            .sync_create()
            .build();

        item.bind_property("traffic-down", &*imp.lbl_down, "label")
            .sync_create()
            .build();
        item.bind_property("traffic-up", &*imp.lbl_up, "label")
            .sync_create()
            .build();
        item.bind_property("time-connected", &*imp.lbl_time, "label")
            .sync_create()
            .build();
        item.bind_property("ping", &*imp.lbl_ping, "label")
            .sync_create()
            .build();

        let row_weak = self.downgrade();
        item.connect_is_active_notify(move |item| {
            let row = match row_weak.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        let row_weak_loading = self.downgrade();
        item.connect_is_loading_notify(move |item| {
            let row = match row_weak_loading.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        let row_weak_error = self.downgrade();
        item.connect_is_error_notify(move |item| {
            let row = match row_weak_error.upgrade() {
                Some(r) => r,
                None => return,
            };
            row.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
        });

        self.update_visual_state(item.is_active(), item.is_loading(), item.is_error());
    }

    pub fn item(&self) -> Option<VpnKeyObject> {
        self.imp().item.borrow().clone()
    }

    fn setup_callbacks(&self) {
        let row_weak = self.downgrade();
        self.imp().btn_refresh_ping.connect_clicked(move |_| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-ping", &[]);
            }
        });
    }

    fn update_visual_state(&self, is_active: bool, is_loading: bool, is_error: bool) {
        let imp = self.imp();
        imp.details_revealer.set_reveal_child(is_active);

        if is_loading {
            imp.icon_stack.set_visible_child_name("loading");
        } else if is_error {
            imp.icon_stack.set_visible_child_name("error");
        } else if is_active {
            imp.icon_stack.set_visible_child_name("active");
        } else {
            imp.icon_stack.set_visible_child_name("inactive");
        }
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();

        // Action: Info
        let info_action = gio::SimpleAction::new("key_info", None);
        let row_weak_info = self.downgrade();
        info_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak_info.upgrade() {
                row.emit_by_name::<()>("request-info", &[]);
            }
        });
        action_group.add_action(&info_action);

        // Action: Delete
        let delete_action = gio::SimpleAction::new("delete", None);
        let row_weak = self.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-delete", &[]);
            }
        });
        action_group.add_action(&delete_action);

        // Action: Edit
        let edit_action = gio::SimpleAction::new("key_edit", None);
        let row_weak = self.downgrade();
        edit_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-edit", &[]);
            }
        });
        action_group.add_action(&edit_action);

        // Action: Copy Link
        let copy_link_action = gio::SimpleAction::new("key_copy_link", None);
        let row_weak = self.downgrade();
        copy_link_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-copy-link", &[]);
            }
        });
        action_group.add_action(&copy_link_action);

        // Action: Copy JSON
        let copy_json_action = gio::SimpleAction::new("key_copy_json", None);
        let row_weak = self.downgrade();
        copy_json_action.connect_activate(move |_, _| {
            if let Some(row) = row_weak.upgrade() {
                row.emit_by_name::<()>("request-copy-json", &[]);
            }
        });
        action_group.add_action(&copy_json_action);

        self.insert_action_group("row", Some(&action_group));
    }
}
