use crate::ipc::DaemonProxy;
use crate::settings::SettingsManager;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::runtime::Runtime;

pub trait VpnCore: Send + Sync + std::fmt::Debug {
    #[allow(dead_code)]
    fn start(&self, config_json: &str) -> Result<()>;
    #[allow(dead_code)]
    fn stop(&self) -> Result<()>;
    fn is_running(&self) -> bool;
}

#[derive(Debug)]
pub struct CoreBackend {
    rt: Arc<Runtime>,
}

impl Default for CoreBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreBackend {
    pub fn new() -> Self {
        let rt = Runtime::new().expect("Failed to create tokio runtime");

        // --- Раздел: Проверка окружения ---
        // HACK: Фоновая проверка доступности демона при инициализации.
        // Это позволяет избежать подвисания UI, если D-Bus недоступен.
        let rt_clone = Arc::new(rt);
        let rt_bg = rt_clone.clone();
        std::thread::spawn(move || {
            rt_bg.block_on(async {
                match crate::ipc::get_system_connection().await {
                    Ok(conn) => match DaemonProxy::new(&conn).await {
                        Ok(proxy) => match proxy.ping().await {
                            Ok(pong) => tracing::info!(
                                "D-Bus Daemon availability checked on initialization: {}",
                                pong
                            ),
                            Err(e) => tracing::warn!(
                                "D-Bus Daemon not available on initialization: {}",
                                e
                            ),
                        },
                        Err(e) => {
                            tracing::warn!("Failed to create DaemonProxy on initialization: {}", e)
                        }
                    },
                    Err(e) => tracing::warn!(
                        "Failed to connect to D-Bus System Bus on initialization: {}",
                        e
                    ),
                }
            });
        });
        // ================================

        Self { rt: rt_clone }
    }

    // --- Раздел: IPC Взаимодействие ---
    // OPTIMIZE: Cache D-Bus DaemonProxy here instead of recreating it per proxy call (reduces D-Bus overhead during is_running polling)
    async fn get_proxy(&self) -> Result<DaemonProxy<'static>> {
        crate::ipc::get_daemon_proxy()
            .await
            .context("Failed to get DaemonProxy")
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
    /// Запускает ядро через привилегированный демон
    fn start(&self, config_json: &str) -> Result<()> {
        let settings = SettingsManager::new().load();
        let core_type = settings.core.clone();
        let tun_mode = settings.tun_mode;

        tracing::info!("Requesting daemon to start proxy (core: {})...", core_type);

        self.rt.block_on(async {
            let proxy = self.get_proxy().await?;
            proxy
                .start_proxy(core_type, config_json.to_string(), tun_mode)
                .await
                .map_err(|e| anyhow::anyhow!("D-Bus error: {e}"))?;
            Ok(())
        })
    }

    /// Останавливает ядро через привилегированный демон
    fn stop(&self) -> Result<()> {
        tracing::info!("Requesting daemon to stop proxy...");

        self.rt.block_on(async {
            let proxy = self.get_proxy().await?;
            proxy
                .stop_proxy()
                .await
                .map_err(|e| anyhow::anyhow!("D-Bus error: {e}"))?;
            Ok(())
        })
    }

    fn is_running(&self) -> bool {
        self.rt.block_on(async {
            if let Ok(proxy) = self.get_proxy().await {
                proxy.is_running().await.unwrap_or(false)
            } else {
                false
            }
        })
    }
}
