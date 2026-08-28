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

//! # Привилегированный фоновый демон (VRXX Daemon Subsystem)
//!
//! Модуль содержит реализацию фонового демона, выполняющего системные сетевые операции:
//! - [`api`]: HTTP REST API и Server-Sent Events (SSE) эндпоинты (на `127.0.0.1:13337`)
//! - [`core`]: Менеджер процесса ядра `sing-box` ([`core::ProxyManager`]), парсинг логов и мониторинг
//! - [`network`]: Управление виртуальным сетевым интерфейсом TUN (`vrxx-tun`), таблицами маршрутизации Netlink
//! - [`dns`]: Настройка и защита DNS через `systemd-resolved` (D-Bus `org.freedesktop.resolve1`)
//! - [`events`]: Шина широковещательных событий [`events::EventManager`] и кольцевой буфер логов
//! - [`updater`]: Обнаружение исполняемых файлов сетевого ядра в системе

pub mod api;
pub mod core;
pub mod dns;
pub mod events;
pub mod network;
pub mod updater;

pub use core::StartProxyRequest;
pub use events::DaemonEvent;
use std::sync::Arc;

/// Точка входа для запуска демона с предустановленным менеджером событий.
pub async fn run_with_manager(event_manager: Arc<events::EventManager>) -> anyhow::Result<()> {
    tracing::info!("Запуск REST API демона vrxx на 127.0.0.1:13337...");

    // 0. Запуск процедур самовосстановления сети (Self-Healing)
    if let Err(e) = network::self_heal().await {
        tracing::warn!("Предупреждение процедуры самовосстановления сети: {}", e);
    }

    // 1. Инициализируем менеджер прокси
    let proxy_manager = Arc::new(core::ProxyManager::new(event_manager.clone()));

    // 2. Создаем состояние API
    let state = Arc::new(api::ApiState {
        proxy_manager,
        event_manager,
    });

    // 3. Запускаем сервер
    let app = api::create_router(state);
    let bind_addr = "127.0.0.1:13337";
    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => {
            tracing::info!("VRXX демон успешно привязан к http://{}", bind_addr);
            l
        }
        Err(e) => {
            tracing::error!(
                "Не удалось привязать VRXX демон к {}: {}. Запущен ли другой экземпляр?",
                bind_addr,
                e
            );
            return Err(anyhow::anyhow!(
                "Не удалось привязать к {}: {}",
                bind_addr,
                e
            ));
        }
    };

    axum::serve(listener, app).await?;
    tracing::info!("Сервер VRXX демона корректно остановлен");

    Ok(())
}

/// Точка входа для запуска демона (создает свой менеджер событий).
pub async fn run() -> anyhow::Result<()> {
    let (event_manager, _) = events::EventManager::new(100);
    run_with_manager(Arc::new(event_manager)).await
}

#[cfg(test)]
mod tests;
