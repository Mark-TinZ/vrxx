/* log_window.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Окно просмотра системных и сетевых логов (VrxxLogWindow)
//!
//! Отвечает за:
//! - Отображение потока логов в реальном времени из системного демона через SSE
//! - Быструю фильтрацию по источникам (Все, Ядро sing-box, Приложение, Трафик/Access)
//! - Поиск и подсветку по ключевым словам (Ctrl+F)
//! - Регулировку масштаба шрифта (Ctrl++, Ctrl+-, Ctrl+0, Ctrl+Колесо мыши)
//! - Экспорт логов в текстовый файл и копирование в буфер обмена

use crate::daemon::events::LogSource;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;

/// Максимальное количество записей в кольцевом буфере окна логов (минимизация потребления RAM).
const MAX_LOG_ENTRIES: usize = 1000;

/// Внутренний элемент лога для сверхбыстрой фильтрации и поиска в памяти.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntryItem {
    pub source: LogSource,
    pub level: String,
    pub message: String,
}

/// Возвращает канонический путь к каталогу логов приложения (`~/.local/share/vrxx/logs`).
pub fn get_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("vrxx")
        .join("logs")
}

mod imp {
    use super::*;

    /// Структура CompositeTemplate для окна логов VrxxLogWindow
    #[derive(Debug, gtk::CompositeTemplate)]
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
        pub font_size: RefCell<i32>,
        pub scroll_accum: RefCell<f64>,
        pub logs: RefCell<VecDeque<LogEntryItem>>,
        pub css_provider: gtk::CssProvider,
    }

    impl Default for VrxxLogWindow {
        fn default() -> Self {
            Self {
                text_view: TemplateChild::default(),
                scrolled_window: TemplateChild::default(),
                btn_autoscroll: TemplateChild::default(),
                dropdown_filter: TemplateChild::default(),
                zoom_label: TemplateChild::default(),
                search_bar: TemplateChild::default(),
                search_entry: TemplateChild::default(),
                font_size: RefCell::new(10),
                scroll_accum: RefCell::new(0.0),
                logs: RefCell::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)),
                css_provider: gtk::CssProvider::new(),
            }
        }
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

            // Регистрируем переиспользуемый CSS-провайдер один раз
            gtk::style_context_add_provider_for_display(
                &WidgetExt::display(&*obj),
                &self.css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            // Настройка тегов цветовой подсветки уровней логирования
            let buffer = self.text_view.buffer();
            buffer.create_tag(
                Some("error"),
                &[("foreground", &"#E01B24"), ("weight", &700)],
            );
            buffer.create_tag(Some("warning"), &[("foreground", &"#E5A50A")]);
            buffer.create_tag(Some("debug"), &[("foreground", &"#9A9996")]);
            buffer.create_tag(Some("info"), &[("foreground", &"#3584E4")]);
            buffer.create_tag(Some("app"), &[("foreground", &"#2EC27E"), ("weight", &700)]);

            obj.setup_actions();
            obj.setup_callbacks();
            obj.setup_event_controllers();
            obj.setup_shortcuts();
            obj.update_font_size();
            obj.load_history();
            obj.setup_daemon_logs();
        }
    }
    impl WidgetImpl for VrxxLogWindow {}
    impl WindowImpl for VrxxLogWindow {}
    impl AdwWindowImpl for VrxxLogWindow {}
}

glib::wrapper! {
    /// Обертка GObject для окна системных логов
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
    /// Создает новый экземпляр окна логов.
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Проверяет, подходит ли запись лога под выбранную категорию и поисковый фильтр.
    pub fn entry_matches(entry: &LogEntryItem, filter_idx: u32, query_lower: &str) -> bool {
        let source_match = match filter_idx {
            1 => entry.source == LogSource::Core,
            2 => entry.source == LogSource::App,
            3 => entry.source == LogSource::Access,
            _ => true,
        };

        if !source_match {
            return false;
        }

        if query_lower.is_empty() {
            return true;
        }

        entry.message.to_lowercase().contains(query_lower)
    }

    /// Вставляет форматированную строку в буфер TextView с соответствующим стилем цвета.
    fn insert_entry_to_buffer(
        buffer: &gtk::TextBuffer,
        iter: &mut gtk::TextIter,
        entry: &LogEntryItem,
    ) {
        let tag_name = match entry.level.to_lowercase().as_str() {
            "error" | "fatal" | "panic" => "error",
            "warning" | "warn" => "warning",
            "debug" | "trace" => "debug",
            _ => "info",
        };

        let mut line = entry.message.clone();
        if !line.ends_with('\n') {
            line.push('\n');
        }

        buffer.insert_with_tags_by_name(iter, &line, &[tag_name]);
    }

    /// Мгновенная перерисовка буфера из оперативной памяти при смене фильтра или поискового запроса.
    pub fn rebuild_buffer(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        buffer.set_text("");

        let filter_idx = imp.dropdown_filter.selected();
        let query = imp.search_entry.text().to_lowercase();
        let logs = imp.logs.borrow();

        let mut iter = buffer.end_iter();
        for entry in logs.iter() {
            if Self::entry_matches(entry, filter_idx, &query) {
                Self::insert_entry_to_buffer(&buffer, &mut iter, entry);
            }
        }

        if imp.btn_autoscroll.is_active() {
            self.scroll_to_bottom();
        }
    }

    /// Регистрация локальных GActions для окна логов.
    fn setup_actions(&self) {
        let action_group = gio::SimpleActionGroup::new();
        self.insert_action_group("win", Some(&action_group));

        // Действие: Скопировать логи в буфер обмена
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

        // Действие: Очистить буфер и файлы логов
        let clear_action = gio::SimpleAction::new("clear_logs", None);
        let window_weak = self.downgrade();
        clear_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                let log_dir = get_log_dir();
                if let Ok(entries) = std::fs::read_dir(&log_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let _ = std::fs::write(&path, "");
                        }
                    }
                }
                window.imp().logs.borrow_mut().clear();
                window.imp().text_view.buffer().set_text("");
            }
        });
        action_group.add_action(&clear_action);

        // Действие: Увеличить масштаб шрифта
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

        // Действие: Уменьшить масштаб шрифта
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

        // Действие: Сбросить масштаб шрифта к 100%
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

        // Действие: Экспорт логов в файл
        let export_action = gio::SimpleAction::new("export_logs", None);
        let window_weak = self.downgrade();
        export_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.export_logs();
            }
        });
        action_group.add_action(&export_action);
    }

    /// Настройка сочетаний горячих клавиш окна логов.
    fn setup_shortcuts(&self) {
        let controller = gtk::ShortcutController::new();
        controller.set_scope(gtk::ShortcutScope::Managed);

        // Ctrl+F: Открыть/закрыть панель поиска
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

        // Ctrl+S: Сохранить логи в файл
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>s");
        let action = gtk::NamedAction::new("win.export_logs");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+Plus / Ctrl+=: Увеличение шрифта
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>equal");
        let action = gtk::NamedAction::new("win.zoom_in");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        let trigger = gtk::ShortcutTrigger::parse_string("<Control>plus");
        let action = gtk::NamedAction::new("win.zoom_in");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+-: Уменьшение шрифта
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>minus");
        let action = gtk::NamedAction::new("win.zoom_out");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        // Ctrl+0: Сброс масштаба к 100%
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>0");
        let action = gtk::NamedAction::new("win.zoom_normal");
        controller.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));

        self.add_controller(controller);
    }

    /// Подключение контроллера масштабирования колесиком мыши с зажатым Ctrl.
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

    /// Обновляет размер шрифта в окне логов через инъекцию CSS.
    fn update_font_size(&self) {
        let size = *self.imp().font_size.borrow();
        let css = format!(
            ".log-view {{ font-family: monospace; padding: 12px; font-size: {}pt; }}",
            size
        );
        self.imp().css_provider.load_from_string(&css);
    }

    /// Настройка сигналов интерфейса (поиск, фильтр категорий, автоскролл).
    fn setup_callbacks(&self) {
        let window_weak_filter = self.downgrade();
        self.imp()
            .dropdown_filter
            .connect_selected_notify(move |_| {
                if let Some(window) = window_weak_filter.upgrade() {
                    window.rebuild_buffer();
                }
            });

        let window_weak_search = self.downgrade();
        self.imp().search_entry.connect_search_changed(move |_| {
            if let Some(window) = window_weak_search.upgrade() {
                window.rebuild_buffer();
            }
        });

        let window_weak_scroll = self.downgrade();
        self.imp().btn_autoscroll.connect_toggled(move |btn| {
            if btn.is_active() {
                btn.add_css_class("suggested-action");
                if let Some(window) = window_weak_scroll.upgrade() {
                    window.scroll_to_bottom();
                }
            } else {
                btn.remove_css_class("suggested-action");
            }
        });
    }

    /// Прокручивает текстовый буфер вниз к последней строке.
    fn scroll_to_bottom(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mark = buffer.create_mark(None, &buffer.end_iter(), false);
        imp.text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
        buffer.delete_mark(&mark);
    }

    /// Подписывается на трансляцию логов системного демона через SSE поток.
    fn setup_daemon_logs(&self) {
        let window_weak = self.downgrade();
        glib::spawn_future_local(async move {
            let client = crate::ipc::DaemonClient::new();
            let logs = client.subscribe_events();

            loop {
                match logs.recv().await {
                    Ok(event) => {
                        let mut batch = vec![event];
                        while let Ok(next) = logs.try_recv() {
                            batch.push(next);
                            if batch.len() >= 100 {
                                break;
                            }
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.append_log_batch(&batch);
                        } else {
                            tracing::debug!("Log window closed, terminating SSE subscription");
                            break;
                        }
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        if window_weak.upgrade().is_none() {
                            tracing::debug!("Log window closed, terminating SSE subscription");
                            break;
                        }
                        tracing::warn!("Log stream disconnected: {}. Reconnecting...", e);
                        glib::timeout_future(std::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        });
    }

    /// Загружает историю последних логов при открытии окна.
    fn load_history(&self) {
        let client = crate::ipc::DaemonClient::new();
        let window_weak = self.downgrade();

        glib::spawn_future_local(async move {
            if let Ok(history) = client.get_history().await {
                if let Some(window) = window_weak.upgrade() {
                    let events: Vec<crate::daemon::DaemonEvent> = history.into_iter().collect();
                    window.append_log_batch(&events);
                }
            }
        });
    }

    /// Добавляет пачку событий логов в кольцевой буфер и отображает в TextView.
    pub fn append_log_batch(&self, events: &[crate::daemon::DaemonEvent]) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let filter_idx = imp.dropdown_filter.selected();
        let query = imp.search_entry.text().to_lowercase();

        let mut logs = imp.logs.borrow_mut();
        let mut iter = buffer.end_iter();
        let mut any_inserted = false;

        for event in events {
            if let crate::daemon::DaemonEvent::Log {
                source,
                level,
                message,
            } = event
            {
                let entry = LogEntryItem {
                    source: *source,
                    level: level.clone(),
                    message: message.clone(),
                };

                if logs.len() >= MAX_LOG_ENTRIES {
                    logs.pop_front();
                }
                logs.push_back(entry.clone());

                if Self::entry_matches(&entry, filter_idx, &query) {
                    Self::insert_entry_to_buffer(&buffer, &mut iter, &entry);
                    any_inserted = true;
                }
            }
        }

        // Ограничитель количества строк в UI буфере для максимальной отзывчивости интерфейса
        let line_count = buffer.line_count();
        if line_count > (MAX_LOG_ENTRIES as i32) {
            let mut start = buffer.start_iter();
            let mut end = buffer.start_iter();
            end.forward_lines(line_count - (MAX_LOG_ENTRIES as i32));
            buffer.delete(&mut start, &mut end);
        }

        if any_inserted && imp.btn_autoscroll.is_active() {
            self.scroll_to_bottom();
        }
    }

    /// Добавляет одиночную запись лога в окно.
    pub fn append_log(&self, level: &str, message: &str) {
        let event = crate::daemon::DaemonEvent::Log {
            source: LogSource::App,
            level: level.to_string(),
            message: message.to_string(),
        };
        self.append_log_batch(&[event]);
    }

    /// Открывает системный диалог для экспорта содержимого логов в текстовый файл.
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
                            Ok(_) => tracing::info!("Logs successfully exported to {}", file.uri()),
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
    fn test_entry_matches_logic() {
        let app_entry = LogEntryItem {
            source: LogSource::App,
            level: "info".to_string(),
            message: "Connected to VPN".to_string(),
        };
        let core_entry = LogEntryItem {
            source: LogSource::Core,
            level: "warning".to_string(),
            message: "DNS exchange timeout".to_string(),
        };
        let access_entry = LogEntryItem {
            source: LogSource::Access,
            level: "info".to_string(),
            message: "router: match -> direct".to_string(),
        };

        // 0: Все логи
        assert!(VrxxLogWindow::entry_matches(&app_entry, 0, ""));
        assert!(VrxxLogWindow::entry_matches(&core_entry, 0, ""));
        assert!(VrxxLogWindow::entry_matches(&access_entry, 0, ""));

        // 1: Логи ядра sing-box
        assert!(!VrxxLogWindow::entry_matches(&app_entry, 1, ""));
        assert!(VrxxLogWindow::entry_matches(&core_entry, 1, ""));
        assert!(!VrxxLogWindow::entry_matches(&access_entry, 1, ""));

        // 2: Логи приложения GUI
        assert!(VrxxLogWindow::entry_matches(&app_entry, 2, ""));
        assert!(!VrxxLogWindow::entry_matches(&core_entry, 2, ""));
        assert!(!VrxxLogWindow::entry_matches(&access_entry, 2, ""));

        // 3: Логи трафика / Access
        assert!(!VrxxLogWindow::entry_matches(&app_entry, 3, ""));
        assert!(!VrxxLogWindow::entry_matches(&core_entry, 3, ""));
        assert!(VrxxLogWindow::entry_matches(&access_entry, 3, ""));

        // Поисковый запрос
        assert!(VrxxLogWindow::entry_matches(&app_entry, 0, "vpn"));
        assert!(!VrxxLogWindow::entry_matches(&app_entry, 0, "timeout"));
        assert!(VrxxLogWindow::entry_matches(&core_entry, 1, "dns"));
        assert!(!VrxxLogWindow::entry_matches(&core_entry, 1, "vpn"));
    }

    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_log_window_append() {
        let _ = gtk::init();

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
