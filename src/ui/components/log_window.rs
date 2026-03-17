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
        <property name='title' translatable='yes'>Системные логи</property>
        <property name='default-width'>800</property>
        <property name='default-height'>500</property>
        <property name='content'>
          <object class='AdwToolbarView'>
            <child type='top'>
              <object class='AdwHeaderBar'>
                <property name='title-widget'>
                  <object class='AdwWindowTitle'>
                    <property name='title' translatable='yes'>Системные логи</property>
                  </object>
                </property>
                <child type='start'>
                  <object class='GtkToggleButton' id='btn_autoscroll'>
                    <property name='icon-name'>go-down-symbolic</property>
                    <property name='tooltip-text' translatable='yes'>Автопрокрутка</property>
                    <property name='active'>True</property>
                  </object>
                </child>
                <child type='start'>
                  <object class='GtkDropDown' id='dropdown_filter'>
                  </object>
                </child>
                <child type='end'>
                  <object class='GtkButton' id='btn_copy'>
                    <property name='icon-name'>edit-copy-symbolic</property>
                    <property name='tooltip-text' translatable='yes'>Скопировать логи</property>
                  </object>
                </child>
                <child type='end'>
                  <object class='GtkButton' id='btn_clear'>
                    <property name='icon-name'>edit-clear-all-symbolic</property>
                    <property name='tooltip-text' translatable='yes'>Очистить логи</property>
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
        pub last_pos: RefCell<u64>,
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
                gettextrs::gettext("Все логи").as_str(),
                gettextrs::gettext("Логи приложения").as_str(),
                gettextrs::gettext("Логи ядра").as_str(),
            ]);
            self.dropdown_filter.set_model(Some(&strings));

            let buffer = self.text_view.buffer();
            buffer.create_tag(Some("error"), &[("foreground", &"red"), ("weight", &700)]);
            buffer.create_tag(Some("warning"), &[("foreground", &"orange")]);
            buffer.create_tag(Some("debug"), &[("foreground", &"gray")]);
            buffer.create_tag(Some("app"), &[("foreground", &"#3584e4"), ("weight", &700)]); // GNOME blue

            obj.setup_callbacks();
            obj.start_log_polling();
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
                let log_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx");
                let log_path = log_dir.join("core.log");
                let _ = std::fs::write(log_path, ""); // Очищаем файл
                window.imp().text_view.buffer().set_text("");
                *window.imp().last_pos.borrow_mut() = 0;
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
    }

    fn start_log_polling(&self) {
        let log_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx");
        let log_path = log_dir.join("core.log");
        let window_weak = self.downgrade();
        
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if let Some(window) = window_weak.upgrade() {
                let imp = window.imp();
                if let Ok(mut file) = File::open(&log_path) {
                    let mut last_pos = *imp.last_pos.borrow();
                    let len = file.metadata().map(|m| m.len()).unwrap_or(0);

                    // Если файл укоротился (например, после очистки)
                    if len < last_pos {
                        last_pos = 0;
                        imp.text_view.buffer().set_text("");
                    }

                    if len > last_pos {
                        let _ = file.seek(SeekFrom::Start(last_pos));
                        
                        // Ограничиваем чтение, если файл слишком большой
                        let to_read = if last_pos == 0 && len > 102400 {
                            let _ = file.seek(SeekFrom::End(-102400));
                            102400
                        } else {
                            len - last_pos
                        };

                        let mut buffer_bytes = vec![0u8; to_read as usize];
                        let _ = file.read_exact(&mut buffer_bytes);
                        let content = String::from_utf8_lossy(&buffer_bytes);
                        
                        let buffer = imp.text_view.buffer();
                        let mut iter = buffer.end_iter();
                        let filter_index = imp.dropdown_filter.selected(); // 0 = Все, 1 = App, 2 = Core
                        
                        let mut has_new_lines = false;

                        for line in content.lines() {
                            let is_app = line.contains("[APP]");
                            
                            // Применяем фильтр
                            if filter_index == 1 && !is_app { continue; }
                            if filter_index == 2 && is_app { continue; }

                            let tag_name = if is_app {
                                Some("app")
                            } else if line.contains("ERROR") || line.contains("error") {
                                Some("error")
                            } else if line.contains("WARN") || line.contains("warning") {
                                Some("warning")
                            } else if line.contains("DEBUG") || line.contains("debug") {
                                Some("debug")
                            } else {
                                None
                            };

                            let mut line_with_newline = line.to_string();
                            line_with_newline.push('\n');

                            if let Some(tag) = tag_name {
                                buffer.insert_with_tags_by_name(&mut iter, &line_with_newline, &[tag]);
                            } else {
                                buffer.insert(&mut iter, &line_with_newline);
                            }
                            has_new_lines = true;
                        }
                        
                        // Автопрокрутка
                        if has_new_lines && imp.btn_autoscroll.is_active() {
                            let mark = buffer.create_mark(None, &buffer.end_iter(), false);
                            imp.text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
                            buffer.delete_mark(&mark);
                        }
                        
                        *imp.last_pos.borrow_mut() = len;
                    }
                }
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
}
