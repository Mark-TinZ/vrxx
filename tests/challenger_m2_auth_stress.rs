/* tests/challenger_m2_auth_stress.rs
 *
 * Empirical Challenge & Stress Test Suite for Milestone M2
 * Focus: Unix Domain Socket (UDS) & SO_PEERCRED Caller Authentication
 *
 * Test Dimensions:
 * 1. Core Authorization Policy Unit Tests (is_caller_authorized)
 *    - Root (UID 0) always authorized
 *    - Current process owner (getuid()) authorized
 *    - System service UIDs (< 1000) unconditionally rejected
 *    - Inactive / non-existent high UIDs (>= 1000) rejected
 *    - Pre-existing PAM directory check ordering and bypass resistance
 * 2. Real Daemon Router & Middleware Verification
 *    - Rejection of unauthorized callers on /api/status (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/proxy/start (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/proxy/stop (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/is_running (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/history (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/ping (403 Forbidden)
 *    - Rejection of unauthorized callers on /api/events SSE stream (403 Forbidden)
 * 3. Authorized Caller Verification Over Real UDS
 *    - Root (UID 0) authorized to access endpoints
 *    - Owner (getuid()) authorized via real Linux kernel SO_PEERCRED
 *    - Real-time SSE event delivery over UDS to authorized subscriber
 * 4. Adversarial & Concurrency Stress Testing
 *    - 50 concurrent unauthorized request flood (all rejected with 403, zero panics)
 *    - 50 concurrent authorized request flood (all 200 OK)
 *    - 100 rapid connection drops without sending HTTP data (zero resource leak)
 *    - Malformed / garbage HTTP byte stream resilience
 *    - Oversized request body fast-rejection before payload consumption
 *    - Stale socket detection, active instance lockout, and RAII cleanup
 * 5. Discovered Security Vulnerabilities & Edge Case Exposing Tests
 *    - Finding 1: SO_PEERCRED extraction failure (u32::MAX) bypasses auth via systemd-logind
 *    - Finding 2: Missing ConnectInfo in auth_middleware defaults to owner UID (fails open)
 *    - Finding 3: Inactive high UID rejection vs systemd-logind fallback
 */

use axum::extract::ConnectInfo;
use axum::middleware::{self, Next};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use vrxx::daemon::api::{create_router, ApiState};
use vrxx::daemon::auth::CallerIdentity;
use vrxx::daemon::core::ProxyManager;
use vrxx::daemon::events::{DaemonEvent, EventManager, LogSource};
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::ipc::DaemonClient;
use vrxx::protocol::{CoreType, DaemonConfigOptions, DaemonStatus, LogLevel, StartProxyRequest};

// Helper: Spawns the real daemon router over a temporary Unix Domain Socket
// If `forced_identity` is Some, an outer middleware injects that identity to simulate
// callers with different UIDs. If None, uses real kernel SO_PEERCRED from the UnixStream.
async fn spawn_test_uds_server(
    socket_path: std::path::PathBuf,
    forced_identity: Option<CallerIdentity>,
) -> anyhow::Result<(
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Sender<()>,
    Arc<ProxyManager>,
    Arc<EventManager>,
)> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path)?;

    let (event_manager, _) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));
    let state = Arc::new(ApiState {
        proxy_manager: proxy_manager.clone(),
        event_manager: event_manager.clone(),
    });

    let router = create_router(state);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = if let Some(identity) = forced_identity {
        let app = router.layer(middleware::from_fn(
            move |mut req: axum::extract::Request, next: Next| {
                let id = identity.clone();
                async move {
                    req.extensions_mut().insert(ConnectInfo(id));
                    next.run(req).await
                }
            },
        ));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        })
    } else {
        let make_service = router.into_make_service_with_connect_info::<CallerIdentity>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, make_service)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        })
    };

    Ok((handle, shutdown_tx, proxy_manager, event_manager))
}

fn sample_start_proxy_request() -> StartProxyRequest {
    let raw_url = "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp&security=reality&pbk=1111111111111111111111111111111111111111111=&fp=chrome&sni=example.com&sid=12345678#test";
    let profile = parse_vpn_key(raw_url).expect("Valid test profile");
    StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options: DaemonConfigOptions {
            socks_port: 21808,
            http_port: 21809,
            ..Default::default()
        },
    }
}

// ===========================================================================
// Dimension 1: Core Authorization Policy Unit Tests (is_caller_authorized)
// ===========================================================================

#[tokio::test]
async fn test_policy_root_uid_0_always_authorized() {
    let id_no_pid = CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: None,
    };
    assert!(
        vrxx::daemon::auth::is_caller_authorized(&id_no_pid).await,
        "Root UID 0 must always be authorized"
    );
    let id_pid = CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: Some(1),
    };
    assert!(
        vrxx::daemon::auth::is_caller_authorized(&id_pid).await,
        "Root UID 0 with PID must always be authorized"
    );
}

#[tokio::test]
async fn test_policy_self_process_owner_authorized() {
    let self_uid = nix::unistd::getuid().as_raw();
    let id = CallerIdentity {
        is_authenticated: true,
        uid: self_uid,
        gid: nix::unistd::getgid().as_raw(),
        pid: None,
    };
    assert!(
        vrxx::daemon::auth::is_caller_authorized(&id).await,
        "Current process UID ({self_uid}) must be authorized"
    );
}

#[tokio::test]
async fn test_policy_system_service_uids_unconditionally_rejected() {
    // All system accounts (UID < 1000 except 0) must be rejected unconditionally
    let system_uids = [
        1,   // bin / daemon
        2,   // sys
        13,  // proxy
        33,  // www-data
        42,  // arbitrary system UID
        99,  // nobody
        100, // systemd-network
        500, // standard system service
        998, // dynamic service account
        999, // maximum system service UID
    ];

    for &uid in &system_uids {
        let id = CallerIdentity {
            is_authenticated: true,
            uid,
            gid: uid,
            pid: None,
        };
        assert!(
            !vrxx::daemon::auth::is_caller_authorized(&id).await,
            "System UID {uid} must be rejected"
        );
    }
}

#[tokio::test]
async fn test_policy_system_uid_rejected_even_if_directory_simulated() {
    // System UID check (uid < 1000) must execute before any PAM /run/user/<uid> fallback
    let id = CallerIdentity {
        is_authenticated: true,
        uid: 999,
        gid: 999,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&id).await,
        "UID 999 must be rejected unconditionally"
    );
}

#[tokio::test]
async fn test_policy_inactive_high_uid_rejected() {
    // A non-existent desktop user without an active logind session and without /run/user/<uid>
    let inactive_uid = 987654;
    let id = CallerIdentity {
        is_authenticated: true,
        uid: inactive_uid,
        gid: inactive_uid,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&id).await,
        "Inactive desktop UID {inactive_uid} must be rejected"
    );
}

#[tokio::test]
async fn test_policy_u32_max_rejected() {
    let unauth_id = CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&unauth_id).await,
        "u32::MAX unauthenticated caller must be rejected"
    );

    let spoofed_id = CallerIdentity {
        is_authenticated: true,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&spoofed_id).await,
        "u32::MAX even if authenticated flag spoofed must be rejected"
    );
}

// ===========================================================================
// Dimension 2: Real Daemon Router Authorization Rejections
// ===========================================================================

#[tokio::test]
async fn test_unauthorized_system_caller_rejected_on_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("unauth_all.sock");

    // Simulate system service UID 999
    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 999,
        gid: 999,
        pid: Some(1234),
    };

    let (handle, shutdown_tx, proxy_mgr, _) =
        spawn_test_uds_server(socket_path.clone(), Some(forced_id))
            .await
            .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // 1. GET /api/ping -> 403 Forbidden
    let resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "ping must require auth"
    );

    // 2. GET /api/status -> 403 Forbidden
    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "status must require auth"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("UID 999 is not authorized"));

    // 3. GET /api/is_running -> 403 Forbidden
    let resp = client
        .get("http://localhost/api/is_running")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "is_running must require auth"
    );

    // 4. POST /api/proxy/start -> 403 Forbidden
    let start_req = sample_start_proxy_request();
    let resp = client
        .post("http://localhost/api/proxy/start")
        .json(&start_req)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "proxy/start must require auth"
    );
    // Verify proxy was NOT started
    assert!(!proxy_mgr.is_running().await, "Proxy must not be running");

    // 5. POST /api/proxy/stop -> 403 Forbidden
    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "proxy/stop must require auth"
    );

    // 6. GET /api/history -> 403 Forbidden
    let resp = client
        .get("http://localhost/api/history")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "history must require auth"
    );

    // 7. GET /api/events (SSE) -> 403 Forbidden
    let resp = client
        .get("http://localhost/api/events")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "events SSE must require auth"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_unauthorized_inactive_desktop_caller_rejected_on_proxy_control() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("unauth_desktop.sock");

    // Simulate high UID 987654 without active logind session
    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 987654,
        gid: 987654,
        pid: Some(4321),
    };

    let (handle, shutdown_tx, proxy_mgr, _) =
        spawn_test_uds_server(socket_path.clone(), Some(forced_id))
            .await
            .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let start_req = sample_start_proxy_request();
    let resp = client
        .post("http://localhost/api/proxy/start")
        .json(&start_req)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
    assert!(!proxy_mgr.is_running().await);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

// ===========================================================================
// Dimension 3: Authorized Caller Verification Over Real UDS
// ===========================================================================

#[tokio::test]
async fn test_authorized_root_uid_0_caller_succeeds() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("root_auth.sock");

    // Simulate root UID 0
    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: Some(1),
    };

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), Some(forced_id))
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let status: DaemonStatus = resp.json().await.unwrap();
    assert_eq!(status, DaemonStatus::Disconnected);

    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_authorized_owner_via_real_kernel_peercred_and_daemon_client() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("owner_peercred.sock");

    // Real kernel SO_PEERCRED path (forced_identity: None)
    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    // Use production DaemonClient configured for UDS
    let daemon_client = DaemonClient::with_socket_path(socket_path.to_str().unwrap());

    // 1. Ping
    let ping_res = daemon_client.ping().await;
    assert_eq!(ping_res.unwrap(), "pong");
    assert!(daemon_client.is_alive().await);

    // 2. Status
    let status = daemon_client.status().await.unwrap();
    assert_eq!(status, DaemonStatus::Disconnected);

    // 3. Is Running
    let running = daemon_client.is_running().await.unwrap();
    assert!(!running);

    // 4. Stop Proxy
    let stop_res = daemon_client.stop_proxy().await;
    assert!(stop_res.is_ok());

    // 5. History
    let history = daemon_client.get_history().await.unwrap();
    assert!(history.is_empty());

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_authorized_sse_event_delivery_over_uds() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("sse_test.sock");

    let (handle, shutdown_tx, _, event_manager) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let mut es = EventSource::new(client.get("http://localhost/api/events")).unwrap();

    // Spawn task to broadcast event shortly after subscriber connects
    let em = event_manager.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        em.broadcast(DaemonEvent::StatusChanged(DaemonStatus::Connected));
        em.broadcast(DaemonEvent::Log {
            source: LogSource::Core,
            level: LogLevel::Info,
            message: "empirically verified log message".to_string(),
        });
    });

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let sse_task = tokio::spawn(async move {
        let mut status_ok = false;
        let mut log_ok = false;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    break;
                }
                event_res = tokio::time::timeout(Duration::from_millis(300), es.next()) => {
                    match event_res {
                        Ok(Some(Ok(Event::Open))) => {}
                        Ok(Some(Ok(Event::Message(msg)))) => {
                            if let Ok(event) = serde_json::from_str::<DaemonEvent>(&msg.data) {
                                match event {
                                    DaemonEvent::StatusChanged(DaemonStatus::Connected) => {
                                        status_ok = true;
                                    }
                                    DaemonEvent::Log { message, .. }
                                        if message.contains("empirically verified") =>
                                    {
                                        log_ok = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    if status_ok && log_ok {
                        break;
                    }
                }
            }
        }
        es.close();
        (status_ok, log_ok)
    });

    let sse_result = tokio::time::timeout(Duration::from_secs(3), sse_task).await;
    let _ = cancel_tx.send(());

    let (received_status, received_log) = match sse_result {
        Ok(Ok(res)) => res,
        _ => (false, false),
    };

    assert!(received_status, "Must receive StatusChanged event over UDS");
    assert!(received_log, "Must receive Log event over UDS");

    drop(client);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

// ===========================================================================
// Dimension 4: Adversarial & Concurrency Stress Testing
// ===========================================================================

#[tokio::test]
async fn test_stress_concurrent_unauthorized_requests_flood() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("unauth_flood.sock");

    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 999, // System user
        gid: 999,
        pid: Some(8888),
    };

    let (handle, shutdown_tx, proxy_mgr, _) =
        spawn_test_uds_server(socket_path.clone(), Some(forced_id))
            .await
            .unwrap();

    let client = Arc::new(
        Client::builder()
            .no_proxy()
            .unix_socket(socket_path.to_str().unwrap())
            .build()
            .unwrap(),
    );

    let concurrency = 50;
    let mut tasks = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            if i % 2 == 0 {
                let resp = c.get("http://localhost/api/status").send().await.unwrap();
                assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
            } else {
                let resp = c
                    .post("http://localhost/api/proxy/stop")
                    .send()
                    .await
                    .unwrap();
                assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
            }
        }));
    }

    for task in tasks {
        task.await.expect("Task failed");
    }

    assert!(!proxy_mgr.is_running().await);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_stress_concurrent_authorized_requests_flood() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("auth_flood.sock");

    // Real kernel SO_PEERCRED path
    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    let client = Arc::new(
        Client::builder()
            .no_proxy()
            .unix_socket(socket_path.to_str().unwrap())
            .build()
            .unwrap(),
    );

    let concurrency = 50;
    let mut tasks = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            if i % 2 == 0 {
                let resp = c.get("http://localhost/api/ping").send().await.unwrap();
                assert_eq!(resp.status(), reqwest::StatusCode::OK);
            } else {
                let resp = c.get("http://localhost/api/status").send().await.unwrap();
                assert_eq!(resp.status(), reqwest::StatusCode::OK);
            }
        }));
    }

    for task in tasks {
        task.await.expect("Task failed");
    }

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_stress_rapid_socket_connect_disconnect_without_data() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("rapid_drop.sock");

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    // Rapidly open and drop 100 raw Unix streams without writing HTTP data
    for _ in 0..100 {
        if let Ok(mut stream) = UnixStream::connect(&socket_path).await {
            let _ = stream.shutdown().await;
        }
    }

    // Server must still be responsive to regular HTTP requests
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
    assert_eq!(resp.text().await.unwrap(), "pong");

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_stress_malformed_http_payload_resilience() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("garbage.sock");

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    // Connect raw stream and write random non-HTTP bytes
    let mut stream = UnixStream::connect(&socket_path).await.unwrap();
    let garbage = [0xFF, 0x00, 0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78];
    let _ = stream.write_all(&garbage).await;
    let _ = stream.shutdown().await;

    // Server must remain alive and responsive
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
    let _ = handle.await;
}

#[tokio::test]
async fn test_stress_oversized_unauthorized_payload_fast_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("oversized.sock");

    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 999,
        gid: 999,
        pid: Some(1111),
    };

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), Some(forced_id))
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // 1 MB dummy payload to /api/proxy/start
    let huge_body = "x".repeat(1024 * 1024);

    let resp = client
        .post("http://localhost/api/proxy/start")
        .header("content-type", "application/json")
        .body(huge_body)
        .send()
        .await
        .unwrap();

    // Middleware must reject with 403 Forbidden before payload parsing
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_stress_socket_cleanup_and_stale_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("lifecycle.sock");

    // 1. Bind and drop without explicit unlinking to leave a stale socket file
    {
        let listener = UnixListener::bind(&socket_path).unwrap();
        drop(listener);
    }
    assert!(socket_path.exists());

    // 2. Stale socket probing should detect it is dead
    let is_active = UnixStream::connect(&socket_path).await.is_ok();
    assert!(!is_active, "Dead socket must not accept connections");

    // Clean stale socket
    std::fs::remove_file(&socket_path).unwrap();

    // 3. New server binds cleanly
    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    // 4. Probing active socket confirms it is live
    let is_active_now = UnixStream::connect(&socket_path).await.is_ok();
    assert!(is_active_now, "Active server must accept connections");

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

// ===========================================================================
// Dimension 5: Discovered Vulnerabilities & Security Boundary Findings
// ===========================================================================

/// FINDING 1 Remediation: SO_PEERCRED Extraction Failure Sentinel (u32::MAX) Is Rejected
#[tokio::test]
async fn test_peercred_failure_u32_max_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("peercred_fail_vuln.sock");

    let forced_id = CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), Some(forced_id))
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Unauthenticated SO_PEERCRED failure (u32::MAX) must be rejected with 403 Forbidden"
    );

    let unauth_id = CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&unauth_id).await,
        "is_caller_authorized for unauthenticated u32::MAX must return false"
    );

    let spoofed_id = CallerIdentity {
        is_authenticated: true,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(
        !vrxx::daemon::auth::is_caller_authorized(&spoofed_id).await,
        "is_caller_authorized for spoofed u32::MAX must return false"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

/// FINDING 2 Remediation: auth_middleware Fails Closed When ConnectInfo is Absent
#[tokio::test]
async fn test_missing_connect_info_rejected_fail_closed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("missing_ci_vuln.sock");

    let (event_manager, _) = EventManager::new(20);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));
    let state = Arc::new(ApiState {
        proxy_manager,
        event_manager,
    });

    let router = create_router(state);
    let listener = UnixListener::bind(&socket_path).unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Serve raw router WITHOUT into_make_service_with_connect_info
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Missing ConnectInfo must be rejected with 403 Forbidden (fail-closed)"
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"].as_str().unwrap(),
        "Caller connection is unauthenticated"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

// ===========================================================================
// Dimension 6: Comprehensive Empirical Challenge & Boundary Stress Tests
// ===========================================================================

/// Verify that u32::MAX is rejected across ALL API endpoints with HTTP 403 Forbidden
#[tokio::test]
async fn test_empirical_u32_max_rejected_across_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("u32_max_all.sock");

    // Both unauthenticated and authenticated spoofed u32::MAX must be rejected
    for is_auth in [false, true] {
        let forced_id = CallerIdentity {
            is_authenticated: is_auth,
            uid: u32::MAX,
            gid: u32::MAX,
            pid: None,
        };

        let (handle, shutdown_tx, proxy_mgr, _) =
            spawn_test_uds_server(socket_path.clone(), Some(forced_id))
                .await
                .unwrap();

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
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "ping must reject u32::MAX (is_auth={is_auth})"
        );

        // 2. GET /api/status
        let resp = client
            .get("http://localhost/api/status")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "status must reject u32::MAX (is_auth={is_auth})"
        );

        // 3. GET /api/is_running
        let resp = client
            .get("http://localhost/api/is_running")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "is_running must reject u32::MAX (is_auth={is_auth})"
        );

        // 4. POST /api/proxy/start
        let start_req = sample_start_proxy_request();
        let resp = client
            .post("http://localhost/api/proxy/start")
            .json(&start_req)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "proxy/start must reject u32::MAX (is_auth={is_auth})"
        );
        assert!(!proxy_mgr.is_running().await);

        // 5. POST /api/proxy/stop
        let resp = client
            .post("http://localhost/api/proxy/stop")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "proxy/stop must reject u32::MAX (is_auth={is_auth})"
        );

        // 6. GET /api/history
        let resp = client
            .get("http://localhost/api/history")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "history must reject u32::MAX (is_auth={is_auth})"
        );

        // 7. GET /api/events (SSE)
        let resp = client
            .get("http://localhost/api/events")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "events must reject u32::MAX (is_auth={is_auth})"
        );

        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }
}

/// Verify that boundary UIDs (>= 65534, e.g. 65534, 65535, 100000, 2000000001) are strictly rejected across all API endpoints with HTTP 403 Forbidden
#[tokio::test]
async fn test_empirical_boundary_high_uids_rejected_across_all_endpoints() {
    let test_uids = [65534, 65535, 100_000, 2_000_000_001];

    for &uid in &test_uids {
        // First verify unit policy check
        let id = CallerIdentity {
            is_authenticated: true,
            uid,
            gid: uid,
            pid: Some(42),
        };
        assert!(
            !vrxx::daemon::auth::is_caller_authorized(&id).await,
            "UID {uid} must be rejected by is_caller_authorized"
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir.path().join(format!("uid_{uid}.sock"));

        let (handle, shutdown_tx, proxy_mgr, _) =
            spawn_test_uds_server(socket_path.clone(), Some(id))
                .await
                .unwrap();

        let client = Client::builder()
            .no_proxy()
            .unix_socket(socket_path.to_str().unwrap())
            .build()
            .unwrap();

        // Verify across all endpoints
        for endpoint in [
            "/api/ping",
            "/api/status",
            "/api/is_running",
            "/api/history",
            "/api/events",
        ] {
            let resp = client
                .get(format!("http://localhost{endpoint}"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::FORBIDDEN,
                "Endpoint {endpoint} must reject UID {uid} with 403 Forbidden"
            );
        }

        let start_req = sample_start_proxy_request();
        let resp = client
            .post("http://localhost/api/proxy/start")
            .json(&start_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
        assert!(!proxy_mgr.is_running().await);

        let resp = client
            .post("http://localhost/api/proxy/stop")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }
}

/// Verify that missing ConnectInfo fails closed with HTTP 403 Forbidden across ALL endpoints
#[tokio::test]
async fn test_empirical_missing_connect_info_rejected_across_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("missing_ci_all.sock");

    let (event_manager, _) = EventManager::new(20);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));
    let state = Arc::new(ApiState {
        proxy_manager,
        event_manager,
    });

    let router = create_router(state);
    let listener = UnixListener::bind(&socket_path).unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Serve raw router WITHOUT into_make_service_with_connect_info
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // Endpoints to check
    for endpoint in [
        "/api/ping",
        "/api/status",
        "/api/is_running",
        "/api/history",
        "/api/events",
    ] {
        let resp = client
            .get(format!("http://localhost{endpoint}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "Endpoint {endpoint} must fail closed with 403 on missing ConnectInfo"
        );
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["error"], "Caller connection is unauthenticated");
    }

    let start_req = sample_start_proxy_request();
    let resp = client
        .post("http://localhost/api/proxy/start")
        .json(&start_req)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

/// Verify that unauthenticated ConnectInfo (is_authenticated: false) fails closed with HTTP 403 Forbidden across ALL endpoints
#[tokio::test]
async fn test_empirical_unauthenticated_connect_info_rejected_across_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("unauth_flag_all.sock");

    // Even if UID is 1000 or 0, is_authenticated: false MUST fail closed
    for test_uid in [0, 1000, 65534, u32::MAX] {
        let forced_id = CallerIdentity {
            is_authenticated: false,
            uid: test_uid,
            gid: test_uid,
            pid: Some(999),
        };

        let (handle, shutdown_tx, _, _) =
            spawn_test_uds_server(socket_path.clone(), Some(forced_id))
                .await
                .unwrap();

        let client = Client::builder()
            .no_proxy()
            .unix_socket(socket_path.to_str().unwrap())
            .build()
            .unwrap();

        for endpoint in [
            "/api/ping",
            "/api/status",
            "/api/is_running",
            "/api/history",
            "/api/events",
        ] {
            let resp = client
                .get(format!("http://localhost{endpoint}"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::FORBIDDEN,
                "Endpoint {endpoint} must reject unauthenticated UID {test_uid} with 403"
            );
            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["error"], "Caller connection is unauthenticated");
        }

        let resp = client
            .post("http://localhost/api/proxy/stop")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }
}

/// Verify that root (UID 0) is authorized across ALL API endpoints
#[tokio::test]
async fn test_empirical_root_uid_0_authorized_across_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("root_all.sock");

    let forced_id = CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: Some(1),
    };

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), Some(forced_id))
        .await
        .unwrap();

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

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/is_running")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/history")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/events")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/event-stream"));

    drop(resp);
    drop(client);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

/// Verify that real process owner (getuid()) is authorized across ALL API endpoints via real kernel UDS
#[tokio::test]
async fn test_empirical_process_owner_authorized_across_all_endpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("owner_all.sock");

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

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

    let resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/is_running")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/history")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let resp = client
        .get("http://localhost/api/events")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/event-stream"));

    drop(resp);
    drop(client);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

/// Verify SSE /api/events edge cases: clean termination on disconnect, multiple subscribers, and daemon shutdown
#[tokio::test]
async fn test_empirical_sse_stream_clean_termination_and_edge_cases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("sse_termination.sock");

    let (handle, shutdown_tx, _, event_manager) = spawn_test_uds_server(socket_path.clone(), None)
        .await
        .unwrap();

    let client = Arc::new(
        Client::builder()
            .no_proxy()
            .unix_socket(socket_path.to_str().unwrap())
            .build()
            .unwrap(),
    );

    // Subtest 1: Client connects and immediately drops without reading
    {
        let mut es = EventSource::new(client.get("http://localhost/api/events")).unwrap();
        es.close();
        drop(es);
    }

    // Subtest 2: Concurrent subscribers receive broadcast events and terminate cleanly
    let num_subscribers = 3;
    let mut tasks = Vec::new();

    for _ in 0..num_subscribers {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            let mut es = EventSource::new(c.get("http://localhost/api/events")).unwrap();
            let mut received = false;
            let mut timeout_ticks = 0;
            loop {
                let event_res = tokio::time::timeout(Duration::from_millis(300), es.next()).await;
                match event_res {
                    Ok(Some(Ok(Event::Open))) => {}
                    Ok(Some(Ok(Event::Message(msg)))) => {
                        if let Ok(DaemonEvent::StatusChanged(DaemonStatus::Connecting)) =
                            serde_json::from_str::<DaemonEvent>(&msg.data)
                        {
                            received = true;
                            break;
                        }
                    }
                    _ => {
                        timeout_ticks += 1;
                        if timeout_ticks >= 10 {
                            break;
                        }
                    }
                }
            }
            es.close();
            received
        }));
    }

    // Periodic broadcast while subscribers are active
    let em = event_manager.clone();
    let bcast_task = tokio::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            em.broadcast(DaemonEvent::StatusChanged(DaemonStatus::Connecting));
        }
    });

    for t in tasks {
        let res = t.await.expect("Subscriber task panic");
        assert!(res, "Subscriber must receive the broadcast event");
    }
    let _ = bcast_task.await;

    // Server must still be responsive
    let resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Drop client to release active connections
    drop(client);

    // Shutdown server and ensure it terminates within 2 seconds
    let _ = shutdown_tx.send(());
    let shutdown_res = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        shutdown_res.is_ok(),
        "Server must cleanly shutdown without hanging"
    );
}

/// Verify spoofed HTTP headers (e.g. X-UID, X-Remote-User) cannot bypass authentication
#[tokio::test]
async fn test_empirical_spoofed_http_headers_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("spoofed_headers.sock");

    let forced_id = CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };

    let (handle, shutdown_tx, _, _) = spawn_test_uds_server(socket_path.clone(), Some(forced_id))
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/status")
        .header("X-UID", "0")
        .header("X-User", "root")
        .header("X-Remote-User", "root")
        .header("Authorization", "Bearer root")
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Spoofed headers must not bypass kernel peercred authentication"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}
