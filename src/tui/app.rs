/* app.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Состояние и бизнес-логика TUI приложения (TUI App State)
//!
//! Модуль содержит модель [`App`], хранящую:
//! - Настройки приложения, загруженные из `SettingsManager`
//! - Текущий индекс выбранного VPN-профиля
//! - Буферы измерений скорости входящего и исходящего трафика (для Sparkline)
//! - Кольцевой буфер текстовых логов
//! - Режим отображения экрана ([`ViewMode::Main`] или [`ViewMode::Logs`])

use crate::daemon::DaemonEvent;
use crate::domain::key_parser::ParsedKey;
use crate::ipc::DaemonClient;
use crate::protocol::DaemonConfigOptions;
use crate::settings::{AppSettings, SettingsManager};
use anyhow::Result;

/// Режим отображения экрана в TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Main,
    Logs,
}

/// Емкость кольцевого буфера истории трафика (60 замеров = 1 минута при частоте 1 Гц).
pub const TRAFFIC_HISTORY_CAPACITY: usize = 60;

/// Состояние TUI приложения
pub struct App {
    pub settings: AppSettings,
    pub selected_index: usize,
    pub is_connected: bool,
    pub status: String,
    pub active_server: Option<String>,
    pub tun_mode: bool,
    pub download_history: Vec<u64>,
    pub upload_history: Vec<u64>,
    pub current_down_speed: u64,
    pub current_up_speed: u64,
    pub total_down: u64,
    pub total_up: u64,
    pub logs: Vec<String>,
    pub view_mode: ViewMode,
    pub ipc_client: DaemonClient,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Создает новый экземпляр App с загрузкой настроек.
    pub fn new() -> Self {
        let settings = SettingsManager::new().load();
        let tun_mode = settings.tun_mode;

        Self {
            settings,
            selected_index: 0,
            is_connected: false,
            status: "Disconnected".to_string(),
            active_server: None,
            tun_mode,
            download_history: vec![0; TRAFFIC_HISTORY_CAPACITY],
            upload_history: vec![0; TRAFFIC_HISTORY_CAPACITY],
            current_down_speed: 0,
            current_up_speed: 0,
            total_down: 0,
            total_up: 0,
            logs: Vec::new(),
            view_mode: ViewMode::Main,
            ipc_client: DaemonClient::new(),
            should_quit: false,
        }
    }

    /// Обновление статуса демона и истории логов
    pub async fn refresh_status(&mut self) -> Result<()> {
        match self.ipc_client.is_running().await {
            Ok(running) => {
                self.is_connected = running;
                if running {
                    let status_str = match self.ipc_client.status().await {
                        Ok(st) => st.to_string(),
                        Err(_) => "Connected".to_string(),
                    };
                    self.status = status_str;
                } else {
                    self.status = "Disconnected".to_string();
                    self.active_server = None;
                    self.reset_traffic();
                }
            }
            Err(_) => {
                self.is_connected = false;
                self.status = "Error (Daemon Offline)".to_string();
                self.reset_traffic();
            }
        }
        Ok(())
    }

    /// Загрузка истории логов из демона
    pub async fn load_initial_logs(&mut self) {
        if let Ok(history) = self.ipc_client.get_history().await {
            for event in history {
                if let DaemonEvent::Log { level, message, .. } = event {
                    self.push_log(format!("[{}] {}", level.as_str().to_uppercase(), message));
                }
            }
        }
    }

    /// Переключение соединения (Подключить / Отключить)
    pub async fn toggle_connect(&mut self) -> Result<()> {
        if self.is_connected {
            // Остановка
            self.ipc_client.stop_proxy().await?;
            self.is_connected = false;
            self.status = "Disconnected".to_string();
            self.active_server = None;
            self.reset_traffic();
            self.push_log("[INFO] Proxy stopped via TUI".to_string());
        } else {
            // Запуск выбранного сервера
            if self.settings.keys.is_empty() {
                self.push_log("[WARN] No profiles available to connect".to_string());
                return Ok(());
            }

            let (key_name, key_url) = match self.settings.keys.get(self.selected_index) {
                Some(k) => (
                    k.name.clone(),
                    if !k.url.is_empty() {
                        k.url.clone()
                    } else {
                        k.name.clone()
                    },
                ),
                None => return Ok(()),
            };

            let parsed_key = match ParsedKey::parse(&key_url) {
                Ok(pk) => pk,
                Err(e) => {
                    self.status = "Error (Invalid Key)".to_string();
                    self.push_log(format!("[ERROR] Failed to parse key: {e}"));
                    return Ok(());
                }
            };

            if let Err(val_err) = parsed_key.validate() {
                self.status = "Error (Invalid Key)".to_string();
                self.push_log(format!("[ERROR] Key validation error: {val_err}"));
                return Ok(());
            }

            let options = DaemonConfigOptions::from_settings(&self.settings, self.tun_mode);
            match self.ipc_client.start_proxy(&parsed_key, &options).await {
                Ok(_) => {
                    self.push_log(
                        "[INFO] Core started. Verifying end-to-end proxy connectivity..."
                            .to_string(),
                    );
                    let target_url = if self.settings.ping_target_url.is_empty() {
                        "https://www.gstatic.com/generate_204"
                    } else {
                        &self.settings.ping_target_url
                    };
                    let probe = crate::services::ping::verify_proxy_connectivity(
                        self.settings.socks_port,
                        target_url,
                        std::time::Duration::from_secs(8),
                    )
                    .await;

                    match probe {
                        Ok(latency) => {
                            self.is_connected = true;
                            self.status = "Connected".to_string();
                            self.active_server = Some(key_name.clone());
                            self.total_down = 0;
                            self.total_up = 0;
                            self.reset_traffic();
                            self.push_log(format!(
                                "[INFO] Successfully connected to {} ({} ms)",
                                key_name, latency
                            ));
                        }
                        Err(e) => {
                            let _ = self.ipc_client.stop_proxy().await;
                            self.is_connected = false;
                            self.status = "Error (Server/Auth Failed)".to_string();
                            self.reset_traffic();
                            self.push_log(format!("[ERROR] Connectivity verification failed: {e}"));
                        }
                    }
                }
                Err(e) => {
                    self.is_connected = false;
                    self.status = "Error".to_string();
                    self.reset_traffic();
                    self.push_log(format!("[ERROR] Start proxy failed: {e}"));
                }
            }
        }
        Ok(())
    }

    /// Переключение режима (TUN / Proxy)
    pub async fn toggle_mode(&mut self) -> Result<()> {
        self.tun_mode = !self.tun_mode;
        self.settings.tun_mode = self.tun_mode;
        let manager = SettingsManager::new();
        manager.save(&self.settings);

        let mode_str = if self.tun_mode { "TUN" } else { "Proxy" };
        self.push_log(format!("[INFO] Mode switched to {}", mode_str));

        // Если активное подключение уже запущенно - переподключаем
        if self.is_connected {
            self.toggle_connect().await?; // disconnect
            self.toggle_connect().await?; // reconnect
        }
        Ok(())
    }

    /// Добавление лога в локальный буфер
    pub fn push_log(&mut self, log: String) {
        self.logs.push(log);
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }

    /// Добавление замера трафика (входящая / исходящая скорость)
    pub fn push_traffic_sample(&mut self, down: u64, up: u64) {
        self.download_history.push(down);
        if self.download_history.len() > TRAFFIC_HISTORY_CAPACITY {
            self.download_history.remove(0);
        }
        self.upload_history.push(up);
        if self.upload_history.len() > TRAFFIC_HISTORY_CAPACITY {
            self.upload_history.remove(0);
        }
    }

    /// Применение телеметрии трафика из события демона через SSE.
    pub fn apply_traffic(
        &mut self,
        down_speed: u64,
        up_speed: u64,
        down_total: u64,
        up_total: u64,
    ) {
        if self.is_connected {
            self.current_down_speed = down_speed;
            self.current_up_speed = up_speed;
            self.total_down = down_total;
            self.total_up = up_total;
            self.push_traffic_sample(down_speed, up_speed);
        } else {
            self.current_down_speed = 0;
            self.current_up_speed = 0;
            self.push_traffic_sample(0, 0);
        }
    }

    /// Сброс текущей скорости и обнуление буферов трафика при отключении.
    pub fn reset_traffic(&mut self) {
        self.current_down_speed = 0;
        self.current_up_speed = 0;
        self.download_history.fill(0);
        self.upload_history.fill(0);
    }

    /// Навигация по списку профилей вверх
    pub fn previous_profile(&mut self) {
        if !self.settings.keys.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.settings.keys.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    /// Навигация по списку профилей вниз
    pub fn next_profile(&mut self) {
        if !self.settings.keys.is_empty() {
            if self.selected_index + 1 >= self.settings.keys.len() {
                self.selected_index = 0;
            } else {
                self.selected_index += 1;
            }
        }
    }

    /// Переключение окна логов
    pub fn toggle_logs_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Main => ViewMode::Logs,
            ViewMode::Logs => ViewMode::Main,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_initial_state() {
        let app = App::new();
        assert!(!app.is_connected);
        assert_eq!(app.current_down_speed, 0);
        assert_eq!(app.current_up_speed, 0);
        assert_eq!(app.total_down, 0);
        assert_eq!(app.total_up, 0);
        assert_eq!(app.download_history.len(), TRAFFIC_HISTORY_CAPACITY);
        assert_eq!(app.upload_history.len(), TRAFFIC_HISTORY_CAPACITY);
        assert!(app.download_history.iter().all(|&v| v == 0));
        assert!(app.upload_history.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_app_push_traffic_sample_capacity() {
        let mut app = App::new();
        for i in 1..=80 {
            app.push_traffic_sample(i, i * 2);
        }
        assert_eq!(app.download_history.len(), TRAFFIC_HISTORY_CAPACITY);
        assert_eq!(app.upload_history.len(), TRAFFIC_HISTORY_CAPACITY);
        assert_eq!(app.download_history.last(), Some(&80));
        assert_eq!(app.upload_history.last(), Some(&160));
        assert_eq!(app.download_history.first(), Some(&21)); // 80 - 60 + 1
    }

    #[test]
    fn test_app_apply_traffic_when_connected() {
        let mut app = App::new();
        app.is_connected = true;
        app.apply_traffic(1_048_576, 524_288, 100_000_000, 50_000_000);

        assert_eq!(app.current_down_speed, 1_048_576);
        assert_eq!(app.current_up_speed, 524_288);
        assert_eq!(app.total_down, 100_000_000);
        assert_eq!(app.total_up, 50_000_000);
        assert_eq!(app.download_history.last(), Some(&1_048_576));
        assert_eq!(app.upload_history.last(), Some(&524_288));
    }

    #[test]
    fn test_app_apply_traffic_when_disconnected() {
        let mut app = App::new();
        app.is_connected = false;
        app.apply_traffic(1_048_576, 524_288, 100_000_000, 50_000_000);

        assert_eq!(app.current_down_speed, 0);
        assert_eq!(app.current_up_speed, 0);
        assert_eq!(app.download_history.last(), Some(&0));
        assert_eq!(app.upload_history.last(), Some(&0));
    }

    #[test]
    fn test_app_reset_traffic() {
        let mut app = App::new();
        app.is_connected = true;
        app.apply_traffic(5000, 2000, 50000, 20000);
        assert_eq!(app.current_down_speed, 5000);

        app.reset_traffic();
        assert_eq!(app.current_down_speed, 0);
        assert_eq!(app.current_up_speed, 0);
        assert!(app.download_history.iter().all(|&v| v == 0));
        assert!(app.upload_history.iter().all(|&v| v == 0));
    }
}
