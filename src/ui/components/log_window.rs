use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::path::PathBuf;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(string = "
    <interface>
      <template class='VrxxLogWindow' parent='AdwWindow'>
        <property name='title' translatable='yes'>System Logs</property>
        <property name='default-width'>800</property>
        <property name='default-height'>500</property>
        <property name='content'>
          <object class='AdwToolbarView'>
            <child type='top'>
              <object class='AdwHeaderBar'>
                <property name='title-widget'>
                  <object class='AdwWindowTitle'>
                    <property name='title' translatable='yes'>System Logs</property>
                  </object>
                </property>
                <child type='start'>
                  <object class='GtkToggleButton' id='btn_autoscroll'>
                    <property name='icon-name'>go-down-symbolic</property>
                    <property name='tooltip-text' translatable='yes'>Auto-scroll</property>
                    <property name='active'>True</property>
                  </object>
                </child>
                <child type='start'>
                  <object class='GtkDropDown' id='dropdown_filter'>
                  </object>
                </child>
                <child type='end'>
                  <object class='GtkMenuButton' id='menu_btn'>
                    <property name='icon-name'>open-menu-symbolic</property>
                    <property name='tooltip-text' translatable='yes'>Menu</property>
                    <property name='popover'>
                      <object class='GtkPopover'>
                        <child>
                          <object class='GtkBox'>
                            <property name='orientation'>vertical</property>
                            <property name='spacing'>6</property>
                            <property name='margin-start'>6</property>
                            <property name='margin-end'>6</property>
                            <property name='margin-top'>6</property>
                            <property name='margin-bottom'>6</property>
                            
                            <child>
                              <object class='GtkBox'>
                                <property name='orientation'>horizontal</property>
                                <property name='spacing'>6</property>
                                <child>
                                  <object class='GtkButton' id='btn_zoom_out'>
                                    <property name='icon-name'>zoom-out-symbolic</property>
                                    <property name='tooltip-text' translatable='yes'>Zoom Out</property>
                                  </object>
                                </child>
                                <child>
                                  <object class='GtkLabel' id='lbl_zoom_percent'>
                                    <property name='label' translatable='yes'>100%</property>
                                    <property name='hexpand'>True</property>
                                  </object>
                                </child>
                                <child>
                                  <object class='GtkButton' id='btn_zoom_in'>
                                    <property name='icon-name'>zoom-in-symbolic</property>
                                    <property name='tooltip-text' translatable='yes'>Zoom In</property>
                                  </object>
                                </child>
                              </object>
                            </child>
                            
                            <child>
                              <object class='GtkSeparator'/>
                            </child>

                            <child>
                              <object class='GtkButton' id='btn_copy'>
                                <property name='label' translatable='yes'>Copy logs</property>
                              </object>
                            </child>
                            <child>
                              <object class='GtkButton' id='btn_clear'>
                                <property name='label' translatable='yes'>Clear logs</property>
                                <style>
                                  <class name='destructive-action'/>
                                </style>
                              </object>
                            </child>
                            
                          </object>
                        </child>
                      </object>
                    </property>
                  </object>
                </child>
              </object>
            </child>
            <property name='content'>
              <object class='GtkScrolledWindow' id='scrolled_window'>
                <property name='child'>
                  <object class='GtkTextView' id='text_view'>
                    <property name='editable'>False</property>
                    <property name='monospace'>True</property>
                    <property name='left-margin'>12</property>
                    <property name='right-margin'>12</property>
                    <property name='top-margin'>12</property>
                    <property name='bottom-margin'>12</property>
                    <style>
                      <class name='view'/>
                    </style>
                  </object>
                </property>
              </object>
            </property>
          </object>
        </property>
      </template>
    </interface>
    ")]
    pub struct VrxxLogWindow {
        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub btn_copy: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_clear: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_autoscroll: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub dropdown_filter: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub btn_zoom_in: TemplateChild<gtk::Button>,
        #[template_child]
        pub btn_zoom_out: TemplateChild<gtk::Button>,
        #[template_child]
        pub lbl_zoom_percent: TemplateChild<gtk::Label>,
        pub last_pos: RefCell<u64>,
        pub font_size: RefCell<i32>,
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

            obj.setup_callbacks();
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
                   gtk::Native, gtk::Root, gtk::ShortcutManager;
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

    fn setup_callbacks(&self) {
        *self.imp().font_size.borrow_mut() = 12;

        let window_weak = self.downgrade();

        self.imp().btn_copy.connect_clicked(move |_| {
            if let Some(window) = window_weak.upgrade() {
                let buffer = window.imp().text_view.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false);
                window.clipboard().set_text(&text);
            }
        });

        let window_weak_clear = self.downgrade();
        self.imp().btn_clear.connect_clicked(move |_| {
            if let Some(window) = window_weak_clear.upgrade() {
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
                    let _ = std::fs::write(log_path, ""); // Очищаем файл
                }

                window.imp().text_view.buffer().set_text("");
                *window.imp().last_pos.borrow_mut() = 0;
            }
        });

        let window_weak_in = self.downgrade();
        self.imp().btn_zoom_in.connect_clicked(move |_| {
            if let Some(window) = window_weak_in.upgrade() {
                let imp = window.imp();
                *imp.font_size.borrow_mut() += 2;
                let size = *imp.font_size.borrow();
                let percent = (size as f32 / 12.0 * 100.0) as i32;
                imp.lbl_zoom_percent.set_label(&format!("{percent}%"));

                let provider = gtk::CssProvider::new();
                provider.load_from_string(&format!("textview {{ font-size: {size}pt; }}"));
                if let Some(display) = gdk::Display::default() {
                    gtk::style_context_add_provider_for_display(
                        &display,
                        &provider,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
            }
        });

        let window_weak_out = self.downgrade();
        self.imp().btn_zoom_out.connect_clicked(move |_| {
            if let Some(window) = window_weak_out.upgrade() {
                let imp = window.imp();
                let mut size = *imp.font_size.borrow();
                if size > 6 {
                    size -= 2;
                    *imp.font_size.borrow_mut() = size;
                    let percent = (size as f32 / 12.0 * 100.0) as i32;
                    imp.lbl_zoom_percent.set_label(&format!("{percent}%"));

                    let provider = gtk::CssProvider::new();
                    provider.load_from_string(&format!("textview {{ font-size: {size}pt; }}"));
                    if let Some(display) = gdk::Display::default() {
                        gtk::style_context_add_provider_for_display(
                            &display,
                            &provider,
                            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                        );
                    }
                }
            }
        });

        let window_weak_filter = self.downgrade();
        self.imp()
            .dropdown_filter
            .connect_selected_notify(move |_| {
                if let Some(window) = window_weak_filter.upgrade() {
                    // --- Раздел: Смена фильтра ---
                    // REVIEW: При смене фильтра очищаем экран и читаем всё заново из соответствующего файла
                    window.imp().text_view.buffer().set_text("");
                    *window.imp().last_pos.borrow_mut() = 0;
                    window.load_logs_from_file();
                }
            });

        let window_weak_scroll = self.downgrade();
        self.imp().btn_autoscroll.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(window) = window_weak_scroll.upgrade() {
                    let imp = window.imp();
                    let buffer = imp.text_view.buffer();
                    let mark = buffer.create_mark(None, &buffer.end_iter(), false);
                    imp.text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
                    buffer.delete_mark(&mark);
                }
            }
        });
    }

    fn setup_daemon_logs(&self) {
        let window_weak = self.downgrade();
        glib::spawn_future_local(async move {
            match crate::ipc::get_daemon_proxy().await {
                Ok(proxy) => {
                    use futures_util::StreamExt;
                    let mut logs = match proxy.receive_log_message().await {
                        Ok(stream) => stream,
                        Err(e) => {
                            tracing::error!("Failed to receive log messages: {}", e);
                            return;
                        }
                    };

                    while let Some(signal) = logs.next().await {
                        if let Ok(args) = signal.args() {
                            if let Some(window) = window_weak.upgrade() {
                                window.append_log(args.level(), args.message());
                            }
                        }
                    }
                }
                Err(e) => tracing::error!("Failed to get DaemonProxy for logs: {}", e),
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

        // OPTIMIZE: Не читаем весь файл целиком (может быть огромным),
        // а берем только последние N килобайт
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
                // Пропускаем первую (возможно неполную) строку, если мы читали не с начала
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

                    // REVIEW: При загрузке из файла мы также применяем фильтрацию
                    self.append_log(level, line);
                }
            }
        }
    }

    pub fn append_log(&self, level: &str, message: &str) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();

        // --- Раздел: Фильтрация логов ---
        // XXX: Мы фильтруем логи в зависимости от выбранного раздела в DropDown
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
        // ================================

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

        if let Some(tag) = tag_name {
            buffer.insert_with_tags_by_name(&mut iter, &line, &[tag]);
        } else {
            buffer.insert(&mut iter, &line);
        }

        if imp.btn_autoscroll.is_active() {
            let mark = buffer.create_mark(None, &buffer.end_iter(), false);
            imp.text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
            buffer.delete_mark(&mark);
        }
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
}
