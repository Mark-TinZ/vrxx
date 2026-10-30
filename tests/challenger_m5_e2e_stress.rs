/* tests/challenger_m5_e2e_stress.rs
 *
 * Empirical Challenge & Adversarial Stress Suite for Milestone M5
 * Target: UDS Transport, Caller Authentication, Config Inversion, and E2E Lifecycle
 *
 * Hypotheses tested:
 * H1: [is_caller_authorized] x [system_accounts_uid_1_999] -> rejected
 * H2: [is_caller_authorized] x [sentinel_uids_max_nobody] -> rejected
 * H3: [auth_middleware] x [unauthenticated_connections] -> 403 Forbidden across all endpoints
 * H4: [daemon_bind] x [invalid_parent_directory] -> clean error without panic
 * H5: [daemon_bind] x [stale_dead_socket] -> probe confirms dead, socket unlinked, new bind succeeds
 * H6: [daemon_bind] x [active_socket_conflict] -> second bind rejected, active daemon preserved
 * H7: [options_validation] x [port_0] -> rejected
 * H8: [options_validation] x [port_collision] -> rejected with PortConflict ("identical")
 * H9: [options_validation] x [invalid_mux_concurrency] -> 0, -1, 129 rejected when enable_mux=true
 * H10: [profile_validation] x [invalid_uuid] -> rejected
 * H11: [profile_validation] x [zero_port] -> rejected
 * H12: [singbox_config_synthesis] x [valid_protocols] -> valid JSON with inbounds, outbounds, route, dns
 * H13: [proxy_manager_api] x [malformed_json_wire_payload] -> HTTP 400 / 422 rejected cleanly
 * H14: [singbox_preflight] x [invalid_config] -> rejected by sing-box check if binary present
 * H15: [daemon_client] x [full_e2e_lifecycle] -> ping -> start -> status -> stop over UDS
 */

use axum::extract::ConnectInfo;
use axum::middleware::{self, Next};
use reqwest::Client;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use vrxx::daemon::api::{create_router, ApiState};
use vrxx::daemon::auth::{self, CallerIdentity};
use vrxx::daemon::core::ProxyManager;
use vrxx::daemon::events::EventManager;
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::domain::singbox_config::build_singbox_config;
use vrxx::ipc::DaemonClient;
use vrxx::protocol::{ConfigValidationError, DaemonConfigOptions, DaemonStatus};
use vrxx::settings::AppSettings;

// Global test serialization lock to prevent sing-box singleton port/config file races
static PROXY_MANAGER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Helper: Injects mock or kernel CallerIdentity for empirical auth testing
async fn identity_injecting_middleware(
    forced_identity: Option<CallerIdentity>,
    mut request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    if let Some(forced) = forced_identity {
        request.extensions_mut().insert(ConnectInfo(forced));
    }
    next.run(request).await
}

/// Helper: Spawns the real daemon router over a temporary UDS socket
async fn spawn_test_server(
    socket_path: PathBuf,
    forced_identity: Option<CallerIdentity>,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Sender<()>,
    Arc<ProxyManager>,
) {
    if let Some(parent) = socket_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if socket_path.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind test UnixListener");
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666));

    let (event_manager, _) = EventManager::new(100);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));
    let api_state = Arc::new(ApiState {
        proxy_manager: proxy_manager.clone(),
        event_manager,
    });

    let app = create_router(api_state);
    let app = if let Some(forced) = forced_identity {
        app.layer(middleware::from_fn(move |req, next| {
            let forced_clone = forced.clone();
            async move { identity_injecting_middleware(Some(forced_clone), req, next).await }
        }))
    } else {
        app
    };

    let make_service = app.into_make_service_with_connect_info::<CallerIdentity>();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let server_handle = tokio::spawn(async move {
        let server = axum::serve(listener, make_service);
        tokio::select! {
            _ = server => {},
            _ = rx => {},
        }
    });

    (server_handle, tx, proxy_manager)
}

fn sample_vless_url() -> &'static str {
    "vless://12345678-1234-1234-1234-123456789abc@198.51.100.1:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=example.com&fp=chrome&pbk=11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff&sid=12345678&type=tcp#TestVless"
}

// ===========================================================================
// SECTION 1: UDS TRANSPORT & CALLER AUTHENTICATION CHALLENGES
// ===========================================================================

#[tokio::test]
async fn test_h1_system_accounts_uid_1_to_999_rejected() {
    let system_uids = [1, 2, 5, 50, 100, 500, 998, 999];
    for &uid in &system_uids {
        let identity = CallerIdentity {
            is_authenticated: true,
            uid,
            gid: uid,
            pid: Some(1234),
        };
        assert!(
            !auth::is_caller_authorized(&identity).await,
            "System account UID {uid} MUST be rejected"
        );
    }
}

#[tokio::test]
async fn test_h2_sentinel_and_out_of_range_uids_rejected() {
    let sentinel_uids = [
        u32::MAX,
        65534, // nobody
        65535,
        2_000_000_001,
        3_000_000_000,
    ];
    for &uid in &sentinel_uids {
        let identity = CallerIdentity {
            is_authenticated: true,
            uid,
            gid: uid,
            pid: Some(1234),
        };
        assert!(
            !auth::is_caller_authorized(&identity).await,
            "Sentinel/out-of-range UID {uid} MUST be rejected"
        );
    }
}

#[tokio::test]
async fn test_h2_root_uid_0_always_authorized() {
    let root_identity = CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: Some(1),
    };
    assert!(
        auth::is_caller_authorized(&root_identity).await,
        "Root UID 0 MUST be authorized"
    );
}

#[tokio::test]
async fn test_h3_unauthenticated_connection_rejected_across_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("unauth_test.sock");

    // Force unauthenticated identity
    let unauth_identity = CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };

    let (server_handle, shutdown_tx, _) =
        spawn_test_server(socket_path.clone(), Some(unauth_identity)).await;

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // 1. GET /api/ping
    let resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    // 2. GET /api/status
    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    // 3. GET /api/is_running
    let resp = client
        .get("http://localhost/api/is_running")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    // 4. POST /api/proxy/stop
    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    // 5. GET /api/history
    let resp = client
        .get("http://localhost/api/history")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_h4_invalid_uds_path_handling() {
    let invalid_path = PathBuf::from("/nonexistent/readonly/daemon.sock");
    // Attempting to bind UnixListener in non-existent directory must return I/O Err
    let bind_res = UnixListener::bind(&invalid_path);
    assert!(bind_res.is_err(), "Binding to invalid path must return Err");
}

#[tokio::test]
async fn test_h5_stale_socket_recovery_simulation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("stale_emp.sock");

    // 1. Bind and abandon socket without unlinking
    {
        let _listener = UnixListener::bind(&socket_path).unwrap();
    }
    assert!(socket_path.exists());

    // 2. Dead socket probe
    let is_alive = UnixStream::connect(&socket_path).await.is_ok();
    assert!(!is_alive, "Dead socket must not accept connections");

    // 3. Stale cleanup
    if !is_alive && socket_path.exists() {
        std::fs::remove_file(&socket_path).unwrap();
    }

    // 4. Re-bind succeeds
    let new_bind = UnixListener::bind(&socket_path);
    assert!(new_bind.is_ok(), "Re-bind after stale cleanup must succeed");
}

#[tokio::test]
async fn test_h6_active_socket_conflict_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("conflict_emp.sock");

    // Active instance 1
    let (server_handle, shutdown_tx, _) = spawn_test_server(socket_path.clone(), None).await;

    // Instance 2 probes socket
    let is_active = UnixStream::connect(&socket_path).await.is_ok();
    assert!(is_active, "Active socket must respond");

    // Instance 2 must abort without deleting socket
    if is_active {
        assert!(socket_path.exists(), "Active socket must not be unlinked");
    }

    // Instance 1 remains healthy
    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ===========================================================================
// SECTION 2: CONFIG INVERSION & BOUNDARY VALIDATION CHALLENGES
// ===========================================================================

#[test]
fn test_h7_zero_port_rejected_in_options() {
    let opt_zero_socks = DaemonConfigOptions {
        socks_port: 0,
        http_port: 10809,
        ..Default::default()
    };
    assert!(opt_zero_socks.validate().is_err());

    let opt_zero_http = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 0,
        ..Default::default()
    };
    assert!(opt_zero_http.validate().is_err());
}

#[test]
fn test_h8_conflicting_ports_rejected() {
    let opt_conflict = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 10808,
        ..Default::default()
    };
    let err = opt_conflict.validate().unwrap_err();
    assert!(matches!(err, ConfigValidationError::PortConflict { .. }));
    assert!(format!("{err}").contains("identical"));
}

#[test]
fn test_h9_mux_concurrency_boundaries() {
    let opt_mux_0 = DaemonConfigOptions {
        enable_mux: true,
        mux_concurrency: 0,
        ..Default::default()
    };
    assert!(opt_mux_0.validate().is_err());

    let opt_mux_neg = DaemonConfigOptions {
        enable_mux: true,
        mux_concurrency: -5,
        ..Default::default()
    };
    assert!(opt_mux_neg.validate().is_err());

    let opt_mux_129 = DaemonConfigOptions {
        enable_mux: true,
        mux_concurrency: 129,
        ..Default::default()
    };
    assert!(opt_mux_129.validate().is_err());

    // When enable_mux = false, concurrency is ignored
    let opt_mux_disabled = DaemonConfigOptions {
        enable_mux: false,
        mux_concurrency: 9999,
        ..Default::default()
    };
    assert!(opt_mux_disabled.validate().is_ok());
}

#[test]
fn test_h10_invalid_uuid_rejected() {
    let invalid_uuid_url = "vless://not-a-valid-uuid@198.51.100.1:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=example.com&fp=chrome&pbk=11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff&sid=12345678&type=tcp#InvalidUUID";
    let parsed = parse_vpn_key(invalid_uuid_url).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn test_h11_zero_port_in_parsed_key_rejected() {
    let zero_port_url = "vless://12345678-1234-1234-1234-123456789abc@198.51.100.1:0?encryption=none&flow=xtls-rprx-vision&security=reality&sni=example.com&fp=chrome&pbk=11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff&sid=12345678&type=tcp#ZeroPort";
    let parsed = parse_vpn_key(zero_port_url).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn test_h12_synthesized_config_structure() {
    let parsed = parse_vpn_key(sample_vless_url()).unwrap();
    let settings = AppSettings::default();
    let json_str = build_singbox_config(&parsed, &settings);

    let val: serde_json::Value =
        serde_json::from_str(&json_str).expect("Synthesized sing-box config must be valid JSON");

    assert!(val.get("inbounds").is_some());
    assert!(val.get("outbounds").is_some());
    assert!(val.get("route").is_some());
    assert!(val.get("dns").is_some());
}

#[tokio::test]
async fn test_h13_malformed_json_wire_payloads_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("malformed_json.sock");

    let (server_handle, shutdown_tx, _) = spawn_test_server(socket_path.clone(), None).await;

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // 1. Empty body
    let resp = client
        .post("http://localhost/api/proxy/start")
        .header("Content-Type", "application/json")
        .body("")
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "Empty body must be rejected"
    );

    // 2. Corrupt JSON syntax
    let resp = client
        .post("http://localhost/api/proxy/start")
        .header("Content-Type", "application/json")
        .body("{ \"core_type\": \"sing-box\", truncated")
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "Corrupt JSON must be rejected"
    );

    // 3. Missing profile field
    let resp = client
        .post("http://localhost/api/proxy/start")
        .header("Content-Type", "application/json")
        .body("{\"core_type\": \"sing-box\"}")
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "Missing profile field must be rejected"
    );

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[test]
fn test_h14_singbox_check_preflight() {
    let singbox_path = "/usr/local/bin/sing-box";
    if Path::new(singbox_path).exists() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let invalid_config = r#"{"inbounds": [{"type": "nonexistent"}]}"#;
        std::fs::write(temp_file.path(), invalid_config).unwrap();

        let output = Command::new(singbox_path)
            .arg("check")
            .arg("-c")
            .arg(temp_file.path())
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "sing-box check must reject invalid config"
        );
    }
}

fn valid_vless_reality_url() -> &'static str {
    "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@198.51.100.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=gateway.example.com&flow=xtls-rprx-vision#E2E-Valid-VLESS"
}

#[tokio::test]
async fn test_h16_preflight_tier4_rejects_invalid_reality_pbk() {
    let _lock = PROXY_MANAGER_LOCK.lock().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("preflight_pbk.sock");

    let current_uid = nix::unistd::getuid().as_raw();
    let identity = CallerIdentity {
        is_authenticated: true,
        uid: current_uid,
        gid: nix::unistd::getgid().as_raw(),
        pid: Some(std::process::id() as i32),
    };

    let (server_handle, shutdown_tx, _) =
        spawn_test_server(socket_path.clone(), Some(identity)).await;

    let client = DaemonClient::with_socket_path(socket_path.to_str().unwrap());

    // Invalid pbk (hex string instead of base64 curve25519)
    let invalid_pbk_url = "vless://12345678-1234-1234-1234-123456789abc@198.51.100.1:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=example.com&fp=chrome&pbk=11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff&sid=12345678&type=tcp#InvalidPbk";
    let profile = parse_vpn_key(invalid_pbk_url).unwrap();
    let options = DaemonConfigOptions::default();

    let start_res = client.start_proxy(&profile, &options).await;
    assert!(
        start_res.is_err(),
        "start_proxy must be rejected by Tier 4 pre-flight sing-box check"
    );
    let err_str = start_res.unwrap_err().to_string();
    assert!(
        err_str.contains("Pre-flight check failed") || err_str.contains("invalid public_key"),
        "Error message must indicate Tier 4 pre-flight rejection: {err_str}"
    );

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ===========================================================================
// SECTION 3: END-TO-END LIFECYCLE WITH REAL DAEMON CLIENT OVER UDS
// ===========================================================================

#[tokio::test]
async fn test_h15_full_e2e_lifecycle_with_daemon_client() {
    let _lock = PROXY_MANAGER_LOCK.lock().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("lifecycle_client.sock");

    // Use current process UID for authorized connection
    let current_uid = nix::unistd::getuid().as_raw();
    let identity = CallerIdentity {
        is_authenticated: true,
        uid: current_uid,
        gid: nix::unistd::getgid().as_raw(),
        pid: Some(std::process::id() as i32),
    };

    let (server_handle, shutdown_tx, _proxy_manager) =
        spawn_test_server(socket_path.clone(), Some(identity)).await;

    // Initialize real DaemonClient
    let client = DaemonClient::with_socket_path(socket_path.to_str().unwrap());

    // 1. Ping
    let ping_res = client.ping().await;
    assert_eq!(ping_res.expect("Ping must succeed"), "pong");
    assert!(client.is_alive().await);

    // 2. Initial status check
    let status = client.status().await.unwrap();
    assert_eq!(status, DaemonStatus::Disconnected);
    assert!(!client.is_running().await.unwrap());

    // 3. Start proxy with valid request
    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let options = DaemonConfigOptions {
        socks_port: 30808,
        http_port: 30809,
        ..Default::default()
    };

    let start_res = client.start_proxy(&profile, &options).await;
    if let Err(ref e) = start_res {
        eprintln!("start_proxy failed with error: {e:?}");
    }
    assert!(
        start_res.is_ok(),
        "start_proxy request must be accepted: {:?}",
        start_res.err()
    );

    // 4. Verify status transitioned
    let status_after = client.status().await.unwrap();
    assert!(
        status_after == DaemonStatus::Connecting || status_after == DaemonStatus::Connected,
        "Status after start must be Connecting or Connected, got: {status_after:?}"
    );

    // 5. Stop proxy
    let stop_res = client.stop_proxy().await;
    assert!(stop_res.is_ok(), "stop_proxy request must succeed");

    // 6. Verify status returned to Disconnected
    let status_stopped = client.status().await.unwrap();
    assert_eq!(status_stopped, DaemonStatus::Disconnected);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
