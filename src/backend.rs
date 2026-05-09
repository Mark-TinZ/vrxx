use crate::ipc::DaemonClient;
use crate::settings::SettingsManager;
use anyhow::Result;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Интерфейс для управления ядром VPN.
pub trait VpnCore: Send + Sync + std::fmt::Debug {
    /// Запуск ядра с заданной конфигурацией.
    #[allow(dead_code)]
    fn start(&self, config_json: &str) -> Result<()>;
    /// Остановка ядра.
    #[allow(dead_code)]
    fn stop(&self) -> Result<()>;
    /// Проверка, запущено ли ядро.
    fn is_running(&self) -> bool;
}

/// Высокоуровневый бэкенд, взаимодействующий с привилегированным демоном.
#[derive(Debug)]
pub struct CoreBackend {
    rt: Arc<Runtime>,
    client: DaemonClient,
}

impl Default for CoreBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreBackend {
    pub fn new() -> Self {
        let rt = Runtime::new().expect("Failed to create tokio runtime");
        let client = DaemonClient::new();

        // --- Раздел: Проверка окружения ---
        let rt_clone = Arc::new(rt);
        let rt_bg = rt_clone.clone();
        let client_bg = client.clone();
        std::thread::spawn(move || {
            rt_bg.block_on(async {
                match client_bg.ping().await {
                    Ok(pong) => tracing::info!(
                        "REST API Daemon availability checked on initialization: {}",
                        pong
                    ),
                    Err(e) => {
                        tracing::warn!("REST API Daemon not available on initialization: {}", e)
                    }
                }
            });
        });
        // ================================

        Self {
            rt: rt_clone,
            client,
        }
    }

    pub fn update_system_proxy(&self, enable: bool) {
        use gtk::gio::Settings;
        use gtk::prelude::SettingsExt;

        let settings = Settings::new("org.gnome.system.proxy");
        let mode = if enable { "manual" } else { "none" };
        if let Err(e) = settings.set_string("mode", mode) {
            tracing::error!("Failed to set GNOME system proxy: {}", e);
        } else {
            tracing::info!("GNOME system proxy mode set to: {}", mode);
        }
    }
}

impl VpnCore for CoreBackend {
    /// Запускает ядро через REST API демон
    fn start(&self, config_json: &str) -> Result<()> {
        let settings = SettingsManager::new().load();
        let tun_mode = settings.tun_mode;

        // --- Раздел: Логирование для отладки (God Tier Backend) ---
        // Эти логи помогут детально отслеживать старт ядра в консоли.
        tracing::debug!(
            "Preparing to start proxy. Core Type: sing-box, TUN Mode: {}",
            tun_mode
        );
        tracing::debug!("Generated sing-box config:\n{}", config_json);

        tracing::info!("Requesting daemon to start proxy (core: sing-box)...");

        self.rt.block_on(async {
            self.client
                .start_proxy("sing-box".to_string(), config_json.to_string(), tun_mode)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to start proxy via Daemon REST API: {}", e);
                    anyhow::anyhow!("REST API error: {e}")
                })?;
            tracing::debug!("Proxy started successfully via Daemon");
            Ok(())
        })
    }

    /// Останавливает ядро через REST API демон
    fn stop(&self) -> Result<()> {
        tracing::info!("Requesting daemon to stop proxy...");

        self.rt.block_on(async {
            self.client
                .stop_proxy()
                .await
                .map_err(|e| anyhow::anyhow!("REST API error: {e}"))?;
            Ok(())
        })
    }

    fn is_running(&self) -> bool {
        self.rt
            .block_on(async { self.client.is_running().await.unwrap_or(false) })
    }
}
