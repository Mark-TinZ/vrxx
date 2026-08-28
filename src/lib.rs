/* lib.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Библиотека VRXX
//!
//! Экспортирует основные архитектурные модули приложения:
//! - [`crypto`]: Подсистема криптографической защиты данных и профилей (Host-Bound Keystore)
//! - [`daemon`]: Привилегированная системная служба управления ядром sing-box и сетью
//! - [`domain`]: Парсеры VPN-ключей, валидация и генератор конфигураций sing-box
//! - [`ipc`]: Клиент и протокол межпроцессного взаимодействия (REST API / SSE)
//! - [`services`]: Фоновые службы (замер пинга, обновление баз GeoIP/GeoSite)
//! - [`settings`]: Менеджер персистентного хранения конфигурации пользователя
//! - [`tui`]: Консольный интерфейс терминала на базе Ratatui

pub mod crypto;
pub mod daemon;
pub mod domain;
pub mod ipc;
pub mod services;
pub mod settings;
pub mod tui;
