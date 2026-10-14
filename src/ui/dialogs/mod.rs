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

//! # Модуль диалоговых окон приложения (UI Dialogs Subsystem)
//!
//! Содержит специализированные модальные диалоги:
//! - [`error_dialog`]: Окно критических ошибок и диагностики
//! - [`export_dialog`]: Мастер экспорта конфигураций и профилей
//! - [`import_dialog`]: Интерактивный импорт ключей
//! - [`key_dialog`]: Создание и редактирование параметров VPN-профилей
//! - [`qr_dialog`]: Просмотр и экспорт QR-кода профиля
//! - [`rule_dialog`]: Создание и настройка правил маршрутизации

pub mod error_dialog;
pub mod export_dialog;
pub mod import_dialog;
pub mod key_dialog;
pub mod qr_dialog;
pub mod rule_dialog;
