use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/whitelist_page.ui")]
    pub struct VrxxWhitelistPage {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWhitelistPage {
        const NAME: &'static str = "VrxxWhitelistPage";
        type Type = super::VrxxWhitelistPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxWhitelistPage {}
    impl WidgetImpl for VrxxWhitelistPage {}
    impl BinImpl for VrxxWhitelistPage {}
}

glib::wrapper! {
    pub struct VrxxWhitelistPage(ObjectSubclass<imp::VrxxWhitelistPage>)
        @extends gtk::Widget, adw::Bin;
}

impl VrxxWhitelistPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}
