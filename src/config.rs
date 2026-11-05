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

/// Инициализация подсистемы интернационализации (Gettext) и системной локали.
///
/// Настраивает локаль libc, устанавливает системные переменные окружения языка (`LANGUAGE`),
/// осуществляет каскадный поиск каталога скомпилированных файлов переводов (`.mo`)
/// и привязывает текстовый домен Gettext.
pub fn init_i18n(preferred_language: &str) {
    // 1. Инициализируем локаль libc
    let mut loc = gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");

    // Если локаль не была установлена (None) или сбросилась в "C" / "POSIX" / "C.UTF-8",
    // gettext в glibc отключает перевод по переменной LANGUAGE. Переключаем процесс на доступную UTF-8 локаль:
    if loc
        .as_deref()
        .is_none_or(|l| l == b"C" || l == b"POSIX" || l == b"C.UTF-8" || l == b"C.utf8")
    {
        loc = gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "en_US.UTF-8")
            .or_else(|| gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "en_US.utf8"))
            .or_else(|| gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "ru_RU.UTF-8"))
            .or_else(|| gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "ru_RU.utf8"))
            .or_else(|| gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "C.UTF-8"));
    }

    // 2. Установка языка приложения (LANGUAGE)
    match preferred_language {
        "ru" => {
            std::env::set_var("LANGUAGE", "ru");
        }
        "en" => {
            std::env::set_var("LANGUAGE", "en");
        }
        _ => {
            // "system": если в системном LANG указан русский (ru), но в системе нет сгенерированной ru_RU локали
            // и LANGUAGE не был задан явно, активируем LANGUAGE=ru
            if let Ok(sys_lang) = std::env::var("LANG") {
                if sys_lang.starts_with("ru") && std::env::var("LANGUAGE").is_err() {
                    std::env::set_var("LANGUAGE", "ru");
                }
            }
        }
    }

    tracing::info!(
        "VRXX localization initialized (locale: {:?}, language: {})",
        loc.as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned()),
        preferred_language
    );

    // 3. Каскадный поиск каталога файлов локализации (.mo)
    let configured_locale_dir = if std::path::Path::new(LOCALEDIR).is_relative() {
        std::env::current_dir()
            .map(|p| p.join(LOCALEDIR))
            .unwrap_or_else(|_| std::path::PathBuf::from(LOCALEDIR))
    } else {
        std::path::PathBuf::from(LOCALEDIR)
    };

    let candidate_dirs = [
        configured_locale_dir.clone(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../share/locale")))
            .unwrap_or_default(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("locale")))
            .unwrap_or_default(),
        std::env::current_dir()
            .map(|p| p.join("locale"))
            .unwrap_or_default(),
    ];

    let mut locale_dir = configured_locale_dir;
    for dir in &candidate_dirs {
        if dir.exists()
            && (dir.join("ru/LC_MESSAGES/vrxx.mo").exists()
                || dir.join("en/LC_MESSAGES/vrxx.mo").exists())
        {
            locale_dir = dir.clone();
            break;
        }
    }

    if let Err(e) = gettextrs::bindtextdomain(GETTEXT_PACKAGE, &locale_dir) {
        tracing::warn!("Failed to bind gettext localization domain: {}", e);
    }
    if let Err(e) = gettextrs::bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8") {
        tracing::warn!("Failed to set gettext domain codeset: {}", e);
    }
    if let Err(e) = gettextrs::textdomain(GETTEXT_PACKAGE) {
        tracing::warn!("Failed to switch gettext domain: {}", e);
    }
}
