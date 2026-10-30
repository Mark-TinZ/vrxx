/* vrxx-tui.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Точка входа терминального интерфейса (VRXX TUI Binary)

use anyhow::Result;
use clap::Parser;

/// Терминальный клиент управления VPN и сетевыми прокси VRXX (TUI).
#[derive(Parser, Debug)]
#[command(
    name = "vrxx-tui",
    author,
    version,
    about = "VRXX TUI - Терминальный клиент управления VPN и сетевыми прокси",
    long_about = None
)]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    // Разбор аргументов командной строки (--help, --version)
    let _ = Cli::parse();

    // Инициализация Tracing для TUI: вывод направляется в файл, а не в stdout/stderr терминала
    let manager = vrxx::settings::SettingsManager::new();
    let app_settings = manager.load();

    let logger_config = vrxx::logger::LoggerConfig {
        is_tui: true,
        is_daemon: false,
        initial_level: app_settings.log_level,
    };
    let _guard = vrxx::logger::init_gui_logging(logger_config);

    vrxx::tui::run_tui().await
}
