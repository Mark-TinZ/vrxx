/* core.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Управление процессом ядра sing-box (Daemon Core Manager)
//!
//! Модуль отвечает за:
//! - Управление жизненным циклом дочернего процесса sing-box (запуск, мониторинг, корректное завершение SIGINT/SIGKILL)
//! - Создание и очистку временных файлов конфигурации (`config.json`)
//! - Асинхронное чтение stdout/stderr потоков ядра и парсинг логов ([`parse_singbox_log`])
//! - Автоматическую маршрутизацию и разделение логов по категориям (Core vs Access)
//! - Интеграцию с TUN-менеджером и Dns-менеджером для системного проксирования

use super::events::{DaemonEvent, EventManager, LogSource};
use super::{dns, network};

/// Очищает строку от ANSI escape-последовательностей.
fn clean_ansi(input: &str) -> String {
    static ANSI_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = ANSI_RE.get_or_init(|| {
        regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]")
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
    });
    re.replace_all(input, "").to_string()
}

/// Парсит строку лога из stdout/stderr sing-box, извлекая источник, уровень и очищенное сообщение.
/// Поддерживает как стандартный формат sing-box с таймзоной (+0600 YYYY-MM-DD HH:MM:SS LEVEL message),
/// так и формат скобок LEVEL[0000] и [LEVEL].
pub fn parse_singbox_log(raw_line: &str) -> (LogSource, String, String) {
    let clean = clean_ansi(raw_line).trim().to_string();
    if clean.is_empty() {
        return (LogSource::Core, "info".to_string(), String::new());
    }

    // Регулярное выражение для извлечения уровня логирования
    static LOG_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = LOG_REGEX.get_or_init(|| {
        regex::Regex::new(r"^(?:[+-]\d{4}\s+)?(?:\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\s+)?(?:\[)?([A-Za-z]+)(?:\]|\[\d+\])?\s*")
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
    });

    let mut level = "info".to_string();
    if let Some(caps) = re.captures(&clean) {
        if let Some(matched_level) = caps.get(1) {
            let lvl_upper = matched_level.as_str().to_uppercase();
            level = match lvl_upper.as_str() {
                "FATAL" | "PANIC" | "ERROR" => "error".to_string(),
                "WARN" | "WARNING" => "warning".to_string(),
                "DEBUG" => "debug".to_string(),
                "TRACE" => "debug".to_string(),
                "INFO" => "info".to_string(),
                _ => {
                    let upper = clean.to_uppercase();
                    if upper.contains("FATAL") || upper.contains("PANIC") || upper.contains("ERROR")
                    {
                        "error".to_string()
                    } else if upper.contains("WARN") || upper.contains("WARNING") {
                        "warning".to_string()
                    } else if upper.contains("DEBUG") || upper.contains("TRACE") {
                        "debug".to_string()
                    } else {
                        "info".to_string()
                    }
                }
            };
        }
    }

    // Определение источника (Core vs Access/Traffic)
    let clean_lower = clean.to_lowercase();
    let is_access = clean_lower.contains("router: match")
        || clean_lower.contains("router: route")
        || clean_lower.contains("accepted ")
        || clean_lower.contains("proxying ")
        || clean_lower.contains("->")
        || clean_lower.contains("connected to ")
        || clean_lower.contains("inbound connection")
        || clean_lower.contains("outbound connection")
        || clean_lower.contains("tunnel:");

    let source = if is_access {
        LogSource::Access
    } else {
        LogSource::Core
    };

    (source, level, clean)
}

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex};
use tokio::time::Duration;

use serde::{Deserialize, Serialize};

/// Структура запроса на запуск прокси-сервера.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartProxyRequest {
    /// Тип ядра (сейчас поддерживается только sing-box).
    pub core_type: String,
    /// Полный JSON конфигурации ядра.
    pub config_json: String,
    /// Флаг включения режима TUN (прозрачное проксирование всего трафика).
    pub tun_mode: bool,
}

/// Основной менеджер жизненного цикла прокси-процесса.
pub struct ProxyManager {
    child_pid: Arc<Mutex<Option<u32>>>,
    status: Arc<Mutex<String>>,
    event_manager: Arc<EventManager>,
    stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    dns_manager: Arc<Mutex<Option<dns::DnsManager>>>,
    tun_manager: Arc<Mutex<Option<network::TunManager>>>,
}

impl ProxyManager {
    /// Создает новый экземпляр ProxyManager.
    pub fn new(event_manager: Arc<EventManager>) -> Self {
        Self {
            child_pid: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Disconnected".to_string())),
            event_manager,
            stop_tx: Arc::new(Mutex::new(None)),
            dns_manager: Arc::new(Mutex::new(None)),
            tun_manager: Arc::new(Mutex::new(None)),
        }
    }

    /// Возвращает текущий статус ядра ("Connected", "Connecting", "Disconnected", "Error").
    pub async fn get_status(&self) -> String {
        let status = self.status.lock().await;
        status.clone()
    }

    /// Проверяет, запущен ли процесс ядра в данный момент.
    pub async fn is_running(&self) -> bool {
        let pid = self.child_pid.lock().await;
        pid.is_some()
    }

    /// Запускает процесс ядра с переданной конфигурацией.
    pub async fn start_proxy(
        &self,
        core_type: &str,
        config_json: &str,
        tun_mode: bool,
    ) -> anyhow::Result<()> {
        if self.is_running().await {
            tracing::warn!(
                "Proxy is already running. Stopping previous process before starting..."
            );
            self.stop_proxy().await?;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        {
            let mut status = self.status.lock().await;
            *status = "Connecting".to_string();
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged("Connecting".to_string()));
        }

        let bin_path = match super::updater::find_singbox_binary() {
            Some(path) => path,
            None => {
                let err_msg = "sing-box executable not found in system".to_string();
                tracing::error!("{}", err_msg);
                let mut status = self.status.lock().await;
                *status = "Error".to_string();
                self.event_manager
                    .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
                anyhow::bail!("{err_msg}");
            }
        };

        // Запись временного файла конфигурации
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("vrxx");
        tokio::fs::create_dir_all(&config_dir).await?;
        let config_path = config_dir.join("config.json");

        // Безопасное сохранение с правами 0o600 на Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.create(true).write(true).truncate(true).mode(0o600);
            let mut file = opts.open(&config_path)?;
            std::io::Write::write_all(&mut file, config_json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            tokio::fs::write(&config_path, config_json).await?;
        }

        // Настройка TUN интерфейса при необходимости
        if tun_mode {
            if core_type == "sing-box" {
                // Для sing-box сетевой интерфейс TUN и таблицы маршрутизации создаются и управляются
                // ядром sing-box самостоятельно благодаря auto_route: true и strict_route: true в config.json.
                // Выполняем превентивную очистку возможного сиротского интерфейса перед запуском.
                let _ = tokio::process::Command::new("ip")
                    .args(["link", "del", "vrxx-tun"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await;
                tracing::debug!("TUN interface and routing are managed natively by sing-box (auto_route: true).");
            } else {
                tracing::info!("Setting up TUN interface and DNS for proxy (manual mode)...");
                let mut tun_mgr = network::TunManager::new().await?;
                if let Err(e) = tun_mgr.setup().await {
                    tracing::error!("Failed to configure TUN interface: {}", e);
                    let mut status = self.status.lock().await;
                    *status = "Error".to_string();
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
                    anyhow::bail!("TUN interface configuration error: {e}");
                }

                if let Ok(dns_mgr) = dns::DnsManager::new().await {
                    if let Err(e) = dns_mgr
                        .set_dns(tun_mgr.if_index as i32, vec!["172.19.0.1".to_string()])
                        .await
                    {
                        tracing::warn!("Failed to configure DNS via systemd-resolved: {}", e);
                    }
                    let mut dns_guard = self.dns_manager.lock().await;
                    *dns_guard = Some(dns_mgr);
                }

                let mut tun_guard = self.tun_manager.lock().await;
                *tun_guard = Some(tun_mgr);
            }
        }

        tracing::info!(
            "Starting {} core at path: {:?} with config {:?}",
            core_type,
            bin_path,
            config_path
        );

        let mut child = Command::new(&bin_path)
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .env("ENABLE_DEPRECATED_LEGACY_DNS_FAKEIP_OPTIONS", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let child_pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("Failed to obtain child process PID"))?;
        tracing::info!("Core {} started with PID: {}", core_type, child_pid);

        {
            let mut pid_guard = self.child_pid.lock().await;
            *pid_guard = Some(child_pid);
        }

        {
            let mut status = self.status.lock().await;
            *status = "Connected".to_string();
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged("Connected".to_string()));
        }

        // Подготовка асинхронного логирования в файл core.log
        let log_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("vrxx")
            .join("logs");
        let _ = tokio::fs::create_dir_all(&log_dir).await;
        let log_file_path = log_dir.join("core.log");

        let log_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .await
            .ok();
        let log_file_arc = Arc::new(Mutex::new(log_file));

        // Поток чтения stdout
        if let Some(stdout) = child.stdout.take() {
            let event_manager = self.event_manager.clone();
            let log_file_ref = log_file_arc.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let (source, level, clean) = parse_singbox_log(&line);
                    if !clean.is_empty() {
                        event_manager.broadcast(DaemonEvent::Log {
                            source,
                            level,
                            message: clean.clone(),
                        });
                        let mut file_guard = log_file_ref.lock().await;
                        if let Some(file) = file_guard.as_mut() {
                            let _ = file.write_all(format!("{clean}\n").as_bytes()).await;
                        }
                    }
                }
            });
        }

        // Поток чтения stderr
        if let Some(stderr) = child.stderr.take() {
            let event_manager = self.event_manager.clone();
            let log_file_ref = log_file_arc.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let (source, level, clean) = parse_singbox_log(&line);
                    if !clean.is_empty() {
                        event_manager.broadcast(DaemonEvent::Log {
                            source,
                            level,
                            message: clean.clone(),
                        });
                        let mut file_guard = log_file_ref.lock().await;
                        if let Some(file) = file_guard.as_mut() {
                            let _ = file.write_all(format!("{clean}\n").as_bytes()).await;
                        }
                    }
                }
            });
        }

        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        {
            let mut guard = self.stop_tx.lock().await;
            *guard = Some(stop_tx);
        }

        // Фоновый мониторинг состояния процесса
        let status_arc = self.status.clone();
        let event_manager = self.event_manager.clone();
        let stop_tx_arc = self.stop_tx.clone();
        let dns_manager = self.dns_manager.clone();
        let tun_manager = self.tun_manager.clone();

        tokio::spawn(async move {
            tokio::select! {
                res = child.wait() => {
                    {
                        let mut guard = stop_tx_arc.lock().await;
                        *guard = None;
                    }
                    match res {
                        Ok(exit_status) => {
                            let mut status = status_arc.lock().await;
                            if *status != "Disconnecting" && *status != "Disconnected" {
                                tracing::warn!(
                                    "Core (PID {}) exited unexpectedly with status: {}",
                                    child_pid,
                                    exit_status
                                );
                                *status = "Error".to_string();
                                event_manager
                                    .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
                            } else {
                                tracing::debug!("Core (PID {}) exited with status: {}", child_pid, exit_status);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error waiting for core process (PID {}): {}", child_pid, e);
                            let mut status = status_arc.lock().await;
                            *status = "Error".to_string();
                            event_manager
                                .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
                        }
                    }
                }
                _ = stop_rx => {
                    tracing::debug!("Received stop signal for core (PID {})", child_pid);
                }
            }

            // Очистка сетевых ресурсов при завершении
            let mut dns_guard = dns_manager.lock().await;
            let mut tun_guard = tun_manager.lock().await;

            if let Some(tun) = tun_guard.as_mut() {
                if let Some(dns) = dns_guard.as_ref() {
                    let _ = dns.reset_dns(tun.if_index as i32).await;
                }
                let _ = tun.teardown().await;
            }
            *dns_guard = None;
            *tun_guard = None;

            // Гарантированная очистка интерфейса vrxx-tun при завершении процесса
            let _ = tokio::process::Command::new("ip")
                .args(["link", "del", "vrxx-tun"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;

            tracing::debug!("Background task for core PID {} finished", child_pid);
        });

        Ok(())
    }

    /// Выполняет корректную остановку процесса ядра sing-box.
    pub async fn stop_proxy(&self) -> anyhow::Result<()> {
        let pid_opt = {
            let guard = self.child_pid.lock().await;
            *guard
        };

        let pid = match pid_opt {
            Some(p) => p,
            None => {
                tracing::debug!("Proxy process is already stopped");
                let mut status = self.status.lock().await;
                *status = "Disconnected".to_string();
                return Ok(());
            }
        };

        {
            let mut status = self.status.lock().await;
            *status = "Disconnecting".to_string();
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged("Disconnecting".to_string()));
        }

        tracing::info!("Sending SIGINT signal to core process (PID {})...", pid);

        #[cfg(unix)]
        {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT);
        }

        // Ожидание завершения до 3 секунд перед SIGKILL
        let start = tokio::time::Instant::now();
        let mut exited = false;

        while start.elapsed() < Duration::from_secs(3) {
            #[cfg(unix)]
            {
                // Проверка существования процесса
                if signal::kill(Pid::from_raw(pid as i32), None).is_err() {
                    exited = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !exited {
            tracing::warn!(
                "Core process (PID {}) did not exit on SIGINT. Forcing SIGKILL...",
                pid
            );
            #[cfg(unix)]
            {
                let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
        }

        // Активация канала остановки для освобождения фонового таска
        if let Some(stop_tx) = self.stop_tx.lock().await.take() {
            let _ = stop_tx.send(());
        }

        {
            let mut pid_guard = self.child_pid.lock().await;
            *pid_guard = None;
        }

        // Сброс сетевых параметров
        {
            let mut dns_guard = self.dns_manager.lock().await;
            let mut tun_guard = self.tun_manager.lock().await;

            if let Some(tun) = tun_guard.as_mut() {
                if let Some(dns) = dns_guard.as_ref() {
                    let _ = dns.reset_dns(tun.if_index as i32).await;
                }
                let _ = tun.teardown().await;
            }
            *dns_guard = None;
            *tun_guard = None;
        }

        // Гарантированная очистка интерфейса vrxx-tun при остановке
        let _ = tokio::process::Command::new("ip")
            .args(["link", "del", "vrxx-tun"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        {
            let mut status = self.status.lock().await;
            *status = "Disconnected".to_string();
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged("Disconnected".to_string()));
        }

        tracing::info!("Proxy stopped successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_singbox_log_formats() {
        let (source, level, msg) = parse_singbox_log(
            "+0600 2026-08-15 21:48:00 INFO network: updated default interface wlan0, index 2",
        );
        assert_eq!(level, "info");
        assert_eq!(source, LogSource::Core);
        assert_eq!(
            msg,
            "+0600 2026-08-15 21:48:00 INFO network: updated default interface wlan0, index 2"
        );

        let (source, level, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:08 DEBUG [2869722053 0ms] dns: lookup domain fra-8dd974.wb-cdn-global.com",
        );
        assert_eq!(level, "debug");
        assert_eq!(source, LogSource::Core);

        let (source, level, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:08 INFO [2869722053 0ms] inbound/socks[socks-in]: inbound connection from 127.0.0.1:54220",
        );
        assert_eq!(level, "info");
        assert_eq!(source, LogSource::Access);

        // Слово "error" в информационном сообщении НЕ должно вызывать уровень ERROR
        let (_, level_info, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:00 INFO network: recovered from previous error safely",
        );
        assert_eq!(level_info, "info");
    }

    #[test]
    fn test_parse_singbox_log_access_classification() {
        let (source, _, _) =
            parse_singbox_log("2026-08-15 12:00:00 INFO [TCP] 127.0.0.1:54321 -> 1.1.1.1:443");
        assert_eq!(source, LogSource::Access);

        let (source, _, _) =
            parse_singbox_log("inbound/tun[tun-in] accepted tcp:192.168.1.5:12345");
        assert_eq!(source, LogSource::Access);

        let (source, _, _) =
            parse_singbox_log("router: match[0] rule[1] inbound=socks-in -> proxy");
        assert_eq!(source, LogSource::Access);
    }
}
