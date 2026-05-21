use crate::daemon::DaemonEvent;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use std::sync::OnceLock;
use tokio_stream::StreamExt;

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
    /// Создает новый экземпляр клиента.
    pub fn new() -> Self {
        static CLIENT: OnceLock<Client> = OnceLock::new();

        // Явно отключаем системные прокси, чтобы локальный трафик до демона
        // не маршрутизировался через VPN и не блокировался ядром.
        // Кэшируем клиент для переиспользования пула соединений.
        let client = CLIENT
            .get_or_init(|| {
                Client::builder()
                    .no_proxy()
                    .build()
                    .unwrap_or_else(|_| Client::new())
            })
            .clone();

        Self {
            client,
            base_url: "http://127.0.0.1:13337",
        }
    }

    /// Проверяет доступность демона (Health check).
    pub async fn ping(&self) -> anyhow::Result<String> {
        let res = self
            .client
            .get(format!("{}/api/ping", self.base_url))
            .send()
            .await?
            .text()
            .await?;
        Ok(res)
    }

    /// Отправляет команду на запуск ядра прокси.
    ///
    /// # Аргументы
    /// * `core_type` - тип ядра (например, "sing-box").
    /// * `config_json` - JSON конфигурация.
    /// * `tun_mode` - флаг включения TUN-режима.
    pub async fn start_proxy(
        &self,
        core_type: String,
        config_json: String,
        tun_mode: bool,
    ) -> anyhow::Result<String> {
        let payload = crate::daemon::StartProxyRequest {
            core_type,
            config_json,
            tun_mode,
        };
        let res = self
            .client
            .post(format!("{}/api/proxy/start", self.base_url))
            .json(&payload)
            .send()
            .await?
            .text()
            .await?;
        Ok(res)
    }

    /// Отправляет команду на остановку ядра прокси.
    pub async fn stop_proxy(&self) -> anyhow::Result<String> {
        let res = self
            .client
            .post(format!("{}/api/proxy/stop", self.base_url))
            .send()
            .await?
            .text()
            .await?;
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

    /// Запрашивает текущий текстовый статус демона.
    pub async fn status(&self) -> anyhow::Result<String> {
        let res = self
            .client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .await?
            .text()
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

        tokio::spawn(async move {
            let mut retry_count = 0;
            loop {
                let mut es = EventSource::get(&url);
                while let Some(event) = es.next().await {
                    match event {
                        Ok(Event::Open) => {
                            tracing::debug!("SSE connected");
                            retry_count = 0;
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
                            // Логируем ошибку только если это не первый раз, чтобы не спамить при запуске
                            if retry_count % 10 == 0 {
                                tracing::warn!("SSE connection lost, retrying... (error: {})", err);
                            }
                            retry_count += 1;
                            es.close();
                            break;
                        }
                    }
                }
                // Повторная попытка подключения при разрыве.
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });

        receiver
    }
}
