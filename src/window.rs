use adw::subclass::prelude::*;
use gtk::{gio, glib, prelude::*, TemplateChild};
// Не забудь импортировать страницы, если нужно (обычно main делает это глобально)

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

            // Подключаем логику переключения
            obj.setup_navigation();
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

    fn setup_navigation(&self) {
        let imp = self.imp();

        // Логика: При клике на меню берем имя строки и открываем страницу с таким же именем
        imp.navigation_list.connect_row_activated(glib::clone!(@weak self as window => move |_, row| {
            let imp = window.imp();
            if let Some(child) = row.child() {
                // Получаем имя виджета, заданное в XML (page_vpn, page_proxy)
                let page_name = child.widget_name();
                imp.view_stack.set_visible_child_name(&page_name);
            }
        }));

        // Активируем первую страницу при старте
        if let Some(row) = imp.navigation_list.row_at_index(0) {
            imp.navigation_list.select_row(Some(&row));
            // Имитируем клик, чтобы открылась страница
            row.activate();
        }
    }
}

