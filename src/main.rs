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
mod config;
mod window;
mod ui;
mod backend;
mod settings;
mod protocol;
pub mod domain;
pub mod services;

use self::application::VrxxApplication;
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, setlocale, LocaleCategory};
use gtk::{gio, glib};
use gtk::prelude::*;


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
    crate::services::geo_updater::spawn_background_updater();

    // Устанавливаем язык ДО любой инициализации GTK и gettext
    let manager = settings::SettingsManager::new();
    let app_settings = manager.load();
    if app_settings.language != "system" {
        let lang = if app_settings.language == "ru" { "ru_RU.UTF-8" } else { &app_settings.language };
        std::env::set_var("LANGUAGE", lang);
        std::env::set_var("LC_ALL", lang);
        std::env::set_var("LANG", lang);
        std::env::set_var("LC_MESSAGES", lang);
    }
    
    let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx").join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    
    // Пишем логи приложения в отдельный файл app.log и all.log
    let app_log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("app.log"))
        .unwrap_or_else(|_| std::fs::File::create("app.log").unwrap());

    let all_log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("all.log"))
        .unwrap_or_else(|_| std::fs::File::create("all.log").unwrap());

    let multi_writer = MultiWriter {
        app_log: app_log_file,
        all_log: all_log_file,
    };

    let (non_blocking, _guard) = tracing_appender::non_blocking(multi_writer);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tracing::info!("Vrxx Application Started");

    // Инициализируем Gettext
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8").expect("Unable to set the text domain encoding");
    textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    // Загружаем ресурсы
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    let res = gio::Resource::from_data(&glib::Bytes::from(res_data))
        .expect("Failed to load compiled resources");
    gio::resources_register(&res);

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

    // Запускаем приложение
    let app = VrxxApplication::new("ru.mark.vrxx", &gio::ApplicationFlags::empty());
    app.run()
}

