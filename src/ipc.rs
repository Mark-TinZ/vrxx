/* ipc.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Клиент межпроцессного взаимодействия (IPC Client)
//!
//! Модуль содержит клиент [`DaemonClient`] для отправки команд и подписки на события
//! привилегированного демона `vrxx-daemon` по HTTP REST API и Server-Sent Events (SSE).

use crate::daemon::DaemonEvent;
use crate::domain::key_parser::ParsedKey;
use crate::protocol::{CoreType, DaemonConfigOptions, DaemonStatus, StartProxyRequest};
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use tokio_stream::StreamExt;

/// Состояние подключения графического интерфейса к системному демону vrxx-daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonConnectionState {
    /// Демон не обнаружен или соединение не установлено
    Disconnected,
    /// Выполняется попытка подключения / повторное подключение
    Connecting,
    /// Соединение установлено и активно
    Connected,
}

pub const DEFAULT_SOCKET_PATH: &str = "/run/vrxx/daemon.sock";

/// Клиент для взаимодействия с привилегированным демоном через REST API и SSE.
#[derive(Clone, Debug)]
pub struct DaemonClient {
    client: Client,
    base_url: &'static str,
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonClient {
    /// Создает новый экземпляр клиента, подключающегося к демону через Unix Domain Socket.
    /// Путь к сокету определяется переменной окружения VRXX_DAEMON_SOCKET или DEFAULT_SOCKET_PATH.
    pub fn new() -> Self {
        let socket_path =
            std::env::var("VRXX_DAEMON_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());
        Self::with_socket_path(socket_path)
    }

    /// Создает новый экземпляр клиента с явно указанным путем к сокету.
    pub fn with_socket_path(socket_path: impl Into<String>) -> Self {
        let path = socket_path.into();
        let client = Client::builder()
            .no_proxy()
            .unix_socket(path)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            base_url: "http://localhost",
        }
    }

    /// Проверяет доступность демона (Health check) с быстрым таймаутом 1 сек.
    pub async fn ping(&self) -> anyhow::Result<String> {
        let res = self
            .client
            .get(format!("{}/api/ping", self.base_url))
            .timeout(std::time::Duration::from_secs(1))
            .send()
            .await?
            .text()
            .await?;
        Ok(res)
    }

    /// Быстрая неблокирующая проверка, запущен ли демон и отвечает ли он на запросы.
    pub async fn is_alive(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Отправляет команду на запуск ядра прокси.
    ///
    /// # Аргументы
    /// Отправляет команду на запуск ядра прокси с валидированным профилем и типизированными параметрами.
    ///
    /// # Аргументы
    /// * `profile` - распарсенный профиль VPN-ключа ([`ParsedKey`]).
    /// * `options` - строго типизированные параметры конфигурации ([`DaemonConfigOptions`]).
    pub async fn start_proxy(
        &self,
        profile: &ParsedKey,
        options: &DaemonConfigOptions,
    ) -> anyhow::Result<String> {
        let payload = StartProxyRequest {
            core_type: CoreType::SingBox,
            profile: profile.clone(),
            options: options.clone(),
        };
        let resp = self
            .client
            .post(format!("{}/api/proxy/start", self.base_url))
            .json(&payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            let err_text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Daemon error".to_string());
            anyhow::bail!("{err_text}");
        }
        let res = resp.text().await?;
        Ok(res)
    }

    /// Отправляет команду на остановку ядра прокси.
    pub async fn stop_proxy(&self) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(format!("{}/api/proxy/stop", self.base_url))
            .send()
            .await?;
        if !resp.status().is_success() {
            let err_text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Daemon error".to_string());
            anyhow::bail!("{err_text}");
        }
        let res = resp.text().await?;
        Ok(res)
    }

    /// Проверяет, запущен ли процесс ядра на стороне демона.
    pub async fn is_running(&self) -> anyhow::Result<bool> {
        let res = self
            .client
            .get(format!("{}/api/is_running", self.base_url))
            .send()
            .await?
            .json::<bool>()
            .await?;
        Ok(res)
    }

    /// Запрашивает текущий типизированный статус демона.
    pub async fn status(&self) -> anyhow::Result<DaemonStatus> {
        let res = self
            .client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .await?
            .json::<DaemonStatus>()
            .await?;
        Ok(res)
    }

    /// Запрашивает историю последних событий от демона.
    pub async fn get_history(&self) -> anyhow::Result<Vec<DaemonEvent>> {
        let res = self
            .client
            .get(format!("{}/api/history", self.base_url))
            .send()
            .await?
            .json::<Vec<DaemonEvent>>()
            .await?;
        Ok(res)
    }

    /// Подписывается на поток событий SSE (статус, логи) от демона.
    /// Возвращает асинхронный канал для получения событий.
    pub fn subscribe_events(&self) -> async_channel::Receiver<DaemonEvent> {
        let (sender, receiver) = async_channel::unbounded();
        let url = format!("{}/api/events", self.base_url);
        let client = self.client.clone();

        let task = async move {
            let mut retry_count = 0;
            let mut ever_connected = false;
            let mut delay_secs = 2u64;

            loop {
                let mut es = match EventSource::new(client.get(&url)) {
                    Ok(es) => es,
                    Err(err) => {
                        tracing::error!("Failed to initialize SSE EventSource: {}", err);
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                        delay_secs = (delay_secs * 2).min(30);
                        continue;
                    }
                };
                while let Some(event) = es.next().await {
                    match event {
                        Ok(Event::Open) => {
                            tracing::info!("SSE connection to vrxx-daemon established");
                            ever_connected = true;
                            retry_count = 0;
                            delay_secs = 2;
                        }
                        Ok(Event::Message(message)) => {
                            if let Ok(daemon_event) =
                                serde_json::from_str::<DaemonEvent>(&message.data)
                            {
                                if sender.send(daemon_event).await.is_err() {
                                    return; // receiver dropped
                                }
                            }
                        }
                        Err(err) => {
                            if ever_connected {
                                if retry_count == 0 {
                                    tracing::warn!(
                                        "Lost SSE connection to daemon, reconnecting in {}s... (error: {})",
                                        delay_secs,
                                        err
                                    );
                                } else {
                                    tracing::debug!(
                                        "Reconnecting to daemon (attempt {}, backoff {}s): {}",
                                        retry_count,
                                        delay_secs,
                                        err
                                    );
                                }
                            } else {
                                // До первого подключения логируем на уровне debug, чтобы не спамить в терминал при старте без демона
                                tracing::debug!(
                                    "Awaiting vrxx-daemon connection (retry in {}s): {}",
                                    delay_secs,
                                    err
                                );
                            }
                            retry_count += 1;
                            es.close();
                            break;
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                // Экспоненциальный backoff: 2 -> 4 -> 8 -> 16 -> 30 (макс)
                delay_secs = (delay_secs * 2).min(30);
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(task);
        } else {
            tracing::debug!("No active Tokio runtime reactor, SSE event subscription task skipped");
        }

        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_connection_state_enum() {
        assert_eq!(
            DaemonConnectionState::Disconnected,
            DaemonConnectionState::Disconnected
        );
        assert_ne!(
            DaemonConnectionState::Disconnected,
            DaemonConnectionState::Connected
        );
        assert_ne!(
            DaemonConnectionState::Connecting,
            DaemonConnectionState::Connected
        );
    }

    #[test]
    fn test_daemon_client_initialization() {
        let client = DaemonClient::new();
        assert_eq!(client.base_url, "http://localhost");
    }

    #[test]
    fn test_daemon_client_custom_socket() {
        let client = DaemonClient::with_socket_path("/tmp/custom.sock");
        assert_eq!(client.base_url, "http://localhost");
    }
}
