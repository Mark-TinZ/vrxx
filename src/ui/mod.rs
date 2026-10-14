/* mod.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Графический интерфейс пользователя (GUI UI Subsystem)
//!
//! Модуль содержит все визуальные компоненты GTK4/Libadwaita:
//! - [`pages`]: Основные страницы навигации (`VrxxVpnPage`, `VrxxProxyPage`, `VrxxRoutingPage`, `VrxxSettingsPage`)
//! - [`components`]: Вторичные виджеты (`VrxxVpnKeyRow`, `VrxxThemeSwitcher`, `VrxxLogWindow`)
//! - [`dialogs`]: Модальные диалоговые окна (`VrxxKeyDialog`, `VrxxRuleDialog`, `VrxxQrDialog`, `VrxxExportDialog`, `VrxxImportDialog`)

pub mod change_tracker;
pub mod components;
pub mod dialogs;
pub mod models;
pub mod pages;

// Re-export dialog modules for backward compatibility and clean access
pub use dialogs::{error_dialog, export_dialog, import_dialog, key_dialog, qr_dialog, rule_dialog};

use gtk::prelude::*;

/// Вспомогательная функция для настройки кнопки главного меню в шапке страницы.
/// Загружает общую модель меню `primary_menu` из XML и внедряет кастомный переключатель тем `VrxxThemeSwitcher`.
pub fn setup_primary_menu(menu_button: &gtk::MenuButton) {
    // 1. Загрузка общей модели меню из ресурсов
    let builder = gtk::Builder::from_resource("/ru/mark/vrxx/ui/menus.ui");
    if let Some(model) = builder.object::<gtk::gio::MenuModel>("primary_menu") {
        menu_button.set_menu_model(Some(&model));
    }

    // 2. Внедрение кастомного виджета переключателя темы в всплывающий поповер меню
    if let Some(popover) = menu_button
        .popover()
        .and_then(|p| p.downcast::<gtk::PopoverMenu>().ok())
    {
        let switcher = components::theme_switcher::VrxxThemeSwitcher::new();
        popover.add_child(&switcher, "theme_switcher");
    }
}

#[cfg(test)]
mod proxy_tests;
#[cfg(test)]
pub mod tests;
