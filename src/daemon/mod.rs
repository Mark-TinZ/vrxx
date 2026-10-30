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
//! - [`api`]: HTTP REST API и Server-Sent Events (SSE) эндпоинты (на `/run/vrxx/daemon.sock`)
//! - [`auth`]: Аутентификация вызывающих процессов через `SO_PEERCRED` и `systemd-logind`
//! - [`core`]: Менеджер процесса ядра `sing-box` ([`core::ProxyManager`]), парсинг логов и мониторинг
//! - [`network`]: Управление виртуальным сетевым интерфейсом TUN (`vrxx-tun`), таблицами маршрутизации Netlink
//! - [`dns`]: Настройка и защита DNS через `systemd-resolved` (D-Bus `org.freedesktop.resolve1`)
//! - [`events`]: Шина широковещательных событий [`events::EventManager`] и кольцевой буфер логов
//! - [`updater`]: Обнаружение исполняемых файлов сетевого ядра в системе

pub mod api;
pub mod auth;
pub mod core;
pub mod dns;
pub mod events;
pub mod network;
pub mod updater;

pub use core::StartProxyRequest;
pub use events::DaemonEvent;
use std::sync::Arc;

pub const DEFAULT_SOCKET_PATH: &str = "/run/vrxx/daemon.sock";

/// Возвращает путь к Unix Domain Socket демона с учетом переменной окружения VRXX_DAEMON_SOCKET.
pub fn get_socket_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("VRXX_DAEMON_SOCKET") {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(DEFAULT_SOCKET_PATH)
    }
}

/// RAII-страж для удаления файла сокета при завершении работы или аварийном выходе.
struct SocketCleaner(std::path::PathBuf);

impl Drop for SocketCleaner {
    fn drop(&mut self) {
        if self.0.symlink_metadata().is_ok() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// Точка входа для запуска демона с предустановленным менеджером событий.
pub async fn run_with_manager(event_manager: Arc<events::EventManager>) -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    tracing::info!("Starting VRXX daemon REST API on UDS {:?}...", socket_path);

    // 0. Запуск процедур самовосстановления сети (Self-Healing)
    if let Err(e) = network::self_heal().await {
        tracing::warn!("Network self-healing warning: {}", e);
    }

    // 1. Создание родительского каталога с правами 0755
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
    }

    // 2. Очистка старого/мертвого сокета (stale socket cleanup)
    if socket_path.symlink_metadata().is_ok() {
        if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
            tracing::error!(
                "Failed to bind VRXX daemon to {:?}: another instance is already responding on this socket.",
                socket_path
            );
            anyhow::bail!("VRXX daemon is already running on {:?}", socket_path);
        }
        tracing::warn!("Found stale socket file at {:?}, removing...", socket_path);
        let _ = std::fs::remove_file(&socket_path);
    }

    // 3. Биндинг UnixListener и установка прав 0666 на файл сокета
    let listener = match tokio::net::UnixListener::bind(&socket_path) {
        Ok(l) => {
            tracing::info!("VRXX daemon successfully bound to UDS {:?}", socket_path);
            l
        }
        Err(e) => {
            tracing::error!("Failed to bind VRXX daemon to {:?}: {}", socket_path, e);
            return Err(anyhow::anyhow!(
                "Failed to bind to {:?}: {}",
                socket_path,
                e
            ));
        }
    };

    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666)) {
        tracing::warn!(
            "Failed to set 0666 permissions on socket {:?}: {}",
            socket_path,
            e
        );
    }

    // RAII guard для очистки сокета при выходе
    let _cleaner = SocketCleaner(socket_path.clone());

    // 4. Инициализируем менеджер прокси и API
    let proxy_manager = Arc::new(core::ProxyManager::new(event_manager.clone()));
    let state = Arc::new(api::ApiState {
        proxy_manager,
        event_manager,
    });
    let app = api::create_router(state);
    let make_service = app.into_make_service_with_connect_info::<auth::CallerIdentity>();

    // 5. Обработка сигналов грациозного завершения (SIGINT, SIGTERM)
    let shutdown_signal = async {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT, initiating graceful shutdown");
            },
            _ = async {
                if let Some(term) = sigterm.as_mut() {
                    term.recv().await;
                    tracing::info!("Received SIGTERM, initiating graceful shutdown");
                } else {
                    std::future::pending::<()>().await;
                }
            } => {},
        }
    };

    axum::serve(listener, make_service)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("VRXX daemon server stopped gracefully");
    Ok(())
}

/// Точка входа для запуска демона (создает свой менеджер событий).
pub async fn run() -> anyhow::Result<()> {
    let (event_manager, _) = events::EventManager::new(100);
    run_with_manager(Arc::new(event_manager)).await
}

#[cfg(test)]
mod tests;
