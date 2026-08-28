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

//! # Доменный уровень и парсинг протоколов (Domain Subsystem)
//!
//! Модуль содержит ядро бизнес-логики:
//! - [`key_parser`]: Парсер и валидатор ссылок VPN-протоколов (VLESS, VMess, Trojan, Shadowsocks, Hysteria2, TUIC, WireGuard)
//! - [`singbox_config`]: Генератор конфигурационных JSON-файлов для сетевого ядра sing-box
//! - [`exporter`]: Экспорт ключей в форматы QR-кодов (PNG, SVG, Texture)

pub mod exporter;
pub mod key_parser;
pub mod singbox_config;
