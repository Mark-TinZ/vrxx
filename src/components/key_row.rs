use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/components/key_row.ui")]
    pub struct VrxxKeyRow {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxKeyRow {
        const NAME: &'static str = "VrxxKeyRow";
        type Type = super::VrxxKeyRow;
        type ParentType = adw::ActionRow; // Наследуемся от стандартной строки Adwaita

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxKeyRow {}
    impl WidgetImpl for VrxxKeyRow {}
    impl ListBoxRowImpl for VrxxKeyRow {}
    impl PreferencesRowImpl for VrxxKeyRow {}
    impl ActionRowImpl for VrxxKeyRow {}
}

glib::wrapper! {
    pub struct VrxxKeyRow(ObjectSubclass<imp::VrxxKeyRow>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, adw::ActionRow;
}

impl VrxxKeyRow {
    pub fn new(title: &str, subtitle: &str) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.set_title(title);
        obj.set_subtitle(subtitle);
        obj
    }
}
