use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib}; // Убран prelude::* так как он не использовался явно

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/window.ui")]
    pub struct VrxxWindow {
        #[template_child]
        pub navigation_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWindow {
        const NAME: &'static str = "VrxxWindow";
        type Type = super::VrxxWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
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
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        // Чтобы избежать warning'а "old-style clone syntax", вынесем переменную
        let view_stack = &imp.view_stack;

        imp.navigation_list.connect_row_activated(
            glib::clone!(@weak view_stack => move |_, row| {
                if let Some(child) = row.child() {
                    let page_name = child.widget_name();
                    view_stack.set_visible_child_name(&page_name);
                }
            }),
        );
    }
}

