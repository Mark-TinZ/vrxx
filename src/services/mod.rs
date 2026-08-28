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

//! # Фоновые сервисы и сетевые утилиты (Services Subsystem)
//!
//! Модуль включает:
//! - [`ping`]: Измерение задержки соединений (TCP Handshake, ICMP Ping, HTTP GET/HEAD via Proxy) и E2E Warm-Up валидацию
//! - [`geo_updater`]: Фоновое обновление бинарных баз правил GeoIP и GeoSite (`.srs`) с поддержкой CDN fallback

pub mod geo_updater;
pub mod ping;
