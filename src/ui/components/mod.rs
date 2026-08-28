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

//! # Вспомогательные компоненты пользовательского интерфейса (UI Components)
//!
//! Экспортирует специализированные виджеты:
//! - [`vpn_key_row`]: Компонент строки списка VPN-ключей с деталями и контекстным меню
//! - [`theme_switcher`]: Круглый селектор цветовой схемы оформления
//! - [`log_window`]: Окно просмотра системных и сетевых логов в реальном времени

pub mod log_window;
pub mod routing_rule_row;
pub mod theme_switcher;
pub mod vpn_key_row;

pub use routing_rule_row::VrxxRoutingRuleRow;
