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

#[tokio::main]
async fn main() -> Result<()> {
    vrxx::tui::run_tui().await
}
