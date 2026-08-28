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

#[cfg(test)]
mod ui_testing {
    use gtk::{gio, glib};

    /// Вспомогательная функция инициализации GTK и ресурсов GResource для тестов.
    fn init_gtk() -> bool {
        if gtk::init().is_err() || adw::init().is_err() {
            return false;
        }

        // Загрузка скомпилированных ресурсов
        let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
        if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
            gio::resources_register(&res);
        }
        true
    }

    /// Проверяет, что все составные шаблоны GTK4/Libadwaita инициализируются без сбоев.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_ui_components_init() {
        if !init_gtk() {
            return;
        }

        let _log_window = crate::ui::components::log_window::VrxxLogWindow::new();
        let _settings_page = crate::ui::pages::VrxxSettingsPage::new();
        let _vpn_page = crate::ui::pages::VrxxVpnPage::new();
        let _routing_page = crate::ui::pages::VrxxRoutingPage::new();
        let _proxy_page = crate::ui::pages::VrxxProxyPage::new();
        let _qr_dialog = crate::ui::qr_dialog::VrxxQrDialog::new();
        let _rule_dialog = crate::ui::rule_dialog::VrxxRuleDialog::new();
    }

    /// Проверяет работу фильтрации категорий в окне логов.
    #[test]
    #[ignore = "Требует главного потока GTK для инициализации"]
    fn test_log_filtering_integration() {
        use adw::subclass::prelude::*;
        use gtk::prelude::*;
        if !init_gtk() {
            return;
        }
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
        log_window.append_log("info", "Random core message");

        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false);
        assert!(text.contains("[Vrxx] Important event"));
        assert!(!text.contains("Random core message"));
    }

    /// Проверяет корректность работы интернационализации (gettext) для русского и английского языков.
    #[test]
    fn test_gettext_localization_ru() {
        use crate::config::{GETTEXT_PACKAGE, LOCALEDIR};
        use gettextrs::{
            bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory,
        };

        // 1. Инициализация локали libc с безопасным fallback на UTF-8
        let loc = setlocale(LocaleCategory::LcAll, "");
        if loc.as_deref().is_none_or(|l| l == b"C" || l == b"POSIX")
            && setlocale(LocaleCategory::LcAll, "C.UTF-8").is_none()
            && setlocale(LocaleCategory::LcAll, "C.utf8").is_none()
        {
            let _ = setlocale(LocaleCategory::LcAll, "en_US.UTF-8");
        }

        // 2. Установка русского языка через LANGUAGE
        std::env::set_var("LANGUAGE", "ru");

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
}
