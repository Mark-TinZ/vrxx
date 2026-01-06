use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate, TemplateChild};
use crate::components::key_row::VrxxKeyRow;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/pages/page_vpn.ui")]
    pub struct VrxxPageVpn {
        #[template_child]
        pub keys_list_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub btn_add: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxPageVpn {
        const NAME: &'static str = "VrxxPageVpn";
        type Type = super::VrxxPageVpn;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxPageVpn {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Пример: добавим тестовые данные при запуске
            obj.add_vpn_key("Netherlands #1", "vless://...");
            obj.add_vpn_key("USA Premium", "vmess://...");

            // Обработка нажатия на кнопку "Добавить"
            self.btn_add.connect_clicked(glib::clone!(@weak obj => move |_| {
                obj.add_vpn_key("New Server", "Just added via code");
            }));
        }
    }
    impl WidgetImpl for VrxxPageVpn {}
    impl BinImpl for VrxxPageVpn {}
}

glib::wrapper! {
    pub struct VrxxPageVpn(ObjectSubclass<imp::VrxxPageVpn>)
        @extends gtk::Widget, adw::Bin;
}

impl VrxxPageVpn {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn add_vpn_key(&self, name: &str, protocol: &str) {
        let imp = self.imp();
        // Создаем наш кастомный виджет
        let row = VrxxKeyRow::new(name, protocol);
        // Добавляем в список
        imp.keys_list_box.append(&row);
    }
}
