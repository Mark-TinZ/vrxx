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
mod ui;
mod window;
pub mod utils;

use self::application::VrxxApplication;
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};
use gtk::prelude::*;
use gtk::{gio, glib};

struct MultiWriter {
    app_log: std::fs::File,
    all_log: std::fs::File,
}
impl std::io::Write for MultiWriter {
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

    // --- Раздел: Логирование ---
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vrxx")
        .join("logs");
    crate::utils::secure_create_dir_all(&log_dir).ok();

    let is_daemon = args.iter().any(|arg| arg == "--daemon");
    let log_suffix = if is_daemon { "daemon" } else { "app" };

    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut log_opts = std::fs::OpenOptions::new();
    log_opts.create(true).append(true);
    #[cfg(unix)]
    log_opts.mode(0o600);

    let log_file = match log_opts.open(log_dir.join(format!("{}.log", log_suffix))) {
        Ok(file) => {
            #[cfg(unix)]
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            file
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to open {}.log: {}. Using /dev/null",
                log_suffix, e
            );
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .unwrap_or_else(|_| std::process::exit(1))
        }
    };

    let mut all_log_opts = std::fs::OpenOptions::new();
    all_log_opts.create(true).append(true);
    #[cfg(unix)]
    all_log_opts.mode(0o600);

    let all_log_file = match all_log_opts.open(log_dir.join("all.log")) {
        Ok(file) => {
            #[cfg(unix)]
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            file
        }
        Err(e) => {
            eprintln!("Warning: Failed to open all.log: {}. Using /dev/null", e);
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .unwrap_or_else(|_| std::process::exit(1))
        }
    };

    let multi_writer = MultiWriter {
        app_log: log_file,
        all_log: all_log_file,
    };

    let (non_blocking, _guard) = tracing_appender::non_blocking(multi_writer);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    // ================================

    if is_daemon {
        match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                if let Err(e) = rt.block_on(daemon::run()) {
                    tracing::error!("Daemon failed: {e}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Failed to initialize tokio runtime: {e}");
                std::process::exit(1);
            }
        }
    }

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
    if let Err(e) = bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR) {
        eprintln!("Unable to bind the text domain: {}", e);
    }
    if let Err(e) = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8") {
        eprintln!("Unable to set the text domain encoding: {}", e);
    }
    if let Err(e) = textdomain(GETTEXT_PACKAGE) {
        eprintln!("Unable to switch to the text domain: {}", e);
    }

    // Загружаем ресурсы
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    if let Ok(res) = gio::Resource::from_data(&glib::Bytes::from(res_data)) {
        gio::resources_register(&res);
    } else {
        eprintln!("Failed to load compiled resources");
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
