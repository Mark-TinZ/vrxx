use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use gettextrs::gettext;
use crate::ui::models::DomainObject;
use crate::ui::setup_primary_menu;
use crate::settings::SettingsManager;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/whitelist_page.ui")]
    pub struct VrxxWhitelistPage {
        #[template_child]
        pub domains_list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub enable_routing_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub mode_row: TemplateChild<adw::ComboRow>,

        // Хранилище списка доменов
        pub model: RefCell<Option<gio::ListStore>>,
        pub filter_model: RefCell<Option<gtk::FilterListModel>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWhitelistPage {
        const NAME: &'static str = "VrxxWhitelistPage";
        type Type = super::VrxxWhitelistPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ComboRow::static_type();
            adw::ActionRow::static_type();
            adw::SwitchRow::static_type();
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
            self.obj().setup_settings();
            setup_primary_menu(&self.primary_menu_btn.get());
        }
    }
    impl WidgetImpl for VrxxWhitelistPage {}
    impl BinImpl for VrxxWhitelistPage {}
}

glib::wrapper! {
    pub struct VrxxWhitelistPage(ObjectSubclass<imp::VrxxWhitelistPage>)
        @extends gtk::Widget, adw::Bin,
        @implements gio::ActionGroup, gio::ActionMap,
                   gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VrxxWhitelistPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxWhitelistPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.enable_routing_row.set_active(settings.enable_routing);
        
        let mode_idx = match settings.routing_mode.as_str() {
            "proxy" => 1,
            _ => 0, // bypass
        };
        imp.mode_row.set_selected(mode_idx);

        imp.enable_routing_row.connect_active_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.enable_routing = row.is_active();
            crate::backend::log_app_event("info", &format!("Custom routing toggled to {}", s.enable_routing));
            manager.save(&s);
        });

        imp.mode_row.connect_selected_notify(move |row| {
            let manager = SettingsManager::new();
            let mut s = manager.load();
            let old_mode = s.routing_mode.clone();
            s.routing_mode = match row.selected() {
                1 => "proxy".to_string(),
                _ => "bypass".to_string(),
            };
            if old_mode != s.routing_mode {
                crate::backend::log_app_event("info", &format!("Routing mode changed from {} to {}", old_mode, s.routing_mode));
            }
            manager.save(&s);
        });
    }

    fn setup_model(&self) {
        let model = gio::ListStore::new::<DomainObject>();

        // Load from settings
        let settings = SettingsManager::new().load();
        for domain in settings.whitelist {
            model.append(&DomainObject::new(&domain));
        }

        self.imp().model.replace(Some(model.clone()));

        // Setup filter
        let filter = gtk::CustomFilter::new(|_| true); // Default pass-through
        let filter_model = gtk::FilterListModel::new(Some(model.clone()), Some(filter.clone()));
        self.imp().filter_model.replace(Some(filter_model.clone()));

        // Привязываем filter_model
        self.imp().domains_list.bind_model(Some(&filter_model), move |item| {
            let Some(domain_obj) = item.downcast_ref::<DomainObject>() else {
                return adw::ActionRow::builder().build().upcast();
            };

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
                .tooltip_text(gettext("Delete"))
                .css_classes(vec!["flat", "destructive-action"])
                .build();

            btn_delete.set_action_name(Some("whitelist.remove_domain"));
            btn_delete.set_action_target_value(Some(&domain_obj.domain().to_variant()));

            row.add_suffix(&btn_delete);
            row.upcast()
        });

        // Search entry filtering
        let search_entry = &self.imp().search_entry;
        let filter_model_clone = filter_model.clone();
        search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_lowercase();
            let new_filter = gtk::CustomFilter::new(move |item| {
                if text.is_empty() {
                    return true;
                }
                if let Some(obj) = item.downcast_ref::<DomainObject>() {
                    let domain = obj.domain().to_lowercase();
                    if text.contains('*') {
                        let pattern = text.replace(".", "\\.").replace("*", ".*");
                        if let Ok(re) = regex::Regex::new(&format!("^{pattern}$")) {
                            return re.is_match(&domain);
                        }
                    }
                    return domain.contains(&text);
                }
                true
            });
            filter_model_clone.set_filter(Some(&new_filter));
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
                 let borrowed_filter = imp.filter_model.borrow();
                 if let Some(f_model) = borrowed_filter.as_ref() {
                     if let Some(item) = f_model.item(index as u32) {
                         if let Ok(domain_obj) = item.downcast::<DomainObject>() {
                             page.show_domain_dialog(Some(domain_obj));
                         }
                     }
                 }
            }
        });
    }

    fn save_whitelist(&self) {
        if let Some(model) = self.imp().model.borrow().as_ref() {
            let mut list = Vec::new();
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i).and_then(|o| o.downcast::<DomainObject>().ok()) {
                    list.push(item.domain());
                }
            }
            let manager = SettingsManager::new();
            let mut s = manager.load();
            s.whitelist = list;
            manager.save(&s);
        }
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();

        // Действие: Добавить
        let add_action = gio::SimpleAction::new("add_domain", None);
        let page_weak = self.downgrade();
        add_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak.upgrade() {
                page.show_domain_dialog(None);
            }
        });
        action_group.add_action(&add_action);

        // Действие: Удалить
        let remove_action = gio::SimpleAction::new("remove_domain", Some(glib::VariantTy::STRING));
        let page_weak = self.downgrade();
        remove_action.connect_activate(move |_, parameter| {
             if let Some(page) = page_weak.upgrade() {
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
                             page.save_whitelist();
                         }
                     }
                 }
             }
        });
        action_group.add_action(&remove_action);

        // Действие: Очистить все
        let clear_action = gio::SimpleAction::new("clear", None);
        let page_weak = self.downgrade();
        clear_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak.upgrade() {
                let dialog = adw::AlertDialog::builder()
                    .heading(gettext("Clear Whitelist"))
                    .body(gettext("Are you sure you want to remove all domains from the whitelist?"))
                    .build();
                dialog.add_response("cancel", &gettext("Cancel"));
                dialog.add_response("clear", &gettext("Clear"));
                dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
                
                let p_weak = page.downgrade();
                dialog.connect_response(None, move |_, response| {
                    if response == "clear" {
                        if let Some(p) = p_weak.upgrade() {
                            if let Some(model) = p.imp().model.borrow().as_ref() {
                                model.remove_all();
                                p.save_whitelist();
                            }
                        }
                    }
                });

                if let Some(root) = page.root() {
                    dialog.present(Some(&root));
                }
            }
        });
        action_group.add_action(&clear_action);

        // Действие: Импорт
        let import_action = gio::SimpleAction::new("import", None);
        let page_weak = self.downgrade();
        import_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak.upgrade() {
                let dialog = gtk::FileDialog::builder()
                    .title(gettext("Import Whitelist"))
                    .build();
                
                let p_weak = page.downgrade();
                if let Some(window) = page.root().and_downcast::<gtk::Window>() {
                    dialog.open(Some(&window), gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                if let Ok(content) = std::fs::read_to_string(path) {
                                    if let Some(p) = p_weak.upgrade() {
                                        if let Some(model) = p.imp().model.borrow().as_ref() {
                                            for line in content.lines() {
                                                let t = line.trim();
                                                if !t.is_empty() && !t.starts_with('#') {
                                                    model.append(&DomainObject::new(t));
                                                }
                                            }
                                            p.save_whitelist();
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
        });
        action_group.add_action(&import_action);

        // Действие: Экспорт
        let export_action = gio::SimpleAction::new("export", None);
        let page_weak = self.downgrade();
        export_action.connect_activate(move |_, _| {
            if let Some(page) = page_weak.upgrade() {
                let dialog = gtk::FileDialog::builder()
                    .title(gettext("Export Whitelist"))
                    .initial_name("whitelist.txt")
                    .build();
                
                let p_weak = page.downgrade();
                if let Some(window) = page.root().and_downcast::<gtk::Window>() {
                    dialog.save(Some(&window), gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                if let Some(p) = p_weak.upgrade() {
                                    if let Some(model) = p.imp().model.borrow().as_ref() {
                                        let mut lines = String::new();
                                        for i in 0..model.n_items() {
                                            if let Some(obj) = model.item(i).and_then(|o| o.downcast::<DomainObject>().ok()) {
                                                lines.push_str(&obj.domain());
                                                lines.push('\n');
                                            }
                                        }
                                        let _ = std::fs::write(path, lines);
                                    }
                                }
                            }
                        }
                    });
                }
            }
        });
        action_group.add_action(&export_action);

        self.insert_action_group("whitelist", Some(&action_group));
    }

    fn show_domain_dialog(&self, target_obj: Option<DomainObject>) {
        let is_editing = target_obj.is_some();
        let title = if is_editing { gettext("Edit Domain") } else { gettext("New Domain") };
        let button_label = if is_editing { gettext("Save") } else { gettext("Add") };
        let initial_text = target_obj.as_ref().map(|o| o.domain()).unwrap_or_default();

        // Поле ввода
        let entry_row = adw::EntryRow::builder()
            .title(gettext("Domain"))
            .text(&initial_text)
            .show_apply_button(false)
            .build();

        let group = adw::PreferencesGroup::builder()
            .build();
        group.add(&entry_row);

        let content_area = adw::PreferencesPage::builder()
            .build();
        content_area.add(&group);

        // Создаем диалог
        let dialog = adw::AlertDialog::builder()
            .heading(&title)
            .body(gettext("Enter the domain address or rule (e.g. google.com, domain:vk.com, *.ru)"))
            .extra_child(&content_area)
            .build();

        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("apply", &button_label);
        dialog.set_default_response(Some("apply"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);

        // Фокус при открытии
        entry_row.grab_focus();

        let page_weak = self.downgrade();
        let entry_row_clone = entry_row.clone();
        let target_obj_clone = target_obj.clone();
        
        let dialog_weak = dialog.downgrade();
        entry_row.connect_apply(move |_| {
            if let Some(_d) = dialog_weak.upgrade() {
                // AdwAlertDialog doesn't have a simple way to trigger a response programmatically
                // but we can just call present() and the default response should work if we set it correctly.
                // However, since we want to trigger "apply", we'll just close it.
                // Actually, the best way is to set it as default and just let the entry pass the key.
            }
        });

        dialog.connect_response(None, move |_, response: &str| {
            let page = match page_weak.upgrade() {
                Some(p) => p,
                None => return,
            };

            if response == "apply" {
                let text = entry_row_clone.text().trim().to_string();
                if !text.is_empty() {
                    let imp = page.imp();
                    let model_borrow = imp.model.borrow();

                    if let Some(model) = model_borrow.as_ref() {
                        // Проверка на дубликаты
                        let mut exists = false;
                        for i in 0..model.n_items() {
                            if let Some(obj) = model.item(i).and_then(|o| o.downcast::<DomainObject>().ok()) {
                                if obj.domain() == text && target_obj_clone.as_ref().is_none_or(|to| to.domain() != text) {
                                    exists = true;
                                    break;
                                }
                            }
                        }

                        if !exists {
                            if let Some(existing_obj) = target_obj_clone.as_ref() {
                                existing_obj.set_domain(text.as_str());
                            } else {
                                model.append(&DomainObject::new(text.as_str()));
                            }
                            page.save_whitelist();
                        }
                    }
                }
            }
        });

        if let Some(root) = self.root() {
            dialog.present(Some(&root));
            // Повторный grab_focus после презентации
            entry_row.grab_focus();
        }
    }
}

