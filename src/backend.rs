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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Xfce,
    Sway,
    Other(String),
}

impl std::fmt::Display for DesktopEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gnome => write!(f, "GNOME"),
            Self::Kde => write!(f, "KDE Plasma"),
            Self::Xfce => write!(f, "XFCE"),
            Self::Sway => write!(f, "Sway"),
            Self::Other(name) => write!(f, "{}", name),
        }
    }
}

/// Определяет текущую рабочую среду окружения на основе XDG_CURRENT_DESKTOP.
pub fn detect_desktop_environment() -> DesktopEnvironment {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .unwrap_or_default()
        .to_uppercase();

    if desktop.contains("GNOME") {
        DesktopEnvironment::Gnome
    } else if desktop.contains("KDE") {
        DesktopEnvironment::Kde
    } else if desktop.contains("XFCE") {
        DesktopEnvironment::Xfce
    } else if desktop.contains("SWAY") {
        DesktopEnvironment::Sway
    } else if desktop.is_empty() {
        DesktopEnvironment::Other("Unknown".to_string())
    } else {
        DesktopEnvironment::Other(desktop)
    }
}

/// Безопасно проверяет доступность схемы GSettings `org.gnome.system.proxy` без вызова паники.
#[allow(dead_code)]
pub fn is_gnome_proxy_schema_available() -> bool {
    if let Some(source) = gtk::gio::SettingsSchemaSource::default() {
        source.lookup("org.gnome.system.proxy", true).is_some()
    } else {
        false
    }
}

/// Результат установки системного прокси.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemProxyResult {
    Success,
    SchemaUnavailable { desktop: DesktopEnvironment },
    Error(String),
}

/// Устанавливает переменные окружения HTTP_PROXY и HTTPS_PROXY для текущего процесса.
pub fn set_process_proxy_env(http_port: u16, enable: bool) {
    if enable {
        let proxy_val = format!("http://127.0.0.1:{}", http_port);
        std::env::set_var("HTTP_PROXY", &proxy_val);
        std::env::set_var("HTTPS_PROXY", &proxy_val);
        std::env::set_var("http_proxy", &proxy_val);
        std::env::set_var("https_proxy", &proxy_val);
        tracing::info!(
            "Set HTTP_PROXY and HTTPS_PROXY environment variables to {}",
            proxy_val
        );
    } else {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("http_proxy");
        std::env::remove_var("https_proxy");
        tracing::info!("Cleared HTTP_PROXY and HTTPS_PROXY environment variables");
    }
}

/// Возвращает текстовую команду экпорта переменных окружения для терминала.
#[allow(dead_code)]
pub fn get_proxy_env_export_cmd(http_port: u16) -> String {
    format!(
        "export HTTP_PROXY=http://127.0.0.1:{} HTTPS_PROXY=http://127.0.0.1:{}",
        http_port, http_port
    )
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

    /// Безопасное переключение системного прокси с поддержкой кросс-десктопных сред (KDE/XFCE/Sway).
    pub fn update_system_proxy(&self, enable: bool) -> SystemProxyResult {
        use gtk::gio::{Settings, SettingsBackend, SettingsSchemaSource};
        use gtk::prelude::SettingsExt;

        let desktop = detect_desktop_environment();
        tracing::info!(
            "Updating system proxy. Desktop: {:?}, target state: {}",
            desktop,
            enable
        );

        let source = match SettingsSchemaSource::default() {
            Some(s) => s,
            None => {
                tracing::warn!("Default GSettingsSchemaSource is None. Safe fallback active.");
                return SystemProxyResult::SchemaUnavailable { desktop };
            }
        };

        let schema = match source.lookup("org.gnome.system.proxy", true) {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "GSettings schema 'org.gnome.system.proxy' is missing on desktop '{}'. Safe fallback active.",
                    desktop
                );
                return SystemProxyResult::SchemaUnavailable { desktop };
            }
        };

        let settings = Settings::new_full(&schema, None::<&SettingsBackend>, None);
        let mode = if enable { "manual" } else { "none" };
        if let Err(e) = settings.set_string("mode", mode) {
            tracing::error!("Failed to set GNOME system proxy: {}", e);
            SystemProxyResult::Error(e.to_string())
        } else {
            tracing::info!("GNOME system proxy mode successfully set to: {}", mode);
            SystemProxyResult::Success
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
