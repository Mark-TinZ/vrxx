use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/proxy_page.ui")]
    pub struct VrxxProxyPage {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxProxyPage {
        const NAME: &'static str = "VrxxProxyPage";
        type Type = super::VrxxProxyPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxProxyPage {}
    impl WidgetImpl for VrxxProxyPage {}
    impl BinImpl for VrxxProxyPage {}
}

glib::wrapper! {
    pub struct VrxxProxyPage(ObjectSubclass<imp::VrxxProxyPage>)
        @extends gtk::Widget, adw::Bin;
}

impl VrxxProxyPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}
