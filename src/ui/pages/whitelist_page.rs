use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use crate::ui::setup_primary_menu;
use crate::settings::{SettingsManager, RoutingRule};
use crate::ui::models::RoutingRuleObject;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/pages/whitelist_page.ui")]
    pub struct VrxxWhitelistPage {
        #[template_child]
        pub btn_apply: TemplateChild<gtk::Button>,
        #[template_child]
        pub primary_menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub enable_routing_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub mode_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub btn_add_rule: TemplateChild<gtk::Button>,
        #[template_child]
        pub rules_list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub route_ru_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_cn_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_ir_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub route_antifilter_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub disable_ipv6_row: TemplateChild<adw::SwitchRow>,

        pub model: RefCell<Option<gio::ListStore>>,
        pub has_changes: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxWhitelistPage {
        const NAME: &'static str = "VrxxWhitelistPage";
        type Type = super::VrxxWhitelistPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            adw::ComboRow::static_type();
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
            
            let store = gio::ListStore::new::<RoutingRuleObject>();
            self.model.replace(Some(store.clone()));
            
            self.obj().setup_settings();
            self.obj().setup_rules_list();
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

    fn mark_changed(&self) {
        let imp = self.imp();
        *imp.has_changes.borrow_mut() = true;
        imp.btn_apply.set_visible(true);
    }

    fn setup_rules_list(&self) {
        let imp = self.imp();
        let store = imp.model.borrow().clone().unwrap();
        
        let settings = SettingsManager::new().load();
        for rule in &settings.routing_rules {
            let obj = RoutingRuleObject::new(&rule.name, &rule.type_, &rule.value, &rule.action);
            store.append(&obj);
        }

        let selection_model = gtk::NoSelection::new(Some(store));
        
        let page = self.clone();
        imp.rules_list.bind_model(Some(&selection_model), move |item| {
            let obj = item.downcast_ref::<RoutingRuleObject>().unwrap();
            let row = adw::ActionRow::builder()
                .title(&obj.name())
                .subtitle(&format!("{} | {} -> {}", obj.rule_type(), obj.value(), obj.action()))
                .build();
            
            let btn_remove = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "destructive-action"])
                .build();
                
            let obj_clone = obj.clone();
            let page_clone = page.clone();
            btn_remove.connect_clicked(move |_| {
                let store = page_clone.imp().model.borrow().clone().unwrap();
                for i in 0..store.n_items() {
                    if let Some(o) = store.item(i).and_downcast::<RoutingRuleObject>() {
                        if o.name() == obj_clone.name() {
                            store.remove(i);
                            page_clone.mark_changed();
                            break;
                        }
                    }
                }
            });
            
            row.add_suffix(&btn_remove);
            row.upcast::<gtk::Widget>()
        });

        imp.btn_add_rule.connect_clicked(glib::clone!(
            #[weak(rename_to = page)] self,
            move |_| {
                page.show_add_rule_dialog();
            }
        ));
    }

    fn show_add_rule_dialog(&self) {
        let window = self.root().and_downcast::<gtk::Window>().unwrap();
        
        let dialog = adw::AlertDialog::builder()
            .heading("Add Routing Rule")
            .body("Create a new custom routing rule")
            .build();
            
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("add", "Add");
        dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
        
        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .build();
            
        let entry_name = gtk::Entry::builder().placeholder_text("Rule Name (e.g. Work)").build();
        
        let combo_type = gtk::DropDown::from_strings(&["domain", "ip", "srs_url"]);
        let combo_action = gtk::DropDown::from_strings(&["proxy", "direct", "block"]);
        
        let entry_val = gtk::Entry::builder().placeholder_text("Value (e.g. google.com or .srs URL)").build();
        
        vbox.append(&gtk::Label::builder().label("Name:").xalign(0.0).build());
        vbox.append(&entry_name);
        vbox.append(&gtk::Label::builder().label("Type:").xalign(0.0).build());
        vbox.append(&combo_type);
        vbox.append(&gtk::Label::builder().label("Value:").xalign(0.0).build());
        vbox.append(&entry_val);
        vbox.append(&gtk::Label::builder().label("Action:").xalign(0.0).build());
        vbox.append(&combo_action);
        
        dialog.set_extra_child(Some(&vbox));
        
        let page = self.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let response = dialog.choose_future(Some(&window)).await;
            if response == "add" {
                let name = entry_name.text().to_string();
                let val = entry_val.text().to_string();
                if name.is_empty() || val.is_empty() { return; }
                
                let r_type = match combo_type.selected() {
                    1 => "ip",
                    2 => "srs_url",
                    _ => "domain",
                };
                let act = match combo_action.selected() {
                    1 => "direct",
                    2 => "block",
                    _ => "proxy",
                };
                
                let obj = RoutingRuleObject::new(&name, r_type, &val, act);
                page.imp().model.borrow().clone().unwrap().append(&obj);
                page.mark_changed();
            }
        });
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let manager = SettingsManager::new();
        let settings = manager.load();

        imp.btn_apply.set_visible(false);

        imp.btn_apply.connect_clicked(glib::clone!(
            #[weak(rename_to = page)] self,
            move |btn| {
                let manager = SettingsManager::new();
                let mut s = manager.load();
                
                let imp = page.imp();
                s.enable_routing = imp.enable_routing_row.is_active();
                s.routing_mode = match imp.mode_row.selected() {
                    1 => "proxy".to_string(),
                    _ => "bypass".to_string(),
                };
                s.route_ru = imp.route_ru_row.is_active();
                s.route_cn = imp.route_cn_row.is_active();
                s.route_ir = imp.route_ir_row.is_active();
                s.route_antifilter = imp.route_antifilter_row.is_active();
                s.disable_ipv6 = imp.disable_ipv6_row.is_active();
                
                let mut rules = vec![];
                let store = imp.model.borrow().clone().unwrap();
                for i in 0..store.n_items() {
                    if let Some(obj) = store.item(i).and_downcast::<RoutingRuleObject>() {
                        rules.push(RoutingRule {
                            name: obj.name(),
                            type_: obj.rule_type(),
                            value: obj.value(),
                            action: obj.action(),
                        });
                    }
                }
                s.routing_rules = rules;
                
                manager.save(&s);
                
                *imp.has_changes.borrow_mut() = false;
                btn.set_visible(false);

                let _ = crate::settings::core_restart_channel().0.send_blocking(());
                if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>() {
                    let notification = gtk::gio::Notification::new(&gettextrs::gettext("Settings applied"));
                    notification.set_body(Some(&gettextrs::gettext("Core was restarted to apply new settings.")));
                    app.send_notification(Some("settings_applied"), &notification);
                }
            }
        ));

        imp.enable_routing_row.set_active(settings.enable_routing);
        
        let mode_idx = match settings.routing_mode.as_str() {
            "proxy" => 1,
            _ => 0,
        };
        imp.mode_row.set_selected(mode_idx);

        imp.route_ru_row.set_active(settings.route_ru);
        imp.route_cn_row.set_active(settings.route_cn);
        imp.route_ir_row.set_active(settings.route_ir);
        imp.route_antifilter_row.set_active(settings.route_antifilter);
        imp.disable_ipv6_row.set_active(settings.disable_ipv6);

        imp.enable_routing_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.mode_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.route_ru_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.route_cn_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.route_ir_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.route_antifilter_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
        imp.disable_ipv6_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)] self, move |_| page.mark_changed()
        ));
    }
}
