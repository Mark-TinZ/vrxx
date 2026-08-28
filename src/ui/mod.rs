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
//! - [`models`]: Реактивные GObject модели данных (`VpnKeyObject`, `RoutingRuleObject`, `DomainObject`)
//! - [`qr_dialog`]: Диалог QR-кода с эффектом размытия для приватности
//! - [`rule_dialog`]: Диалог создания и настройки правил маршрутизации
//! - [`import_dialog`]: Диалог интерактивного импорта ключей
//! - [`error_dialog`]: Универсальное окно критических ошибок и диагностики

pub mod change_tracker;
pub mod components;
pub mod error_dialog;
pub mod export_dialog;
pub mod import_dialog;
pub mod models;
pub mod pages;
pub mod qr_dialog;
pub mod rule_dialog;

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
mod tests;
