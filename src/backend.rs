/* backend.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Бэкенд управления ядром VPN и системным окружением (CoreBackend)
//!
//! Отвечает за:
//! - Взаимодействие GUI приложения с системным демоном через [`DaemonClient`]
//! - Определение графического окружения рабочего стола (GNOME, KDE Plasma, XFCE, Sway)
//! - Безопасную установку и сброс системного прокси GNOME GSettings без зависания UI
//! - Экспорт переменных окружения `HTTP_PROXY` и `HTTPS_PROXY` для текущего процесса

use crate::ipc::DaemonClient;
use crate::settings::SettingsManager;
use anyhow::Result;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Интерфейс для абстрактного управления ядром VPN.
pub trait VpnCore: Send + Sync + std::fmt::Debug {
    /// Запуск ядра с заданной JSON конфигурацией.
    #[allow(dead_code)]
    fn start(&self, config_json: &str) -> Result<()>;
    /// Остановка запущенного ядра.
    #[allow(dead_code)]
    fn stop(&self) -> Result<()>;
    /// Проверка активного состояния ядра.
    #[allow(dead_code)]
    fn is_running(&self) -> bool;
}

/// Поддерживаемые типы окружения рабочего стола Linux.
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

/// Определяет текущую рабочую среду пользователя по переменным `XDG_CURRENT_DESKTOP` / `XDG_SESSION_DESKTOP`.
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

/// Безопасно проверяет доступность схемы GSettings `org.gnome.system.proxy` без паники.
#[allow(dead_code)]
pub fn is_gnome_proxy_schema_available() -> bool {
    if let Some(source) = gtk::gio::SettingsSchemaSource::default() {
        source.lookup("org.gnome.system.proxy", true).is_some()
    } else {
        false
    }
}

/// Результат применения настроек системного прокси.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemProxyResult {
    /// Успешно применено в GSettings
    Success,
    /// Схема GSettings отсутствует в данном десктопном окружении (требуется fallback на TUN)
    SchemaUnavailable { desktop: DesktopEnvironment },
    /// Ошибка записи параметров
    Error(String),
}

/// Устанавливает или удаляет переменные окружения `HTTP_PROXY` и `HTTPS_PROXY` для текущего процесса.
pub fn set_process_proxy_env(http_port: u16, enable: bool) {
    if enable {
        let proxy_val = format!("http://127.0.0.1:{}", http_port);
        std::env::set_var("HTTP_PROXY", &proxy_val);
        std::env::set_var("HTTPS_PROXY", &proxy_val);
        std::env::set_var("http_proxy", &proxy_val);
        std::env::set_var("https_proxy", &proxy_val);
        tracing::info!(
            "Set environment variables HTTP_PROXY and HTTPS_PROXY: {}",
            proxy_val
        );
    } else {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("http_proxy");
        std::env::remove_var("https_proxy");
        tracing::info!("Cleared environment variables HTTP_PROXY and HTTPS_PROXY");
    }
}

/// Возвращает текстовую shell-команду для экспорта переменных окружения прокси в терминале.
#[allow(dead_code)]
pub fn get_proxy_env_export_cmd(http_port: u16) -> String {
    format!(
        "export HTTP_PROXY=http://127.0.0.1:{} HTTPS_PROXY=http://127.0.0.1:{}",
        http_port, http_port
    )
}

/// Высокоуровневый бэкенд, связывающий GUI с REST API системного демона.
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
    /// Создает экземпляр бэкенда, запускает внутренний Tokio Runtime и фоновую проверку доступности демона.
    pub fn new() -> Self {
        let client = DaemonClient::new();
        let rt = match Runtime::new() {
            Ok(r) => Arc::new(r),
            Err(e) => {
                tracing::error!(
                    "Не удалось создать multi-thread Tokio Runtime для CoreBackend: {e}"
                );
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(current_rt) => Arc::new(current_rt),
                    Err(err) => {
                        tracing::error!(
                            "Критическая ошибка создания current_thread Tokio Runtime: {err}"
                        );
                        // Возвращаем дефолтный пустой runtime через builders
                        Arc::new(
                            tokio::runtime::Builder::new_current_thread()
                                .build()
                                .unwrap_or_else(|_| {
                                    tracing::error!("Фатальный сбой runtime");
                                    Runtime::new().unwrap_or_else(|_| std::process::exit(1))
                                }),
                        )
                    }
                }
            }
        };

        // Фоновая проверка доступности демона без блокировки основного потока
        let rt_bg = rt.clone();
        let client_bg = client.clone();
        std::thread::spawn(move || {
            rt_bg.block_on(async {
                match client_bg.ping().await {
                    Ok(pong) => tracing::info!(
                        "REST API системного демона успешно проверено при старте: {}",
                        pong
                    ),
                    Err(e) => {
                        tracing::warn!("REST API системного демона недоступно при старте: {}", e)
                    }
                }
            });
        });

        Self { rt, client }
    }

    /// Безопасное переключение системного прокси с поддержкой кросс-десктопных сред (GNOME/KDE/XFCE/Sway).
    /// Является статической функцией, не создающей экземпляр Tokio Runtime.
    pub fn update_system_proxy(enable: bool) -> SystemProxyResult {
        use gtk::gio::{Settings, SettingsBackend, SettingsSchemaSource};
        use gtk::prelude::SettingsExt;

        let desktop = detect_desktop_environment();
        let app_settings = SettingsManager::new().load();

        tracing::info!(
            "Updating system proxy. Desktop: {:?}, enable: {}, SOCKS: {}, HTTP: {}",
            desktop,
            enable,
            app_settings.socks_port,
            app_settings.http_port
        );

        let source = match SettingsSchemaSource::default() {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "GSettingsSchemaSource default is unavailable. Safe fallback activated."
                );
                return SystemProxyResult::SchemaUnavailable { desktop };
            }
        };

        let schema = match source.lookup("org.gnome.system.proxy", true) {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "GSettings schema 'org.gnome.system.proxy' missing in environment '{}'. Safe fallback activated.",
                    desktop
                );
                return SystemProxyResult::SchemaUnavailable { desktop };
            }
        };

        let settings = Settings::new_full(&schema, None::<&SettingsBackend>, None);

        let res = if enable {
            // Настройка SOCKS5 прокси
            if let Some(socks_schema) = source.lookup("org.gnome.system.proxy.socks", true) {
                let socks = Settings::new_full(&socks_schema, None::<&SettingsBackend>, None);
                let _ = socks.set_string("host", "127.0.0.1");
                let _ = socks.set_int("port", app_settings.socks_port as i32);
            }
            // Настройка HTTP прокси
            if let Some(http_schema) = source.lookup("org.gnome.system.proxy.http", true) {
                let http = Settings::new_full(&http_schema, None::<&SettingsBackend>, None);
                let _ = http.set_string("host", "127.0.0.1");
                let _ = http.set_int("port", app_settings.http_port as i32);
                let _ = http.set_boolean("enabled", true);
            }
            // Настройка HTTPS прокси
            if let Some(https_schema) = source.lookup("org.gnome.system.proxy.https", true) {
                let https = Settings::new_full(&https_schema, None::<&SettingsBackend>, None);
                let _ = https.set_string("host", "127.0.0.1");
                let _ = https.set_int("port", app_settings.http_port as i32);
            }

            let mode = "manual";
            if let Err(e) = settings.set_string("mode", mode) {
                tracing::error!("Failed to set GNOME system proxy mode: {}", e);
                SystemProxyResult::Error(e.to_string())
            } else {
                tracing::info!(
                    "GNOME system proxy mode successfully set to 'manual' (SOCKS: {}, HTTP: {})",
                    app_settings.socks_port,
                    app_settings.http_port
                );
                SystemProxyResult::Success
            }
        } else {
            let mode = "none";
            if let Err(e) = settings.set_string("mode", mode) {
                tracing::error!("Failed to reset GNOME system proxy mode: {}", e);
                SystemProxyResult::Error(e.to_string())
            } else {
                tracing::info!("GNOME system proxy mode successfully reset to '{}'", mode);
                SystemProxyResult::Success
            }
        };

        // Запуск команд gsettings CLI в фоновом потоке, чтобы никогда не блокировать главный GTK поток
        let socks_port_str = app_settings.socks_port.to_string();
        let http_port_str = app_settings.http_port.to_string();
        std::thread::spawn(move || {
            if enable {
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy.socks", "host", "127.0.0.1"])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args([
                        "set",
                        "org.gnome.system.proxy.socks",
                        "port",
                        &socks_port_str,
                    ])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy.http", "port", &http_port_str])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy.https", "host", "127.0.0.1"])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args([
                        "set",
                        "org.gnome.system.proxy.https",
                        "port",
                        &http_port_str,
                    ])
                    .output();
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy", "mode", "manual"])
                    .output();
            } else {
                let _ = std::process::Command::new("gsettings")
                    .args(["set", "org.gnome.system.proxy", "mode", "none"])
                    .output();
            }
        });

        res
    }
}

impl VpnCore for CoreBackend {
    /// Отправляет команду демону на запуск ядра sing-box с переданной JSON-конфигурацией.
    fn start(&self, config_json: &str) -> Result<()> {
        let settings = SettingsManager::new().load();
        let tun_mode = settings.tun_mode;

        tracing::debug!("Preparing to start sing-box core. TUN mode: {}", tun_mode);
        tracing::debug!("Generated sing-box JSON config:\n{}", config_json);

        tracing::info!("Sending request to daemon to start proxy (sing-box core)...");

        self.rt.block_on(async {
            self.client
                .start_proxy("sing-box".to_string(), config_json.to_string(), tun_mode)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to start proxy via daemon REST API: {}", e);
                    anyhow::anyhow!("REST API error: {e}")
                })?;
            tracing::debug!("Proxy started successfully via system daemon");
            Ok(())
        })
    }

    /// Отправляет команду демону на остановку ядра sing-box.
    fn stop(&self) -> Result<()> {
        tracing::info!("Sending request to daemon to stop proxy...");

        self.rt.block_on(async {
            self.client
                .stop_proxy()
                .await
                .map_err(|e| anyhow::anyhow!("Ошибка REST API: {e}"))?;
            Ok(())
        })
    }

    /// Проверяет статус активности ядра в системном демоне.
    fn is_running(&self) -> bool {
        self.rt
            .block_on(async { self.client.is_running().await.unwrap_or(false) })
    }
}
