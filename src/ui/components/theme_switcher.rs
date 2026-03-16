use gtk::{glib, subclass::prelude::*, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/theme_switcher.ui")]
    pub struct VrxxThemeSwitcher {}

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxThemeSwitcher {
        const NAME: &'static str = "VrxxThemeSwitcher";
        type Type = super::VrxxThemeSwitcher;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxThemeSwitcher {}
    impl WidgetImpl for VrxxThemeSwitcher {}
    impl BoxImpl for VrxxThemeSwitcher {}
}

glib::wrapper! {
    pub struct VrxxThemeSwitcher(ObjectSubclass<imp::VrxxThemeSwitcher>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VrxxThemeSwitcher {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

