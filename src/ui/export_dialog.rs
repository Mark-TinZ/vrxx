/* export_dialog.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Интерактивный диалог выборочного экспорта конфигурации (Export Dialog)
//!
//! Отвечает за:
//! - Предоставление пользователю выбора компонентов для экспорта через чекбоксы (`AdwCheckRow` / `GtkCheckButton`)
//! - Раздельный выбор: открытые настройки приложения (по умолчанию включены) и VPN-профили/ключи (по умолчанию выключены)
//! - Валидацию: кнопка «Экспортировать» активна только если выбран хотя бы один компонент
//! - Предупреждение о конфиденциальности при экспорте ключей

use adw::prelude::*;
use gettextrs::gettext;

/// Отображает модальный диалог `AdwAlertDialog` для выбора компонентов экспорта.
///
/// Параметры замыкания `on_confirmed`:
/// - `export_settings: bool` — экспортировать ли настройки приложения
/// - `export_keys: bool` — экспортировать ли VPN-профили и ключи
pub fn show_export_dialog<F>(parent: &gtk::Window, on_confirmed: F)
where
    F: Fn(bool, bool) + 'static,
{
    let dialog = adw::AlertDialog::builder()
        .heading(gettext("Export Configuration"))
        .body(gettext(
            "Select the components you want to export to a file.",
        ))
        .build();

    let pref_group = adw::PreferencesGroup::builder()
        .title(gettext("Export Components"))
        .build();

    // 1. Чекбокс «Настройки приложения»
    let check_settings = gtk::CheckButton::builder()
        .active(true)
        .valign(gtk::Align::Center)
        .build();

    let row_settings = adw::ActionRow::builder()
        .title(gettext("Application Settings"))
        .subtitle(gettext(
            "Theme, interface language, proxy ports, DNS parameters, and routing rules",
        ))
        .activatable_widget(&check_settings)
        .build();
    row_settings.add_prefix(&check_settings);
    pref_group.add(&row_settings);

    // 2. Чекбокс «VPN-профили и ключи»
    let check_keys = gtk::CheckButton::builder()
        .active(false)
        .valign(gtk::Align::Center)
        .build();

    let row_keys = adw::ActionRow::builder()
        .title(gettext("VPN Profiles and Keys"))
        .subtitle(gettext(
            "Saved servers, connection links, private keys, and credentials",
        ))
        .activatable_widget(&check_keys)
        .build();
    row_keys.add_prefix(&check_keys);
    pref_group.add(&row_keys);

    // Обертка макета в Clamp для центрирования и адаптивности Libadwaita
    let clamp = adw::Clamp::builder()
        .maximum_size(520)
        .tightening_threshold(400)
        .child(&pref_group)
        .build();
    clamp.set_margin_top(8);
    clamp.set_margin_bottom(8);
    clamp.set_margin_start(8);
    clamp.set_margin_end(8);

    dialog.set_extra_child(Some(&clamp));

    // Кнопки действий
    dialog.add_response("cancel", &gettext("Cancel"));
    dialog.add_response("export", &gettext("Export"));
    dialog.set_response_appearance("export", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("export"));
    dialog.set_close_response("cancel");

    // Валидация: активация кнопки «Экспортировать» только при выборе хотя бы одного чекбокса
    let check_settings_weak = check_settings.downgrade();
    let check_keys_weak = check_keys.downgrade();
    let dialog_weak = dialog.downgrade();

    let update_export_button_state = move || {
        if let (Some(cs), Some(ck), Some(dlg)) = (
            check_settings_weak.upgrade(),
            check_keys_weak.upgrade(),
            dialog_weak.upgrade(),
        ) {
            let has_selection = cs.is_active() || ck.is_active();
            dlg.set_response_enabled("export", has_selection);
        }
    };

    let update_cb1 = update_export_button_state.clone();
    check_settings.connect_toggled(move |_| {
        update_cb1();
    });

    let update_cb2 = update_export_button_state;
    check_keys.connect_toggled(move |_| {
        update_cb2();
    });

    // Обработчик ответа пользователя
    dialog.connect_response(None, move |_, response| {
        if response == "export" {
            let export_settings = check_settings.is_active();
            let export_keys = check_keys.is_active();
            if export_settings || export_keys {
                on_confirmed(export_settings, export_keys);
            }
        }
    });

    dialog.present(Some(parent));
}
