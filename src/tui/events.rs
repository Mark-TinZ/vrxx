/* events.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Обработчик событий клавиатуры в TUI (TUI Event Handler)
//!
//! Модуль обеспечивает неблокирующую обработку ввода пользователя:
//! - Навигация по списку профилей: `Up` / `k`, `Down` / `j`
//! - Подключение / отключение: `Space` / `Enter`
//! - Переключение режима: `Tab` (TUN / Proxy)
//! - Просмотр логов: `L` / `Esc`
//! - Выход из приложения: `Q`

use super::app::{App, ViewMode};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

/// Обработка пользовательского ввода с таймаутом (неблокирующая).
pub async fn handle_events(app: &mut App) -> Result<()> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.view_mode {
                    ViewMode::Main => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.previous_profile();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.next_profile();
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            if let Err(e) = app.toggle_connect().await {
                                app.push_log(format!(
                                    "[ERROR] Ошибка переключения подключения: {e}"
                                ));
                            }
                        }
                        KeyCode::Tab => {
                            if let Err(e) = app.toggle_mode().await {
                                app.push_log(format!("[ERROR] Ошибка переключения режима: {e}"));
                            }
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            app.toggle_logs_view();
                        }
                        _ => {}
                    },
                    ViewMode::Logs => match key.code {
                        KeyCode::Char('l')
                        | KeyCode::Char('L')
                        | KeyCode::Esc
                        | KeyCode::Char('q')
                        | KeyCode::Char('Q') => {
                            app.toggle_logs_view();
                        }
                        _ => {}
                    },
                }
            }
        }
    }
    Ok(())
}
