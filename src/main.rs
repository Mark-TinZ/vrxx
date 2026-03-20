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

use self::application::VrxxApplication;
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, setlocale, LocaleCategory};
use gtk::{gio, glib};
use gtk::prelude::*;

fn main() -> glib::ExitCode {
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
    
    // Пишем логи приложения в отдельный файл app.log
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("app.log"))
        .unwrap_or_else(|_| std::fs::File::create("app.log").unwrap());

    // Fallback to tracing log printing to stdout during dev
    tracing_subscriber::fmt()
        .with_writer(log_file)
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

    // Запускаем приложение
    let app = VrxxApplication::new("ru.mark.vrxx", &gio::ApplicationFlags::empty());
    app.run()
}

