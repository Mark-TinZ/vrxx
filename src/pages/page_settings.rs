use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/pages/page_settings.ui")]
    pub struct VrxxPageSettings {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxPageSettings {
        const NAME: &'static str = "VrxxPageSettings";
        type Type = super::VrxxPageSettings;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxPageSettings {}
    impl WidgetImpl for VrxxPageSettings {}
    impl BinImpl for VrxxPageSettings {}
}

glib::wrapper! {
    pub struct VrxxPageSettings(ObjectSubclass<imp::VrxxPageSettings>)
        @extends gtk::Widget, adw::Bin;
}

impl VrxxPageSettings {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}
