/* main.rs
 *
 * Copyright 2026 Unknown
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

mod application;
mod backend;
mod config;
pub mod daemon;
pub mod domain;
pub mod ipc;
mod protocol;
pub mod services;
mod settings;
pub mod tui;

mod ui;
mod window;

use self::application::VrxxApplication;
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};
use gtk::prelude::*;
use gtk::{gio, glib};

struct MultiWriter<W1: std::io::Write, W2: std::io::Write> {
    app_log: W1,
    all_log: W2,
}
impl<W1: std::io::Write, W2: std::io::Write> std::io::Write for MultiWriter<W1, W2> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.app_log.write_all(buf);
        let _ = self.all_log.write_all(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.app_log.flush();
        let _ = self.all_log.flush();
        Ok(())
    }
}

fn main() -> glib::ExitCode {
    // Точка входа в приложение
    let args: Vec<String> = std::env::args().collect();

    // --- Раздел: Логирование с авторотацией в ~/.local/share/vrxx/logs/ ---
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        })
        .join("vrxx")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let is_daemon = args.iter().any(|arg| arg == "--daemon");
    let is_tui = args.iter().any(|arg| arg == "tui");

    if is_tui {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        if let Err(e) = rt.block_on(tui::run_tui()) {
            tracing::error!("Error running TUI: {e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    let log_prefix = if is_daemon { "daemon.log" } else { "app.log" };

    let log_file = tracing_appender::rolling::daily(&log_dir, log_prefix);
    let all_log_file = tracing_appender::rolling::daily(&log_dir, "all.log");

    let multi_writer = MultiWriter {
        app_log: log_file,
        all_log: all_log_file,
    };

    let (non_blocking, _guard) = tracing_appender::non_blocking(multi_writer);

    // --- Раздел: Архитектурное логирование (Tracing Layers) ---
    use tracing_subscriber::prelude::*;
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    if is_daemon {
        // Для демона создаем менеджер событий заранее, чтобы подключить его к tracing
        let (event_manager, _) = daemon::events::EventManager::new(100);
        let event_manager = std::sync::Arc::new(event_manager);
        let sse_layer = daemon::events::SseTracingLayer::new(event_manager.clone());

        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(sse_layer)
            .init();

        match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                if let Err(e) = rt.block_on(daemon::run_with_manager(event_manager)) {
                    tracing::error!("Daemon failed: {e}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!("Failed to initialize tokio runtime: {e}");
                std::process::exit(1);
            }
        }
    } else {
        tracing_subscriber::registry().with(fmt_layer).init();
    }
    // ================================

    crate::services::geo_updater::spawn_background_updater();

    // Устанавливаем язык ДО любой инициализации GTK и gettext
    let manager = settings::SettingsManager::new();
    let app_settings = manager.load();
    if app_settings.language != "system" {
        let lang = if app_settings.language == "ru" {
            "ru_RU.UTF-8"
        } else {
            &app_settings.language
        };
        std::env::set_var("LANGUAGE", lang);
        std::env::set_var("LC_ALL", lang);
        std::env::set_var("LANG", lang);
        std::env::set_var("LC_MESSAGES", lang);
    }

    tracing::info!("Vrxx Application Started");

    // Инициализируем Gettext
    setlocale(LocaleCategory::LcAll, "");

    // --- Раздел: Локализация ---
    // Указываем GTK/GLib использовать локализацию и настраиваем gettext
    gtk::glib::set_application_name("Vrxx");

    if let Err(e) = bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR) {
        tracing::warn!("Unable to bind the text domain: {}", e);
    }
    if let Err(e) = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8") {
        tracing::warn!("Unable to set the text domain encoding: {}", e);
    }
    if let Err(e) = textdomain(GETTEXT_PACKAGE) {
        tracing::warn!("Unable to switch to the text domain: {}", e);
    }

    // Загружаем ресурсы
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
        gio::resources_register(&res);
    } else {
        tracing::error!("Failed to load compiled resources");
    }

    // Устанавливаем перехватчик логов GLib/GTK
    glib::log_set_writer_func(move |log_level, log_fields| {
        let mut message = String::new();
        let mut domain = String::new();

        for field in log_fields {
            let key = field.key();
            if let Some(val_bytes) = field.value_bytes() {
                if key == "MESSAGE" {
                    message = String::from_utf8_lossy(val_bytes).to_string();
                } else if key == "GLIB_DOMAIN" {
                    domain = String::from_utf8_lossy(val_bytes).to_string();
                }
            }
        }

        let log_msg = format!("[{domain}] {message}");
        match log_level {
            glib::LogLevel::Error | glib::LogLevel::Critical => tracing::error!("{}", log_msg),
            glib::LogLevel::Warning => tracing::warn!("{}", log_msg),
            glib::LogLevel::Message | glib::LogLevel::Info => tracing::info!("{}", log_msg),
            glib::LogLevel::Debug => tracing::debug!("{}", log_msg),
        }
        glib::LogWriterOutput::Handled
    });

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    // Запускаем приложение
    let app = VrxxApplication::new("ru.mark.vrxx", &gio::ApplicationFlags::empty());
    app.run()
}
