use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/settings_page.ui")]
    pub struct VrxxSettingsPage {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxSettingsPage {
        const NAME: &'static str = "VrxxSettingsPage";
        type Type = super::VrxxSettingsPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxSettingsPage {}
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
}
