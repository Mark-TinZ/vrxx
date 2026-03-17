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
mod key_parser;
mod xray_config;
mod singbox_config;

use self::application::VrxxApplication;
use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, setlocale, LocaleCategory};
use gtk::{gio, glib};
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    // Override language if set in settings
    let manager = settings::SettingsManager::new();
    let app_settings = manager.load();
    if app_settings.language != "system" {
        let lang_to_set = if app_settings.language == "en" { "C.UTF-8" } else { &app_settings.language };
        std::env::set_var("LANGUAGE", lang_to_set);
        std::env::set_var("LANG", lang_to_set);
        std::env::set_var("LC_ALL", lang_to_set);
    }
    
    crate::backend::log_app_event("info", "Vrxx Application Started");

    // Set up gettext translations
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8")
        .expect("Unable to set the text domain encoding");
    textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    // Load resources compiled into the binary
    let res_data = include_bytes!(concat!(env!("OUT_DIR"), "/vrxx.gresource"));
    let res = gio::Resource::from_data(&glib::Bytes::from(res_data))
        .expect("Failed to load compiled resources");
    gio::resources_register(&res);

    // Create a new GtkApplication. The application manages our main loop,
    // application windows, integration with the window manager/compositor, and
    // desktop features such as file opening and single-instance applications.
    let app = VrxxApplication::new("ru.mark.vrxx", &gio::ApplicationFlags::empty());

    // Run the application. This function will block until the application
    // exits. Upon return, we have our exit code to return to the shell. (This
    // is the code you see when you do `echo $?` after running a command in a
    // terminal.
    app.run()
}

