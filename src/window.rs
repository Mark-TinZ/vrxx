use adw::subclass::prelude::*;
use gtk::{gio, glib, prelude::*};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/window.ui")]
    pub struct VrxxWindow {
        // Template widgets
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

            // Инициализация логики (Presenter logic)
            let obj = self.obj();
            obj.setup_callbacks();

            // Выбираем первый элемент по умолчанию
            let row = self.navigation_list.row_at_index(0);
            self.navigation_list.select_row(row.as_ref());
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

        // Логика переключения страниц
        // Когда выбирается строка в ListBox, мы берем её имя (например "page_vpn")
        // и переключаем ViewStack на эту страницу.
        imp.navigation_list.connect_row_activated(
            glib::clone!(@weak self as window => move |_, row| {
                let imp = window.imp();

                // Получаем виджет внутри строки (AdwActionRow)
                if let Some(action_row) = row.child().and_downcast::<adw::ActionRow>() {
                    // Имя страницы мы храним в свойстве "name" виджета AdwActionRow
                    let page_name = action_row.widget_name();
                    if !page_name.is_empty() {
                        imp.view_stack.set_visible_child_name(&page_name);
                    }
                }
            }),
        );
    }
}
