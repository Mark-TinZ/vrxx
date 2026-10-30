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

//! # Окно просмотра системных и сетевых логов (VrxxLogWindow / Виртуальная консоль)
//!
//! Отвечает за:
//! - Отображение консольных и системных логов приложения в реальном времени через локальный Tracing хук
//! - Прием потока логов ядра sing-box и фонового демона через SSE (`/api/events`)
//! - Раздельную фильтрацию по источникам (Все, Ядро sing-box, Приложение, Трафик/Access)
//! - Фильтрацию по уровням критичности (Все, Ошибки, Предупреждения, Инфо, Отладка)
//! - Сортировку по хронологии или по степени критичности (Severity: Errors -> Warnings -> Info -> Debug)
//! - Цветную структурированную подсветку по формату (таймстамп, уровень, модуль/таргет, тело)
//! - Строгий защищенный режим Read-Only с удобным копированием
//! - Очистку буфера оперативной памяти и ротацию устаревших журналов на диске
//! - Поиск и подсветку по ключевым словам (Ctrl+F)
//! - Регулировку масштаба шрифта (Ctrl++, Ctrl+-, Ctrl+0, Ctrl+Колесо мыши)
//! - Экспорт логов в текстовый файл и копирование в буфер обмена

use crate::daemon::events::LogSource;
pub use crate::logger::LogEntryItem;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;

/// Максимальное количество записей в кольцевом буфере окна логов (минимизация потребления RAM).
const MAX_LOG_ENTRIES: usize = 1000;

/// Режим упорядочивания записей в виртуальной консоли
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LogSortMode {
    /// Хронологический порядок поступления (по умолчанию)
    #[default]
    Chronological,
    /// Сортировка по степени важности (Errors -> Warnings -> Info -> Debug)
    Severity,
    /// Обратный хронологический порядок (новые записи вверху)
    ReverseChronological,
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
        pub dropdown_level: TemplateChild<gtk::DropDown>,
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
        pub sort_mode: RefCell<LogSortMode>,
    }

    impl Default for VrxxLogWindow {
        fn default() -> Self {
            Self {
                text_view: TemplateChild::default(),
                scrolled_window: TemplateChild::default(),
                btn_autoscroll: TemplateChild::default(),
                dropdown_filter: TemplateChild::default(),
                dropdown_level: TemplateChild::default(),
                zoom_label: TemplateChild::default(),
                search_bar: TemplateChild::default(),
                search_entry: TemplateChild::default(),
                font_size: RefCell::new(10),
                scroll_accum: RefCell::new(0.0),
                logs: RefCell::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)),
                css_provider: gtk::CssProvider::new(),
                sort_mode: RefCell::new(LogSortMode::Chronological),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VrxxLogWindow {
        const NAME: &'static str = "VrxxLogWindow";
        type Type = super::VrxxLogWindow;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            adw::Clamp::static_type();
            adw::Breakpoint::static_type();
            adw::ToolbarView::static_type();
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

            obj.set_width_request(360);
            obj.set_height_request(320);

            obj.imp().text_view.add_css_class("log-view");
            obj.imp().text_view.set_editable(false);
            obj.imp().text_view.set_cursor_visible(false);
            *obj.imp().font_size.borrow_mut() = 10;
            *obj.imp().scroll_accum.borrow_mut() = 0.0;

            let source_strings = gtk::StringList::new(&[
                gettextrs::gettext("All logs").as_str(),
                gettextrs::gettext("Core logs").as_str(),
                gettextrs::gettext("Application logs").as_str(),
                gettextrs::gettext("Access logs").as_str(),
            ]);
            self.dropdown_filter.set_model(Some(&source_strings));

            let level_strings = gtk::StringList::new(&[
                gettextrs::gettext("All levels").as_str(),
                gettextrs::gettext("Error").as_str(),
                gettextrs::gettext("Warning").as_str(),
                gettextrs::gettext("Info").as_str(),
                gettextrs::gettext("Debug").as_str(),
            ]);
            self.dropdown_level.set_model(Some(&level_strings));

            // Регистрируем переиспользуемый CSS-провайдер один раз, если дисплей доступен
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &self.css_provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }

            // Настройка тегов структурированной цветовой подсветки логов
            let buffer = self.text_view.buffer();
            buffer.create_tag(Some("timestamp"), &[("foreground", &"#77767B")]);
            buffer.create_tag(
                Some("level_error"),
                &[("foreground", &"#E01B24"), ("weight", &700)],
            );
            buffer.create_tag(
                Some("level_warning"),
                &[("foreground", &"#E5A50A"), ("weight", &700)],
            );
            buffer.create_tag(
                Some("level_info"),
                &[("foreground", &"#3584E4"), ("weight", &700)],
            );
            buffer.create_tag(
                Some("level_debug"),
                &[("foreground", &"#9141AC"), ("weight", &600)],
            );
            buffer.create_tag(
                Some("level_trace"),
                &[("foreground", &"#77767B"), ("weight", &400)],
            );
            buffer.create_tag(
                Some("target_tag"),
                &[("foreground", &"#2EC27E"), ("weight", &600)],
            );
            buffer.create_tag(Some("msg_error"), &[("foreground", &"#F66151")]);
            buffer.create_tag(Some("msg_warning"), &[("foreground", &"#F9C440")]);
            buffer.create_tag(Some("msg_access"), &[("foreground", &"#33D17A")]);
            buffer.create_tag(Some("msg_default"), &[]);

            // Сохраняем базовые теги для обратной совместимости
            buffer.create_tag(
                Some("error"),
                &[("foreground", &"#E01B24"), ("weight", &700)],
            );
            buffer.create_tag(Some("warning"), &[("foreground", &"#E5A50A")]);
            buffer.create_tag(Some("debug"), &[("foreground", &"#9A9996")]);
            buffer.create_tag(Some("info"), &[("foreground", &"#3584E4")]);
            buffer.create_tag(Some("app"), &[("foreground", &"#2EC27E"), ("weight", &700)]);

            if obj.imp().btn_autoscroll.is_active() {
                obj.imp().btn_autoscroll.add_css_class("suggested-action");
            }

            obj.setup_actions();
            obj.setup_callbacks();
            obj.setup_event_controllers();
            obj.setup_shortcuts();
            obj.update_font_size();
            obj.load_history();
            obj.setup_inprocess_logs();
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

    /// Проверяет, подходит ли запись лога под выбранную категорию и поисковый фильтр (обратная совместимость).
    pub fn entry_matches(entry: &LogEntryItem, filter_idx: u32, query_lower: &str) -> bool {
        Self::entry_matches_full(entry, filter_idx, 0, query_lower)
    }

    /// Проверяет соответствие записи лога категории источника, уровню критичности и поисковому запросу.
    pub fn entry_matches_full(
        entry: &LogEntryItem,
        source_filter_idx: u32,
        level_filter_idx: u32,
        query_lower: &str,
    ) -> bool {
        let source_match = match source_filter_idx {
            1 => entry.source == LogSource::Core,
            2 => entry.source == LogSource::App,
            3 => entry.source == LogSource::Access,
            _ => true,
        };

        if !source_match {
            return false;
        }

        let level_lower = entry.level.to_lowercase();
        let level_match = match level_filter_idx {
            1 => matches!(
                level_lower.as_str(),
                "error" | "err" | "fatal" | "panic" | "critical"
            ),
            2 => matches!(level_lower.as_str(), "warning" | "warn"),
            3 => matches!(level_lower.as_str(), "info" | "app"),
            4 => matches!(level_lower.as_str(), "debug" | "trace"),
            _ => true,
        };

        if !level_match {
            return false;
        }

        if query_lower.is_empty() {
            return true;
        }

        entry.message.to_lowercase().contains(query_lower)
    }

    /// Возвращает численный ранг критичности уровня (чем меньше число, тем выше приоритет).
    pub fn severity_rank(level: &str) -> u8 {
        match level.to_lowercase().as_str() {
            "fatal" | "panic" | "critical" => 0,
            "error" | "err" => 1,
            "warning" | "warn" => 2,
            "info" | "app" => 3,
            "debug" => 4,
            "trace" => 5,
            _ => 6,
        }
    }

    /// Очищает строку лога от терминальных управляющих ANSI escape-кодов.
    pub fn clean_ansi(input: &str) -> String {
        crate::logger::clean_ansi(input)
    }

    /// Вставляет форматированную строку в буфер TextView с цветной структурированной подсветкой по формату.
    fn insert_entry_to_buffer(
        buffer: &gtk::TextBuffer,
        iter: &mut gtk::TextIter,
        entry: &LogEntryItem,
    ) {
        let clean = Self::clean_ansi(&entry.message);
        let line = clean.trim_end();
        if line.is_empty() {
            return;
        }

        // Регулярное выражение для разбора структурированной строки лога с таймстапом:
        // [таймзона + дата/время] [УРОВЕНЬ] [остаток сообщения]
        static LOG_LINE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = LOG_LINE_RE.get_or_init(|| {
            regex::Regex::new(
                r"^(?P<time>\[?(?:[+-]\d{4}\s+)?\d{4}[-/]\d{2}[-/]\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z)?\]?\s+)(?P<level>[A-Za-z]+)(?:\[\d+\])?(?P<rest>(?s:.*))$",
            )
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
        });

        // Регулярное выражение для разбора строк вида [LEVEL] или LEVEL[0000] без таймстампа
        static BRACKET_LEVEL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let bracket_re = BRACKET_LEVEL_RE.get_or_init(|| {
            regex::Regex::new(
                r"^(?:\[(?P<level1>[A-Za-z]+)\]|(?P<level2>[A-Za-z]+)\[\d+\])(?P<rest>.*)$",
            )
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
        });

        static TARGET_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let target_re = TARGET_RE.get_or_init(|| {
            regex::Regex::new(
                r"^(?P<space>\s+)(?P<target>\[[^\]]+\](?::)?|[a-zA-Z0-9_\-\./]+:)(?P<msg>.*)$",
            )
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
        });

        if let Some(caps) = re.captures(line) {
            let time_str = caps.name("time").map(|m| m.as_str()).unwrap_or("");
            let level_str = caps.name("level").map(|m| m.as_str()).unwrap_or("");
            let rest_str = caps.name("rest").map(|m| m.as_str()).unwrap_or("");

            // 1. Таймстамп (приглушенный нейтральный серый)
            buffer.insert_with_tags_by_name(iter, time_str, &["timestamp"]);

            // 2. Уровень логирования (акцентная плашка)
            let level_tag = match level_str.to_lowercase().as_str() {
                "error" | "fatal" | "panic" | "critical" | "err" => "level_error",
                "warning" | "warn" => "level_warning",
                "debug" => "level_debug",
                "trace" => "level_trace",
                "app" => "app",
                _ => "level_info",
            };
            buffer.insert_with_tags_by_name(iter, level_str, &[level_tag]);

            // 3. Разбор модуля/таргета и тела сообщения
            if let Some(t_caps) = target_re.captures(rest_str) {
                let space = t_caps.name("space").map(|m| m.as_str()).unwrap_or(" ");
                let target = t_caps.name("target").map(|m| m.as_str()).unwrap_or("");
                let msg = t_caps.name("msg").map(|m| m.as_str()).unwrap_or("");

                buffer.insert(iter, space);
                buffer.insert_with_tags_by_name(iter, target, &["target_tag"]);

                let body_tag = if entry.source == LogSource::Access {
                    "msg_access"
                } else {
                    match entry.level.to_lowercase().as_str() {
                        "error" | "fatal" | "panic" | "critical" | "err" => "msg_error",
                        "warning" | "warn" => "msg_warning",
                        _ => "msg_default",
                    }
                };
                buffer.insert_with_tags_by_name(iter, msg, &[body_tag]);
            } else {
                let body_tag = if entry.source == LogSource::Access {
                    "msg_access"
                } else {
                    match entry.level.to_lowercase().as_str() {
                        "error" | "fatal" | "panic" | "critical" | "err" => "msg_error",
                        "warning" | "warn" => "msg_warning",
                        _ => "msg_default",
                    }
                };
                buffer.insert_with_tags_by_name(iter, rest_str, &[body_tag]);
            }
        } else if let Some(b_caps) = bracket_re.captures(line) {
            let level_str = b_caps
                .name("level1")
                .or_else(|| b_caps.name("level2"))
                .map(|m| m.as_str())
                .unwrap_or("info");
            let rest_str = b_caps.name("rest").map(|m| m.as_str()).unwrap_or("");

            let level_tag = match level_str.to_lowercase().as_str() {
                "error" | "fatal" | "panic" | "critical" | "err" => "level_error",
                "warning" | "warn" => "level_warning",
                "debug" => "level_debug",
                "trace" => "level_trace",
                "app" => "app",
                _ => "level_info",
            };
            buffer.insert_with_tags_by_name(iter, &format!("[{level_str}]"), &[level_tag]);

            if let Some(t_caps) = target_re.captures(rest_str) {
                let space = t_caps.name("space").map(|m| m.as_str()).unwrap_or(" ");
                let target = t_caps.name("target").map(|m| m.as_str()).unwrap_or("");
                let msg = t_caps.name("msg").map(|m| m.as_str()).unwrap_or("");

                buffer.insert(iter, space);
                buffer.insert_with_tags_by_name(iter, target, &["target_tag"]);

                let body_tag = if entry.source == LogSource::Access {
                    "msg_access"
                } else {
                    match entry.level.to_lowercase().as_str() {
                        "error" | "fatal" | "panic" | "critical" | "err" => "msg_error",
                        "warning" | "warn" => "msg_warning",
                        _ => "msg_default",
                    }
                };
                buffer.insert_with_tags_by_name(iter, msg, &[body_tag]);
            } else {
                buffer.insert(iter, rest_str);
            }
        } else {
            // Неструктурированная строка (например, прямой вывод или стек вызовов)
            let tag_name = match entry.level.to_lowercase().as_str() {
                "error" | "fatal" | "panic" | "critical" | "err" => "error",
                "warning" | "warn" => "warning",
                "debug" | "trace" => "debug",
                "app" => "app",
                _ => "info",
            };
            buffer.insert_with_tags_by_name(iter, line, &[tag_name]);
        }
        buffer.insert(iter, "\n");
    }

    /// Мгновенная перерисовка буфера из оперативной памяти при смене фильтра или поискового запроса.
    pub fn rebuild_buffer(&self) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        buffer.set_text("");

        let source_filter_idx = imp.dropdown_filter.selected();
        let level_filter_idx = imp.dropdown_level.selected();
        let query = imp.search_entry.text().to_lowercase();
        let logs = imp.logs.borrow();
        let sort_mode = *imp.sort_mode.borrow();

        // 1. Фильтрация
        let mut filtered: Vec<&LogEntryItem> = logs
            .iter()
            .filter(|entry| {
                Self::entry_matches_full(entry, source_filter_idx, level_filter_idx, &query)
            })
            .collect();

        // 2. Сортировка по выбранному режиму
        match sort_mode {
            LogSortMode::Chronological => {}
            LogSortMode::Severity => {
                filtered.sort_by_key(|entry| Self::severity_rank(&entry.level));
            }
            LogSortMode::ReverseChronological => {
                filtered.reverse();
            }
        }

        // 3. Вывод строк в буфер
        let mut iter = buffer.end_iter();
        for entry in filtered {
            Self::insert_entry_to_buffer(&buffer, &mut iter, entry);
        }

        if imp.btn_autoscroll.is_active() && sort_mode == LogSortMode::Chronological {
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

        // Действие: Очистить буфер в памяти, обновить отображение и запустить ротацию устаревших логов
        let clear_action = gio::SimpleAction::new("clear_logs", None);
        let window_weak = self.downgrade();
        clear_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                // 1. Очистка глобального буфера Tracing в памяти процесса
                crate::logger::clear_ui_log_history();

                // 2. Ротация и удаление устаревших журналов на диске (старше 7 дней / лимит 50 МБ)
                let log_dir = get_log_dir();
                crate::logger::cleanup_old_logs(&log_dir);

                // 3. Сброс локальной памяти окна и буфера отображения
                window.imp().logs.borrow_mut().clear();
                window.imp().text_view.buffer().set_text("");
            }
        });
        action_group.add_action(&clear_action);

        // Действие: Сортировка по времени (хронологическая)
        let sort_chrono_action = gio::SimpleAction::new("sort_chronological", None);
        let window_weak = self.downgrade();
        sort_chrono_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                *window.imp().sort_mode.borrow_mut() = LogSortMode::Chronological;
                window.rebuild_buffer();
            }
        });
        action_group.add_action(&sort_chrono_action);

        // Действие: Сортировка по важности уровня (Errors -> Warnings -> Info -> Debug)
        let sort_severity_action = gio::SimpleAction::new("sort_severity", None);
        let window_weak = self.downgrade();
        sort_severity_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                *window.imp().sort_mode.borrow_mut() = LogSortMode::Severity;
                window.rebuild_buffer();
            }
        });
        action_group.add_action(&sort_severity_action);

        // Действие: Сортировка в обратном порядке (новые сверху)
        let sort_reverse_action = gio::SimpleAction::new("sort_reverse_chronological", None);
        let window_weak = self.downgrade();
        sort_reverse_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                *window.imp().sort_mode.borrow_mut() = LogSortMode::ReverseChronological;
                window.rebuild_buffer();
            }
        });
        action_group.add_action(&sort_reverse_action);

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

    /// Настройка сигналов интерфейса (поиск, фильтр категорий и уровней, автоскролл).
    fn setup_callbacks(&self) {
        let window_weak_filter = self.downgrade();
        self.imp()
            .dropdown_filter
            .connect_selected_notify(move |_| {
                if let Some(window) = window_weak_filter.upgrade() {
                    window.rebuild_buffer();
                }
            });

        let window_weak_level = self.downgrade();
        self.imp().dropdown_level.connect_selected_notify(move |_| {
            if let Some(window) = window_weak_level.upgrade() {
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

    /// Подписывается на трансляцию консольных и приложенческих логов через локальный Tracing хук.
    fn setup_inprocess_logs(&self) {
        let window_weak = self.downgrade();
        glib::spawn_future_local(async move {
            let mut rx = crate::logger::subscribe_ui_logs();

            loop {
                match rx.recv().await {
                    Ok(entry) => {
                        let mut batch = vec![crate::daemon::DaemonEvent::Log {
                            source: entry.source,
                            level: crate::protocol::LogLevel::from(entry.level.as_str()),
                            message: entry.message,
                        }];
                        while let Ok(next) = rx.try_recv() {
                            batch.push(crate::daemon::DaemonEvent::Log {
                                source: next.source,
                                level: crate::protocol::LogLevel::from(next.level.as_str()),
                                message: next.message,
                            });
                            if batch.len() >= 100 {
                                break;
                            }
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.append_log_batch(&batch);
                        } else {
                            tracing::debug!(
                                "Log window closed, terminating in-process logs subscription"
                            );
                            break;
                        }
                        glib::timeout_future(std::time::Duration::from_millis(30)).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("UI in-process log stream lagged by {} messages", skipped);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }

    /// Добавляет единичную запись лога в виртуальную консоль с автоопределением источника.
    pub fn append_log(&self, level: &str, message: &str) {
        let source = if level == "app" || message.contains("[Vrxx]") {
            LogSource::App
        } else if message.contains("router:") || message.contains("match ->") {
            LogSource::Access
        } else {
            LogSource::Core
        };

        self.append_log_batch(&[crate::daemon::DaemonEvent::Log {
            source,
            level: crate::protocol::LogLevel::from(level),
            message: message.to_string(),
        }]);
    }

    /// Добавляет пачку событий логов в кольцевой буфер и текстовое отображение окна.
    pub fn append_log_batch(&self, events: &[crate::daemon::DaemonEvent]) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let source_filter_idx = imp.dropdown_filter.selected();
        let level_filter_idx = imp.dropdown_level.selected();
        let query = imp.search_entry.text().to_lowercase();
        let sort_mode = *imp.sort_mode.borrow();

        let mut logs = imp.logs.borrow_mut();
        let mut new_matching_entries = Vec::new();

        for event in events {
            if let crate::daemon::DaemonEvent::Log {
                source,
                level,
                message,
            } = event
            {
                let entry = LogEntryItem {
                    source: *source,
                    level: level.as_str().to_string(),
                    message: message.clone(),
                };

                // Предотвращаем дублирование последовательных идентичных событий
                if logs.back() == Some(&entry) {
                    continue;
                }

                if logs.len() >= MAX_LOG_ENTRIES && !logs.is_empty() {
                    logs.pop_front();
                }
                logs.push_back(entry.clone());

                if Self::entry_matches_full(&entry, source_filter_idx, level_filter_idx, &query) {
                    new_matching_entries.push(entry);
                }
            }
        }
        drop(logs);

        if new_matching_entries.is_empty() {
            return;
        }

        // Если включена сортировка по важности или обратная хронология, перерисовываем буфер целиком
        if sort_mode != LogSortMode::Chronological {
            self.rebuild_buffer();
            return;
        }

        // В обычном хронологическом режиме дописываем строки в конец буфера
        let mut iter = buffer.end_iter();
        for entry in &new_matching_entries {
            Self::insert_entry_to_buffer(&buffer, &mut iter, entry);
        }

        // Ограничиваем количество строк в буфере GTK, чтобы не допускать утечек памяти и замедления UI
        if buffer.line_count() > (MAX_LOG_ENTRIES as i32 + 200) {
            self.rebuild_buffer();
            return;
        }

        if imp.btn_autoscroll.is_active() && sort_mode == LogSortMode::Chronological {
            self.scroll_to_bottom();
        }
    }

    /// Загружает сохраненную историю логов из оперативной памяти Tracing буфера и фонового демона.
    fn load_history(&self) {
        let inprocess_history = crate::logger::get_ui_log_history();
        if !inprocess_history.is_empty() {
            let mut logs = self.imp().logs.borrow_mut();
            for entry in inprocess_history {
                if logs.len() >= MAX_LOG_ENTRIES && !logs.is_empty() {
                    logs.pop_front();
                }
                logs.push_back(entry);
            }
        }
        self.rebuild_buffer();

        // Асинхронно запрашиваем историю демона, если он запущен
        let client = crate::ipc::DaemonClient::new();
        let window_weak = self.downgrade();
        glib::spawn_future_local(async move {
            if let Ok(history) = client.get_history().await {
                if !history.is_empty() {
                    if let Some(window) = window_weak.upgrade() {
                        window.append_log_batch(&history);
                    }
                }
            }
        });
    }

    /// Подключается к потоку событий демона через SSE для получения сетевых логов и логов ядра sing-box.
    fn setup_daemon_logs(&self) {
        let client = crate::ipc::DaemonClient::new();
        let rx = client.subscribe_events();
        let window_weak = self.downgrade();

        glib::spawn_future_local(async move {
            while let Ok(event) = rx.recv().await {
                if let crate::daemon::DaemonEvent::Log { .. } = event {
                    let mut batch = vec![event];
                    while let Ok(next) = rx.try_recv() {
                        if let crate::daemon::DaemonEvent::Log { .. } = next {
                            batch.push(next);
                        }
                        if batch.len() >= 100 {
                            break;
                        }
                    }
                    if let Some(window) = window_weak.upgrade() {
                        window.append_log_batch(&batch);
                    } else {
                        break;
                    }
                    glib::timeout_future(std::time::Duration::from_millis(30)).await;
                }
            }
        });
    }

    /// Экспорт текущего содержимого виртуальной консоли в текстовый файл через диалог сохранения.
    pub fn export_logs(&self) {
        let buffer = self.imp().text_view.buffer();
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false).to_string();

        let file_dialog = gtk::FileDialog::builder()
            .title(gettextrs::gettext("Save Logs As"))
            .build();

        let filter_txt = gtk::FileFilter::new();
        filter_txt.set_name(Some(&gettextrs::gettext("Text File (*.txt)")));
        filter_txt.add_mime_type("text/plain");
        filter_txt.add_pattern("*.txt");

        let filter_log = gtk::FileFilter::new();
        filter_log.set_name(Some(&gettextrs::gettext("Log File (*.log)")));
        filter_log.add_mime_type("text/plain");
        filter_log.add_pattern("*.log");

        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter_log);
        filters.append(&filter_txt);

        file_dialog.set_filters(Some(&filters));
        file_dialog.set_default_filter(Some(&filter_log));

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        file_dialog.set_initial_name(Some(&format!("vrxx_logs_{timestamp}.log")));

        file_dialog.save(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    if let Err(e) = std::fs::write(&path, &text) {
                        tracing::error!("Failed to export logs to {:?}: {}", path, e);
                    } else {
                        tracing::info!("Logs successfully exported to {:?}", path);
                    }
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
    fn test_level_filtering_logic() {
        let err_entry = LogEntryItem {
            source: LogSource::App,
            level: "error".to_string(),
            message: "Fatal crash occurred".to_string(),
        };
        let warn_entry = LogEntryItem {
            source: LogSource::App,
            level: "warning".to_string(),
            message: "High memory warning".to_string(),
        };
        let info_entry = LogEntryItem {
            source: LogSource::App,
            level: "info".to_string(),
            message: "Application launched".to_string(),
        };
        let debug_entry = LogEntryItem {
            source: LogSource::App,
            level: "debug".to_string(),
            message: "Cache hit: 42".to_string(),
        };

        // Level 0: Все уровни
        assert!(VrxxLogWindow::entry_matches_full(&err_entry, 0, 0, ""));
        assert!(VrxxLogWindow::entry_matches_full(&warn_entry, 0, 0, ""));
        assert!(VrxxLogWindow::entry_matches_full(&info_entry, 0, 0, ""));
        assert!(VrxxLogWindow::entry_matches_full(&debug_entry, 0, 0, ""));

        // Level 1: Только ошибки
        assert!(VrxxLogWindow::entry_matches_full(&err_entry, 0, 1, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&warn_entry, 0, 1, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&info_entry, 0, 1, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&debug_entry, 0, 1, ""));

        // Level 2: Предупреждения
        assert!(!VrxxLogWindow::entry_matches_full(&err_entry, 0, 2, ""));
        assert!(VrxxLogWindow::entry_matches_full(&warn_entry, 0, 2, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&info_entry, 0, 2, ""));

        // Level 3: Информационные сообщения
        assert!(!VrxxLogWindow::entry_matches_full(&err_entry, 0, 3, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&warn_entry, 0, 3, ""));
        assert!(VrxxLogWindow::entry_matches_full(&info_entry, 0, 3, ""));

        // Level 4: Отладка
        assert!(!VrxxLogWindow::entry_matches_full(&err_entry, 0, 4, ""));
        assert!(VrxxLogWindow::entry_matches_full(&debug_entry, 0, 4, ""));

        // Комбинированный фильтр: Source + Level + Query
        assert!(VrxxLogWindow::entry_matches_full(&err_entry, 2, 1, "fatal"));
        assert!(!VrxxLogWindow::entry_matches_full(
            &err_entry, 1, 1, "fatal"
        )); // не Core
        assert!(!VrxxLogWindow::entry_matches_full(
            &err_entry, 2, 1, "hello"
        )); // несовпадение запроса
    }

    #[test]
    fn test_log_sorting_logic() {
        let mut entries = [
            LogEntryItem {
                source: LogSource::App,
                level: "info".to_string(),
                message: "1. Info message".to_string(),
            },
            LogEntryItem {
                source: LogSource::App,
                level: "error".to_string(),
                message: "2. Error message".to_string(),
            },
            LogEntryItem {
                source: LogSource::App,
                level: "warning".to_string(),
                message: "3. Warning message".to_string(),
            },
            LogEntryItem {
                source: LogSource::App,
                level: "debug".to_string(),
                message: "4. Debug message".to_string(),
            },
        ];

        // Сортировка по степени критичности (Severity)
        entries.sort_by_key(|e| VrxxLogWindow::severity_rank(&e.level));
        assert_eq!(entries[0].level, "error");
        assert_eq!(entries[1].level, "warning");
        assert_eq!(entries[2].level, "info");
        assert_eq!(entries[3].level, "debug");
    }

    #[test]
    fn test_severity_rank() {
        assert_eq!(VrxxLogWindow::severity_rank("fatal"), 0);
        assert_eq!(VrxxLogWindow::severity_rank("panic"), 0);
        assert_eq!(VrxxLogWindow::severity_rank("critical"), 0);
        assert_eq!(VrxxLogWindow::severity_rank("error"), 1);
        assert_eq!(VrxxLogWindow::severity_rank("err"), 1);
        assert_eq!(VrxxLogWindow::severity_rank("warning"), 2);
        assert_eq!(VrxxLogWindow::severity_rank("warn"), 2);
        assert_eq!(VrxxLogWindow::severity_rank("info"), 3);
        assert_eq!(VrxxLogWindow::severity_rank("app"), 3);
        assert_eq!(VrxxLogWindow::severity_rank("debug"), 4);
        assert_eq!(VrxxLogWindow::severity_rank("trace"), 5);
        assert_eq!(VrxxLogWindow::severity_rank("unknown"), 6);
    }

    #[test]
    fn test_reverse_chronological_sorting() {
        let mut entries = [
            LogEntryItem {
                source: LogSource::App,
                level: "info".to_string(),
                message: "1. First message".to_string(),
            },
            LogEntryItem {
                source: LogSource::App,
                level: "info".to_string(),
                message: "2. Second message".to_string(),
            },
        ];
        entries.reverse();
        assert_eq!(entries[0].message, "2. Second message");
        assert_eq!(entries[1].message, "1. First message");
    }

    #[test]
    fn test_entry_matches_extended_levels() {
        let err_item = LogEntryItem {
            source: LogSource::Core,
            level: "err".to_string(),
            message: "Short err level".to_string(),
        };
        let panic_item = LogEntryItem {
            source: LogSource::Core,
            level: "panic".to_string(),
            message: "Singbox panic".to_string(),
        };
        let app_item = LogEntryItem {
            source: LogSource::App,
            level: "app".to_string(),
            message: "App level entry".to_string(),
        };

        // Level 1: Ошибки
        assert!(VrxxLogWindow::entry_matches_full(&err_item, 0, 1, ""));
        assert!(VrxxLogWindow::entry_matches_full(&panic_item, 0, 1, ""));
        assert!(!VrxxLogWindow::entry_matches_full(&app_item, 0, 1, ""));

        // Level 3: Инфо / App
        assert!(VrxxLogWindow::entry_matches_full(&app_item, 0, 3, ""));
    }

    #[test]
    fn test_log_entry_item_serde() {
        let entry = LogEntryItem {
            source: LogSource::App,
            level: "warn".to_string(),
            message: "Warning test message".to_string(),
        };
        let serialized = serde_json::to_string(&entry).expect("Serialization failed");
        assert!(serialized.contains("Warning test message"));
        let deserialized: LogEntryItem =
            serde_json::from_str(&serialized).expect("Deserialization failed");
        assert_eq!(deserialized, entry);
    }

    #[test]
    fn test_clean_ansi() {
        let raw = "\x1B[31mError:\x1B[0m something went wrong \x1B[1;32m[OK]\x1B[0m";
        let clean = VrxxLogWindow::clean_ansi(raw);
        assert_eq!(clean, "Error: something went wrong [OK]");
    }

    #[test]
    fn test_clean_ansi_extended() {
        let raw = "\x1B[1;34m[INFO]\x1B[0m \x1B[33mWarning:\x1B[0m \x1B(BTest";
        let clean = VrxxLogWindow::clean_ansi(raw);
        assert_eq!(clean, "[INFO] Warning: Test");
    }

    #[test]
    fn test_structured_log_parsing_regex() {
        let ts_re = regex::Regex::new(
            r"^(?P<time>\[?(?:[+-]\d{4}\s+)?\d{4}[-/]\d{2}[-/]\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z)?\]?\s+)(?P<level>[A-Za-z]+)(?:\[\d+\])?(?P<rest>(?s:.*))$",
        ).unwrap();

        let line1 = "2026-09-25 12:00:00 INFO [vrxx::ui]: App startup\n";
        let caps1 = ts_re.captures(line1).expect("line1 should match");
        assert_eq!(
            caps1.name("time").unwrap().as_str().trim(),
            "2026-09-25 12:00:00"
        );
        assert_eq!(caps1.name("level").unwrap().as_str(), "INFO");

        let line2 = "+0600 2026-09-25 12:00:01 WARN [dns]: Server timeout\n";
        let caps2 = ts_re.captures(line2).expect("line2 should match");
        assert_eq!(caps2.name("level").unwrap().as_str(), "WARN");

        let line3 = "[2026-09-25T12:00:02Z] ERROR [core]: Failure\n";
        let caps3 = ts_re.captures(line3).expect("line3 should match");
        assert_eq!(caps3.name("level").unwrap().as_str(), "ERROR");

        let bracket_re = regex::Regex::new(
            r"^(?:\[(?P<level1>[A-Za-z]+)\]|(?P<level2>[A-Za-z]+)\[\d+\])(?P<rest>.*)$",
        )
        .unwrap();
        let line4 = "[INFO] [vrxx::test]: Bracketed log line";
        let caps4 = bracket_re.captures(line4).expect("line4 should match");
        assert_eq!(caps4.name("level1").unwrap().as_str(), "INFO");

        let line5 = "WARN[0001] sing-box startup delay";
        let caps5 = bracket_re.captures(line5).expect("line5 should match");
        assert_eq!(caps5.name("level2").unwrap().as_str(), "WARN");
    }

    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_log_window_append() {
        crate::ui::tests::run_test_on_gtk_thread(|| {
            let log_window = VrxxLogWindow::new();
            let buffer = log_window.imp().text_view.buffer();

            log_window.append_log("error", "Test error message");
            log_window.append_log("app", "Test app message");

            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);

            assert!(text.contains("Test error message"));
            assert!(text.contains("Test app message"));
        });
    }
}
