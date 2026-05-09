use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use std::path::PathBuf;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/ru/mark/vrxx/ui/components/log_window.ui")]
    pub struct VrxxLogWindow {
        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub btn_autoscroll: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub dropdown_filter: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub zoom_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        pub last_pos: RefCell<u64>,
        pub font_size: RefCell<i32>,
        pub scroll_accum: RefCell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxLogWindow {
        const NAME: &'static str = "VrxxLogWindow";
        type Type = super::VrxxLogWindow;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VrxxLogWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.imp().text_view.add_css_class("log-view");
            *obj.imp().font_size.borrow_mut() = 10;
            *obj.imp().scroll_accum.borrow_mut() = 0.0;

            let strings = gtk::StringList::new(&[
                gettextrs::gettext("All logs").as_str(),
                gettextrs::gettext("Core logs").as_str(),
                gettextrs::gettext("Application logs").as_str(),
                gettextrs::gettext("Access logs").as_str(),
            ]);
            self.dropdown_filter.set_model(Some(&strings));

            let buffer = self.text_view.buffer();
            buffer.create_tag(Some("error"), &[("foreground", &"red"), ("weight", &700)]);
            buffer.create_tag(Some("warning"), &[("foreground", &"orange")]);
            buffer.create_tag(Some("debug"), &[("foreground", &"gray")]);
            buffer.create_tag(Some("info"), &[("foreground", &"green")]);
            buffer.create_tag(Some("app"), &[("foreground", &"#3584e4"), ("weight", &700)]); // GNOME blue
            buffer.create_tag(Some("hidden"), &[("invisible", &true)]);

            obj.setup_actions();
            obj.setup_callbacks();
            obj.setup_event_controllers();
            obj.setup_shortcuts();
            obj.update_font_size(); // Применяем начальные CSS-стили (monospace, padding)
            obj.load_history();
            obj.load_logs_from_file();
            obj.setup_daemon_logs();
        }
    }
    impl WidgetImpl for VrxxLogWindow {}
    impl WindowImpl for VrxxLogWindow {}
    impl AdwWindowImpl for VrxxLogWindow {}
}

glib::wrapper! {
    pub struct VrxxLogWindow(ObjectSubclass<imp::VrxxLogWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                   gtk::Native, gtk::Root, gtk::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

impl Default for VrxxLogWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl VrxxLogWindow {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        self.insert_action_group("win", Some(&action_group));

        // Action: Copy Logs
        let copy_action = gio::SimpleAction::new("copy_logs", None);
        let window_weak = self.downgrade();
        copy_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                let buffer = window.imp().text_view.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false);
                WidgetExt::display(&window).clipboard().set_text(&text);
            }
        });
        action_group.add_action(&copy_action);

        // Action: Clear Logs
        let clear_action = gio::SimpleAction::new("clear_logs", None);
        let window_weak = self.downgrade();
        clear_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                let log_dir = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("vrxx")
                    .join("logs");
                let filter_index = window.imp().dropdown_filter.selected();
                let file_name = match filter_index {
                    1 => "core.log",
                    2 => "app.log",
                    3 => "access.log",
                    _ => "all.log",
                };
                let log_path = log_dir.join(file_name);

                #[cfg(unix)]
                {
                    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                    let mut opts = std::fs::OpenOptions::new();
                    opts.create(true).write(true).truncate(true).mode(0o600);
                    if let Ok(file) = opts.open(&log_path) {
                        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = std::fs::write(log_path, "");
                }

                window.imp().text_view.buffer().set_text("");
                *window.imp().last_pos.borrow_mut() = 0;
            }
        });
        action_group.add_action(&clear_action);

        // Action: Zoom In
        let zoom_in_action = gio::SimpleAction::new("zoom_in", None);
        let window_weak = self.downgrade();
        zoom_in_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                let mut size = *window.imp().font_size.borrow();
                if size < 32 {
                    size += 1;
                    *window.imp().font_size.borrow_mut() = size;
                    window.imp().zoom_label.set_text(&format!("{}0%", size));
                    window.update_font_size();
                }
            }
        });
        action_group.add_action(&zoom_in_action);

        // Action: Zoom Out
        let zoom_out_action = gio::SimpleAction::new("zoom_out", None);
        let window_weak = self.downgrade();
        zoom_out_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                let mut size = *window.imp().font_size.borrow();
                if size > 6 {
                    size -= 1;
                    *window.imp().font_size.borrow_mut() = size;
                    window.imp().zoom_label.set_text(&format!("{}0%", size));
                    window.update_font_size();
                }
            }
        });
        action_group.add_action(&zoom_out_action);

        // Action: Zoom Normal (Reset)
        let zoom_normal_action = gio::SimpleAction::new("zoom_normal", None);
        let window_weak = self.downgrade();
        zoom_normal_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                *window.imp().font_size.borrow_mut() = 10;
                window.imp().zoom_label.set_text("100%");
                window.update_font_size();
            }
        });
        action_group.add_action(&zoom_normal_action);

        // Action: Export Logs (Save As)
        let export_action = gio::SimpleAction::new("export_logs", None);
        let window_weak = self.downgrade();
        export_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.export_logs();
            }
        });
        action_group.add_action(&export_action);
    }

    fn setup_shortcuts(&self) {
        let controller = gtk::ShortcutController::new();
        controller.set_scope(gtk::ShortcutScope::Managed);

        // Ctrl+F: Toggle Search
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>f");
        let action = gtk::CallbackAction::new(|widget, _| {
            if let Some(window) = widget.downcast_ref::<Self>() {
                let active = window.imp().search_bar.is_search_mode();
                window.imp().search_bar.set_search_mode(!active);
                if !active {
                    window.imp().search_entry.grab_focus();
                }
            }
            glib::Propagation::Stop
        });
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+S: Export
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>s");
        let action = gtk::NamedAction::new("win.export_logs");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+Plus: Zoom In
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>equal");
        let action = gtk::NamedAction::new("win.zoom_in");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        let trigger = gtk::ShortcutTrigger::parse_string("<Control>plus");
        let action = gtk::NamedAction::new("win.zoom_in");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+Minus: Zoom Out
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>minus");
        let action = gtk::NamedAction::new("win.zoom_out");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+0: Zoom Normal
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>0");
        let action = gtk::NamedAction::new("win.zoom_normal");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        self.add_controller(controller);
    }

    // Внутри impl VrxxLogWindow
    fn setup_event_controllers(&self) {
        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let window_weak = self.downgrade();

        scroll_controller.connect_scroll(move |controller, _dx, dy| {
            let state = controller.current_event_state();
            if state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                if let Some(window) = window_weak.upgrade() {
                    let mut accum = window.imp().scroll_accum.borrow_mut();
                    *accum += dy;

                    // Порог 0.8 для плавности на тачпадах
                    if *accum > 0.8 {
                        let _ = WidgetExt::activate_action(&window, "win.zoom_out", None);
                        *accum = 0.0;
                    } else if *accum < -0.8 {
                        let _ = WidgetExt::activate_action(&window, "win.zoom_in", None);
                        *accum = 0.0;
                    }
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });

        self.imp().text_view.add_controller(scroll_controller);
    }

    fn update_font_size(&self) {
        let size = *self.imp().font_size.borrow();
        let css = format!(
            ".log-view {{ font-family: monospace; padding: 12px; font-size: {}pt; }}",
            size
        );
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        gtk::style_context_add_provider_for_display(
            &WidgetExt::display(self),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn setup_callbacks(&self) {
        // --- Раздел: Смена фильтра ---
        let window_weak_filter = self.downgrade();
        self.imp()
            .dropdown_filter
            .connect_selected_notify(move |_| {
                if let Some(window) = window_weak_filter.upgrade() {
                    window.imp().text_view.buffer().set_text("");
                    *window.imp().last_pos.borrow_mut() = 0;
                    window.load_logs_from_file();
                }
            });

        // --- Раздел: Поиск ---
        let window_weak_search = self.downgrade();
        self.imp().search_entry.connect_search_changed(move |_| {
            if let Some(window) = window_weak_search.upgrade() {
                window.apply_search_filter();
            }
        });

        // Авто-прокрутка при изменении состояния
        let window_weak_scroll = self.downgrade();
        self.imp().btn_autoscroll.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(window) = window_weak_scroll.upgrade() {
                    window.scroll_to_bottom();
                }
            }
        });
    }

    fn scroll_to_bottom(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mark = buffer.create_mark(None, &buffer.end_iter(), false);
        imp.text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
        buffer.delete_mark(&mark);
    }

    fn setup_daemon_logs(&self) {
        let window_weak = self.downgrade();
        glib::spawn_future_local(async move {
            let client = crate::ipc::DaemonClient::new();
            let logs = client.subscribe_events();

            loop {
                match logs.recv().await {
                    Ok(event) => {
                        if let crate::daemon::DaemonEvent::Log { level, message } = event {
                            if let Some(window) = window_weak.upgrade() {
                                window.append_log(&level, &message);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Log stream error: {}. Attempting to reconnect...", e);
                        // The subscribe_events itself has a loop and reconnection logic for SSE,
                        // but if the channel closes, we might need to re-subscribe.
                        // However, DaemonClient::subscribe_events returns a receiver that it keeps
                        // sending to in its own internal loop. If recv() fails, the sender was dropped.
                        break;
                    }
                }
            }
        });
    }

    fn load_history(&self) {
        let client = crate::ipc::DaemonClient::new();
        let window_weak = self.downgrade();

        glib::spawn_future_local(async move {
            if let Ok(history) = client.get_history().await {
                if let Some(window) = window_weak.upgrade() {
                    for event in history {
                        if let crate::daemon::events::DaemonEvent::Log { level, message } = event {
                            window.append_log(&level, &message);
                        }
                    }
                }
            }
        });
    }

    fn load_logs_from_file(&self) {
        let log_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vrxx")
            .join("logs");
        let filter_index = self.imp().dropdown_filter.selected();
        let file_name = match filter_index {
            1 => "core.log",
            2 => "app.log",
            3 => "access.log",
            _ => "all.log",
        };
        let log_path = log_dir.join(file_name);

        if let Ok(mut file) = std::fs::File::open(log_path) {
            let metadata = file.metadata().unwrap();
            let file_size = metadata.len();
            let read_size = 128 * 1024; // Читаем последние 128 КБ

            let start_pos = file_size.saturating_sub(read_size);
            let mut buffer = Vec::new();

            use std::io::{Read, Seek, SeekFrom};
            if file.seek(SeekFrom::Start(start_pos)).is_ok() {
                let _ = file.read_to_end(&mut buffer);
                let content = String::from_utf8_lossy(&buffer);

                let lines: Vec<&str> = content.lines().collect();
                let start_idx = if start_pos > 0 { 1 } else { 0 };

                for line in &lines[start_idx..] {
                    let level = if line.contains("ERROR") || line.contains("error") {
                        "error"
                    } else if line.contains("WARN") || line.contains("warning") {
                        "warning"
                    } else if line.contains("DEBUG") || line.contains("debug") {
                        "debug"
                    } else if line.contains("INFO") || line.contains("info") {
                        "info"
                    } else if line.contains("[Vrxx]") {
                        "app"
                    } else {
                        "info"
                    };

                    self.append_log(level, line);
                }
            }
        }
    }

    pub fn append_log(&self, level: &str, message: &str) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();

        // Ограничиваем количество строк в буфере (макс. 5000) для экономии памяти
        let line_count = buffer.line_count();
        if line_count > 5000 {
            let mut start = buffer.start_iter();
            let mut end = buffer.start_iter();
            end.forward_lines(line_count - 5000);
            buffer.delete(&mut start, &mut end);
        }

        // --- Раздел: Фильтрация логов ---
        let filter_index = imp.dropdown_filter.selected();
        let is_app_log = level == "app" || message.contains("[Vrxx]") || message.contains("vrxx::");
        let is_access_log =
            message.contains("accepted") || message.contains("proxying") || message.contains("->");
        let is_core_log = !is_app_log && !is_access_log;

        match filter_index {
            1 if !is_core_log => return,
            2 if !is_app_log => return,
            3 if !is_access_log => return,
            _ => {}
        }

        let mut iter = buffer.end_iter();

        let tag_name = match level {
            "error" => Some("error"),
            "warning" => Some("warning"),
            "debug" => Some("debug"),
            "info" => Some("info"),
            "app" => Some("app"),
            _ => None,
        };

        let mut line = message.to_string();
        line.push('\n');

        let start_offset = buffer.end_iter().offset();
        if let Some(tag) = tag_name {
            buffer.insert_with_tags_by_name(&mut iter, &line, &[tag]);
        } else {
            buffer.insert(&mut iter, &line);
        }
        let end_iter = buffer.end_iter();

        // Применяем текущий поиск к новой строке
        let query = imp.search_entry.text().to_lowercase();
        if !query.is_empty() && !line.to_lowercase().contains(&query) {
            let start_iter = buffer.iter_at_offset(start_offset);
            buffer.apply_tag_by_name("hidden", &start_iter, &end_iter);
        }

        if imp.btn_autoscroll.is_active() {
            let window_weak = self.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.scroll_to_bottom();
                }
            });
        }
    }

    fn apply_search_filter(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let query = imp.search_entry.text().to_lowercase();

        let (start, end) = buffer.bounds();
        buffer.remove_tag_by_name("hidden", &start, &end);

        if query.is_empty() {
            return;
        }

        let mut iter = buffer.start_iter();
        while !iter.is_end() {
            let mut line_end = iter;
            if !line_end.forward_line() {
                line_end = buffer.end_iter();
            }

            let line_text = buffer.text(&iter, &line_end, true).to_lowercase();
            if !line_text.contains(&query) {
                buffer.apply_tag_by_name("hidden", &iter, &line_end);
            }
            iter = line_end;
        }
    }

    fn export_logs(&self) {
        let dialog = gtk::FileDialog::new();
        dialog.set_title(&gettextrs::gettext("Save Logs"));
        dialog.set_initial_name(Some("vrxx-logs.txt"));

        let buffer = self.imp().text_view.buffer();
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false).to_string();

        let window_weak = self.downgrade();
        dialog.save(Some(self), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(_window) = window_weak.upgrade() {
                    let text_bytes = text.as_bytes().to_vec();
                    glib::spawn_future_local(async move {
                        match file
                            .replace_contents_future(
                                text_bytes,
                                None,
                                false,
                                gio::FileCreateFlags::REPLACE_DESTINATION,
                            )
                            .await
                        {
                            Ok(_) => tracing::info!("Logs exported to {}", file.uri()),
                            Err(e) => tracing::error!("Failed to export logs: {}", e.1),
                        }
                    });
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires main thread for GTK initialization"]
    fn test_log_window_append() {
        let _ = gtk::init();

        // Load resources for templates
        let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
        if let Ok(res) = gtk::gio::Resource::from_data(&glib::Bytes::from(res_data)) {
            gtk::gio::resources_register(&res);
        }

        let log_window = VrxxLogWindow::new();
        let buffer = log_window.imp().text_view.buffer();

        log_window.append_log("error", "Test error message");
        log_window.append_log("app", "Test app message");

        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false);

        assert!(text.contains("Test error message"));
        assert!(text.contains("Test app message"));
    }

    #[test]
    #[ignore = "Requires main thread for GTK initialization"]
    fn test_log_window_search_filter() {
        let _ = gtk::init();

        // Load resources for templates
        let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
        if let Ok(res) = gtk::gio::Resource::from_data(&glib::Bytes::from(res_data)) {
            gtk::gio::resources_register(&res);
        }

        let log_window = VrxxLogWindow::new();
        let buffer = log_window.imp().text_view.buffer();
        buffer.set_text(""); // Clear any automatic logs (e.g. SSE errors)
        let hidden_tag = buffer.tag_table().lookup("hidden").unwrap();

        log_window.append_log("info", "Visible message");
        log_window.append_log("info", "Hidden message");

        // 1. Initial state (nothing hidden)
        let mut iter = buffer.start_iter();
        assert!(!iter.has_tag(&hidden_tag));
        iter.forward_line();
        assert!(!iter.has_tag(&hidden_tag));

        // 2. Search for "visible"
        log_window.imp().search_entry.set_text("visible");
        log_window.apply_search_filter();

        let mut iter = buffer.start_iter();
        assert!(!iter.has_tag(&hidden_tag)); // "Visible message" contains "visible"
        iter.forward_line();
        assert!(iter.has_tag(&hidden_tag)); // "Hidden message" doesn't

        // 3. Clear search
        log_window.imp().search_entry.set_text("");
        log_window.apply_search_filter();

        let mut iter = buffer.start_iter();
        assert!(!iter.has_tag(&hidden_tag));
        iter.forward_line();
        assert!(!iter.has_tag(&hidden_tag));

        // 4. Test filtering on append
        log_window.imp().search_entry.set_text("new");
        log_window.append_log("info", "old message");
        log_window.append_log("info", "new message");

        let mut iter = buffer.end_iter();
        iter.backward_line();
        assert!(!iter.has_tag(&hidden_tag)); // "new message" matches
        iter.backward_line();
        assert!(iter.has_tag(&hidden_tag)); // "old message" doesn't
    }
}
