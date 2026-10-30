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
//! - 4-уровневую валидацию конфигурации (Semantic Profile, Options, In-daemon Synthesis, Pre-flight sing-box check)
//! - Безопасную запись конфигурации с правами 0600 (/run/vrxx/config.json)
//! - Асинхронное чтение stdout/stderr потоков ядра и парсинг логов ([`parse_singbox_log`])
//! - Автоматическую маршрутизацию и разделение логов по категориям (Core vs Access)
//! - Интеграцию с TUN-менеджером и Dns-менеджером для системного проксирования

use super::events::{DaemonEvent, EventManager, LogSource};
use super::{dns, network};
pub use crate::protocol::StartProxyRequest;
use crate::protocol::{CoreType, DaemonStatus, LogLevel};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex};
use tokio::time::Duration;

/// Определяет путь к файлу конфигурации демона с учетом системных директорий и прав доступа.
pub fn resolve_daemon_config_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("VRXX_CONFIG_PATH") {
        return PathBuf::from(override_path);
    }

    // Вспомогательная функция проверки доступности записи
    let can_write_dir = |dir: &Path| -> bool {
        if !dir.exists() && std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        let test_file = dir.join(".write_test");
        if std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&test_file)
            .is_ok()
        {
            let _ = std::fs::remove_file(&test_file);
            true
        } else {
            false
        }
    };

    // 1. Приоритетный путь: /run/vrxx/config.json (стандарт systemd RuntimeDirectory)
    let run_dir = Path::new("/run/vrxx");
    if can_write_dir(run_dir) {
        return run_dir.join("config.json");
    }

    // 2. Пользовательский runtime directory (/run/user/<uid>/vrxx)
    if let Some(user_run) = dirs::runtime_dir() {
        let dir = user_run.join("vrxx");
        if can_write_dir(&dir) {
            return dir.join("config.json");
        }
    }

    // 3. Fallback: /tmp/vrxx
    let tmp_dir = Path::new("/tmp/vrxx");
    if can_write_dir(tmp_dir) {
        return tmp_dir.join("config.json");
    }

    // 4. Fallback: ~/.config/vrxx
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vrxx")
        .join("config.json")
}

/// Парсит строку лога из stdout/stderr sing-box, извлекая источник, уровень и очищенное сообщение.
pub fn parse_singbox_log(raw_line: &str) -> (LogSource, LogLevel, String) {
    let clean = crate::logger::clean_ansi(raw_line).trim().to_string();
    if clean.is_empty() {
        return (LogSource::Core, LogLevel::Info, String::new());
    }

    static LOG_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = LOG_REGEX.get_or_init(|| {
        regex::Regex::new(r"^(?:[+-]\d{4}\s+)?(?:\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\s+)?(?:\[)?([A-Za-z]+)(?:\]|\[\d+\])?\s*")
            .unwrap_or_else(|_| regex::Regex::new("$^").unwrap_or_else(|_| unreachable!()))
    });

    let mut level = LogLevel::Info;
    if let Some(caps) = re.captures(&clean) {
        if let Some(matched_level) = caps.get(1) {
            let lvl_upper = matched_level.as_str().to_uppercase();
            level = match lvl_upper.as_str() {
                "FATAL" | "PANIC" | "ERROR" => LogLevel::Error,
                "WARN" | "WARNING" => LogLevel::Warning,
                "DEBUG" => LogLevel::Debug,
                "TRACE" => LogLevel::Trace,
                "INFO" => LogLevel::Info,
                _ => {
                    let upper = clean.to_uppercase();
                    if upper.contains("FATAL") || upper.contains("PANIC") || upper.contains("ERROR")
                    {
                        LogLevel::Error
                    } else if upper.contains("WARN") || upper.contains("WARNING") {
                        LogLevel::Warning
                    } else if upper.contains("DEBUG") || upper.contains("TRACE") {
                        LogLevel::Debug
                    } else {
                        LogLevel::Info
                    }
                }
            };
        }
    }

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

/// Хранилище последних зафиксированных счетчиков трафика сессии.
#[derive(Debug, Default)]
struct TelemetryState {
    down_total: u64,
    up_total: u64,
}

/// Основной менеджер жизненного цикла прокси-процесса.
pub struct ProxyManager {
    child_pid: Arc<Mutex<Option<u32>>>,
    status: Arc<Mutex<DaemonStatus>>,
    event_manager: Arc<EventManager>,
    stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    dns_manager: Arc<Mutex<Option<dns::DnsManager>>>,
    tun_manager: Arc<Mutex<Option<network::TunManager>>>,
    telemetry_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    telemetry_state: Arc<Mutex<TelemetryState>>,
}

impl ProxyManager {
    /// Создает новый экземпляр ProxyManager.
    pub fn new(event_manager: Arc<EventManager>) -> Self {
        Self {
            child_pid: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(DaemonStatus::Disconnected)),
            event_manager,
            stop_tx: Arc::new(Mutex::new(None)),
            dns_manager: Arc::new(Mutex::new(None)),
            tun_manager: Arc::new(Mutex::new(None)),
            telemetry_handle: Arc::new(Mutex::new(None)),
            telemetry_state: Arc::new(Mutex::new(TelemetryState::default())),
        }
    }

    /// Корректно останавливает задачу опроса телеметрии Clash API и рассылает событие нулевой скорости.
    async fn stop_telemetry(&self) {
        if let Some(handle) = self.telemetry_handle.lock().await.take() {
            handle.abort();
        }
        let (down_total, up_total) = {
            let state = self.telemetry_state.lock().await;
            (state.down_total, state.up_total)
        };
        self.event_manager.broadcast(DaemonEvent::Traffic {
            down_speed: 0,
            up_speed: 0,
            down_total,
            up_total,
        });
    }

    /// Возвращает текущий строго типизированный статус ядра.
    pub async fn get_status(&self) -> DaemonStatus {
        let status = self.status.lock().await;
        *status
    }

    /// Проверяет, запущен ли процесс ядра в данный момент.
    pub async fn is_running(&self) -> bool {
        let pid = self.child_pid.lock().await;
        pid.is_some()
    }

    /// Запускает процесс ядра с выполнением 4-уровневого конвейера валидации.
    pub async fn start_proxy(&self, request: StartProxyRequest) -> anyhow::Result<()> {
        let StartProxyRequest {
            core_type,
            profile,
            options,
        } = request;

        if core_type != CoreType::SingBox {
            anyhow::bail!("Unsupported proxy core type: {:?}", core_type);
        }

        if self.is_running().await {
            tracing::warn!(
                "Proxy is already running. Stopping previous process before starting..."
            );
            self.stop_proxy().await?;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        {
            let mut status = self.status.lock().await;
            *status = DaemonStatus::Connecting;
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Connecting));
        }

        // =========================================================================
        // TIER 1: SEMANTIC PROFILE VALIDATION
        // =========================================================================
        if let Err(e) = profile.validate() {
            let err_msg = format!("Tier 1 profile validation failed: {e}");
            tracing::warn!("{err_msg}");
            {
                let mut status = self.status.lock().await;
                *status = DaemonStatus::Error;
                self.event_manager
                    .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
            }
            anyhow::bail!("{err_msg}");
        }

        // =========================================================================
        // TIER 2: CONFIGURATION OPTIONS VALIDATION
        // =========================================================================
        if let Err(e) = options.validate() {
            let err_msg = format!("Tier 2 configuration options validation failed: {e}");
            tracing::warn!("{err_msg}");
            {
                let mut status = self.status.lock().await;
                *status = DaemonStatus::Error;
                self.event_manager
                    .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
            }
            anyhow::bail!("{err_msg}");
        }

        let bin_path = match super::updater::find_singbox_binary() {
            Some(path) => path,
            None => {
                let err_msg = "sing-box executable not found in system".to_string();
                tracing::error!("{}", err_msg);
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("{err_msg}");
            }
        };

        // =========================================================================
        // TIER 3: INTERNAL CONFIG SYNTHESIS & SECURE STAGING WRITE (0600)
        // =========================================================================
        let config_json = crate::domain::singbox_config::build_singbox_config(&profile, &options);
        let config_path = resolve_daemon_config_path();

        if let Some(parent) = config_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                let err_msg = format!("Failed to create config directory {:?}: {e}", parent);
                tracing::error!("{err_msg}");
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("{err_msg}");
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.create(true).write(true).truncate(true).mode(0o600);
            let mut file = match opts.open(&config_path) {
                Ok(f) => f,
                Err(e) => {
                    let err_msg = format!(
                        "Failed to open config file {:?} for staging write: {e}",
                        config_path
                    );
                    tracing::error!("{err_msg}");
                    {
                        let mut status = self.status.lock().await;
                        *status = DaemonStatus::Error;
                        self.event_manager
                            .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                    }
                    anyhow::bail!("{err_msg}");
                }
            };
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600));
            use std::io::Write;
            if let Err(e) = file
                .write_all(config_json.as_bytes())
                .and_then(|_| file.sync_all())
            {
                let err_msg = format!("Failed to write config content to {:?}: {e}", config_path);
                tracing::error!("{err_msg}");
                let _ = tokio::fs::remove_file(&config_path).await;
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("{err_msg}");
            }
        }
        #[cfg(not(unix))]
        {
            if let Err(e) = tokio::fs::write(&config_path, config_json.as_bytes()).await {
                let err_msg = format!("Failed to write config to {:?}: {e}", config_path);
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("{err_msg}");
            }
        }

        // =========================================================================
        // TIER 4: NATIVE PRE-FLIGHT VALIDATION (sing-box check -c <file>)
        // =========================================================================
        tracing::debug!(
            "Executing Tier 4 pre-flight check: {:?} check -c {:?}",
            bin_path,
            config_path
        );
        let check_output = tokio::process::Command::new(&bin_path)
            .arg("check")
            .arg("-c")
            .arg(&config_path)
            .env("ENABLE_DEPRECATED_LEGACY_DNS_FAKEIP_OPTIONS", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match check_output {
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let diag = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else {
                    stdout.trim().to_string()
                };
                tracing::error!("Tier 4 pre-flight check failed:\n{}", diag);
                let _ = tokio::fs::remove_file(&config_path).await;
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("Pre-flight check failed (Tier 4): {}", diag);
            }
            Err(e) => {
                let err_msg = format!("Failed to execute pre-flight check command: {e}");
                tracing::error!("{err_msg}");
                let _ = tokio::fs::remove_file(&config_path).await;
                {
                    let mut status = self.status.lock().await;
                    *status = DaemonStatus::Error;
                    self.event_manager
                        .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                }
                anyhow::bail!("{err_msg}");
            }
            Ok(_) => {
                tracing::info!(
                    "Tier 4 pre-flight validation succeeded for {:?}",
                    config_path
                );
            }
        }

        // =========================================================================
        // 5. SUPERVISED PROCESS EXECUTION
        // =========================================================================
        if options.tun_mode {
            let _ = tokio::process::Command::new("ip")
                .args(["link", "del", "vrxx-tun"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
            tracing::debug!("TUN interface cleaned prior to sing-box launch");
        }

        tracing::info!(
            "Starting {:?} core at path: {:?} with config {:?}",
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
        tracing::info!("Core {:?} started with PID: {}", core_type, child_pid);

        {
            let mut pid_guard = self.child_pid.lock().await;
            *pid_guard = Some(child_pid);
        }

        {
            let mut status = self.status.lock().await;
            *status = DaemonStatus::Connected;
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Connected));
        }

        // =========================================================================
        // ЗАПУСК ФОНОВОЙ ТЕЛЕМЕТРИИ CLASH API (127.0.0.1:9090)
        // =========================================================================
        self.stop_telemetry().await;
        {
            let mut state = self.telemetry_state.lock().await;
            *state = TelemetryState::default();
        }

        let telemetry_event_mgr = self.event_manager.clone();
        let telemetry_state_arc = self.telemetry_state.clone();
        let clash_api_base = std::env::var("VRXX_CLASH_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());
        let connections_url = format!("{clash_api_base}/connections");

        let telemetry_task = tokio::spawn(async move {
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_millis(800))
                .connect_timeout(Duration::from_millis(500))
                .no_proxy()
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to create HTTP client for Clash API telemetry: {e}");
                    return;
                }
            };

            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut last_tick = tokio::time::Instant::now();
            let mut prev_down: u64 = 0;
            let mut prev_up: u64 = 0;
            let mut initialized = false;

            loop {
                interval.tick().await;

                match client.get(&connections_url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            match resp.json::<serde_json::Value>().await {
                                Ok(json) => {
                                    let down_total = json
                                        .get("downloadTotal")
                                        .or_else(|| json.get("download_total"))
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let up_total = json
                                        .get("uploadTotal")
                                        .or_else(|| json.get("upload_total"))
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);

                                    let now = tokio::time::Instant::now();
                                    let dt = now.duration_since(last_tick).as_secs_f64();

                                    let (down_speed, up_speed) = if initialized && dt > 0.0 {
                                        let ds = ((down_total.saturating_sub(prev_down)) as f64
                                            / dt)
                                            .round()
                                            as u64;
                                        let us = ((up_total.saturating_sub(prev_up)) as f64 / dt)
                                            .round()
                                            as u64;
                                        (ds, us)
                                    } else {
                                        (0, 0)
                                    };

                                    prev_down = down_total;
                                    prev_up = up_total;
                                    last_tick = now;
                                    initialized = true;

                                    {
                                        let mut state = telemetry_state_arc.lock().await;
                                        state.down_total = down_total;
                                        state.up_total = up_total;
                                    }

                                    telemetry_event_mgr.broadcast(DaemonEvent::Traffic {
                                        down_speed,
                                        up_speed,
                                        down_total,
                                        up_total,
                                    });
                                }
                                Err(e) => {
                                    tracing::trace!(
                                        "Failed to parse Clash API telemetry JSON: {e}"
                                    );
                                }
                            }
                        } else {
                            tracing::trace!(
                                "Clash API telemetry returned status: {}",
                                resp.status()
                            );
                        }
                    }
                    Err(e) => {
                        if e.is_connect() || e.is_timeout() {
                            tracing::trace!("Clash API telemetry not yet reachable: {e}");
                        } else {
                            tracing::debug!("Error polling Clash API telemetry: {e}");
                        }
                    }
                }
            }
        });

        {
            let mut handle_guard = self.telemetry_handle.lock().await;
            *handle_guard = Some(telemetry_task);
        }

        let log_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
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

        let status_arc = self.status.clone();
        let event_manager = self.event_manager.clone();
        let stop_tx_arc = self.stop_tx.clone();
        let child_pid_arc = self.child_pid.clone();
        let dns_manager = self.dns_manager.clone();
        let tun_manager = self.tun_manager.clone();
        let telemetry_handle_arc = self.telemetry_handle.clone();
        let telemetry_state_arc = self.telemetry_state.clone();

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
                            if *status != DaemonStatus::Disconnecting && *status != DaemonStatus::Disconnected {
                                tracing::warn!(
                                    "Core (PID {}) exited unexpectedly with status: {}",
                                    child_pid,
                                    exit_status
                                );
                                *status = DaemonStatus::Error;
                                event_manager
                                    .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                            } else {
                                tracing::debug!("Core (PID {}) exited with status: {}", child_pid, exit_status);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error waiting for core process (PID {}): {}", child_pid, e);
                            let mut status = status_arc.lock().await;
                            *status = DaemonStatus::Error;
                            event_manager
                                .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Error));
                        }
                    }
                }
                _ = stop_rx => {
                    tracing::debug!("Received stop signal for core (PID {})", child_pid);
                }
            }

            {
                let mut pid_guard = child_pid_arc.lock().await;
                *pid_guard = None;
            }

            // Остановка телеметрии при завершении процесса ядра
            if let Some(handle) = telemetry_handle_arc.lock().await.take() {
                handle.abort();
            }
            let (down_total, up_total) = {
                let state = telemetry_state_arc.lock().await;
                (state.down_total, state.up_total)
            };
            event_manager.broadcast(DaemonEvent::Traffic {
                down_speed: 0,
                up_speed: 0,
                down_total,
                up_total,
            });

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
                self.stop_telemetry().await;
                let mut status = self.status.lock().await;
                *status = DaemonStatus::Disconnected;
                return Ok(());
            }
        };

        {
            let mut status = self.status.lock().await;
            *status = DaemonStatus::Disconnecting;
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Disconnecting));
        }

        // Немедленная остановка телеметрии и рассылка нулевой скорости
        self.stop_telemetry().await;

        tracing::info!("Sending SIGINT signal to core process (PID {})...", pid);

        #[cfg(unix)]
        {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT);
        }

        let start = tokio::time::Instant::now();
        let mut exited = false;

        while start.elapsed() < Duration::from_secs(3) {
            #[cfg(unix)]
            {
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

        if let Some(stop_tx) = self.stop_tx.lock().await.take() {
            let _ = stop_tx.send(());
        }

        {
            let mut pid_guard = self.child_pid.lock().await;
            *pid_guard = None;
        }

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

        let _ = tokio::process::Command::new("ip")
            .args(["link", "del", "vrxx-tun"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        let config_path = resolve_daemon_config_path();
        let _ = tokio::fs::remove_file(&config_path).await;

        {
            let mut status = self.status.lock().await;
            *status = DaemonStatus::Disconnected;
            self.event_manager
                .broadcast(DaemonEvent::StatusChanged(DaemonStatus::Disconnected));
        }

        tracing::info!("Proxy stopped successfully");
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_parse_singbox_log_formats() {
        let (source, level, msg) = parse_singbox_log(
            "+0600 2026-08-15 21:48:00 INFO network: updated default interface wlan0, index 2",
        );
        assert_eq!(level, LogLevel::Info);
        assert_eq!(source, LogSource::Core);
        assert_eq!(
            msg,
            "+0600 2026-08-15 21:48:00 INFO network: updated default interface wlan0, index 2"
        );

        let (source, level, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:08 DEBUG [2869722053 0ms] dns: lookup domain fra-8dd974.wb-cdn-global.com",
        );
        assert_eq!(level, LogLevel::Debug);
        assert_eq!(source, LogSource::Core);

        let (source, level, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:08 INFO [2869722053 0ms] inbound/socks[socks-in]: inbound connection from 127.0.0.1:54220",
        );
        assert_eq!(level, LogLevel::Info);
        assert_eq!(source, LogSource::Access);

        // Слово "error" в информационном сообщении НЕ должно вызывать уровень ERROR
        let (_, level_info, _) = parse_singbox_log(
            "+0600 2026-08-15 21:48:00 INFO network: recovered from previous error safely",
        );
        assert_eq!(level_info, LogLevel::Info);
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
