use crate::daemon::DaemonEvent;
use crate::domain::key_parser::ParsedKey;
use crate::domain::singbox_config::build_singbox_config;
use crate::ipc::DaemonClient;
use crate::settings::{AppSettings, SettingsManager};
use anyhow::Result;

/// Режим отображения в TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Main,
    Logs,
}

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
    pub logs: Vec<String>,
    pub view_mode: ViewMode,
    pub ipc_client: DaemonClient,
    pub should_quit: bool,
}

impl App {
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
            download_history: vec![0; 40],
            upload_history: vec![0; 40],
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
                    let status_str = self
                        .ipc_client
                        .status()
                        .await
                        .unwrap_or_else(|_| "Connected".to_string());
                    self.status = status_str;
                } else {
                    self.status = "Disconnected".to_string();
                    self.active_server = None;
                }
            }
            Err(_) => {
                self.is_connected = false;
                self.status = "Error (Daemon Offline)".to_string();
            }
        }
        Ok(())
    }

    /// Загрузка истории логов из демона
    pub async fn load_initial_logs(&mut self) {
        if let Ok(history) = self.ipc_client.get_history().await {
            for event in history {
                if let DaemonEvent::Log { level, message } = event {
                    self.push_log(format!("[{}] {}", level.to_uppercase(), message));
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
            self.push_log("[INFO] Proxy stopped via TUI".to_string());
        } else {
            // Запуск выбранного сервера
            if self.settings.keys.is_empty() {
                self.push_log("[WARN] No profiles available to connect".to_string());
                return Ok(());
            }

            let key = match self.settings.keys.get(self.selected_index) {
                Some(k) => k,
                None => return Ok(()),
            };

            let key_url = if !key.url.is_empty() {
                &key.url
            } else {
                &key.name
            };

            let parsed_key = match ParsedKey::parse(key_url) {
                Ok(pk) => pk,
                Err(e) => {
                    self.status = "Error (Invalid Key)".to_string();
                    self.push_log(format!("[ERROR] Failed to parse key: {e}"));
                    return Ok(());
                }
            };

            let config_json = build_singbox_config(&parsed_key, &self.settings);
            match self
                .ipc_client
                .start_proxy("sing-box".to_string(), config_json, self.tun_mode)
                .await
            {
                Ok(_) => {
                    self.is_connected = true;
                    self.status = "Connected".to_string();
                    self.active_server = Some(key.name.clone());
                    self.push_log(format!("[INFO] Connected to {}", key.name));
                }
                Err(e) => {
                    self.is_connected = false;
                    self.status = "Error".to_string();
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
        if self.download_history.len() > 40 {
            self.download_history.remove(0);
        }
        self.upload_history.push(up);
        if self.upload_history.len() > 40 {
            self.upload_history.remove(0);
        }
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
