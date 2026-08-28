/* config.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Конфигурация сборки и константы окружения
//!
//! Содержит глобальные константы пакета (версия, домен gettext, путь к локалям).
//! Поддерживает как стандартную сборку через `cargo build`, так и дистрибутивную через `meson`.

#[cfg(not(meson_build))]
/// Версия приложения из Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(not(meson_build))]
/// Имя пакета для Gettext переводов
pub const GETTEXT_PACKAGE: &str = "vrxx";

#[cfg(not(meson_build))]
/// Системный каталог файлов локализации (.mo)
pub const LOCALEDIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/locale");

#[cfg(meson_build)]
include!(concat!(env!("OUT_DIR"), "/config_fallback.rs"));
