use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/pages/page_whitelist.ui")]
    pub struct VrxxPageWhitelist {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxPageWhitelist {
        const NAME: &'static str = "VrxxPageWhitelist";
        type Type = super::VrxxPageWhitelist;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxPageWhitelist {}
    impl WidgetImpl for VrxxPageWhitelist {}
    impl BinImpl for VrxxPageWhitelist {}
}

glib::wrapper! {
    pub struct VrxxPageWhitelist(ObjectSubclass<imp::VrxxPageWhitelist>)
        @extends gtk::Widget, adw::Bin;
}

impl VrxxPageWhitelist {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}
