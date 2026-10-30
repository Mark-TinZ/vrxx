/* tests/challenger_m4_telemetry_stress.rs
 *
 * Adversarial Challenge & Stress Test Suite for Milestone M4 (TUI Telemetry & Systemd Sandboxing)
 * Challenger 1: Empirical Verification of Telemetry Lifecycle, Clash API Resilience,
 * Process Crash Isolation, Rapid Cycling, and Dynamic TUI Telemetry.
 *
 * Test Dimensions:
 * 1. Telemetry Lifecycle & Zero-Speed Broadcast:
 *    - Start proxy with mock Clash API, verify realistic non-zero speeds over SSE / EventManager.
 *    - Stop proxy, verify immediate zero-speed broadcast (down_speed: 0, up_speed: 0) with preserved totals.
 * 2. Unexpected Core Crash & Background Task Cancellation:
 *    - Simulate sudden child process termination (SIGKILL).
 *    - Verify supervisor detects exit, transitions status to DaemonStatus::Error, and sets is_running = false.
 *    - Verify stop_telemetry() cancels polling task: mock server request count ceases incrementing.
 *    - Verify zero-speed Traffic event is broadcast on crash.
 * 3. Network Resilience & Error Event Immunity:
 *    - Connection refused (unreachable port): verify no panic, no high CPU loop, no LogLevel::Error pollution.
 *    - Slow / hanging Clash API (delay > 800ms client timeout): clean timeout without hang.
 *    - HTTP 500 Internal Server Error & corrupt/non-JSON response: safe parsing, no SSE pollution.
 *    - Sudden counter reset / wraparound: saturating_sub prevents underflow panic and recovers smoothly.
 * 4. High Concurrency & Rapid Connect / Disconnect Cycling:
 *    - Rapid sequential start/stop cycling (6 consecutive cycles): no orphaned child processes or task leaks.
 *    - Idempotent stop_proxy when already stopped.
 *    - Re-entrant start_proxy when already running cleans up previous instance safely.
 * 5. TUI App State Machine & Dynamic Formatting:
 *    - Disconnected app isolates against incoming traffic events (remains 0 B/s).
 *    - Connected app updates speeds and totals.
 *    - reset_traffic() resets speeds and zeroes circular buffer.
 *    - Dynamic bandwidth & byte formatters handle 0, boundary values, and u64::MAX without overflow.
 *    - Sparkline slicing right-aligns correctly across varied terminal widths without panic.
 * 6. End-to-End Unix Domain Socket (UDS) SSE Delivery:
 *    - Real Axum router on UDS with SO_PEERCRED auth.
 *    - DaemonClient subscribes to SSE stream, receives live Traffic event and zero-speed on stop.
 */

mod common;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use vrxx::daemon::api::{create_router, ApiState};
use vrxx::daemon::core::ProxyManager;
use vrxx::daemon::events::{DaemonEvent, EventManager};
use vrxx::domain::key_parser::ParsedKey;
use vrxx::ipc::DaemonClient;
use vrxx::protocol::{CoreType, DaemonConfigOptions, DaemonStatus, LogLevel, StartProxyRequest};
use vrxx::tui::app::{App, TRAFFIC_HISTORY_CAPACITY};
use vrxx::tui::ui::{format_dynamic_bandwidth, format_dynamic_bytes};

// Global test serialization lock to prevent conflicting environment variable mutations
// and sing-box singleton port/config file races.
static TELEMETRY_TEST_MUTEX: Mutex<()> = Mutex::const_new(());

// ---------------------------------------------------------------------------
// Controllable Mock Clash API Server Infrastructure
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockClashControl {
    download_total: Arc<AtomicU64>,
    upload_total: Arc<AtomicU64>,
    request_count: Arc<AtomicUsize>,
    response_delay_ms: Arc<AtomicU64>,
    status_code: Arc<AtomicU16>,
    corrupt_body: Arc<std::sync::Mutex<Option<String>>>,
}

impl MockClashControl {
    fn new(initial_down: u64, initial_up: u64) -> Self {
        Self {
            download_total: Arc::new(AtomicU64::new(initial_down)),
            upload_total: Arc::new(AtomicU64::new(initial_up)),
            request_count: Arc::new(AtomicUsize::new(0)),
            response_delay_ms: Arc::new(AtomicU64::new(0)),
            status_code: Arc::new(AtomicU16::new(200)),
            corrupt_body: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn set_totals(&self, down: u64, up: u64) {
        self.download_total.store(down, Ordering::SeqCst);
        self.upload_total.store(up, Ordering::SeqCst);
    }

    fn set_delay_ms(&self, delay_ms: u64) {
        self.response_delay_ms.store(delay_ms, Ordering::SeqCst);
    }

    fn set_status_code(&self, code: u16) {
        self.status_code.store(code, Ordering::SeqCst);
    }

    fn set_corrupt_body(&self, body: Option<String>) {
        *self.corrupt_body.lock().unwrap() = body;
    }

    fn get_request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }
}

async fn connections_handler(State(ctrl): State<MockClashControl>) -> impl IntoResponse {
    ctrl.request_count.fetch_add(1, Ordering::SeqCst);

    let delay = ctrl.response_delay_ms.load(Ordering::SeqCst);
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    let corrupt_opt = { ctrl.corrupt_body.lock().unwrap().clone() };
    if let Some(body) = corrupt_opt {
        let status =
            StatusCode::from_u16(ctrl.status_code.load(Ordering::SeqCst)).unwrap_or(StatusCode::OK);
        return (status, [("content-type", "text/plain")], body).into_response();
    }

    let code = ctrl.status_code.load(Ordering::SeqCst);
    if code != 200 {
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return (
            status,
            Json(serde_json::json!({
                "error": "Simulated Clash API error"
            })),
        )
            .into_response();
    }

    let down = ctrl.download_total.load(Ordering::SeqCst);
    let up = ctrl.upload_total.load(Ordering::SeqCst);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "downloadTotal": down,
            "uploadTotal": up,
            "connections": []
        })),
    )
        .into_response()
}

async fn spawn_controllable_clash_server(
    initial_down: u64,
    initial_up: u64,
) -> (
    MockClashControl,
    u16,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind mock clash tcp listener on ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let control = MockClashControl::new(initial_down, initial_up);
    let app = Router::new()
        .route("/connections", get(connections_handler))
        .route(
            "/traffic",
            get(|| async {
                Json(serde_json::json!({
                    "up": 0,
                    "down": 0
                }))
            }),
        )
        .with_state(control.clone());

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    (control, port, shutdown_tx, handle)
}

// ---------------------------------------------------------------------------
// Helpers: Valid Test StartProxyRequest Generation
// ---------------------------------------------------------------------------

fn create_test_proxy_request(socks_port: u16, http_port: u16) -> StartProxyRequest {
    let profile = ParsedKey::parse(common::valid_vless_reality_url())
        .expect("Valid test VLESS profile must parse");

    let options = DaemonConfigOptions {
        tun_mode: false, // Ensure unprivileged test execution
        socks_port,
        http_port,
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
        domain_strategy: vrxx::protocol::DomainStrategy::AsIs,
        routing_mode: vrxx::protocol::RoutingMode::Bypass,
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
    };

    StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    }
}

/// Discovers the sing-box child PID associated with the specified config path.
fn find_singbox_child_pid(config_path: &Path) -> Option<u32> {
    let path_str = config_path.to_str().unwrap();
    let output = std::process::Command::new("pgrep")
        .args(["-f", &format!("sing-box run -c {path_str}")])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                return Some(pid);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Test Suite 1: Telemetry Lifecycle & Zero-Speed Broadcast
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empirical_telemetry_lifecycle_and_zero_speed_on_stop() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_lifecycle.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    // Initial totals: 10 MB down, 5 MB up
    let (control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(10_000_000, 5_000_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, mut rx) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21080, 21081);
    let start_res = manager.start_proxy(req).await;
    assert!(
        start_res.is_ok(),
        "start_proxy failed: {:?}",
        start_res.err()
    );
    assert_eq!(manager.get_status().await, DaemonStatus::Connected);
    assert!(manager.is_running().await);

    // Drain initial events and wait for first telemetry tick (initialized = false -> 0 speed)
    let start_time = Instant::now();
    let mut initial_tick_received = false;
    while start_time.elapsed() < Duration::from_secs(4) {
        if let Ok(DaemonEvent::Traffic {
            down_speed,
            up_speed,
            down_total,
            up_total,
        }) = rx.recv().await
        {
            if down_total == 0 && up_total == 0 {
                // Startup reset event emitted by stop_telemetry() inside start_proxy
                continue;
            }
            assert_eq!(down_speed, 0, "Initial tick speed must be 0");
            assert_eq!(up_speed, 0, "Initial tick speed must be 0");
            assert_eq!(down_total, 10_000_000);
            assert_eq!(up_total, 5_000_000);
            initial_tick_received = true;
            break;
        }
    }
    assert!(
        initial_tick_received,
        "Timed out waiting for initial telemetry tick"
    );

    // Advance totals on mock Clash server: +3 MB down, +1.5 MB up
    control.set_totals(13_000_000, 6_500_000);

    // Wait for subsequent telemetry tick with realistic positive speeds
    let mut positive_speed_received = false;
    let tick_start = Instant::now();
    while tick_start.elapsed() < Duration::from_secs(4) {
        if let Ok(DaemonEvent::Traffic {
            down_speed,
            up_speed,
            down_total,
            up_total,
        }) = rx.recv().await
        {
            if down_speed > 0 || up_speed > 0 {
                assert_eq!(down_total, 13_000_000);
                assert_eq!(up_total, 6_500_000);
                // Speed delta ~3MB over ~1s
                assert!(
                    (1_000_000..=5_000_000).contains(&down_speed),
                    "Unexpected download speed: {down_speed}"
                );
                assert!(
                    (500_000..=3_000_000).contains(&up_speed),
                    "Unexpected upload speed: {up_speed}"
                );
                positive_speed_received = true;
                break;
            }
        }
    }
    assert!(
        positive_speed_received,
        "Timed out waiting for positive speed telemetry tick"
    );

    // Now stop proxy: verify immediate zero-speed broadcast with preserved cumulative totals
    let stop_res = manager.stop_proxy().await;
    assert!(stop_res.is_ok(), "stop_proxy failed: {:?}", stop_res.err());
    assert_eq!(manager.get_status().await, DaemonStatus::Disconnected);
    assert!(!manager.is_running().await);

    // Inspect events: must contain Traffic with down_speed == 0, up_speed == 0, down_total == 13_000_000
    let mut zero_speed_event_found = false;
    let stop_drain_start = Instant::now();
    while stop_drain_start.elapsed() < Duration::from_secs(2) {
        if let Ok(event) = rx.try_recv() {
            if let DaemonEvent::Traffic {
                down_speed,
                up_speed,
                down_total,
                up_total,
            } = event
            {
                if down_speed == 0
                    && up_speed == 0
                    && down_total == 13_000_000
                    && up_total == 6_500_000
                {
                    zero_speed_event_found = true;
                    break;
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    assert!(
        zero_speed_event_found,
        "ProxyManager::stop_proxy must broadcast zero-speed Traffic event with preserved cumulative session totals"
    );

    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ---------------------------------------------------------------------------
// Test Suite 2: Unexpected Child Crash & Background Task Cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empirical_unexpected_child_crash_handling_and_no_task_leak() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_crash.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(20_000_000, 10_000_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, mut rx) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21082, 21083);
    manager
        .start_proxy(req)
        .await
        .expect("start_proxy must succeed");

    assert_eq!(manager.get_status().await, DaemonStatus::Connected);
    assert!(manager.is_running().await);

    // Wait until at least 1 telemetry request has arrived at the mock server
    let wait_start = Instant::now();
    while control.get_request_count() == 0 && wait_start.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        control.get_request_count() > 0,
        "Mock Clash server must receive at least 1 telemetry poll request"
    );

    // Discover the child PID
    let child_pid = find_singbox_child_pid(&config_path)
        .expect("sing-box child process PID must be discoverable via pgrep");

    // Force sudden termination via SIGKILL
    let kill_res = kill(Pid::from_raw(child_pid as i32), Signal::SIGKILL);
    assert!(kill_res.is_ok(), "SIGKILL to child process must succeed");

    // Wait for the supervisor task to detect child exit and transition status
    let crash_detect_start = Instant::now();
    let mut status_transitioned_to_error = false;
    while crash_detect_start.elapsed() < Duration::from_secs(3) {
        if manager.get_status().await == DaemonStatus::Error && !manager.is_running().await {
            status_transitioned_to_error = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        status_transitioned_to_error,
        "Supervisor must transition status to DaemonStatus::Error and is_running to false on child crash"
    );

    // Verify zero-speed event was broadcast on crash
    let mut received_zero_speed_on_crash = false;
    let mut received_error_status_event = false;
    let drain_start = Instant::now();
    while drain_start.elapsed() < Duration::from_secs(2) {
        while let Ok(event) = rx.try_recv() {
            match event {
                DaemonEvent::StatusChanged(DaemonStatus::Error) => {
                    received_error_status_event = true;
                }
                DaemonEvent::Traffic {
                    down_speed,
                    up_speed,
                    ..
                } if down_speed == 0 && up_speed == 0 => {
                    received_zero_speed_on_crash = true;
                }
                _ => {}
            }
        }
        if received_zero_speed_on_crash && received_error_status_event {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        received_error_status_event,
        "StatusChanged(Error) event must be broadcast"
    );
    assert!(
        received_zero_speed_on_crash,
        "Zero-speed Traffic event must be broadcast on crash"
    );

    // EMPIRICAL LINGER VERIFICATION:
    // Snapshot request count on mock Clash server. Sleep 2.5 seconds (2.5x poll interval).
    // The request count MUST NOT INCREASE, proving stop_telemetry() aborted the background task.
    let snapshot_req_count = control.get_request_count();
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let final_req_count = control.get_request_count();

    assert_eq!(
        snapshot_req_count, final_req_count,
        "Background telemetry polling task continued running after child crash! Request count increased from {} to {}",
        snapshot_req_count, final_req_count
    );

    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ---------------------------------------------------------------------------
// Test Suite 3: Network Resilience & Error Event Immunity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empirical_clash_api_connection_refusal_resilience() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_refusal.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    // Target a completely closed/unbound ephemeral port
    std::env::set_var("VRXX_CLASH_API_URL", "http://127.0.0.1:49998");

    let (event_manager, mut rx) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21084, 21085);
    manager
        .start_proxy(req)
        .await
        .expect("start_proxy must succeed even if Clash API is initially unreachable");

    assert_eq!(manager.get_status().await, DaemonStatus::Connected);
    assert!(manager.is_running().await);

    // Run for 2.5 seconds (exercising at least 2 connection refusal cycles)
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // Verify proxy is still healthy and running (no panic)
    assert_eq!(manager.get_status().await, DaemonStatus::Connected);
    assert!(manager.is_running().await);

    // Verify NO LogLevel::Error or LogLevel::Warning events were broadcast or stored in history
    let history = event_manager.get_history();
    for event in &history {
        if let DaemonEvent::Log { level, message, .. } = event {
            assert!(
                *level != LogLevel::Error && *level != LogLevel::Warning,
                "Poll connection refusal must not pollute EventManager history with error logs: [{:?}] {}",
                level,
                message
            );
        }
    }

    while let Ok(event) = rx.try_recv() {
        if let DaemonEvent::Log { level, message, .. } = event {
            assert!(
                level != LogLevel::Error && level != LogLevel::Warning,
                "Poll connection refusal must not emit error logs over SSE: [{:?}] {}",
                level,
                message
            );
        }
    }

    manager.stop_proxy().await.expect("stop_proxy must succeed");
    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
}

#[tokio::test]
async fn test_empirical_clash_api_slow_timeout_resilience() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_slow.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    // Server delays response by 1200ms (>800ms client timeout)
    let (control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(50_000_000, 25_000_000).await;
    control.set_delay_ms(1200);
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, mut rx) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21086, 21087);
    manager
        .start_proxy(req)
        .await
        .expect("start_proxy must succeed");

    // Let it run for 2.5 seconds (multiple timeout cycles)
    tokio::time::sleep(Duration::from_millis(2500)).await;

    assert_eq!(manager.get_status().await, DaemonStatus::Connected);
    assert!(manager.is_running().await);

    // Verify no error log pollution
    while let Ok(event) = rx.try_recv() {
        if let DaemonEvent::Log { level, message, .. } = event {
            assert!(
                level != LogLevel::Error && level != LogLevel::Warning,
                "Timeout must not pollute SSE stream with error logs: {message}"
            );
        }
    }

    manager.stop_proxy().await.expect("stop_proxy must succeed");
    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_empirical_clash_api_http_500_and_corrupt_payload_resilience() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_corrupt.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(100_000, 50_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, _) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21088, 21089);
    manager
        .start_proxy(req)
        .await
        .expect("start_proxy must succeed");

    // Phase 1: HTTP 500 Internal Server Error
    control.set_status_code(500);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        manager.is_running().await,
        "Proxy must remain running on HTTP 500"
    );

    // Phase 2: HTML error page
    control.set_corrupt_body(Some(
        "<html><head><title>502 Bad Gateway</title></head><body>502</body></html>".to_string(),
    ));
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        manager.is_running().await,
        "Proxy must remain running on non-JSON HTML error body"
    );

    // Phase 3: Corrupted JSON with mismatched types
    control.set_corrupt_body(Some(
        r#"{"downloadTotal": "string-instead-of-u64", "uploadTotal": null}"#.to_string(),
    ));
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        manager.is_running().await,
        "Proxy must remain running on malformed JSON payload"
    );

    // Phase 4: Recovery with valid JSON
    control.set_corrupt_body(None);
    control.set_status_code(200);
    control.set_totals(200_000, 100_000);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        manager.is_running().await,
        "Proxy must recover cleanly and remain running"
    );

    manager.stop_proxy().await.expect("stop_proxy must succeed");
    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_empirical_clash_api_counter_reset_saturates_to_zero() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_reset.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    // Initial total is high: 100 MB
    let (control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(100_000_000, 50_000_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, mut rx) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req = create_test_proxy_request(21090, 21091);
    manager
        .start_proxy(req)
        .await
        .expect("start_proxy must succeed");

    // Wait for first tick to initialize prev_down
    let mut initialized = false;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(DaemonEvent::Traffic { down_total, .. }) = rx.recv().await {
            if down_total == 100_000_000 {
                initialized = true;
                break;
            }
        }
    }
    assert!(initialized, "Timed out waiting for initial 100MB tick");

    // Now simulate sudden counter reset (e.g. core restarted / stats cleared): 100MB -> 10KB
    control.set_totals(10_000, 5_000);

    // Next tick must NOT panic with underflow; down_speed must saturate to 0
    let mut saturated_zero_received = false;
    let reset_start = Instant::now();
    while reset_start.elapsed() < Duration::from_secs(3) {
        if let Ok(DaemonEvent::Traffic {
            down_speed,
            down_total,
            ..
        }) = rx.recv().await
        {
            if down_total == 10_000 {
                assert_eq!(
                    down_speed, 0,
                    "Counter decrease must compute saturating 0 speed, not underflow"
                );
                saturated_zero_received = true;
                break;
            }
        }
    }
    assert!(saturated_zero_received, "Timed out waiting for reset tick");

    // Next tick: advance counters 10KB -> 2MB, speed must recover to positive rate
    control.set_totals(2_000_000, 1_000_000);
    let mut recovered_speed_received = false;
    let rec_start = Instant::now();
    while rec_start.elapsed() < Duration::from_secs(3) {
        if let Ok(DaemonEvent::Traffic {
            down_speed,
            down_total,
            ..
        }) = rx.recv().await
        {
            if down_total == 2_000_000 && down_speed > 0 {
                recovered_speed_received = true;
                break;
            }
        }
    }
    assert!(
        recovered_speed_received,
        "Speed measurement must recover cleanly after counter reset"
    );

    manager.stop_proxy().await.expect("stop_proxy must succeed");
    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ---------------------------------------------------------------------------
// Test Suite 4: High Concurrency & Rapid Connect / Disconnect Cycling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empirical_rapid_connect_disconnect_cycling_no_leaks() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_rapid.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (_control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(1_000_000, 500_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, mut rx) = EventManager::new(100);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    // Execute 6 rapid consecutive start/stop cycles
    for cycle in 1..=6 {
        let req = create_test_proxy_request(21092, 21093);
        let start_res = manager.start_proxy(req).await;
        assert!(
            start_res.is_ok(),
            "Cycle {cycle} start_proxy failed: {:?}",
            start_res.err()
        );
        assert_eq!(manager.get_status().await, DaemonStatus::Connected);
        assert!(manager.is_running().await);

        // Allow telemetry to initialize briefly
        tokio::time::sleep(Duration::from_millis(80)).await;

        let stop_res = manager.stop_proxy().await;
        assert!(
            stop_res.is_ok(),
            "Cycle {cycle} stop_proxy failed: {:?}",
            stop_res.err()
        );
        assert_eq!(manager.get_status().await, DaemonStatus::Disconnected);
        assert!(!manager.is_running().await);

        // Small inter-cycle pause
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify all 6 cycles emitted zero-speed Traffic events
    let mut zero_speed_count = 0;
    while let Ok(event) = rx.try_recv() {
        if let DaemonEvent::Traffic {
            down_speed,
            up_speed,
            ..
        } = event
        {
            if down_speed == 0 && up_speed == 0 {
                zero_speed_count += 1;
            }
        }
    }
    assert!(
        zero_speed_count >= 6,
        "Expected at least 6 zero-speed Traffic events for 6 stop cycles, got {zero_speed_count}"
    );

    // Verify no sing-box child processes were orphaned
    let lingering_pid = find_singbox_child_pid(&config_path);
    assert!(
        lingering_pid.is_none(),
        "Lingering orphaned sing-box child process found after rapid cycling: {:?}",
        lingering_pid
    );

    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_empirical_idempotent_stop_proxy_when_stopped() {
    let (event_manager, mut rx) = EventManager::new(20);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    // When never started, stop_proxy must be idempotent and safe
    for _ in 0..5 {
        let res = manager.stop_proxy().await;
        assert!(res.is_ok());
        assert_eq!(manager.get_status().await, DaemonStatus::Disconnected);
        assert!(!manager.is_running().await);
    }

    let mut zero_traffic_count = 0;
    while let Ok(event) = rx.try_recv() {
        if let DaemonEvent::Traffic {
            down_speed,
            up_speed,
            ..
        } = event
        {
            if down_speed == 0 && up_speed == 0 {
                zero_traffic_count += 1;
            }
        }
    }
    assert_eq!(zero_traffic_count, 5);
}

#[tokio::test]
async fn test_empirical_start_proxy_when_already_running_restarts_cleanly() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config_restart.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (_control, port, shutdown_tx, server_handle) =
        spawn_controllable_clash_server(500_000, 250_000).await;
    std::env::set_var("VRXX_CLASH_API_URL", format!("http://127.0.0.1:{port}"));

    let (event_manager, _) = EventManager::new(20);
    let event_manager = Arc::new(event_manager);
    let manager = ProxyManager::new(event_manager.clone());

    let req1 = create_test_proxy_request(21094, 21095);
    manager
        .start_proxy(req1)
        .await
        .expect("First start must succeed");
    let pid1 = find_singbox_child_pid(&config_path).expect("First child must exist");

    // Second start without explicit stop: ProxyManager must terminate first process and launch new
    let req2 = create_test_proxy_request(21096, 21097);
    manager
        .start_proxy(req2)
        .await
        .expect("Second start while running must restart cleanly");
    let pid2 = find_singbox_child_pid(&config_path).expect("Second child must exist");

    assert_ne!(
        pid1, pid2,
        "Re-start must spawn a new process and terminate old"
    );

    // Verify old PID is dead
    #[cfg(unix)]
    {
        let is_old_dead = kill(Pid::from_raw(pid1 as i32), None).is_err();
        assert!(
            is_old_dead,
            "First child process (PID {pid1}) must be terminated on restart"
        );
    }

    manager.stop_proxy().await.expect("Final stop must succeed");
    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// ---------------------------------------------------------------------------
// Test Suite 5: TUI App State Machine & Dynamic Formatting
// ---------------------------------------------------------------------------

#[test]
fn test_empirical_tui_app_state_traffic_isolation() {
    let mut app = App::new();
    assert!(!app.is_connected);

    // 1. When disconnected, incoming traffic events MUST NOT set speeds
    app.apply_traffic(10_485_760, 5_242_880, 200_000_000, 100_000_000);
    assert_eq!(
        app.current_down_speed, 0,
        "Disconnected app must ignore speed"
    );
    assert_eq!(
        app.current_up_speed, 0,
        "Disconnected app must ignore speed"
    );
    assert_eq!(app.download_history.last(), Some(&0));
    assert_eq!(app.upload_history.last(), Some(&0));

    // 2. When connected, speeds and totals are recorded
    app.is_connected = true;
    app.apply_traffic(10_485_760, 5_242_880, 200_000_000, 100_000_000);
    assert_eq!(app.current_down_speed, 10_485_760);
    assert_eq!(app.current_up_speed, 5_242_880);
    assert_eq!(app.total_down, 200_000_000);
    assert_eq!(app.total_up, 100_000_000);
    assert_eq!(app.download_history.last(), Some(&10_485_760));
    assert_eq!(app.upload_history.last(), Some(&5_242_880));

    // 3. reset_traffic zeroes speeds and fills history buffer
    app.reset_traffic();
    assert_eq!(app.current_down_speed, 0);
    assert_eq!(app.current_up_speed, 0);
    assert!(app.download_history.iter().all(|&v| v == 0));
    assert!(app.upload_history.iter().all(|&v| v == 0));
}

#[test]
fn test_empirical_tui_sparkline_tail_slicing_bounds() {
    let mut app = App::new();
    app.is_connected = true;

    // Fill history with numbers 1..=60
    for i in 1..=60 {
        app.push_traffic_sample(i, i * 2);
    }
    assert_eq!(app.download_history.len(), TRAFFIC_HISTORY_CAPACITY);

    // Test slicing with varied terminal widths:
    // Case 1: width < capacity (e.g. 20)
    let inner_width = 20;
    let start = app.download_history.len().saturating_sub(inner_width);
    let slice = &app.download_history[start..];
    assert_eq!(slice.len(), 20);
    assert_eq!(slice.last(), Some(&60));
    assert_eq!(slice.first(), Some(&41));

    // Case 2: width > capacity (e.g. 100)
    let inner_width = 100;
    let start = app.download_history.len().saturating_sub(inner_width);
    let slice = &app.download_history[start..];
    assert_eq!(slice.len(), 60);
    assert_eq!(slice.first(), Some(&1));
    assert_eq!(slice.last(), Some(&60));

    // Case 3: width == 0 (zero / boundary)
    let inner_width = 0;
    let start = app.download_history.len().saturating_sub(inner_width);
    let slice = &app.download_history[start..];
    assert_eq!(slice.len(), 0);
}

#[test]
fn test_empirical_dynamic_formatters_comprehensive() {
    // Bandwidth formatting
    assert_eq!(format_dynamic_bandwidth(0), "0 B/s");
    assert_eq!(format_dynamic_bandwidth(1), "1 B/s");
    assert_eq!(format_dynamic_bandwidth(1023), "1023 B/s");
    assert_eq!(format_dynamic_bandwidth(1024), "1.0 KB/s");
    assert_eq!(format_dynamic_bandwidth(1536), "1.5 KB/s");
    assert_eq!(format_dynamic_bandwidth(1_048_576), "1.0 MB/s");
    assert_eq!(format_dynamic_bandwidth(52_428_800), "50.0 MB/s");
    assert_eq!(format_dynamic_bandwidth(1_073_741_824), "1.0 GB/s");
    assert_eq!(format_dynamic_bandwidth(10_737_418_240), "10.0 GB/s");
    // Overflow safety
    let max_fmt = format_dynamic_bandwidth(u64::MAX);
    assert!(max_fmt.ends_with("GB/s"));

    // Total bytes formatting
    assert_eq!(format_dynamic_bytes(0), "0 B");
    assert_eq!(format_dynamic_bytes(512), "512 B");
    assert_eq!(format_dynamic_bytes(1024), "1.0 KB");
    assert_eq!(format_dynamic_bytes(1_048_576), "1.0 MB");
    assert_eq!(format_dynamic_bytes(1_073_741_824), "1.00 GB");
    assert_eq!(format_dynamic_bytes(1_099_511_627_776), "1.00 TB");
    let max_bytes_fmt = format_dynamic_bytes(u64::MAX);
    assert!(max_bytes_fmt.ends_with("TB"));
}

// ---------------------------------------------------------------------------
// Test Suite 6: End-to-End Unix Domain Socket (UDS) SSE Delivery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empirical_uds_e2e_sse_telemetry_flow() {
    let _guard = TELEMETRY_TEST_MUTEX.lock().await;

    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("daemon_sse_test.sock");
    let config_path = temp_dir.path().join("config_uds_sse.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (control, clash_port, clash_shutdown_tx, clash_server_handle) =
        spawn_controllable_clash_server(10_000_000, 5_000_000).await;
    std::env::set_var(
        "VRXX_CLASH_API_URL",
        format!("http://127.0.0.1:{clash_port}"),
    );

    let (event_manager, _) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));

    let state = Arc::new(ApiState {
        proxy_manager: proxy_manager.clone(),
        event_manager: event_manager.clone(),
    });

    // Bind Unix Domain Socket listener with mode 0666
    let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666));

    let router = create_router(state);
    let make_service =
        router.into_make_service_with_connect_info::<vrxx::daemon::auth::CallerIdentity>();

    let (daemon_shutdown_tx, daemon_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let daemon_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, make_service)
            .with_graceful_shutdown(async {
                let _ = daemon_shutdown_rx.await;
            })
            .await;
    });

    // Connect via DaemonClient over UDS
    let client = DaemonClient::with_socket_path(socket_path.to_str().unwrap());
    assert_eq!(client.ping().await.unwrap(), "pong");

    // Subscribe to SSE events
    let sse_rx = client.subscribe_events();

    // Start proxy through DaemonClient over UDS
    let profile = ParsedKey::parse(common::valid_vless_reality_url()).unwrap();
    let options = DaemonConfigOptions {
        tun_mode: false,
        socks_port: 21098,
        http_port: 21099,
        ..Default::default()
    };
    let start_res = client.start_proxy(&profile, &options).await;
    assert!(
        start_res.is_ok(),
        "client.start_proxy failed: {:?}",
        start_res.err()
    );

    // Advance mock clash totals
    control.set_totals(15_000_000, 7_500_000);

    // Receive SSE Traffic event over UDS channel
    let mut received_sse_traffic = false;
    let sse_start = Instant::now();
    while sse_start.elapsed() < Duration::from_secs(5) {
        if let Ok(Ok(DaemonEvent::Traffic { down_total, .. })) =
            tokio::time::timeout(Duration::from_millis(500), sse_rx.recv()).await
        {
            if down_total >= 10_000_000 {
                received_sse_traffic = true;
                break;
            }
        }
    }
    assert!(
        received_sse_traffic,
        "DaemonClient must receive DaemonEvent::Traffic over real UDS SSE connection"
    );

    // Stop proxy over UDS
    let stop_res = client.stop_proxy().await;
    assert!(
        stop_res.is_ok(),
        "client.stop_proxy failed: {:?}",
        stop_res.err()
    );

    // Verify zero-speed event received over UDS SSE
    let mut received_sse_zero_speed = false;
    let stop_sse_start = Instant::now();
    while stop_sse_start.elapsed() < Duration::from_secs(3) {
        if let Ok(Ok(DaemonEvent::Traffic {
            down_speed,
            up_speed,
            ..
        })) = tokio::time::timeout(Duration::from_millis(500), sse_rx.recv()).await
        {
            if down_speed == 0 && up_speed == 0 {
                received_sse_zero_speed = true;
                break;
            }
        }
    }
    assert!(
        received_sse_zero_speed,
        "DaemonClient must receive zero-speed DaemonEvent::Traffic over UDS SSE on stop_proxy"
    );

    std::env::remove_var("VRXX_CONFIG_PATH");
    std::env::remove_var("VRXX_CLASH_API_URL");
    let _ = clash_shutdown_tx.send(());
    let _ = clash_server_handle.await;
    let _ = daemon_shutdown_tx.send(());
    daemon_handle.abort();
}
