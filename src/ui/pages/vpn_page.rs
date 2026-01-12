use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate}; // Removed prelude::*

// Импортируем модель и компонент строки
use crate::ui::models::VpnKeyObject;
use crate::ui::components::vpn_key_row::VrxxVpnKeyRow;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/vpn_page.ui")]
    pub struct VrxxVpnPage {
        #[template_child]
        pub keys_list: TemplateChild<gtk::ListBox>,

        // ДОБАВЛЯЕМ ЭТО ПОЛЕ:
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,

        // ДОБАВЛЕНО: Поле для хранения модели данных
        pub model: RefCell<Option<gio::ListStore>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxVpnPage {
        const NAME: &'static str = "VrxxVpnPage";
        type Type = super::VrxxVpnPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            // Регистрируем тип строки, чтобы GtkBuilder его увидел
            VrxxVpnKeyRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxVpnPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_model();
            self.obj().setup_actions();
            self.obj().setup_callbacks();
        }
    }
    impl WidgetImpl for VrxxVpnPage {}
    impl BinImpl for VrxxVpnPage {}
}

glib::wrapper! {
    pub struct VrxxVpnPage(ObjectSubclass<imp::VrxxVpnPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxVpnPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_model(&self) {
        let model = gio::ListStore::new::<VpnKeyObject>();

        // Тестовые данные
        let key1 = VpnKeyObject::new("Mark-Vless", "VLESS+Reality", true);
        key1.set_traffic_down("120.4 MB");
        key1.set_ping("25 ms");

        let key2 = VpnKeyObject::new("Wumt-Vless", "VMess", false);

        let key3 = VpnKeyObject::new("Eleon-Vless", "VMess", false);
        key3.set_traffic_down("560.2 MB");
        key3.set_traffic_up("205.9 MB");
        key3.set_time_connected("00:50:25");
        key3.set_ping("105 ms");


        model.append(&key1);
        model.append(&key2);
        model.append(&key3);

        self.imp().model.replace(Some(model.clone()));

        self.imp().keys_list.bind_model(Some(&model), move |item| {
            let key_obj = item.downcast_ref::<VpnKeyObject>().unwrap();
            let row = VrxxVpnKeyRow::new();
            row.bind(key_obj);
            row.upcast::<gtk::Widget>()
        });
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        let page_weak = self.downgrade();
        imp.keys_list.connect_row_activated(move |_, row| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            // ИСПРАВЛЕНИЕ 1: Клонируем row перед downcast, так как row здесь ссылка
            if let Ok(key_row) = row.clone().downcast::<VrxxVpnKeyRow>() {
                if let Some(selected_item) = key_row.item() {
                    // 1. Обновляем активный ключ в модели
                    page.set_active_key(&selected_item);

                    // 2. Формируем строку
                    let new_subtitle = format!("{} Подключено", selected_item.name());

                    // 3. Обращаемся к виджету заголовка через imp()
                    page.imp().window_title.set_subtitle(&new_subtitle);

                    println!("Выбран ключ: {}", selected_item.name());
                }
            }
        });
    }

    fn set_active_key(&self, active_item: &VpnKeyObject) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i).and_then(|obj| obj.downcast::<VpnKeyObject>().ok()) {
                    let is_target = item.name() == active_item.name();
                    item.set_is_active(is_target);
                }
            }
        }
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();

        let add_action = gio::SimpleAction::new("add_key", None);
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, _| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            println!("Создаем новый ключ...");
            // ИСПРАВЛЕНИЕ 2: Разбиваем цепочку вызовов, чтобы избежать временных заимствований
            let imp = page.imp();
            let borrowed_model = imp.model.borrow(); // Продлеваем жизнь Ref

            if let Some(model) = borrowed_model.as_ref() {
                let new_key = VpnKeyObject::new("New Key", "VLESS", false);
                model.append(&new_key);
            }
        });
        action_group.add_action(&add_action);

        // Действие: Редактировать
        let edit_action = gio::SimpleAction::new("key_edit", None);
        let page_weak = self.downgrade();
        edit_action.connect_activate(move |_, _| {
            let _page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            println!("Page Logic: Редактируем ключ (внутри VpnPage)");
            // Здесь мы имеем доступ к `page` и её внутреннему состоянию!
        });
        action_group.add_action(&edit_action);

        // Действие: Дублировать
        let dup_action = gio::SimpleAction::new("key_duplicate", None);
        let page_weak = self.downgrade();
        dup_action.connect_activate(move |_, _| {
            let _page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            println!("Page Logic: Дублируем ключ");
        });
        action_group.add_action(&dup_action);

        // Действие: Удалить
        let del_action = gio::SimpleAction::new("key_delete", None);
        let page_weak = self.downgrade();
        del_action.connect_activate(move |_, _| {
            let _page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            println!("Page Logic: Удаляем ключ");
        });
        action_group.add_action(&del_action);

        self.insert_action_group("vpn", Some(&action_group));
    }
}

