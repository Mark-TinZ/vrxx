/* change_tracker.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Менеджер отслеживания несохраненных изменений и навигационный страж
//!
//! Отвечает за:
//! - Отслеживание состояния изменений страниц настроек
//! - Предотвращение случайной потери настроек при переключении страниц (Navigation Guard)
//! - Отображение унифицированного диалога подтверждения/отката изменений
//! - Отправку сигнала перезапуска ядра VPN при сохранении параметров

use adw::prelude::*;
use gtk::glib;

/// Отправляет сигнал на перезапуск сетевого ядра sing-box (если соединение активно) и выводит всплывающий тост.
pub fn apply_and_restart_core(toast_msg: &str, window: Option<&gtk::Window>) {
    // Отправка сигнала в канал перезапуска
    let _ = crate::settings::core_restart_channel().0.send_blocking(());

    if let Some(w) = window {
        if let Some(win) = w.downcast_ref::<crate::window::VrxxWindow>() {
            win.add_toast(adw::Toast::new(toast_msg));
        }
    }
}

/// Отображает модальный диалог навигационного стража при наличии несохраненных изменений.
///
/// # Аргументы
/// * `window` - Родительское окно GTK
/// * `on_apply` - Функция обратного вызова при согласии сохранить изменения
/// * `on_discard` - Функция обратного вызова при отказе (сбросе к исходному)
/// * `on_cancel` - Функция обратного вызова при отмене перехода (остаться на странице)
pub fn show_unsaved_changes_dialog<FApply, FDiscard, FCancel>(
    window: &gtk::Window,
    on_apply: FApply,
    on_discard: FDiscard,
    on_cancel: FCancel,
) where
    FApply: FnOnce() + 'static,
    FDiscard: FnOnce() + 'static,
    FCancel: FnOnce() + 'static,
{
    let dialog = adw::AlertDialog::builder()
        .heading(gettextrs::gettext("Unsaved Changes"))
        .body(gettextrs::gettext(
            "You have unsaved changes on this page. Would you like to apply them or discard before leaving?",
        ))
        .build();

    dialog.add_response("cancel", &gettextrs::gettext("Stay"));
    dialog.add_response("discard", &gettextrs::gettext("Discard"));
    dialog.add_response("apply", &gettextrs::gettext("Apply"));

    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");

    let win_clone = window.clone();
    glib::MainContext::default().spawn_local(async move {
        let response = dialog.choose_future(&win_clone).await;
        match response.as_str() {
            "apply" => on_apply(),
            "discard" => on_discard(),
            _ => on_cancel(),
        }
    });
}
