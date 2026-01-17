use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, gdk, CompositeTemplate};
use crate::ui::models::DomainObject;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/whitelist_page.ui")]
    pub struct VrxxWhitelistPage {
        #[template_child]
        pub domains_list: TemplateChild<gtk::ListBox>,

        // Хранилище списка доменов
        pub model: RefCell<Option<gio::ListStore>>,
    }

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

    impl ObjectImpl for VrxxWhitelistPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_model();
            self.obj().setup_actions();
        }
    }
    impl WidgetImpl for VrxxWhitelistPage {}
    impl BinImpl for VrxxWhitelistPage {}
}

glib::wrapper! {
    pub struct VrxxWhitelistPage(ObjectSubclass<imp::VrxxWhitelistPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl VrxxWhitelistPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_model(&self) {
        let model = gio::ListStore::new::<DomainObject>();

        model.append(&DomainObject::new("*.ru"));
        model.append(&DomainObject::new("vk.com"));

        self.imp().model.replace(Some(model.clone()));

        // Привязываем модель
        self.imp().domains_list.bind_model(Some(&model), move |item| {
            let domain_obj = item.downcast_ref::<DomainObject>().unwrap();

            let row = adw::ActionRow::builder()
                .selectable(false)
                .activatable(true) // Строка кликабельна для редактирования
                .build();

            // Привязываем свойство объекта к заголовку строки
            domain_obj.bind_property("domain", &row, "title")
                .sync_create()
                .build();

            // Кнопка удаления
            let btn_delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .valign(gtk::Align::Center)
                .has_frame(false)
                .tooltip_text("Удалить")
                .css_classes(vec!["flat"])
                .build();

            btn_delete.set_action_name(Some("whitelist.remove_domain"));
            btn_delete.set_action_target_value(Some(&domain_obj.domain().to_variant()));

            row.add_suffix(&btn_delete);
            row.upcast()
        });

        // Обработка клика по строке (Редактирование)
        let page_weak = self.downgrade();
        self.imp().domains_list.connect_row_activated(move |_, row| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            let index = row.index();
            if index >= 0 {
                 let imp = page.imp();
                 let borrowed_model = imp.model.borrow();
                 if let Some(model) = borrowed_model.as_ref() {
                     if let Some(item) = model.item(index as u32) {
                         let domain_obj = item.downcast::<DomainObject>().unwrap();
                         page.show_domain_dialog(Some(domain_obj));
                     }
                 }
            }
        });
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();

        // Действие: Добавить
        let add_action = gio::SimpleAction::new("add_domain", None);
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, _| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };
            page.show_domain_dialog(None);
        });
        action_group.add_action(&add_action);

        // Действие: Удалить
        let remove_action = gio::SimpleAction::new("remove_domain", Some(glib::VariantTy::STRING));
        let page_weak = self.downgrade();
        remove_action.connect_activate(move |_, parameter| {
             let page = match page_weak.upgrade() {
                 Some(p) => p,
                 None => return,
             };
             if let Some(domain_str) = parameter.and_then(|p| p.get::<String>()) {
                 let imp = page.imp();
                 let model_borrow = imp.model.borrow();

                 if let Some(model) = model_borrow.as_ref() {
                     let mut index_to_remove = None;
                     for i in 0..model.n_items() {
                         if let Some(obj) = model.item(i).and_then(|o| o.downcast::<DomainObject>().ok()) {
                             if obj.domain() == domain_str {
                                 index_to_remove = Some(i);
                                 break;
                             }
                         }
                     }
                     if let Some(i) = index_to_remove {
                         model.remove(i);
                     }
                 }
             }
        });
        action_group.add_action(&remove_action);

        self.insert_action_group("whitelist", Some(&action_group));
    }

    #[allow(deprecated)]
    fn show_domain_dialog(&self, target_obj: Option<DomainObject>) {
        let is_editing = target_obj.is_some();
        let title = if is_editing { "Изменить домен" } else { "Новый домен" };
        let button_label = if is_editing { "Сохранить" } else { "Добавить" };
        let initial_text = target_obj.as_ref().map(|o| o.domain()).unwrap_or_default();

        // Поле ввода
        let entry_row = adw::EntryRow::builder()
            .title("Домен")
            .text(&initial_text)
            .show_apply_button(false)
            .build();

        entry_row.grab_focus();

        let group = adw::PreferencesGroup::builder()
            .build();
        group.add(&entry_row);

        let content_area = adw::PreferencesPage::builder()
            .build();
        content_area.add(&group);

        // Создаем диалог
        let dialog = adw::MessageDialog::builder()
            .heading(title)
            .body("Введите адрес домена (например, google.com или *.ru)")
            .extra_child(&content_area)
            .modal(true)
            .close_response("cancel")
            .default_response("apply")
            .build();

        // Устанавливаем родителя
        if let Some(root) = self.root().and_then(|w| w.downcast::<gtk::Window>().ok()) {
            dialog.set_transient_for(Some(&root));
        }

        dialog.add_response("cancel", "Отмена");
        dialog.add_response("apply", button_label);
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);

        // --- ДОБАВЛЕНО: Обработка нажатия Enter ---
        let controller = gtk::EventControllerKey::new();
        // Используем Capture, чтобы перехватить событие до того, как его поглотит внутренняя Entry
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);

        let dialog_weak = dialog.downgrade();
        controller.connect_key_pressed(move |_, keyval, _, _| {
            match keyval {
                gdk::Key::Return | gdk::Key::ISO_Enter | gdk::Key::KP_Enter => {
                    if let Some(d) = dialog_weak.upgrade() {
                        d.response("apply");
                        return glib::Propagation::Stop;
                    }
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
        entry_row.add_controller(controller);

        // --- ПОДГОТОВКА ЗАМЫКАНИЯ (РУЧНОЙ ЗАХВАТ) ---
        // 1. Создаем слабую ссылку на страницу (self)
        let page_weak = self.downgrade();
        // 2. Клонируем переменные для перемещения в замыкание
        let entry_row_clone = entry_row.clone();
        let target_obj_clone = target_obj.clone();

        // 3. Создаем замыкание без макроса glib::clone!
        dialog.connect_response(None, move |d: &adw::MessageDialog, response: &str| {
            // Восстанавливаем self из слабой ссылки
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return, // Если страница уже уничтожена, выходим
            };

            if response == "apply" {
                let text = entry_row_clone.text();
                if !text.is_empty() {
                    let imp = page.imp();
                    let model_borrow = imp.model.borrow();

                    if let Some(model) = model_borrow.as_ref() {
                        if let Some(existing_obj) = target_obj_clone.as_ref() {
                            // Редактирование
                            existing_obj.set_domain(text.as_str());
                        } else {
                            // Создание нового
                            model.append(&DomainObject::new(text.as_str()));
                        }
                    }
                }
            }
            d.close();
        });

        dialog.present();
    }
}

