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

//! # Страницы интерфейса пользователя (UI Pages)
//!
//! Экспортирует основные страницы приложения:
//! - [`VrxxVpnPage`]: Страница списка и подключения VPN-ключей
//! - [`VrxxProxyPage`]: Страница настройки локального и системного прокси
//! - [`VrxxRoutingPage`]: Страница правил маршрутизации трафика
//! - [`VrxxSettingsPage`]: Страница глобальных параметров приложения и ядра

mod vpn_page;
pub use vpn_page::VrxxVpnPage;

mod proxy_page;
pub use proxy_page::VrxxProxyPage;

mod routing_page;
pub use routing_page::VrxxRoutingPage;

mod settings_page;
pub use settings_page::VrxxSettingsPage;
