/* tests/adversarial_challenger_m1.rs
 *
 * Adversarial Challenge Suite for Milestone M1 (Core Protocol & Config Inversion)
 * Challenger 2: Empirical Verification & Adversarial Testing
 *
 * Test Dimensions:
 * 1. Pre-flight check failure handling (sing-box check rejection, staging cleanup, status=Error, diagnostics).
 * 2. File permission security (fresh 0600 creation vs. pre-existing file permission retention vulnerability).
 * 3. Re-injection attack resistance (raw JSON rejection, profile string sanitization).
 * 4. Staging path symlink attack vulnerability (CWE-59).
 * 5. Tier 1 & Tier 2 early abort verification (zero filesystem side-effects).
 */

mod common;

use common::*;
use reqwest::Client;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use vrxx::daemon::api::{create_router, ApiState};
use vrxx::daemon::core::ProxyManager;
use vrxx::daemon::events::{DaemonEvent, EventManager};
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::domain::singbox_config::build_singbox_config;
use vrxx::protocol::{CoreType, DaemonConfigOptions, DaemonStatus, StartProxyRequest};

// Process-wide mutex to serialize tests that mutate the VRXX_CONFIG_PATH environment variable
static CONFIG_ENV_LOCK: Mutex<()> = Mutex::const_new(());

// Helper: Spawns the real daemon Axum router on a random local TCP port
async fn spawn_real_daemon_api() -> anyhow::Result<(
    Arc<ProxyManager>,
    Arc<EventManager>,
    u16,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
)> {
    let (event_manager, _) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));

    let state = Arc::new(ApiState {
        proxy_manager: proxy_manager.clone(),
        event_manager: event_manager.clone(),
    });

    let app = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    Ok((proxy_manager, event_manager, port, shutdown_tx, handle))
}

// ---------------------------------------------------------------------------
// Adversarial Dimension 1: Pre-Flight Check Failure Handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_preflight_check_failure_clean_cleanup_and_error_status() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let config_path = temp_dir.path().join("staging_config.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (event_manager, _) = EventManager::new(20);
    let event_manager = Arc::new(event_manager);
    let manager = Arc::new(ProxyManager::new(event_manager.clone()));

    let mut event_rx = event_manager.subscribe();

    // Craft a profile that passes Tier 1 (valid UUID, valid Reality parameters)
    // but causes Tier 4 sing-box check to reject it due to an invalid flow parameter.
    let mut profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    profile.query_params.insert(
        "flow".to_string(),
        "unsupported-adversarial-flow-xyz".to_string(),
    );

    let options = DaemonConfigOptions {
        socks_port: 21808,
        http_port: 21809,
        tun_mode: true, // Request TUN to test if network interfaces are touched
        ..Default::default()
    };

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    // Execute start_proxy
    let result = manager.start_proxy(request).await;

    // 1. Verify that start_proxy returns Err
    assert!(result.is_err(), "Expected start_proxy to fail Tier 4 check");
    let err_msg = result.unwrap_err().to_string();

    // 2. Verify returned error contains the diagnostic message from sing-box check
    assert!(
        err_msg.contains("Pre-flight check failed (Tier 4):"),
        "Error message must indicate Tier 4 failure. Got: {err_msg}"
    );
    assert!(
        err_msg.contains("unsupported-adversarial-flow-xyz")
            || err_msg.contains("unsupported flow"),
        "Diagnostic error must include sing-box details. Got: {err_msg}"
    );

    // 3. Verify that the staging file was CLEANLY DELETED
    assert!(
        !config_path.exists(),
        "Staging config file at {:?} must be cleanly removed upon preflight failure",
        config_path
    );

    // 4. Verify that manager status is set to Error
    assert_eq!(
        manager.get_status().await,
        DaemonStatus::Error,
        "Daemon status must be DaemonStatus::Error"
    );

    // 5. Verify that no child process is running
    assert!(
        !manager.is_running().await,
        "Proxy process must not be running after failed check"
    );

    // 6. Verify that TUN network interface vrxx-tun was NOT created
    let ip_check = std::process::Command::new("ip")
        .args(["link", "show", "vrxx-tun"])
        .output();
    if let Ok(output) = ip_check {
        assert!(
            !output.status.success(),
            "vrxx-tun interface must NOT exist after preflight check failure"
        );
    }

    // 7. Verify that DaemonEvent::StatusChanged(DaemonStatus::Error) was broadcast
    let mut received_error_event = false;
    while let Ok(event) = event_rx.try_recv() {
        if let DaemonEvent::StatusChanged(DaemonStatus::Error) = event {
            received_error_event = true;
            break;
        }
    }
    assert!(
        received_error_event,
        "DaemonEvent::StatusChanged(DaemonStatus::Error) must be broadcast"
    );

    std::env::remove_var("VRXX_CONFIG_PATH");
}

// ---------------------------------------------------------------------------
// Adversarial Dimension 2: File Permission Security
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_staging_file_permissions_fresh_creation() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let config_path = temp_dir.path().join("secure_config.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (event_manager, _) = EventManager::new(20);
    let manager = ProxyManager::new(Arc::new(event_manager));

    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let options = DaemonConfigOptions {
        socks_port: 22808,
        http_port: 22809,
        tun_mode: false,
        ..Default::default()
    };

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    assert!(!config_path.exists());

    let res = manager.start_proxy(request).await;
    assert!(res.is_ok(), "start_proxy should succeed: {:?}", res);

    assert!(
        config_path.exists(),
        "Staging config file should exist while running"
    );

    let metadata = std::fs::metadata(&config_path).expect("Failed to read metadata");
    let file_mode = metadata.permissions().mode() & 0o777;

    assert_eq!(
        file_mode, 0o600,
        "Staging configuration file must have strict 0600 permissions upon creation"
    );

    let stop_res = manager.stop_proxy().await;
    assert!(stop_res.is_ok());
    assert!(!config_path.exists(), "File must be removed on stop");

    std::env::remove_var("VRXX_CONFIG_PATH");
}

#[tokio::test]
async fn test_adversarial_preexisting_insecure_file_permission_retention() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    // Attack scenario: An attacker pre-creates the staging config file at
    // /tmp/vrxx/config.json with world-writable/readable permissions (0666).
    // Because open(create, truncate, mode=0600) only sets permissions on CREATE,
    // if the file already exists, it is NOT chmod-ed to 0600.
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let config_path = temp_dir.path().join("preexisting_insecure.json");

    // Pre-create the file with 0666 permissions
    std::fs::write(&config_path, b"{}").unwrap();
    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(
        std::fs::metadata(&config_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o666
    );

    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (event_manager, _) = EventManager::new(20);
    let manager = ProxyManager::new(Arc::new(event_manager));

    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let options = DaemonConfigOptions {
        socks_port: 23808,
        http_port: 23809,
        tun_mode: false,
        ..Default::default()
    };

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    let res = manager.start_proxy(request).await;
    assert!(res.is_ok(), "start_proxy should succeed: {:?}", res);

    // Inspect permissions of the config file containing credentials
    let metadata = std::fs::metadata(&config_path).unwrap();
    let current_mode = metadata.permissions().mode() & 0o777;

    // Cleanup running proxy
    let _ = manager.stop_proxy().await;
    std::env::remove_var("VRXX_CONFIG_PATH");

    // EMPIRICAL VERIFICATION:
    // If current_mode is NOT 0600 (e.g. it remained 0666), this proves that
    // the implementation does NOT call fchmod/set_permissions to enforce 0600!
    if current_mode != 0o600 {
        eprintln!(
            "VULNERABILITY CONFIRMED: Pre-existing file retained mode 0o{:o} instead of strict 0o600!",
            current_mode
        );
    }
}

// ---------------------------------------------------------------------------
// Adversarial Dimension 3: Re-Injection Attempt Testing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_raw_json_rejection_at_rest_api() {
    let (_proxy_manager, _event_manager, port, shutdown_tx, handle) =
        spawn_real_daemon_api().await.unwrap();

    let client = Client::new();
    let url = format!("http://127.0.0.1:{port}/api/proxy/start");

    // Attack 1: Client submits raw sing-box JSON configuration
    let raw_singbox_json = serde_json::json!({
        "inbounds": [{
            "type": "socks",
            "tag": "malicious-inbound",
            "listen": "0.0.0.0",
            "listen_port": 6666
        }],
        "outbounds": [{
            "type": "direct"
        }]
    });

    let resp = client
        .post(&url)
        .json(&raw_singbox_json)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "Raw sing-box JSON configuration must be rejected with 4xx, got {}",
        resp.status()
    );

    // Attack 2: Client submits legacy config_json payload format
    let legacy_payload = serde_json::json!({
        "core_type": "sing-box",
        "config_json": "{\"inbounds\": []}"
    });

    let resp2 = client
        .post(&url)
        .json(&legacy_payload)
        .send()
        .await
        .unwrap();
    assert!(
        resp2.status().is_client_error(),
        "Legacy config_json field must be rejected with 4xx, got {}",
        resp2.status()
    );

    // Attack 3: Arbitrary string literal payload
    let resp3 = client
        .post(&url)
        .header("content-type", "application/json")
        .body("\"raw_malicious_string\"")
        .send()
        .await
        .unwrap();
    assert!(
        resp3.status().is_client_error(),
        "String primitive payload must be rejected, got {}",
        resp3.status()
    );

    // Attack 4: Empty JSON body
    let resp4 = client
        .post(&url)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert!(
        resp4.status().is_client_error(),
        "Empty JSON payload must be rejected, got {}",
        resp4.status()
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[test]
fn test_adversarial_json_injection_in_profile_fields_is_escaped() {
    // Attempt injection of arbitrary top-level sing-box directives via ParsedKey fields
    let mut profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    profile.host =
        "127.0.0.1\", \"inbounds\": [{\"type\": \"direct\", \"tag\": \"injected\"}], \"evil\": \""
            .to_string();
    profile.name = "MyServer\", \"arbitrary_command\": \"rm -rf /\"".to_string();
    profile.query_params.insert(
        "sni".to_string(),
        "gateway.example.com\", \"malicious_key\": true, \"x\": \"".to_string(),
    );

    let options = DaemonConfigOptions::default();

    // Synthesize config in daemon
    let synthesized_json = build_singbox_config(&profile, &options);

    // 1. Synthesized document must still be syntactically valid JSON
    let parsed_val: serde_json::Value = serde_json::from_str(&synthesized_json)
        .expect("Synthesized config must be strictly valid JSON despite injection characters");

    // 2. Injected "evil" or "arbitrary_command" keys must NOT exist as top-level keys
    assert!(
        parsed_val.get("evil").is_none(),
        "Injected top-level key 'evil' must not exist in output"
    );
    assert!(
        parsed_val.get("arbitrary_command").is_none(),
        "Injected top-level key 'arbitrary_command' must not exist in output"
    );

    // 3. Injected inbound tag must NOT be parsed as an inbound
    if let Some(inbounds) = parsed_val.get("inbounds").and_then(|i| i.as_array()) {
        for inb in inbounds {
            let tag = inb.get("tag").and_then(|t| t.as_str()).unwrap_or("");
            assert_ne!(
                tag, "injected",
                "Injected inbound tag must not exist in synthesized inbounds"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Adversarial Dimension 4: Tier 1 & Tier 2 Early Rejection (No File Touched)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_tier1_failure_does_not_create_staging_file() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("should_not_exist.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (event_manager, _) = EventManager::new(10);
    let manager = ProxyManager::new(Arc::new(event_manager));

    let mut profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    // Invalidate profile: invalid UUID format
    profile.uuid = "00000000-not-a-valid-uuid-here".to_string();

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options: DaemonConfigOptions::default(),
    };

    let res = manager.start_proxy(request).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Tier 1"));

    // Verify staging file was NEVER created
    assert!(
        !config_path.exists(),
        "Staging file must not be created when Tier 1 validation fails"
    );
    assert_eq!(manager.get_status().await, DaemonStatus::Error);
    assert!(!manager.is_running().await);

    std::env::remove_var("VRXX_CONFIG_PATH");
}

#[tokio::test]
async fn test_adversarial_tier2_failure_does_not_create_staging_file() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("should_not_exist_tier2.json");
    std::env::set_var("VRXX_CONFIG_PATH", &config_path);

    let (event_manager, _) = EventManager::new(10);
    let manager = ProxyManager::new(Arc::new(event_manager));

    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    // Conflicting ports
    let options = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 10808,
        ..Default::default()
    };

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    let res = manager.start_proxy(request).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Tier 2"));

    // Verify staging file was NEVER created
    assert!(
        !config_path.exists(),
        "Staging file must not be created when Tier 2 validation fails"
    );
    assert_eq!(manager.get_status().await, DaemonStatus::Error);
    assert!(!manager.is_running().await);

    std::env::remove_var("VRXX_CONFIG_PATH");
}

// ---------------------------------------------------------------------------
// Adversarial Dimension 5: Symlink Traversal Attack on Staging Path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_symlink_at_staging_path() {
    let _lock = CONFIG_ENV_LOCK.lock().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let canary_file = temp_dir.path().join("canary_target.txt");
    std::fs::write(&canary_file, b"CANARY_SECRET_DATA").unwrap();

    let symlink_path = temp_dir.path().join("staging_symlink.json");
    std::os::unix::fs::symlink(&canary_file, &symlink_path).unwrap();

    std::env::set_var("VRXX_CONFIG_PATH", &symlink_path);

    let (event_manager, _) = EventManager::new(10);
    let manager = ProxyManager::new(Arc::new(event_manager));

    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let options = DaemonConfigOptions {
        socks_port: 24808,
        http_port: 24809,
        tun_mode: false,
        ..Default::default()
    };

    let request = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    let res = manager.start_proxy(request).await;
    if res.is_ok() {
        // Read canary file
        let canary_content = std::fs::read_to_string(&canary_file).unwrap();
        if canary_content != "CANARY_SECRET_DATA" {
            println!(
                "SYMLINK BEHAVIOR OBSERVED: Daemon followed symlink and overwrote target file!"
            );
        }
        let _ = manager.stop_proxy().await;
    }

    std::env::remove_var("VRXX_CONFIG_PATH");
}
