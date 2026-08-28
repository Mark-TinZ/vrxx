/* ping.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Сервис замера сетевой задержки и сквозной проверки соединения (Ping Service)
//!
//! Модуль реализует:
//! - Несколько алгоритмов измерения задержки ([`PingAlgorithm`]):
//!   - `TcpHandshake`: прямой 3-way handshake к хосту и порту
//!   - `IcmpPing`: системный ICMP Echo Request через вызов `ping`
//!   - `ViaProxyGet` / `ViaProxyHead`: сквозной HTTP-запрос через локальный SOCKS5/HTTP прокси
//!     или изолированный L7 Sandbox Probe для неактивных ключей ([`ping_isolated_proxy_probe`])
//! - Параллельный замер списка серверов с ограничением конкурентности (`buffer_unordered`)
//! - E2E Warm-Up верификацию соединения после старта ядра sing-box ([`verify_proxy_connectivity`])

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

/// Алгоритм замера сетевой задержки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PingAlgorithm {
    /// HTTP GET запрос через SOCKS5/HTTP прокси или изолированный Sandbox Probe
    #[default]
    ViaProxyGet,
    /// HTTP HEAD запрос через SOCKS5/HTTP прокси или изолированный Sandbox Probe
    ViaProxyHead,
    /// Прямое TCP-рукопожатие с удаленным узлом
    TcpHandshake,
    /// Системный ICMP Ping
    IcmpPing,
}

impl PingAlgorithm {
    /// Разбирает строковое представление алгоритма.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().replace('-', "_").as_str() {
            "via_proxy_get" | "viaproxyget" | "get" => PingAlgorithm::ViaProxyGet,
            "via_proxy_head" | "viaproxyhead" | "head" => PingAlgorithm::ViaProxyHead,
            "tcp_handshake" | "tcphandshake" | "tcp" => PingAlgorithm::TcpHandshake,
            "icmp_ping" | "icmpping" | "icmp" => PingAlgorithm::IcmpPing,
            _ => PingAlgorithm::ViaProxyGet,
        }
    }

    /// Возвращает канонический строковый идентификатор алгоритма.
    pub fn as_str(&self) -> &'static str {
        match self {
            PingAlgorithm::ViaProxyGet => "via_proxy_get",
            PingAlgorithm::ViaProxyHead => "via_proxy_head",
            PingAlgorithm::TcpHandshake => "tcp_handshake",
            PingAlgorithm::IcmpPing => "icmp_ping",
        }
    }
}

/// Результат замера задержки.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PingResult {
    /// Успешный замер, значение задержки в миллисекундах
    Success(u128),
    /// Превышение времени ожидания ответа
    Timeout,
    /// Ошибка сетевого взаимодействия с описанием
    Error(String),
}

impl PingResult {
    /// Возвращает true, если замер успешен.
    pub fn is_success(&self) -> bool {
        matches!(self, PingResult::Success(_))
    }

    /// Возвращает значение задержки в миллисекундах при успешном замере.
    pub fn latency_ms(&self) -> Option<u128> {
        match self {
            PingResult::Success(ms) => Some(*ms),
            _ => None,
        }
    }

    /// Форматирует результат в строку для отображения в интерфейсе.
    pub fn display_string(&self) -> String {
        match self {
            PingResult::Success(ms) => format!("{ms} ms"),
            PingResult::Timeout => "timeout".to_string(),
            PingResult::Error(_) => "error".to_string(),
        }
    }
}

/// Целевой узел для выполнения замера задержки.
#[derive(Debug, Clone)]
pub struct PingTarget {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub raw_url: String,
}

/// Настройки и параметры выполнения замера.
#[derive(Debug, Clone)]
pub struct PingOptions {
    pub algorithm: PingAlgorithm,
    pub target_url: String,
    pub timeout: Duration,
    pub proxy_url: Option<String>,
    pub concurrency_limit: usize,
}

impl Default for PingOptions {
    fn default() -> Self {
        Self {
            algorithm: PingAlgorithm::ViaProxyGet,
            target_url: "https://www.gstatic.com/generate_204".to_string(),
            timeout: Duration::from_secs(4),
            proxy_url: None,
            concurrency_limit: 5,
        }
    }
}

/// 1. TcpHandshake: Измеряет время установки прямого TCP 3-way handshake к host:port
pub async fn ping_tcp_handshake(host: &str, port: u16, timeout_duration: Duration) -> PingResult {
    let addr_str = format!("{host}:{port}");
    let start = Instant::now();

    let res = tokio::time::timeout(timeout_duration, async {
        match tokio::net::lookup_host(&addr_str).await {
            Ok(mut addrs) => {
                if let Some(addr) = addrs.next() {
                    match tokio::net::TcpStream::connect(addr).await {
                        Ok(mut stream) => {
                            let _ = stream.shutdown().await;
                            Ok(())
                        }
                        Err(e) => Err(format!("Ошибка TCP-подключения: {e}")),
                    }
                } else {
                    Err("Не удалось определить сокет-адрес".to_string())
                }
            }
            Err(e) => Err(format!("Ошибка DNS-резолвинга: {e}")),
        }
    })
    .await;

    match res {
        Ok(Ok(())) => PingResult::Success(start.elapsed().as_millis()),
        Ok(Err(e)) => PingResult::Error(e),
        Err(_) => PingResult::Timeout,
    }
}

/// 2. IcmpPing: Отправляет пакеты ICMP Echo Request через системную утилиту ping
pub async fn ping_icmp(host: &str, timeout_duration: Duration) -> PingResult {
    let start = Instant::now();
    let timeout_secs = timeout_duration.as_secs().max(1);

    let child = tokio::process::Command::new("ping")
        .arg("-c")
        .arg("1")
        .arg("-W")
        .arg(timeout_secs.to_string())
        .arg(host)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => return PingResult::Error(format!("Не удалось запустить процесс ping: {e}")),
    };

    let res = tokio::time::timeout(timeout_duration, child.wait()).await;

    match res {
        Ok(Ok(status)) if status.success() => {
            let elapsed = start.elapsed().as_millis();
            PingResult::Success(elapsed)
        }
        Ok(Ok(status)) => PingResult::Error(format!(
            "Процесс ping завершился с кодом {}",
            status.code().unwrap_or(-1)
        )),
        Ok(Err(e)) => PingResult::Error(format!("Ошибка ожидания процесса ping: {e}")),
        Err(_) => {
            let _ = child.kill().await;
            PingResult::Timeout
        }
    }
}

/// Ошибки сквозной проверки доступности туннеля (E2E Connectivity).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectivityError {
    #[error("Таймаут подключения: сервер не ответил в течение {0:?}")]
    Timeout(Duration),
    #[error("Ошибка аутентификации или TLS рукопожатия: {0}")]
    HandshakeFailed(String),
    #[error("Ошибка соединения с локальным сокетом прокси: {0}")]
    ProxyError(String),
    #[error("Ошибка выполнения HTTP-запроса: {0}")]
    RequestFailed(String),
}

/// Проверяет сквозную доступность интернета через локальный SOCKS5 вход ядра.
pub async fn verify_proxy_connectivity(
    socks_port: u16,
    target_url: &str,
    timeout_duration: Duration,
) -> Result<u128, ConnectivityError> {
    let proxy_url = format!("socks5://127.0.0.1:{}", socks_port);
    tracing::debug!(
        "Запуск E2E Warm-Up проверки сквозной связи через {} к {} (таймаут: {:?})",
        proxy_url,
        target_url,
        timeout_duration
    );
    let start = Instant::now();

    // -------------------------------------------------------------------------
    // Фаза 1: Ожидание готовности локального SOCKS5 порта ядра (до 2.5 сек или timeout_duration)
    // -------------------------------------------------------------------------
    let warmup_limit = timeout_duration.min(Duration::from_millis(2500));
    let mut port_ready = false;
    let poll_interval = Duration::from_millis(60);

    while start.elapsed() < warmup_limit {
        let remaining_warmup = warmup_limit.saturating_sub(start.elapsed());
        let connect_timeout = remaining_warmup.min(Duration::from_millis(200));

        if tokio::time::timeout(
            connect_timeout,
            tokio::net::TcpStream::connect(("127.0.0.1", socks_port)),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
        {
            port_ready = true;
            tracing::debug!(
                "Локальный SOCKS5 порт {} открыт и готов к приему трафика за {} ms",
                socks_port,
                start.elapsed().as_millis()
            );
            break;
        }

        tokio::time::sleep(poll_interval).await;
    }

    if !port_ready {
        tracing::warn!(
            "Локальный SOCKS5 порт {} не начал отвечать за {} ms",
            socks_port,
            start.elapsed().as_millis()
        );
        return Err(ConnectivityError::ProxyError(format!(
            "Локальный SOCKS5 сокет ядра (127.0.0.1:{}) не открылся вовремя",
            socks_port
        )));
    }

    // -------------------------------------------------------------------------
    // Фаза 2: Сквозная E2E HTTP проверка через SOCKS5 прокси
    // -------------------------------------------------------------------------
    let proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|e| ConnectivityError::ProxyError(e.to_string()))?;

    let single_attempt_timeout = Duration::from_secs(3);

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(single_attempt_timeout)
        .build()
        .map_err(|e| ConnectivityError::ProxyError(e.to_string()))?;

    let mut last_error = None;
    let mut attempt = 0;

    while start.elapsed() < timeout_duration {
        attempt += 1;
        let remaining = timeout_duration.saturating_sub(start.elapsed());
        if remaining < Duration::from_millis(150) {
            break;
        }

        let current_timeout = remaining.min(single_attempt_timeout);
        let res = tokio::time::timeout(current_timeout, client.get(target_url).send()).await;

        match res {
            Ok(Ok(resp)) => {
                let status = resp.status();
                if status.is_success() || status.as_u16() == 204 {
                    let latency = start.elapsed().as_millis();
                    tracing::info!(
                        "E2E Warm-Up проверка успешно пройдена с попытки {} за {} ms (статус {})",
                        attempt,
                        latency,
                        status
                    );
                    return Ok(latency);
                } else {
                    tracing::warn!("E2E проверка вернула неуспешный HTTP-статус: {}", status);
                    last_error = Some(ConnectivityError::RequestFailed(format!(
                        "Неожиданный HTTP-статус {}",
                        status
                    )));
                }
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                tracing::debug!(
                    "E2E проверка: попытка {} завершилась с ошибкой: {}",
                    attempt,
                    err_str
                );
                if e.is_timeout() {
                    last_error = Some(ConnectivityError::Timeout(timeout_duration));
                } else if err_str.contains("connection reset")
                    || err_str.contains("broken pipe")
                    || err_str.contains("handshake")
                    || err_str.contains("authentication")
                    || err_str.contains("closed")
                {
                    last_error = Some(ConnectivityError::HandshakeFailed(err_str));
                } else if err_str.contains("refused") || err_str.contains("socks") {
                    last_error = Some(ConnectivityError::ProxyError(err_str));
                } else {
                    last_error = Some(ConnectivityError::RequestFailed(err_str));
                }
            }
            Err(_) => {
                tracing::debug!("E2E проверка: таймаут попытки {}", attempt);
                last_error = Some(ConnectivityError::Timeout(timeout_duration));
            }
        }

        let retry_pause = Duration::from_millis(350);
        if start.elapsed() + retry_pause < timeout_duration {
            tokio::time::sleep(retry_pause).await;
        } else {
            break;
        }
    }

    let final_err = last_error.unwrap_or(ConnectivityError::Timeout(timeout_duration));
    tracing::warn!(
        "E2E проверка сквозной связи не удалась за {} ms: {:?}",
        start.elapsed().as_millis(),
        final_err
    );
    Err(final_err)
}

/// 3. ViaProxyGet / ViaProxyHead: Выполняет HTTP GET/HEAD запрос к целевому URL через SOCKS5/HTTP прокси
pub async fn ping_via_proxy(
    is_head: bool,
    target_url: &str,
    proxy_url: Option<&str>,
    timeout_duration: Duration,
) -> PingResult {
    let start = Instant::now();

    let mut builder = reqwest::Client::builder().timeout(timeout_duration);

    if let Some(p) = proxy_url {
        match reqwest::Proxy::all(p) {
            Ok(proxy) => {
                builder = builder.proxy(proxy);
            }
            Err(e) => return PingResult::Error(format!("Некорректная конфигурация прокси: {e}")),
        }
    }

    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return PingResult::Error(format!("Не удалось создать HTTP-клиент: {e}")),
    };

    let req = if is_head {
        client.head(target_url)
    } else {
        client.get(target_url)
    };

    let res = tokio::time::timeout(timeout_duration, req.send()).await;

    match res {
        Ok(Ok(_resp)) => PingResult::Success(start.elapsed().as_millis()),
        Ok(Err(e)) => {
            if e.is_timeout() {
                PingResult::Timeout
            } else {
                PingResult::Error(format!("Ошибка HTTP-запроса: {e}"))
            }
        }
        Err(_) => PingResult::Timeout,
    }
}

/// 4. L7 Sandbox Probe: Выполняет изолированную глубокую проверку неактивного VPN-ключа через легковесный инстанс sing-box.
pub async fn ping_isolated_proxy_probe(
    raw_url: &str,
    target_url: &str,
    timeout_duration: Duration,
) -> PingResult {
    let parsed_key = match crate::domain::key_parser::parse_vpn_key(raw_url) {
        Ok(k) => k,
        Err(e) => return PingResult::Error(format!("Некорректный URL ключа: {e}")),
    };

    if let Err(e) = parsed_key.validate() {
        return PingResult::Error(format!("Невалидный ключ: {e}"));
    }

    let bin_path = match crate::daemon::updater::find_singbox_binary() {
        Some(p) => p,
        None => {
            // Фолбэк на TCP Handshake, если бинарник не найден в системе
            return ping_tcp_handshake(&parsed_key.host, parsed_key.port, timeout_duration).await;
        }
    };

    // Выделение свободного эфемерного порта ОС на 127.0.0.1
    let probe_port = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(e) => return PingResult::Error(format!("Не удалось получить адрес сокета: {e}")),
        },
        Err(e) => return PingResult::Error(format!("Не удалось выделить свободный порт: {e}")),
    };

    let config_json =
        crate::domain::singbox_config::build_singbox_probe_config(&parsed_key, probe_port);

    // Запуск легковесного дочернего процесса sing-box с автоматическим уничтожением при drop
    let mut cmd = tokio::process::Command::new(&bin_path);
    cmd.arg("run")
        .arg("-c")
        .arg("/dev/stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return PingResult::Error(format!("Не удалось запустить sing-box probe: {e}")),
    };

    // Передача конфигурации через stdin
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(config_json.as_bytes()).await {
            let _ = child.kill().await;
            return PingResult::Error(format!("Ошибка записи конфигурации в stdin: {e}"));
        }
        let _ = stdin.shutdown().await;
    }

    let start = Instant::now();

    // Ожидание открытия SOCKS-порта sing-box (до 350 мс)
    let mut port_ready = false;
    let warmup_limit = timeout_duration.min(Duration::from_millis(350));
    while start.elapsed() < warmup_limit {
        if tokio::net::TcpStream::connect(("127.0.0.1", probe_port))
            .await
            .is_ok()
        {
            port_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    if !port_ready {
        let _ = child.kill().await;
        return PingResult::Error(
            "Не удалось инициализировать сокет проверки sing-box".to_string(),
        );
    }

    // Выполнение сквозного HTTP GET запроса через SOCKS5
    let socks_url = format!("socks5h://127.0.0.1:{}", probe_port);
    let proxy = match reqwest::Proxy::all(&socks_url) {
        Ok(p) => p,
        Err(e) => {
            let _ = child.kill().await;
            return PingResult::Error(format!("Ошибка создания SOCKS5 прокси: {e}"));
        }
    };

    let remaining_timeout = timeout_duration.saturating_sub(start.elapsed());
    let client = match reqwest::Client::builder()
        .proxy(proxy)
        .timeout(remaining_timeout)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = child.kill().await;
            return PingResult::Error(format!("Ошибка сборки HTTP-клиента: {e}"));
        }
    };

    let http_start = Instant::now();
    let effective_url = if target_url.trim().is_empty() {
        "https://www.gstatic.com/generate_204"
    } else {
        target_url
    };

    let request_res =
        tokio::time::timeout(remaining_timeout, client.get(effective_url).send()).await;

    // Завершение дочернего процесса sing-box
    let _ = child.kill().await;

    match request_res {
        Ok(Ok(resp)) => {
            let status = resp.status();
            if status.is_success() || status.as_u16() == 204 {
                PingResult::Success(http_start.elapsed().as_millis())
            } else {
                PingResult::Error(format!("HTTP {}", status))
            }
        }
        Ok(Err(e)) => {
            let err_str = e.to_string();
            if e.is_timeout() {
                PingResult::Timeout
            } else if err_str.contains("connection reset")
                || err_str.contains("broken pipe")
                || err_str.contains("handshake")
                || err_str.contains("authentication")
                || err_str.contains("closed")
                || err_str.contains("General failure")
            {
                PingResult::Error("Handshake failed".to_string())
            } else {
                PingResult::Error(err_str)
            }
        }
        Err(_) => PingResult::Timeout,
    }
}

/// Выполняет замер задержки для одиночной цели согласно переданным параметрам.
pub async fn ping_target(target: &PingTarget, options: &PingOptions) -> PingResult {
    match options.algorithm {
        PingAlgorithm::TcpHandshake => {
            ping_tcp_handshake(&target.host, target.port, options.timeout).await
        }
        PingAlgorithm::IcmpPing => ping_icmp(&target.host, options.timeout).await,
        PingAlgorithm::ViaProxyGet => {
            if let Some(proxy_url) = options.proxy_url.as_deref() {
                ping_via_proxy(false, &options.target_url, Some(proxy_url), options.timeout).await
            } else {
                ping_isolated_proxy_probe(&target.raw_url, &options.target_url, options.timeout)
                    .await
            }
        }
        PingAlgorithm::ViaProxyHead => {
            if let Some(proxy_url) = options.proxy_url.as_deref() {
                ping_via_proxy(true, &options.target_url, Some(proxy_url), options.timeout).await
            } else {
                ping_isolated_proxy_probe(&target.raw_url, &options.target_url, options.timeout)
                    .await
            }
        }
    }
}

/// Выполняет параллельный замер задержки списка целей с ограничением конкурентности.
pub async fn ping_targets_parallel(
    targets: Vec<PingTarget>,
    options: PingOptions,
) -> Vec<(PingTarget, PingResult)> {
    let limit = options.concurrency_limit.max(1);

    stream::iter(targets)
        .map(|target| {
            let opts = options.clone();
            tokio::spawn(async move {
                let result = ping_target(&target, &opts).await;
                (target, result)
            })
        })
        .buffer_unordered(limit)
        .filter_map(|res| async { res.ok() })
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_algorithm_parse() {
        assert_eq!(
            PingAlgorithm::parse("via_proxy_get"),
            PingAlgorithm::ViaProxyGet
        );
        assert_eq!(
            PingAlgorithm::parse("ViaProxyHead"),
            PingAlgorithm::ViaProxyHead
        );
        assert_eq!(
            PingAlgorithm::parse("tcp-handshake"),
            PingAlgorithm::TcpHandshake
        );
        assert_eq!(PingAlgorithm::parse("icmp_ping"), PingAlgorithm::IcmpPing);
        assert_eq!(PingAlgorithm::parse("unknown"), PingAlgorithm::ViaProxyGet);

        assert_eq!(PingAlgorithm::ViaProxyGet.as_str(), "via_proxy_get");
        assert_eq!(PingAlgorithm::ViaProxyHead.as_str(), "via_proxy_head");
        assert_eq!(PingAlgorithm::TcpHandshake.as_str(), "tcp_handshake");
        assert_eq!(PingAlgorithm::IcmpPing.as_str(), "icmp_ping");
    }

    #[test]
    fn test_ping_result_methods() {
        let success = PingResult::Success(42);
        assert!(success.is_success());
        assert_eq!(success.latency_ms(), Some(42));
        assert_eq!(success.display_string(), "42 ms");

        let timeout = PingResult::Timeout;
        assert!(!timeout.is_success());
        assert_eq!(timeout.latency_ms(), None);
        assert_eq!(timeout.display_string(), "timeout");

        let err = PingResult::Error("Connection refused".to_string());
        assert!(!err.is_success());
        assert_eq!(err.latency_ms(), None);
        assert_eq!(err.display_string(), "error");
    }

    #[tokio::test]
    async fn test_tcp_handshake_invalid_host() {
        let res = ping_tcp_handshake("127.0.0.1", 1, Duration::from_millis(100)).await;
        assert!(matches!(res, PingResult::Error(_)) || matches!(res, PingResult::Timeout));
    }

    #[tokio::test]
    async fn test_icmp_ping_localhost() {
        let res = ping_icmp("127.0.0.1", Duration::from_secs(2)).await;
        assert!(res.is_success() || matches!(res, PingResult::Error(_)));
    }

    #[tokio::test]
    async fn test_parallel_ping_execution() {
        let targets = vec![
            PingTarget {
                id: "1".to_string(),
                host: "127.0.0.1".to_string(),
                port: 1,
                raw_url: "trojan://pass@127.0.0.1:1#Test1".to_string(),
            },
            PingTarget {
                id: "2".to_string(),
                host: "127.0.0.1".to_string(),
                port: 2,
                raw_url: "trojan://pass@127.0.0.1:2#Test2".to_string(),
            },
        ];

        let opts = PingOptions {
            algorithm: PingAlgorithm::TcpHandshake,
            target_url: "https://www.gstatic.com/generate_204".to_string(),
            timeout: Duration::from_millis(200),
            proxy_url: None,
            concurrency_limit: 2,
        };

        let results = ping_targets_parallel(targets, opts).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_verify_proxy_connectivity_refused() {
        let res =
            verify_proxy_connectivity(59999, "http://127.0.0.1:59998", Duration::from_millis(200))
                .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_verify_proxy_connectivity_socket_warmup() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let _ = tokio::time::sleep(Duration::from_millis(50)).await;
                let _ = socket.shutdown().await;
            }
        });

        let res =
            verify_proxy_connectivity(port, "http://127.0.0.1:59998", Duration::from_millis(300))
                .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ping_isolated_probe_invalid_url() {
        let res = ping_isolated_proxy_probe(
            "not-a-valid-url",
            "https://www.gstatic.com/generate_204",
            Duration::from_millis(500),
        )
        .await;
        assert!(!res.is_success());
    }

    #[tokio::test]
    async fn test_ping_isolated_probe_dead_key_rejection() {
        let dead_key = "trojan://pass@127.0.0.1:59997#DeadKey";
        let res = ping_isolated_proxy_probe(
            dead_key,
            "https://www.gstatic.com/generate_204",
            Duration::from_millis(800),
        )
        .await;
        assert!(
            !res.is_success(),
            "Dead proxy should not return success: {:?}",
            res
        );
    }
}
