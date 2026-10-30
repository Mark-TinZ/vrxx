/* tests.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Интеграционные и модульные тесты для компонентов пользовательского интерфейса (UI)

pub fn run_test_on_gtk_thread<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    ui_testing::run_on_gtk_thread(f);
}

#[cfg(test)]
mod ui_testing {
    use gtk::{gio, glib};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::OnceLock;

    static GTK_THREAD_TX: OnceLock<Sender<Box<dyn FnOnce() + Send>>> = OnceLock::new();

    pub(crate) fn run_on_gtk_thread<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let tx = GTK_THREAD_TX.get_or_init(|| {
            let (tx, rx) = channel::<Box<dyn FnOnce() + Send>>();
            std::thread::spawn(move || {
                if gtk::init().is_err() || adw::init().is_err() {
                    return;
                }

                // Загрузка скомпилированных ресурсов
                let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
                if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
                    gio::resources_register(&res);
                }

                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to initialize Tokio runtime for tests");
                let _rt_guard = rt.enter();

                while let Ok(task) = rx.recv() {
                    task();
                }
            });
            tx
        });

        let (done_tx, done_rx) = channel();
        tx.send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let _ = done_tx.send(result);
        }))
        .expect("GTK test runner thread terminated");

        match done_rx.recv().expect("Failed to receive test completion") {
            Ok(()) => {}
            Err(panic_err) => std::panic::resume_unwind(panic_err),
        }
    }

    /// Проверяет, что все составные шаблоны GTK4/Libadwaita инициализируются без сбоев.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_ui_components_init() {
        run_on_gtk_thread(|| {
            let _log_window = crate::ui::components::log_window::VrxxLogWindow::new();
            let _settings_page = crate::ui::pages::VrxxSettingsPage::new();
            let _vpn_page = crate::ui::pages::VrxxVpnPage::new();
            let _routing_page = crate::ui::pages::VrxxRoutingPage::new();
            let _proxy_page = crate::ui::pages::VrxxProxyPage::new();
            let _qr_dialog = crate::ui::qr_dialog::VrxxQrDialog::new();
            let _rule_dialog = crate::ui::rule_dialog::VrxxRuleDialog::new();
            let _key_dialog = crate::ui::key_dialog::VrxxKeyDialog::new();
            let _export_dialog = crate::ui::export_dialog::VrxxExportDialog::new();
            let _import_dialog = crate::ui::import_dialog::VrxxImportDialog::new();

            use adw::subclass::prelude::*;
            use gtk::prelude::*;

            // Проверка явных минимальных размеров компонентов во избежание предупреждений Libadwaita
            assert_eq!(_vpn_page.width_request(), 360);
            assert_eq!(_vpn_page.height_request(), 200);
            assert_eq!(_proxy_page.width_request(), 360);
            assert_eq!(_proxy_page.height_request(), 200);
            assert_eq!(_routing_page.width_request(), 360);
            assert_eq!(_routing_page.height_request(), 200);
            assert_eq!(_settings_page.width_request(), 360);
            assert_eq!(_settings_page.height_request(), 200);
            assert_eq!(_qr_dialog.width_request(), 360);
            assert_eq!(_qr_dialog.height_request(), 360);
            assert_eq!(_rule_dialog.width_request(), 360);
            assert_eq!(_rule_dialog.height_request(), 360);
            assert_eq!(_key_dialog.width_request(), 360);
            assert_eq!(_key_dialog.height_request(), 360);
            assert_eq!(_export_dialog.width_request(), 360);
            assert_eq!(_export_dialog.height_request(), 300);
            assert_eq!(_import_dialog.width_request(), 360);
            assert_eq!(_import_dialog.height_request(), 360);
            assert_eq!(_log_window.width_request(), 360);
            assert_eq!(_log_window.height_request(), 320);

            let app_id = format!("ru.mark.vrxx.test.init_{}", std::process::id());
            let app = adw::Application::builder()
                .application_id(&app_id)
                .flags(gio::ApplicationFlags::NON_UNIQUE)
                .build();
            let _ = app.register(None as Option<&gio::Cancellable>);
            let _window = crate::window::VrxxWindow::new(&app);
            assert_eq!(_window.width_request(), 360);
            assert_eq!(_window.height_request(), 500);

            let row = crate::ui::components::vpn_key_row::VrxxVpnKeyRow::new();
            let key_obj = crate::ui::models::VpnKeyObject::new(
                "Server 1",
                "VLESS",
                false,
                "vless://test@127.0.0.1:443",
            );
            row.bind(&key_obj);

            // По умолчанию пинг пуст: метка скрыта
            assert!(!row.imp().lbl_row_ping.is_visible());

            // При наличии пинга: метка отображается с правильным стилем
            key_obj.set_ping("45 ms".to_string());
            assert!(row.imp().lbl_row_ping.is_visible());
            assert_eq!(row.imp().lbl_row_ping.text(), "45 ms");
            assert!(row.imp().lbl_row_ping.has_css_class("success"));

            // При сбросе в 0 ms: метка скрывается
            key_obj.set_ping("0 ms".to_string());
            assert!(!row.imp().lbl_row_ping.is_visible());

            // При таймауте: отображается с классом ошибки
            key_obj.set_ping("timeout".to_string());
            assert!(row.imp().lbl_row_ping.is_visible());
            assert!(row.imp().lbl_row_ping.has_css_class("error"));
        });
    }

    /// Проверяет работу фильтрации категорий в окне логов.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_log_filtering_integration() {
        run_on_gtk_thread(|| {
            use adw::subclass::prelude::*;
            use gtk::prelude::*;

            let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
            let buffer = log_window.imp().text_view.buffer();

            // 1. Тест фильтра "Все логи"
            log_window.imp().dropdown_filter.set_selected(0);
            log_window.append_log("info", "[Vrxx] App started");
            log_window.append_log("info", "Core accepted connection");

            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);
            assert!(text.contains("[Vrxx] App started"));
            assert!(text.contains("Core accepted connection"));

            buffer.set_text("");

            // 2. Тест фильтра "Логи приложения"
            log_window.imp().dropdown_filter.set_selected(2);
            log_window.append_log("info", "[Vrxx] Important event");
            log_window.append_log_batch(&[crate::daemon::DaemonEvent::Log {
                source: crate::daemon::events::LogSource::Core,
                level: crate::protocol::LogLevel::Info,
                message: "Random core message".to_string(),
            }]);

            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);
            assert!(text.contains("[Vrxx] Important event"));
            assert!(!text.contains("Random core message"));
        });
    }

    /// Проверяет работу фильтрации по уровням критичности и сортировки в окне логов.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_log_window_level_filtering_and_sorting() {
        run_on_gtk_thread(|| {
            use adw::subclass::prelude::*;
            use gtk::prelude::*;

            let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
            let buffer = log_window.imp().text_view.buffer();

            // Проверка флагов только для чтения
            assert!(!log_window.imp().text_view.is_editable());
            assert!(!log_window.imp().text_view.is_cursor_visible());

            // Добавляем тестовые события разных уровней
            log_window.append_log(
                "info",
                "2026-09-25 12:00:00 INFO [ui]: Standard info message",
            );
            log_window.append_log(
                "error",
                "2026-09-25 12:00:01 ERROR [vpn]: Critical error occurred",
            );
            log_window.append_log(
                "warning",
                "2026-09-25 12:00:02 WARN [dns]: DNS server unreachable",
            );

            // 1. Фильтр "Только ошибки" (индекс 1)
            log_window.imp().dropdown_level.set_selected(1);
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);
            assert!(text.contains("Critical error occurred"));
            assert!(!text.contains("Standard info message"));
            assert!(!text.contains("DNS server unreachable"));

            // 2. Сброс фильтра в "Все уровни" (индекс 0)
            log_window.imp().dropdown_level.set_selected(0);
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);
            assert!(text.contains("Critical error occurred"));
            assert!(text.contains("Standard info message"));
            assert!(text.contains("DNS server unreachable"));

            // 3. Проверка свойств вторичного окна (заголовок, геометрия, модальность/родитель)
            assert_eq!(log_window.title().as_deref(), Some("System Logs"));
            assert_eq!(log_window.default_width(), 800);
            assert_eq!(log_window.default_height(), 500);

            let parent_win = gtk::Window::new();
            log_window.set_transient_for(Some(&parent_win));
            assert_eq!(log_window.transient_for().as_ref(), Some(&parent_win));

            // 4. Проверка действий сортировки (win.sort_severity, win.sort_reverse_chronological и win.sort_chronological)
            let _ =
                gtk::prelude::WidgetExt::activate_action(&log_window, "win.sort_severity", None);
            assert_eq!(
                *log_window.imp().sort_mode.borrow(),
                crate::ui::components::log_window::LogSortMode::Severity
            );
            let _ = gtk::prelude::WidgetExt::activate_action(
                &log_window,
                "win.sort_reverse_chronological",
                None,
            );
            assert_eq!(
                *log_window.imp().sort_mode.borrow(),
                crate::ui::components::log_window::LogSortMode::ReverseChronological
            );
            let _ = gtk::prelude::WidgetExt::activate_action(
                &log_window,
                "win.sort_chronological",
                None,
            );
            assert_eq!(
                *log_window.imp().sort_mode.borrow(),
                crate::ui::components::log_window::LogSortMode::Chronological
            );

            // 5. Проверка масштабирования шрифта
            let _ = gtk::prelude::WidgetExt::activate_action(&log_window, "win.zoom_in", None);
            assert_eq!(*log_window.imp().font_size.borrow(), 11);
            let _ = gtk::prelude::WidgetExt::activate_action(&log_window, "win.zoom_normal", None);
            assert_eq!(*log_window.imp().font_size.borrow(), 10);

            // 6. Проверка действия очистки логов (clear_logs)
            let _ = gtk::prelude::WidgetExt::activate_action(&log_window, "win.clear_logs", None);
            let (start, end) = buffer.bounds();
            let text_cleared = buffer.text(&start, &end, false);
            assert!(text_cleared.is_empty());
            assert_eq!(log_window.imp().logs.borrow().len(), 0);
        });
    }

    /// Проверяет корректность работы интернационализации (gettext) для русского и английского языков.
    #[test]
    fn test_gettext_localization_ru() {
        use crate::config::{GETTEXT_PACKAGE, LOCALEDIR};
        use gettextrs::{
            bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory,
        };

        // 1. Очистка ограничивающих переменных и установка не-C локали libc для работы GNU gettext
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANGUAGE", "ru");

        let loc = setlocale(LocaleCategory::LcAll, "en_US.UTF-8")
            .or_else(|| setlocale(LocaleCategory::LcAll, "en_US.utf8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "ru_RU.UTF-8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "ru_RU.utf8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "C.UTF-8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, ""));

        // В минимальных средах/контейнерах без сгенерированных локалей (где активна только C/POSIX)
        // GNU gettext в glibc отключает перевод по LANGUAGE.
        let is_c_locale = loc.as_deref().is_none_or(|l| l == b"C" || l == b"POSIX");

        let configured_locale_dir = if std::path::Path::new(LOCALEDIR).is_relative() {
            std::env::current_dir()
                .map(|p| p.join(LOCALEDIR))
                .unwrap_or_else(|_| std::path::PathBuf::from(LOCALEDIR))
        } else {
            std::path::PathBuf::from(LOCALEDIR)
        };

        let candidate_dirs = [
            configured_locale_dir.clone(),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("../share/locale")))
                .unwrap_or_default(),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("locale")))
                .unwrap_or_default(),
            std::env::current_dir()
                .map(|p| p.join("locale"))
                .unwrap_or_default(),
        ];

        let mut locale_dir = configured_locale_dir;
        for dir in &candidate_dirs {
            if dir.exists()
                && (dir.join("ru/LC_MESSAGES/vrxx.mo").exists()
                    || dir.join("en/LC_MESSAGES/vrxx.mo").exists())
            {
                locale_dir = dir.clone();
                break;
            }
        }

        bindtextdomain(GETTEXT_PACKAGE, &locale_dir).expect("bindtextdomain завершился ошибкой");
        bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8")
            .expect("bind_textdomain_codeset завершился ошибкой");
        textdomain(GETTEXT_PACKAGE).expect("textdomain завершился ошибкой");

        if is_c_locale {
            eprintln!("Warning: environment lacks UTF-8 locales (active locale is C/POSIX). Skipping gettext assertions.");
            return;
        }

        let translated = gettextrs::gettext("General");
        assert_eq!(translated, "Общие");

        // Проверка локализации Секции 3 и новых компонентов
        assert_eq!(
            gettextrs::gettext("Custom Rules"),
            "Пользовательские правила"
        );
        assert_eq!(gettextrs::gettext("Add Rule"), "Добавить правило");
        assert_eq!(
            gettextrs::gettext("Unsaved Changes"),
            "Несохраненные изменения"
        );
        assert_eq!(
            gettextrs::gettext("Predefined Regional Rules"),
            "Предустановленные региональные правила"
        );
        assert_eq!(
            gettextrs::gettext("Direct Bypass (Direct Connection)"),
            "Прямой обход (Прямое подключение)"
        );
        assert_eq!(gettextrs::gettext("Latency Testing"), "Замер задержки");
        assert_eq!(
            gettextrs::gettext("Block QUIC (UDP 443)"),
            "Блокировать QUIC (UDP 443)"
        );
        assert_eq!(gettextrs::gettext("Update Geo Data"), "Обновление гео-баз");
        assert_eq!(
            gettextrs::gettext("Export Configuration"),
            "Экспорт конфигурации"
        );
        assert_eq!(
            gettextrs::gettext("Application Settings"),
            "Настройки приложения"
        );
        assert_eq!(
            gettextrs::gettext("VPN Profiles and Keys"),
            "VPN-профили и ключи"
        );
        assert_eq!(
            gettextrs::gettext("Concurrency"),
            "Параллельность (Concurrency)"
        );
        assert_eq!(
            gettextrs::gettext("Add VPN Profile"),
            "Добавить VPN-профиль"
        );
        assert_eq!(gettextrs::gettext("Add Profile"), "Добавить профиль");
        assert_eq!(
            gettextrs::gettext("Add and Connect"),
            "Добавить и подключиться"
        );
        assert_eq!(
            gettextrs::gettext("Quick Import from Link"),
            "Быстрый импорт по ссылке"
        );
        assert_eq!(gettextrs::gettext("_Ping"), "_Пинг");
    }

    /// Проверяет безопасное переключение системного прокси без создания экземпляра Tokio Runtime.
    #[test]
    fn test_update_system_proxy_static_call() {
        let res = crate::backend::CoreBackend::update_system_proxy(false);
        match res {
            crate::backend::SystemProxyResult::Success => (),
            crate::backend::SystemProxyResult::SchemaUnavailable { .. } => (),
            crate::backend::SystemProxyResult::Error(_) => (),
        }
    }

    /// Проверяет создание и реактивность модели RoutingRuleObject.
    #[test]
    fn test_routing_rule_object() {
        use crate::ui::models::RoutingRuleObject;

        let rule = RoutingRuleObject::new("Example Domain", "domain", "example.com", "direct");
        assert_eq!(rule.name(), "Example Domain");
        assert_eq!(rule.rule_type(), "domain");
        assert_eq!(rule.value(), "example.com");
        assert_eq!(rule.action(), "direct");

        rule.set_name("Telegram".to_string());
        rule.set_rule_type("ip".to_string());
        rule.set_value("91.108.56.0/22".to_string());
        rule.set_action("proxy".to_string());

        assert_eq!(rule.name(), "Telegram");
        assert_eq!(rule.rule_type(), "ip");
        assert_eq!(rule.value(), "91.108.56.0/22");
        assert_eq!(rule.action(), "proxy");
    }

    /// Проверяет путь к каталогу хранения гео-баз geodata.
    #[test]
    fn test_geo_updater_dir() {
        let dir = crate::services::geo_updater::get_geodata_dir();
        assert!(dir.to_string_lossy().contains("geodata"));
        assert!(dir.to_string_lossy().contains("vrxx"));

        let status = crate::services::geo_updater::get_geo_status();
        assert!(!status.is_empty());
    }

    /// Проверяет генерацию конфигурации sing-box с раздельными региональными правилами.
    #[test]
    fn test_singbox_config_split_regional_rules() {
        use crate::domain::key_parser::parse_vpn_key;
        use crate::domain::singbox_config::build_singbox_config;
        use crate::settings::{AppSettings, RoutingRule};

        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443?security=reality&pbk=pubkey&sid=sid&sni=google.com#TestVless").unwrap();

        let mut settings = AppSettings::new();
        settings.enable_routing = true;
        settings.route_ru_sites = true;
        settings.route_ru_ips = false;
        settings.route_cn_sites = false;
        settings.route_cn_ips = true;
        settings.route_antifilter = true;

        settings.routing_rules.push(RoutingRule {
            name: "My Custom Domain".to_string(),
            type_: "domain".to_string(),
            value: "habr.com".to_string(),
            action: "direct".to_string(),
        });

        let config_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&config_str).unwrap();

        let route_rules = val["route"]["rules"].as_array().unwrap();

        // Проверяем наличие правила geosite-ru (outbound: direct)
        assert!(route_rules.iter().any(|r| {
            r.get("rule_set")
                .and_then(|rs| rs.as_array())
                .map(|arr| arr.iter().any(|item| item == "geosite-ru"))
                .unwrap_or(false)
                && r.get("outbound").and_then(|o| o.as_str()) == Some("direct")
        }));

        // Проверяем, что geoip-ru ОТСУТСТВУЕТ (route_ru_ips = false)
        assert!(!route_rules.iter().any(|r| {
            r.get("rule_set")
                .and_then(|rs| rs.as_array())
                .map(|arr| arr.iter().any(|item| item == "geoip-ru"))
                .unwrap_or(false)
        }));

        // Проверяем наличие правила geoip-cn (outbound: direct)
        assert!(route_rules.iter().any(|r| {
            r.get("rule_set")
                .and_then(|rs| rs.as_array())
                .map(|arr| arr.iter().any(|item| item == "geoip-cn"))
                .unwrap_or(false)
                && r.get("outbound").and_then(|o| o.as_str()) == Some("direct")
        }));

        // Проверяем наличие правила geosite-antifilter (outbound: proxy)
        assert!(route_rules.iter().any(|r| {
            r.get("rule_set")
                .and_then(|rs| rs.as_array())
                .map(|arr| arr.iter().any(|item| item == "geosite-antifilter"))
                .unwrap_or(false)
                && r.get("outbound").and_then(|o| o.as_str()) == Some("proxy")
        }));

        // Проверяем пользовательское правило habr.com
        assert!(route_rules.iter().any(|r| {
            r.get("domain_suffix")
                .and_then(|ds| ds.as_array())
                .map(|arr| arr.iter().any(|item| item == "habr.com"))
                .unwrap_or(false)
                && r.get("outbound").and_then(|o| o.as_str()) == Some("direct")
        }));
    }

    /// Проверяет разбор и сбор данных в диалоге VrxxKeyDialog.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_key_dialog_data_flow() {
        run_on_gtk_thread(|| {
            use crate::domain::key_parser::parse_vpn_key;
            use crate::ui::key_dialog::{KeyDialogMode, VrxxKeyDialog};

            let dialog = VrxxKeyDialog::new();
            dialog.set_mode(KeyDialogMode::Add);

            let test_url = "vless://11111111-2222-3333-4444-555555555555@example.com:443?security=reality&sni=test.com&pbk=pubkey123#MyProfile";
            let parsed = parse_vpn_key(test_url).unwrap();
            dialog.populate_from_parsed(&parsed);

            let collected = dialog.collect_parsed_key().unwrap();
            assert_eq!(collected.protocol, "VLESS");
            assert_eq!(collected.name, "MyProfile");
            assert_eq!(collected.host, "example.com");
            assert_eq!(collected.port, 443);
            assert_eq!(collected.uuid, "11111111-2222-3333-4444-555555555555");
            assert_eq!(
                collected.query_params.get("security").map(|s| s.as_str()),
                Some("reality")
            );
            assert_eq!(
                collected.query_params.get("sni").map(|s| s.as_str()),
                Some("test.com")
            );
            assert_eq!(
                collected.query_params.get("pbk").map(|s| s.as_str()),
                Some("pubkey123")
            );
        });
    }

    /// Проверяет разбор, установку и валидацию данных в диалоге VrxxRuleDialog.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_rule_dialog_data_flow() {
        run_on_gtk_thread(|| {
            use crate::ui::rule_dialog::{RuleDialogMode, VrxxRuleDialog};

            let dialog = VrxxRuleDialog::new();
            dialog.set_mode(RuleDialogMode::Add);

            dialog.set_rule("Block Tracker", "domain", "ads.example.com", "block");
            assert_eq!(dialog.name(), "Block Tracker");
            assert_eq!(dialog.rule_type(), "domain");
            assert_eq!(dialog.value(), "ads.example.com");
            assert_eq!(dialog.action(), "block");
            assert!(dialog.validate_input().is_ok());

            dialog.set_rule("Local Net", "ip", "192.168.1.0/24", "direct");
            assert_eq!(dialog.rule_type(), "ip");
            assert_eq!(dialog.value(), "192.168.1.0/24");
            assert_eq!(dialog.action(), "direct");
            assert!(dialog.validate_input().is_ok());
        });
    }

    /// Проверяет отсутствие предупреждений Libadwaita о минимальных размерах,
    /// корректные замеры measure() всех компонентов, диалогов и окон, а также
    /// переключение адаптивных точек останова (AdwBreakpoint) при изменении размеров.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_responsive_sizing_and_breakpoints() {
        run_on_gtk_thread(|| {
            use adw::subclass::prelude::*;
            use gtk::prelude::*;

            let temp_config_dir = tempfile::tempdir().unwrap();
            let orig_xdg = std::env::var("XDG_CONFIG_HOME").ok();
            std::env::set_var("XDG_CONFIG_HOME", temp_config_dir.path());

            // 1. Проверка страницы VrxxVpnPage (AdwBreakpointBin)
            let vpn_page = crate::ui::pages::VrxxVpnPage::new();
            assert_eq!(vpn_page.width_request(), 360);
            assert_eq!(vpn_page.height_request(), 200);

            // Проверка измерений GTK measure: не должны вызывать предупреждений и должны соответствовать запросу
            let (min_w, nat_w, _, _) = vpn_page.measure(gtk::Orientation::Horizontal, -1);
            assert!(min_w >= 360, "min_w was {min_w}, expected >= 360");
            assert!(nat_w >= min_w);

            let (min_h, nat_h, _, _) = vpn_page.measure(gtk::Orientation::Vertical, -1);
            assert!(min_h >= 200, "min_h was {min_h}, expected >= 200");
            assert!(nat_h >= min_h);

            // Исходное состояние: десктопный вид (кнопки на панели видны, меню скрыто)
            assert!(vpn_page.imp().header_actions_box.is_visible());
            assert!(!vpn_page.imp().btn_add_compact_menu.is_visible());

            // Проверка пустого состояния (TASK-UI-04): при отсутствии подключений отображается страница "empty"
            assert_eq!(
                vpn_page.imp().main_stack.visible_child_name().as_deref(),
                Some("empty"),
                "При отсутствии ключей/групп main_stack должен показывать 'empty'"
            );

            // Тестирование адаптивной точки останова AdwBreakpoint (max-width: 720sp)
            // При сжатии до <= 720px: кнопки скрываются, компактное меню отображается
            vpn_page.allocate(400, 300, -1, None);
            assert!(
                !vpn_page.imp().header_actions_box.is_visible(),
                "header_actions_box should be hidden at <= 720sp (400px)"
            );
            assert!(
                vpn_page.imp().btn_add_compact_menu.is_visible(),
                "btn_add_compact_menu should be visible at <= 720sp (400px)"
            );

            vpn_page.allocate(720, 300, -1, None);
            assert!(
                !vpn_page.imp().header_actions_box.is_visible(),
                "header_actions_box should be hidden at <= 720sp (720px)"
            );
            assert!(
                vpn_page.imp().btn_add_compact_menu.is_visible(),
                "btn_add_compact_menu should be visible at <= 720sp (720px)"
            );

            // При ширине контента > 720sp (например, 800px):
            // кнопки панели раскрываются, компактное меню скрыто
            vpn_page.allocate(800, 300, -1, None);
            assert!(
                vpn_page.imp().header_actions_box.is_visible(),
                "header_actions_box should be visible at > 720sp (800px)"
            );
            assert!(
                !vpn_page.imp().btn_add_compact_menu.is_visible(),
                "btn_add_compact_menu should be hidden at > 720sp (800px)"
            );

            // 2. Проверка остальных контентных страниц GtkStack
            let proxy_page = crate::ui::pages::VrxxProxyPage::new();
            assert_eq!(proxy_page.width_request(), 360);
            assert_eq!(proxy_page.height_request(), 200);
            let (p_min_w, _, _, _) = proxy_page.measure(gtk::Orientation::Horizontal, -1);
            assert!(p_min_w >= 360);
            let (p_min_h, _, _, _) = proxy_page.measure(gtk::Orientation::Vertical, -1);
            assert!(p_min_h >= 200);

            let routing_page = crate::ui::pages::VrxxRoutingPage::new();
            assert_eq!(routing_page.width_request(), 360);
            assert_eq!(routing_page.height_request(), 200);
            let (r_min_w, _, _, _) = routing_page.measure(gtk::Orientation::Horizontal, -1);
            assert!(r_min_w >= 360);
            let (r_min_h, _, _, _) = routing_page.measure(gtk::Orientation::Vertical, -1);
            assert!(r_min_h >= 200);

            let settings_page = crate::ui::pages::VrxxSettingsPage::new();
            assert_eq!(settings_page.width_request(), 360);
            assert_eq!(settings_page.height_request(), 200);
            let (s_min_w, _, _, _) = settings_page.measure(gtk::Orientation::Horizontal, -1);
            assert!(s_min_w >= 360);
            let (s_min_h, _, _, _) = settings_page.measure(gtk::Orientation::Vertical, -1);
            assert!(s_min_h >= 200);

            // 3. Проверка всех модальных диалогов AdwDialog и окна логов AdwWindow
            let key_dialog = crate::ui::key_dialog::VrxxKeyDialog::new();
            assert_eq!(key_dialog.width_request(), 360);
            assert_eq!(key_dialog.height_request(), 360);
            let (kd_min_w, _, _, _) = key_dialog.measure(gtk::Orientation::Horizontal, -1);
            assert!(kd_min_w >= 360);
            let (kd_min_h, _, _, _) = key_dialog.measure(gtk::Orientation::Vertical, -1);
            assert!(kd_min_h >= 360);

            let rule_dialog = crate::ui::rule_dialog::VrxxRuleDialog::new();
            assert_eq!(rule_dialog.width_request(), 360);
            assert_eq!(rule_dialog.height_request(), 360);
            let (rd_min_w, _, _, _) = rule_dialog.measure(gtk::Orientation::Horizontal, -1);
            assert!(rd_min_w >= 360);
            let (rd_min_h, _, _, _) = rule_dialog.measure(gtk::Orientation::Vertical, -1);
            assert!(rd_min_h >= 360);

            let qr_dialog = crate::ui::qr_dialog::VrxxQrDialog::new();
            assert_eq!(qr_dialog.width_request(), 360);
            assert_eq!(qr_dialog.height_request(), 360);
            let (qd_min_w, _, _, _) = qr_dialog.measure(gtk::Orientation::Horizontal, -1);
            assert!(qd_min_w >= 360);
            let (qd_min_h, _, _, _) = qr_dialog.measure(gtk::Orientation::Vertical, -1);
            assert!(qd_min_h >= 360);

            let import_dialog = crate::ui::import_dialog::VrxxImportDialog::new();
            assert_eq!(import_dialog.width_request(), 360);
            assert_eq!(import_dialog.height_request(), 360);
            let (id_min_w, _, _, _) = import_dialog.measure(gtk::Orientation::Horizontal, -1);
            assert!(id_min_w >= 360);
            let (id_min_h, _, _, _) = import_dialog.measure(gtk::Orientation::Vertical, -1);
            assert!(id_min_h >= 360);

            let export_dialog = crate::ui::export_dialog::VrxxExportDialog::new();
            assert_eq!(export_dialog.width_request(), 360);
            assert_eq!(export_dialog.height_request(), 300);
            let (ed_min_w, _, _, _) = export_dialog.measure(gtk::Orientation::Horizontal, -1);
            assert!(ed_min_w >= 360);
            let (ed_min_h, _, _, _) = export_dialog.measure(gtk::Orientation::Vertical, -1);
            assert!(ed_min_h >= 300);

            let log_window = crate::ui::components::log_window::VrxxLogWindow::new();
            assert_eq!(log_window.width_request(), 360);
            assert_eq!(log_window.height_request(), 320);
            let (lw_min_w, _, _, _) = log_window.measure(gtk::Orientation::Horizontal, -1);
            assert!(lw_min_w >= 360);
            let (lw_min_h, _, _, _) = log_window.measure(gtk::Orientation::Vertical, -1);
            assert!(lw_min_h >= 320);

            // 4. Проверка главного окна VrxxWindow и симуляция изменения размеров
            let app_id = format!("ru.mark.vrxx.test.proc_{}", std::process::id());
            let app = adw::Application::builder()
                .application_id(&app_id)
                .flags(gio::ApplicationFlags::NON_UNIQUE)
                .build();
            let _ = app.register(None as Option<&gio::Cancellable>);
            let window = crate::window::VrxxWindow::new(&app);

            assert_eq!(window.width_request(), 360);
            assert_eq!(window.height_request(), 500);

            // Симулируем замеры окна от десктопного (1200px) до мобильного (360px)
            for &width in &[1200, 960, 720, 600, 500, 360, 480, 720, 1000] {
                window.set_default_size(width, 600);
                let (win_min_w, _, _, _) = window.measure(gtk::Orientation::Horizontal, -1);
                assert!(
                    win_min_w >= 360,
                    "Window min_w ({win_min_w}) should be >= 360"
                );
                let (win_min_h, _, _, _) = window.measure(gtk::Orientation::Vertical, -1);
                assert!(
                    win_min_h >= 500,
                    "Window min_h ({win_min_h}) should be >= 500"
                );
            }

            // Проверка точки останова окна (max-width: 720sp -> split_view collapsed)
            // При граничной ширине 720px (<= 720sp) split_view сворачивает боковую панель
            window.allocate(720, 500, -1, None);
            assert!(
                window.imp().split_view.is_collapsed(),
                "split_view should be collapsed at <= 720sp (720px)"
            );

            // При мобильной ширине 600px (<= 720sp) split_view также свернут
            window.allocate(600, 500, -1, None);
            assert!(
                window.imp().split_view.is_collapsed(),
                "split_view should be collapsed at <= 720sp (600px)"
            );

            // При граничной ширине 721px (> 720sp) split_view раскрывает боковую панель
            window.allocate(721, 500, -1, None);
            assert!(
                !window.imp().split_view.is_collapsed(),
                "split_view should be uncollapsed at > 720sp (721px)"
            );

            // При десктопной ширине 960px (> 720sp) split_view раскрывает боковую панель
            window.allocate(960, 650, -1, None);
            assert!(
                !window.imp().split_view.is_collapsed(),
                "split_view should be uncollapsed at > 720sp"
            );

            if let Some(orig) = orig_xdg {
                std::env::set_var("XDG_CONFIG_HOME", orig);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        });
    }
}
