//! # Точка входа в приложение VRXX
//!
//! Отвечает за:
//! - Разбор аргументов командной строки (`--daemon`, `--tui`, GUI по умолчанию)
//! - Ротацию и настройку промышленной подсистемы логирования `tracing` (`~/.local/share/vrxx/logs/`)
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
pub mod logger;
pub mod protocol;
pub mod services;
pub mod settings;
pub mod tui;

mod ui;
mod window;

use self::application::VrxxApplication;
use clap::{Parser, Subcommand};
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

fn main() -> glib::ExitCode {
    // Разбор аргументов командной строки
    let cli = Cli::parse();

    let is_daemon = cli.daemon || matches!(cli.command, Some(Commands::Daemon));
    let is_tui = cli.tui || matches!(cli.command, Some(Commands::Tui));

    // Загрузка настроек приложения для конфигурации локали и уровня логирования
    let manager = settings::SettingsManager::new();
    let app_settings = manager.load();

    let logger_config = logger::LoggerConfig {
        is_tui,
        is_daemon,
        initial_level: app_settings.log_level.clone(),
    };

    if is_tui {
        // Инициализация Tracing для TUI: вывод направляется в файл, а не в stdout/stderr терминала
        let _guard = logger::init_gui_logging(logger_config);

        // Инициализация подсистемы интернационализации (Gettext) для TUI
        config::init_i18n(&app_settings.language);

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

        let _guard = logger::init_daemon_logging(logger_config, sse_layer);

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
    }

    // Инициализация логирования для основного графического интерфейса
    let _guard = logger::init_gui_logging(logger_config);

    // Фоновая проверка обновления баз GeoIP и GeoSite
    crate::services::geo_updater::spawn_background_updater();

    // Инициализация подсистемы интернационализации (Gettext) и системной локали
    gtk::glib::set_application_name("Vrxx");
    config::init_i18n(&app_settings.language);

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
