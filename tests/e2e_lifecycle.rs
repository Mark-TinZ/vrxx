/* tests/e2e_lifecycle.rs
 *
 * Tiers 3 & 4: Cross-Feature Interactions & Real-World End-to-End Scenarios
 */

mod common;

use common::*;
use reqwest::Client;
use std::os::unix::fs::PermissionsExt;
use tokio::net::{UnixListener, UnixStream};
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::domain::singbox_config::build_singbox_config;
use vrxx::settings::VpnKeyData;

// ---------------------------------------------------------------------------
// Tier 3: Cross-Feature Interactions (Pairwise)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interaction_start_proxy_query_telemetry_stop_proxy_uds() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("interaction.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    // 1. Spawn daemon on UDS
    let (handle, shutdown_tx, state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // 2. Query initial status over UDS
    let status_resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<DaemonStatus>()
        .await
        .unwrap();
    assert_eq!(status_resp, DaemonStatus::Disconnected);

    // 3. Start proxy with typed request over UDS
    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options: DaemonConfigOptions::default(),
    };

    let start_resp = client
        .post("http://localhost/api/proxy/start")
        .json(&req)
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), reqwest::StatusCode::OK);

    // 4. Query status after start
    let status_after = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<DaemonStatus>()
        .await
        .unwrap();
    assert_eq!(status_after, DaemonStatus::Connected);

    // 5. Query live telemetry from mock Clash server
    let (_clash_handle, clash_shutdown, clash_port) =
        spawn_mock_clash_server(0, 250_000, 1_500_000, 20_000_000, 100_000_000)
            .await
            .unwrap();

    let clash_client = Client::new();
    let traffic = clash_client
        .get(format!("http://127.0.0.1:{clash_port}/traffic"))
        .send()
        .await
        .unwrap()
        .json::<ClashTrafficResponse>()
        .await
        .unwrap();
    assert_eq!(traffic.up, 250_000);
    assert_eq!(traffic.down, 1_500_000);

    // 6. Stop proxy over UDS
    let stop_resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(stop_resp.status(), reqwest::StatusCode::OK);

    // 7. Verify status is Disconnected
    let status_stopped = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<DaemonStatus>()
        .await
        .unwrap();
    assert_eq!(status_stopped, DaemonStatus::Disconnected);

    let _ = clash_shutdown.send(());
    let _ = shutdown_tx.send(());
    let _ = handle.await;
    assert_eq!(*state.status.lock().unwrap(), DaemonStatus::Disconnected);
}

#[tokio::test]
async fn test_interaction_keystore_v2_to_uds_proxy_start() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("keystore_uds.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    // 1. Encrypt profile in V2 keystore container
    let user_secret = b"pairwise-interaction-user-secret-key";
    let v2_key = derive_user_key_v2(user_secret).unwrap();
    let original_url = valid_vless_reality_url();
    let original_keys = vec![sample_vpn_key_data("VLESS Keystore Key", original_url)];
    let encrypted_container =
        encrypt_v2_container(&serde_json::to_vec(&original_keys).unwrap(), &v2_key).unwrap();

    // 2. Read and decrypt from keystore
    let decrypted_bytes = decrypt_v2_container(&encrypted_container, &v2_key).unwrap();
    let keys: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_bytes).unwrap();
    let active_profile = parse_vpn_key(&keys[0].url).unwrap();

    // 3. Spawn UDS daemon and submit decrypted profile in StartProxyRequest
    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: active_profile,
        options: DaemonConfigOptions::default(),
    };

    let resp = client
        .post("http://localhost/api/proxy/start")
        .json(&req)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_interaction_so_peercred_auth_with_typed_ipc() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("auth_typed.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // Verify authorized caller invokes ping and status
    let ping_resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(ping_resp.status(), reqwest::StatusCode::OK);

    let profile = parse_vpn_key(valid_vless_reality_url()).unwrap();
    let req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options: DaemonConfigOptions::default(),
    };

    let start_resp = client
        .post("http://localhost/api/proxy/start")
        .json(&req)
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), reqwest::StatusCode::OK);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_interaction_stale_socket_recovery_with_active_client() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("stale_recovery.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    // 1. Create a dead socket
    {
        let _listener = UnixListener::bind(&socket_path).unwrap();
    }
    assert!(socket_path.exists());

    // 2. Launch new daemon over the dead socket path (daemon unlinks dead socket and binds)
    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    // 3. Client connects and succeeds
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

// ---------------------------------------------------------------------------
// Tier 4: Real-World Scenarios
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_scenario_complete_vpn_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("lifecycle.sock");
    let keystore_dir = temp_dir.path().join("keystore");
    std::fs::create_dir_all(&keystore_dir).unwrap();
    std::fs::set_permissions(&keystore_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    let key_file = keystore_dir.join(".key");
    let data_file = keystore_dir.join("data.dat");
    let current_uid = nix::unistd::getuid().as_raw();

    // Step 1: Initialize User Keystore (0600 mode)
    let user_secret = b"lifecycle-scenario-csprng-secret-256";
    std::fs::write(&key_file, user_secret).unwrap();
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let v2_key = derive_user_key_v2(user_secret).unwrap();

    // Step 2: User imports VPN profile and persists to data.dat
    let profile_url = valid_vless_reality_url();
    let user_keys = vec![sample_vpn_key_data("Production VLESS", profile_url)];
    let encrypted_data =
        encrypt_v2_container(&serde_json::to_vec(&user_keys).unwrap(), &v2_key).unwrap();
    std::fs::write(&data_file, &encrypted_data).unwrap();
    std::fs::set_permissions(&data_file, std::fs::Permissions::from_mode(0o600)).unwrap();

    // Step 3: Spawn UDS Privileged Daemon
    let (daemon_handle, shutdown_tx, _state) =
        spawn_test_uds_daemon(socket_path.clone(), current_uid)
            .await
            .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // Step 4: Health check via UDS
    let ping = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(ping.status(), reqwest::StatusCode::OK);

    // Step 5: Read and decrypt profile from storage
    let loaded_bytes = std::fs::read(&data_file).unwrap();
    let decrypted_bytes = decrypt_v2_container(&loaded_bytes, &v2_key).unwrap();
    let loaded_keys: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_bytes).unwrap();
    let parsed_profile = parse_vpn_key(&loaded_keys[0].url).unwrap();

    // Step 6: Pre-flight validation on profile
    assert!(parsed_profile.validate().is_ok());

    // Step 7: Synthesize sing-box config internally
    let settings = sample_app_settings();
    let config_json = build_singbox_config(&parsed_profile, &settings);
    let val: serde_json::Value = serde_json::from_str(&config_json).unwrap();
    assert!(val.get("outbounds").is_some());

    // Step 8: Client requests proxy start over UDS
    let start_req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: parsed_profile,
        options: DaemonConfigOptions::default(),
    };
    let start_resp = client
        .post("http://localhost/api/proxy/start")
        .json(&start_req)
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), reqwest::StatusCode::OK);

    // Step 9: Verify daemon status is Connected
    let status = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<DaemonStatus>()
        .await
        .unwrap();
    assert_eq!(status, DaemonStatus::Connected);

    // Step 10: Client requests proxy stop over UDS
    let stop_resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();
    assert_eq!(stop_resp.status(), reqwest::StatusCode::OK);

    // Step 11: Verify daemon status returned to Disconnected
    let status_final = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<DaemonStatus>()
        .await
        .unwrap();
    assert_eq!(status_final, DaemonStatus::Disconnected);

    let _ = shutdown_tx.send(());
    let _ = daemon_handle.await;
}

#[test]
fn test_scenario_profile_import_encrypted_persistence_and_migration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let data_dir = temp_dir.path().join("vrxx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    let data_file = data_dir.join("data.dat");
    let key_file = data_dir.join(".key");

    // 1. Simulate legacy VRXXDAT1 file existing on disk
    let legacy_machine_id = "fedcba9876543210fedcba9876543210";
    let imported_keys = vec![
        sample_vpn_key_data("Legacy VLESS", valid_vless_reality_url()),
        sample_vpn_key_data("Legacy Shadowsocks", valid_shadowsocks_url()),
    ];

    let v1_bytes =
        vrxx::crypto::keystore::encrypt_keys_with_id(&imported_keys, legacy_machine_id).unwrap();
    std::fs::write(&data_file, &v1_bytes).unwrap();
    assert_eq!(&std::fs::read(&data_file).unwrap()[0..8], MAGIC_HEADER_V1);

    // 2. Migration: Detect V1 header, decrypt using machine-id
    let read_v1 = std::fs::read(&data_file).unwrap();
    let decrypted_legacy =
        vrxx::crypto::keystore::decrypt_keys_with_id(&read_v1, legacy_machine_id).unwrap();
    assert_eq!(decrypted_legacy.len(), 2);

    // 3. Generate user key and upgrade to VRXXDAT2
    let mut user_secret = [0u8; 32];
    use aes_gcm::aead::rand_core::RngCore;
    use aes_gcm::aead::OsRng;
    OsRng.fill_bytes(&mut user_secret);

    std::fs::write(&key_file, user_secret).unwrap();
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();

    let v2_key = derive_user_key_v2(&user_secret).unwrap();
    let v2_bytes =
        encrypt_v2_container(&serde_json::to_vec(&decrypted_legacy).unwrap(), &v2_key).unwrap();

    // 4. Overwrite data.dat with V2 container and verify mode 0600
    std::fs::write(&data_file, &v2_bytes).unwrap();
    std::fs::set_permissions(&data_file, std::fs::Permissions::from_mode(0o600)).unwrap();

    let final_data = std::fs::read(&data_file).unwrap();
    assert_eq!(
        &final_data[0..8],
        MAGIC_HEADER_V2,
        "Migrated file must have VRXXDAT2 header"
    );
    assert!(
        check_file_mode(&data_file, 0o600),
        "Migrated file must be 0600"
    );

    // 5. Verify subsequent reads work seamlessly with V2 key
    let restored_bytes = decrypt_v2_container(&final_data, &v2_key).unwrap();
    let restored_keys: Vec<VpnKeyData> = serde_json::from_slice(&restored_bytes).unwrap();
    assert_eq!(restored_keys.len(), 2);
    assert_eq!(restored_keys[0].name, "Legacy VLESS");
    assert_eq!(restored_keys[1].name, "Legacy Shadowsocks");
}

#[tokio::test]
async fn test_scenario_daemon_crash_and_stale_socket_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("crash_recovery.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    // Step 1: Daemon instance 1 starts
    let (handle1, shutdown_tx1, _state1) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    assert_eq!(
        client
            .get("http://localhost/api/ping")
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );

    // Step 2: Daemon instance 1 crashes abruptly (drop without cleanup)
    let _ = shutdown_tx1.send(());
    let _ = handle1.await;
    // Leave socket file on disk (simulate ungraceful crash)
    assert!(socket_path.exists(), "Socket file left behind after crash");

    // Step 3: Probe confirms socket is dead
    assert!(
        UnixStream::connect(&socket_path).await.is_err(),
        "Dead socket must refuse connection"
    );

    // Step 4: Daemon instance 2 starts up, recovers socket, unlinks, and binds cleanly
    let (handle2, shutdown_tx2, _state2) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    // Step 5: Client immediately reconnects and pings successfully
    let resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(resp.text().await.unwrap(), "pong");

    let _ = shutdown_tx2.send(());
    let _ = handle2.await;
}

#[tokio::test]
async fn test_scenario_corrupt_profile_rejection_no_side_effects() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("corrupt_reject.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    let (handle, shutdown_tx, state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // Parse invalid profile (invalid UUID)
    let invalid_profile = parse_vpn_key(invalid_uuid_url()).unwrap();
    let req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: invalid_profile,
        options: DaemonConfigOptions::default(),
    };

    let resp = client
        .post("http://localhost/api/proxy/start")
        .json(&req)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // Verify daemon status is still Disconnected and is_running is false
    assert_eq!(*state.status.lock().unwrap(), DaemonStatus::Disconnected);
    assert!(!state.is_running.load(std::sync::atomic::Ordering::SeqCst));

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}
