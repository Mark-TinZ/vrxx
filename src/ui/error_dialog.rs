/* error_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Диалог отображения ошибок и диагностики (Error Dialog)
//!
//! Отвечает за:
//! - Форматирование системных ошибок ядра и демона в понятные человеку сообщения (`format_human_error`)
//! - Показ нативного диалога `AdwAlertDialog` с возможностью быстрого копирования технического стектрейса/лога

use adw::prelude::*;
use gettextrs::gettext;

/// Отображает модальный диалог `AdwAlertDialog` при критических ошибках с кнопкой «Скопировать лог».
pub fn show_error_dialog<P: IsA<gtk::Widget>>(
    parent: Option<&P>,
    title: Option<&str>,
    user_message: &str,
    technical_log: &str,
) {
    let default_heading = gettext("Failed to connect to VPN");
    let heading = title.unwrap_or(&default_heading);

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(user_message)
        .build();

    dialog.add_response("copy_log", &gettext("Copy technical log"));
    dialog.add_response("close", &gettext("Close"));
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");

    let log_content = technical_log.to_string();
    dialog.connect_response(Some("copy_log"), move |_, _| {
        if let Some(display) = gdk::Display::default() {
            let clipboard = display.clipboard();
            clipboard.set_text(&log_content);
            tracing::info!("Technical log copied to clipboard.");
        }
    });

    if let Some(parent_widget) = parent {
        dialog.present(Some(parent_widget));
    } else if let Some(app) = gtk::gio::Application::default() {
        if let Some(window) = app
            .downcast_ref::<gtk::Application>()
            .and_then(|a| a.active_window())
        {
            dialog.present(Some(&window));
        }
    }
}

/// Преобразует сырую ошибку бэкенда/сети в понятное пользователю сообщение с поддержкой локализации (i18n).
pub fn format_human_error(raw_err: &str) -> String {
    let lower = raw_err.to_lowercase();
    if lower.contains("core not found") || lower.contains("sing-box") {
        gettext("The sing-box core was not found on your system. Please install or update it in Settings.")
    } else if lower.contains("permission denied") || lower.contains("operation not permitted") {
        gettext("Access denied when configuring network interface (TUN) or routing rules.")
    } else if lower.contains("connection refused") || lower.contains("daemon") {
        gettext("Failed to connect to vrxx-daemon. Please verify daemon status.")
    } else {
        format!(
            "{}: {}",
            gettext("An error occurred during operation"),
            raw_err
        )
    }
}
