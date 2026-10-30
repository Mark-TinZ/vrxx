/* tests/common/mod.rs
 *
 * Test harness, shared fixtures, mock servers, and wire protocol definitions
 * for VRXX E2E integration tests.
 */

#![allow(dead_code)]

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use axum::extract::connect_info::Connected;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::UnixListener;
use vrxx::domain::key_parser::ParsedKey;
use vrxx::settings::{AppSettings, RoutingRule, VpnKeyData};

pub const MAGIC_HEADER_V1: &[u8; 8] = b"VRXXDAT1";
pub const MAGIC_HEADER_V2: &[u8; 8] = b"VRXXDAT2";
pub const NONCE_LENGTH: usize = 12;
pub const TAG_LENGTH: usize = 16;

const APP_UNIQUE_ID_V2: &[u8] = b"ru.mark.vrxx.user-keystore.v2";
const APP_SALT_V2: &[u8] = b"vrxx-user-isolated-salt-2026";
const HKDF_INFO_V2: &[u8] = b"vrxx-aes-256-gcm-data-key-v2";

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Protocol & Wire Models matching PROJECT.md § Interface Contracts
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CoreType {
    #[default]
    SingBox,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DaemonStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warning,
    #[default]
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DomainStrategy {
    #[default]
    AsIs,
    PreferIpv4,
    PreferIpv6,
    Ipv4Only,
    Ipv6Only,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    #[default]
    Bypass,
    Proxy,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DaemonConfigOptions {
    pub tun_mode: bool,
    pub socks_port: u16,
    pub http_port: u16,
    pub allow_lan: bool,
    pub enable_mux: bool,
    pub mux_concurrency: i32,
    pub enable_fragment: bool,
    pub block_quic: bool,
    pub enable_sniffing: bool,
    pub block_ads: bool,
    pub bypass_lan: bool,
    pub disable_ipv6: bool,
    pub enable_fake_dns: bool,
    pub domain_strategy: DomainStrategy,
    pub routing_mode: RoutingMode,
    pub enable_routing: bool,
    pub route_ru_sites: bool,
    pub route_ru_ips: bool,
    pub route_cn_sites: bool,
    pub route_cn_ips: bool,
    pub route_ir_sites: bool,
    pub route_ir_ips: bool,
    pub route_antifilter: bool,
    pub routing_rules: Vec<RoutingRule>,
    pub log_level: LogLevel,
}

impl Default for DaemonConfigOptions {
    fn default() -> Self {
        Self {
            tun_mode: false,
            socks_port: 10808,
            http_port: 10809,
            allow_lan: false,
            enable_mux: false,
            mux_concurrency: 8,
            enable_fragment: false,
            block_quic: false,
            enable_sniffing: true,
            block_ads: false,
            bypass_lan: true,
            disable_ipv6: false,
            enable_fake_dns: false,
            domain_strategy: DomainStrategy::AsIs,
            routing_mode: RoutingMode::Bypass,
            enable_routing: true,
            route_ru_sites: false,
            route_ru_ips: false,
            route_cn_sites: false,
            route_cn_ips: false,
            route_ir_sites: false,
            route_ir_ips: false,
            route_antifilter: false,
            routing_rules: Vec::new(),
            log_level: LogLevel::Info,
        }
    }
}

impl DaemonConfigOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.socks_port == 0 {
            return Err("SOCKS port cannot be 0".to_string());
        }
        if self.http_port == 0 {
            return Err("HTTP port cannot be 0".to_string());
        }
        if self.socks_port == self.http_port {
            return Err("SOCKS port and HTTP port cannot be identical".to_string());
        }
        if self.enable_mux && !(1..=128).contains(&self.mux_concurrency) {
            return Err("Multiplex concurrency must be between 1 and 128".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StartProxyRequest {
    #[serde(default)]
    pub core_type: CoreType,
    pub profile: ParsedKey,
    pub options: DaemonConfigOptions,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum DaemonEventWire {
    StatusChanged(DaemonStatus),
    Log {
        source: String,
        level: LogLevel,
        message: String,
    },
    Traffic {
        down_speed: u64,
        up_speed: u64,
        down_total: u64,
        up_total: u64,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ClashTrafficResponse {
    pub up: u64,
    pub down: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ClashConnectionsResponse {
    #[serde(rename = "downloadTotal")]
    pub download_total: u64,
    #[serde(rename = "uploadTotal")]
    pub upload_total: u64,
}

// ---------------------------------------------------------------------------
// Keystore V2 & Migration Helpers
// ---------------------------------------------------------------------------

pub fn derive_user_key_v2(user_secret: &[u8]) -> anyhow::Result<[u8; 32]> {
    if user_secret.is_empty() {
        anyhow::bail!("User secret cannot be empty");
    }

    let mut mac = <HmacSha256 as Mac>::new_from_slice(APP_UNIQUE_ID_V2)
        .map_err(|e| anyhow::anyhow!("HMAC init error: {e}"))?;
    mac.update(user_secret);
    let prk = mac.finalize().into_bytes();

    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(APP_SALT_V2), &prk);
    let mut derived_key = [0u8; 32];
    hkdf.expand(HKDF_INFO_V2, &mut derived_key)
        .map_err(|e| anyhow::anyhow!("HKDF expansion error: {e}"))?;

    Ok(derived_key)
}

pub fn encrypt_v2_container(data: &[u8], key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    use aes_gcm::aead::rand_core::RngCore;

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES-GCM init error: {e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("AES-GCM encryption error: {e}"))?;

    let mut container = Vec::with_capacity(MAGIC_HEADER_V2.len() + NONCE_LENGTH + ciphertext.len());
    container.extend_from_slice(MAGIC_HEADER_V2);
    container.extend_from_slice(&nonce_bytes);
    container.extend_from_slice(&ciphertext);

    Ok(container)
}

pub fn decrypt_v2_container(raw_data: &[u8], key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    let min_len = MAGIC_HEADER_V2.len() + NONCE_LENGTH + TAG_LENGTH;
    if raw_data.len() < min_len {
        anyhow::bail!("Container too small: {} < {}", raw_data.len(), min_len);
    }

    if &raw_data[0..MAGIC_HEADER_V2.len()] != MAGIC_HEADER_V2 {
        anyhow::bail!("Invalid magic header, expected VRXXDAT2");
    }

    let nonce_start = MAGIC_HEADER_V2.len();
    let nonce_end = nonce_start + NONCE_LENGTH;
    let nonce = Nonce::from_slice(&raw_data[nonce_start..nonce_end]);
    let ciphertext = &raw_data[nonce_end..];

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES-GCM init error: {e}"))?;

    let decrypted = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed: integrity compromised or wrong key"))?;

    Ok(decrypted)
}

pub fn check_file_mode(path: &Path, expected_mode: u32) -> bool {
    if let Ok(metadata) = std::fs::metadata(path) {
        let mode = metadata.permissions().mode() & 0o777;
        mode == expected_mode
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// UDS & SO_PEERCRED Testing Infrastructure
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CallerIdentity {
    pub is_authenticated: bool,
    pub uid: u32,
    pub gid: u32,
    pub pid: Option<i32>,
}

impl Connected<axum::serve::IncomingStream<'_, UnixListener>> for CallerIdentity {
    fn connect_info(stream: axum::serve::IncomingStream<'_, UnixListener>) -> Self {
        match stream.io().peer_cred() {
            Ok(ucred) => CallerIdentity {
                is_authenticated: true,
                uid: ucred.uid(),
                gid: ucred.gid(),
                pid: ucred.pid(),
            },
            Err(_) => CallerIdentity {
                is_authenticated: false,
                uid: u32::MAX,
                gid: u32::MAX,
                pid: None,
            },
        }
    }
}

pub fn is_caller_authorized_mock(uid: u32, allowed_uid: u32) -> bool {
    // 0. Invalid / sentinel UIDs
    if uid == u32::MAX || uid >= 65534 || uid > 2_000_000_000 {
        return false;
    }
    // 1. Root is always authorized
    if uid == 0 {
        return true;
    }
    // 2. Reject system service accounts
    if uid < 1000 {
        return false;
    }
    // 3. User session match
    uid == allowed_uid
}

#[derive(Clone)]
pub struct TestServerState {
    pub status: Arc<std::sync::Mutex<DaemonStatus>>,
    pub is_running: Arc<AtomicBool>,
    pub expected_uid: u32,
}

async fn test_auth_middleware(
    State(s): State<TestServerState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let connect_info = request
        .extensions()
        .get::<axum::extract::ConnectInfo<CallerIdentity>>();

    let identity = match connect_info {
        Some(ci) if ci.0.is_authenticated => ci.0.clone(),
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Caller connection is unauthenticated"
                })),
            )
                .into_response();
        }
    };

    if !is_caller_authorized_mock(identity.uid, s.expected_uid) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!("Caller UID {} is not authorized: no active login session", identity.uid)
            })),
        )
            .into_response();
    }

    next.run(request).await
}

pub fn create_test_uds_router(state: TestServerState) -> Router {
    Router::new()
        .route("/api/ping", get(|| async { "pong" }))
        .route(
            "/api/status",
            get(|State(s): State<TestServerState>| async move {
                let status = *s.status.lock().unwrap();
                Json(status)
            }),
        )
        .route(
            "/api/is_running",
            get(|State(s): State<TestServerState>| async move {
                let running = s.is_running.load(Ordering::SeqCst);
                Json(running)
            }),
        )
        .route(
            "/api/proxy/start",
            post(
                |State(s): State<TestServerState>,
                 Json(payload): Json<StartProxyRequest>| async move {
                    if let Err(e) = payload.profile.validate() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({"error": format!("Invalid profile: {e:?}")})),
                        )
                            .into_response();
                    }
                    if let Err(e) = payload.options.validate() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({"error": format!("Invalid options: {e}")})),
                        )
                            .into_response();
                    }

                    *s.status.lock().unwrap() = DaemonStatus::Connected;
                    s.is_running.store(true, Ordering::SeqCst);
                    (StatusCode::OK, Json(serde_json::json!({"status": "started"}))).into_response()
                },
            ),
        )
        .route(
            "/api/proxy/stop",
            post(
                |State(s): State<TestServerState>| async move {
                    *s.status.lock().unwrap() = DaemonStatus::Disconnected;
                    s.is_running.store(false, Ordering::SeqCst);
                    (StatusCode::OK, Json(serde_json::json!({"status": "stopped"}))).into_response()
                },
            ),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            test_auth_middleware,
        ))
        .with_state(state)
}

pub async fn spawn_test_uds_daemon(
    socket_path: PathBuf,
    expected_uid: u32,
) -> anyhow::Result<(
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Sender<()>,
    TestServerState,
)> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
    }
    if socket_path.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path)?;
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666));

    let state = TestServerState {
        status: Arc::new(std::sync::Mutex::new(DaemonStatus::Disconnected)),
        is_running: Arc::new(AtomicBool::new(false)),
        expected_uid,
    };

    let router = create_test_uds_router(state.clone());
    let make_service = router.into_make_service_with_connect_info::<CallerIdentity>();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, make_service)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    Ok((handle, shutdown_tx, state))
}

// ---------------------------------------------------------------------------
// Mock Clash API Server Infrastructure
// ---------------------------------------------------------------------------

pub async fn spawn_mock_clash_server(
    port: u16,
    up_speed: u64,
    down_speed: u64,
    up_total: u64,
    down_total: u64,
) -> anyhow::Result<(
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Sender<()>,
    u16,
)> {
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    let bound_port = listener.local_addr()?.port();

    let app = Router::new()
        .route(
            "/traffic",
            get(move || async move {
                Json(ClashTrafficResponse {
                    up: up_speed,
                    down: down_speed,
                })
            }),
        )
        .route(
            "/connections",
            get(move || async move {
                Json(ClashConnectionsResponse {
                    download_total: down_total,
                    upload_total: up_total,
                })
            }),
        );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    Ok((handle, shutdown_tx, bound_port))
}

// ---------------------------------------------------------------------------
// Test Data & Fixtures
// ---------------------------------------------------------------------------

pub fn valid_vless_reality_url() -> &'static str {
    "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@198.51.100.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=gateway.example.com&flow=xtls-rprx-vision#E2E-Valid-VLESS"
}

pub fn valid_vmess_url() -> &'static str {
    "vmess://eyJhZGQiOiIxOTguNTEuMTAwLjIiLCJhaWQiOjAsImlkIjoiZDNiMDczODQtZDExMy00MjZiLTg3YjYtOTYzZDBjMzI2MDg0IiwibmV0Ijoid3MiLCJwYXRoIjoiL3ZtZXNzIiwicG9ydCI6NDQzLCJwcyI6IkUyRS1WbWVzcyIsInRscyI6InRscyIsInYiOjJ9"
}

pub fn valid_shadowsocks_url() -> &'static str {
    "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZDEyMw@198.51.100.3:8388#E2E-Shadowsocks"
}

pub fn invalid_uuid_url() -> &'static str {
    "vless://not-a-valid-uuid@198.51.100.1:443?security=none#BadUUID"
}

pub fn invalid_port_url() -> &'static str {
    "vless://d3b07384-d113-426b-87b6-963d0c326084@198.51.100.1:0?security=none#ZeroPort"
}

pub fn sample_vpn_key_data(name: &str, url: &str) -> VpnKeyData {
    VpnKeyData {
        name: name.to_string(),
        protocol: "VLESS".to_string(),
        is_active: false,
        traffic_down: "0 MB".to_string(),
        traffic_up: "0 MB".to_string(),
        time_connected: "00:00:00".to_string(),
        ping: "25 ms".to_string(),
        location: "DE".to_string(),
        country_code: "DE".to_string(),
        city: "Frankfurt".to_string(),
        isp: "Test ISP".to_string(),
        timezone: "UTC".to_string(),
        url: url.to_string(),
        group: "Default".to_string(),
    }
}

pub fn sample_app_settings() -> AppSettings {
    AppSettings {
        tun_mode: false,
        socks_port: 10808,
        http_port: 10809,
        allow_lan: false,
        enable_mux: false,
        mux_concurrency: 8,
        enable_fragment: false,
        block_quic: false,
        enable_sniffing: true,
        block_ads: false,
        bypass_lan: true,
        disable_ipv6: false,
        enable_fake_dns: false,
        domain_strategy: "AsIs".to_string(),
        routing_mode: "Bypass".to_string(),
        enable_routing: true,
        route_ru_sites: false,
        route_ru_ips: false,
        route_cn_sites: false,
        route_cn_ips: false,
        route_ir_sites: false,
        route_ir_ips: false,
        route_antifilter: false,
        routing_rules: Vec::new(),
        log_level: "info".to_string(),
        ..Default::default()
    }
}

pub fn format_dynamic_bandwidth(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_073_741_824 {
        format!("{:.1} GB/s", bytes_per_sec as f64 / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_048_576.0)
    } else if bytes_per_sec >= 1_024 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1_024.0)
    } else {
        format!("{bytes_per_sec} B/s")
    }
}
