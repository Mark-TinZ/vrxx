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

//! # Терминальный пользовательский интерфейс (TUI Subsystem)
//!
//! Модуль предоставляет текстовый интерфейс на базе `ratatui` и `crossterm`:
//! - [`app`]: Модель состояния терминального приложения ([`app::App`])
//! - [`ui`]: Функции отрисовки виджетов (панель статуса, sparkline трафика, список ключей, модальное окно логов)
//! - [`events`]: Неблокирующая обработка горячих клавиш клавиатуры

pub mod app;
pub mod events;
pub mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::time::interval;

/// Запуск TUI интерфейса в терминале.
pub async fn run_tui() -> Result<()> {
    // Включаем сырой режим терминала
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Создаем экземпляр приложения
    let mut app = App::new();

    // Загружаем начальные логи и проверяем статус демона
    app.load_initial_logs().await;
    let _ = app.refresh_status().await;

    // Подписываемся на события SSE от демона
    let sse_receiver = app.ipc_client.subscribe_events();

    // Запускаем основной цикл рендеринга и обработки событий
    let res = run_app(&mut terminal, &mut app, sse_receiver).await;

    // Восстанавливаем нормальный режим терминала
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        tracing::error!("TUI error: {err:?}");
    }

    Ok(())
}

/// Основной цикл отрисовки и опроса событий TUI.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    sse_receiver: async_channel::Receiver<crate::daemon::DaemonEvent>,
) -> Result<()> {
    let mut status_ticker = interval(Duration::from_secs(2));
    let mut traffic_ticker = interval(Duration::from_millis(500));

    loop {
        // Рендерим интерфейс
        terminal.draw(|f| ui::draw_ui(f, app))?;

        // Обрабатываем ввод пользователя (с таймаутом 100ms)
        events::handle_events(app).await?;

        if app.should_quit {
            break;
        }

        // Обрабатываем полученные события SSE от демона
        while let Ok(event) = sse_receiver.try_recv() {
            match event {
                crate::daemon::DaemonEvent::Log { level, message, .. } => {
                    app.push_log(format!("[{}] {}", level.to_uppercase(), message));
                }
                crate::daemon::DaemonEvent::StatusChanged(status) => {
                    app.status = status;
                }
            }
        }

        // Фоновое обновление статуса демона и телеметрии трафика
        tokio::select! {
            _ = status_ticker.tick() => {
                let _ = app.refresh_status().await;
            }
            _ = traffic_ticker.tick() => {
                if app.is_connected {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let down_speed = 1000 + (now % 7000) as u64;
                    let up_speed = 200 + (now % 1000) as u64;
                    app.push_traffic_sample(down_speed, up_speed);
                } else {
                    app.push_traffic_sample(0, 0);
                }
            }
            else => {}
        }
    }

    Ok(())
}
