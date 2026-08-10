use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PingAlgorithm {
    ViaProxyGet,
    ViaProxyHead,
    #[default]
    TcpHandshake,
    IcmpPing,
}

impl PingAlgorithm {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().replace('-', "_").as_str() {
            "via_proxy_get" | "viaproxyget" | "get" => PingAlgorithm::ViaProxyGet,
            "via_proxy_head" | "viaproxyhead" | "head" => PingAlgorithm::ViaProxyHead,
            "icmp_ping" | "icmpping" | "icmp" => PingAlgorithm::IcmpPing,
            _ => PingAlgorithm::TcpHandshake,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PingAlgorithm::ViaProxyGet => "via_proxy_get",
            PingAlgorithm::ViaProxyHead => "via_proxy_head",
            PingAlgorithm::TcpHandshake => "tcp_handshake",
            PingAlgorithm::IcmpPing => "icmp_ping",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PingResult {
    Success(u128),
    Timeout,
    Error(String),
}

impl PingResult {
    pub fn is_success(&self) -> bool {
        matches!(self, PingResult::Success(_))
    }

    pub fn latency_ms(&self) -> Option<u128> {
        match self {
            PingResult::Success(ms) => Some(*ms),
            _ => None,
        }
    }

    pub fn display_string(&self) -> String {
        match self {
            PingResult::Success(ms) => format!("{ms} ms"),
            PingResult::Timeout => "timeout".to_string(),
            PingResult::Error(_) => "error".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PingTarget {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub raw_url: String,
}

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
            algorithm: PingAlgorithm::TcpHandshake,
            target_url: "https://www.gstatic.com/generate_204".to_string(),
            timeout: Duration::from_secs(3),
            proxy_url: None,
            concurrency_limit: 10,
        }
    }
}

/// 1. TcpHandshake: Measures direct TCP 3-way handshake connection time to host:port
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
                        Err(e) => Err(format!("TCP connect error: {e}")),
                    }
                } else {
                    Err("Failed to resolve socket address".to_string())
                }
            }
            Err(e) => Err(format!("DNS resolution failed: {e}")),
        }
    })
    .await;

    match res {
        Ok(Ok(())) => PingResult::Success(start.elapsed().as_millis()),
        Ok(Err(e)) => PingResult::Error(e),
        Err(_) => PingResult::Timeout,
    }
}

/// 2. IcmpPing: Sends ICMP Echo Request packets using system ping process
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
        Err(e) => return PingResult::Error(format!("Failed to execute ping process: {e}")),
    };

    let res = tokio::time::timeout(timeout_duration, child.wait()).await;

    match res {
        Ok(Ok(status)) if status.success() => {
            let elapsed = start.elapsed().as_millis();
            PingResult::Success(elapsed)
        }
        Ok(Ok(status)) => PingResult::Error(format!(
            "Ping process exited with status {}",
            status.code().unwrap_or(-1)
        )),
        Ok(Err(e)) => PingResult::Error(format!("Error waiting for ping process: {e}")),
        Err(_) => {
            let _ = child.kill().await;
            PingResult::Timeout
        }
    }
}

/// 3. ViaProxyGet / ViaProxyHead: Executes HTTP GET/HEAD request to target URL via SOCKS5/HTTP proxy or direct
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
            Err(e) => return PingResult::Error(format!("Invalid proxy configuration: {e}")),
        }
    }

    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return PingResult::Error(format!("Failed to build HTTP client: {e}")),
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
                PingResult::Error(format!("HTTP request error: {e}"))
            }
        }
        Err(_) => PingResult::Timeout,
    }
}

/// Executes single target ping using specified options
pub async fn ping_target(target: &PingTarget, options: &PingOptions) -> PingResult {
    match options.algorithm {
        PingAlgorithm::TcpHandshake => {
            ping_tcp_handshake(&target.host, target.port, options.timeout).await
        }
        PingAlgorithm::IcmpPing => ping_icmp(&target.host, options.timeout).await,
        PingAlgorithm::ViaProxyGet => {
            ping_via_proxy(
                false,
                &options.target_url,
                options.proxy_url.as_deref(),
                options.timeout,
            )
            .await
        }
        PingAlgorithm::ViaProxyHead => {
            ping_via_proxy(
                true,
                &options.target_url,
                options.proxy_url.as_deref(),
                options.timeout,
            )
            .await
        }
    }
}

/// Executes parallel ping for a list of targets using tokio::spawn and buffer_unordered stream limit
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
        assert_eq!(PingAlgorithm::parse("unknown"), PingAlgorithm::TcpHandshake);

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
                raw_url: "tcp://127.0.0.1:1".to_string(),
            },
            PingTarget {
                id: "2".to_string(),
                host: "127.0.0.1".to_string(),
                port: 2,
                raw_url: "tcp://127.0.0.1:2".to_string(),
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
}
