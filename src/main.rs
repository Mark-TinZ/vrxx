/* main.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Точка входа в приложение VRXX
//!
//! Отвечает за:
//! - Разбор аргументов командной строки (`--daemon`, `--tui`, GUI по умолчанию)
//! - Ротацию и настройку подсистемы логирования `tracing` (`~/.local/share/vrxx/logs/`)
//! - Инициализацию подсистемы интернационализации `gettext` и локалей
//! - Регистрацию скомпилированных ресурсов GResource (`vrxx.gresource`)
//! - Перенаправление системных логов GLib/GTK в `tracing`

mod application;
mod backend;
mod config;
pub mod crypto;
pub mod daemon;
pub mod domain;
pub mod ipc;
mod protocol;
pub mod services;
pub mod settings;
pub mod tui;

mod ui;
mod window;

use self::application::VrxxApplication;
use clap::{Parser, Subcommand};
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};
use gtk::prelude::*;
use gtk::{gio, glib};

/// Структура аргументов командной строки приложения VRXX.
#[derive(Parser, Debug)]
#[command(author, version, about = "VRXX - Клиент управления VPN и сетевыми прокси", long_about = None)]
struct Cli {
    /// Запуск в режиме привилегированного фонового демона
    #[arg(long)]
    daemon: bool,

    /// Запуск в режиме консольного терминального интерфейса (TUI)
    #[arg(long)]
    tui: bool,

    /// Опциональная подкоманда (tui, daemon)
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Запуск в режиме консольного терминального интерфейса (TUI)
    Tui,
    /// Запуск в режиме привилегированного фонового демона
    Daemon,
}

/// Очищает файлы логов старше 3 дней для предотвращения раздувания дискового пространства.
fn cleanup_old_logs(log_dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        let max_age = std::time::Duration::from_secs(3 * 24 * 3600); // 3 дня
        let now = std::time::SystemTime::now();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(age) = now.duration_since(modified) {
                            if age > max_age {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn main() -> glib::ExitCode {
    // Разбор аргументов командной строки
    let cli = Cli::parse();

    let is_daemon = cli.daemon || matches!(cli.command, Some(Commands::Daemon));
    let is_tui = cli.tui || matches!(cli.command, Some(Commands::Tui));

    // Настройка каталога и ротации логов в ~/.local/share/vrxx/logs/
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        })
        .join("vrxx")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    cleanup_old_logs(&log_dir);

    let log_prefix = if is_tui {
        "tui.log"
    } else if is_daemon {
        "daemon.log"
    } else {
        "app.log"
    };

    let log_file = tracing_appender::rolling::daily(&log_dir, log_prefix);
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);

    // Архитектурная настройка слоев TracingSubscriber
    use tracing_subscriber::prelude::*;
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    if is_tui {
        // Инициализация Tracing для TUI: вывод направляется в файл, а не в stdout/stderr терминала
        tracing_subscriber::registry().with(fmt_layer).init();

        let rt = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(e) => {
                tracing::error!("Failed to create Tokio Runtime for TUI: {e}");
                std::process::exit(1);
            }
        };
        if let Err(e) = rt.block_on(tui::run_tui()) {
            tracing::error!("TUI execution error: {e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    if is_daemon {
        // Инициализация для системного демона: прикрепление SSE-слоя для трансляции событий в GUI
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
                    tracing::error!("Fatal daemon termination: {e}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!("Failed to create Tokio Runtime for system daemon: {e}");
                std::process::exit(1);
            }
        }
    } else {
        tracing_subscriber::registry().with(fmt_layer).init();
    }

    // Фоновая проверка обновления баз GeoIP и GeoSite
    crate::services::geo_updater::spawn_background_updater();

    // Инициализация подсистемы интернационализации (Gettext) и системной локали
    // 1. Инициализируем локаль libc
    let mut loc = setlocale(LocaleCategory::LcAll, "");

    // Если локаль не была установлена (None) или сбросилась в "C" / "POSIX" / "C.UTF-8",
    // gettext в glibc отключает перевод по переменной LANGUAGE. Переключаем процесс на доступную UTF-8 локаль:
    if loc
        .as_deref()
        .is_none_or(|l| l == b"C" || l == b"POSIX" || l == b"C.UTF-8" || l == b"C.utf8")
    {
        loc = setlocale(LocaleCategory::LcAll, "en_US.UTF-8")
            .or_else(|| setlocale(LocaleCategory::LcAll, "en_US.utf8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "ru_RU.UTF-8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "ru_RU.utf8"))
            .or_else(|| setlocale(LocaleCategory::LcAll, "C.UTF-8"));
    }

    // 2. Установка языка приложения (LANGUAGE)
    let manager = settings::SettingsManager::new();
    let app_settings = manager.load();

    match app_settings.language.as_str() {
        "ru" => {
            std::env::set_var("LANGUAGE", "ru");
        }
        "en" => {
            std::env::set_var("LANGUAGE", "en");
        }
        _ => {
            // "system": если в системном LANG указан русский (ru), но в системе нет сгенерированной ru_RU локали
            // и LANGUAGE не был задан явно, активируем LANGUAGE=ru
            if let Ok(sys_lang) = std::env::var("LANG") {
                if sys_lang.starts_with("ru") && std::env::var("LANGUAGE").is_err() {
                    std::env::set_var("LANGUAGE", "ru");
                }
            }
        }
    }

    tracing::info!(
        "VRXX application started (locale: {:?}, language: {})",
        loc.as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned()),
        app_settings.language
    );

    // 3. Каскадный поиск каталога файлов локализации (.mo)
    gtk::glib::set_application_name("Vrxx");

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

    if let Err(e) = bindtextdomain(GETTEXT_PACKAGE, &locale_dir) {
        tracing::warn!("Failed to bind gettext localization domain: {}", e);
    }
    if let Err(e) = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8") {
        tracing::warn!("Failed to set gettext domain codeset: {}", e);
    }
    if let Err(e) = textdomain(GETTEXT_PACKAGE) {
        tracing::warn!("Failed to switch gettext domain: {}", e);
    }

    // Загрузка скомпилированных ресурсов GResource
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
        gio::resources_register(&res);
    } else {
        tracing::error!("Failed to load compiled GResource bundle");
    }

    // Перехват системных логов GLib/GTK и перенаправление в tracing
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

    let rt = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::error!("Failed to create Tokio Runtime for GUI: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    // Запуск графического приложения GTK4
    let app = VrxxApplication::new("ru.mark.vrxx", &gio::ApplicationFlags::empty());
    app.run()
}
