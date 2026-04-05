use adw::subclass::prelude::*;
use adw::prelude::*;
use gtk::glib;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
            buffer.create_tag(Some("app"), &[("foreground", &"#3584e4"), ("weight", &700)]); // GNOME blue

            obj.setup_callbacks();
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
                let log_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx").join("logs");
                let filter_index = window.imp().dropdown_filter.selected();
                let file_name = match filter_index {
                    1 => "core.log",
                    2 => "app.log",
                    3 => "access.log",
                    _ => "all.log",
                };
                let log_path = log_dir.join(file_name);
                let _ = std::fs::write(log_path, ""); // Очищаем файл
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
        self.imp().dropdown_filter.connect_selected_notify(move |_| {
            if let Some(window) = window_weak_filter.upgrade() {
                // При смене фильтра очищаем экран и читаем всё заново
                window.imp().text_view.buffer().set_text("");
                *window.imp().last_pos.borrow_mut() = 0;
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
            match zbus::Connection::system().await {
                Ok(conn) => {
                    match crate::ipc::DaemonProxy::new(&conn).await {
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
                        Err(e) => tracing::error!("Failed to create DaemonProxy for logs: {}", e),
                    }
                }
                Err(e) => tracing::error!("Failed to connect to D-Bus System Bus for logs: {}", e),
            }
        });
    }

    fn append_log(&self, level: &str, message: &str) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mut iter = buffer.end_iter();
        
        let tag_name = match level {
            "error" => Some("error"),
            "warning" => Some("warning"),
            "debug" => Some("debug"),
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
