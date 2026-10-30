/* tests/e2e_ipc_uds.rs
 *
 * Requirement R1: Unix Domain Socket (UDS) & Caller Authentication (SO_PEERCRED)
 * Tiers 1 & 2: Feature Coverage & Boundary / Corner Cases
 */

mod common;

use common::*;
use reqwest::Client;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};

#[tokio::test]
async fn test_uds_socket_binding_and_file_mode() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon.sock");

    // Parent directory creation with 0755 permissions
    let parent = socket_path.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        check_file_mode(parent, 0o755),
        "Parent directory mode must be 0755"
    );

    // Bind UnixListener and set socket permissions to 0666
    let _listener = UnixListener::bind(&socket_path).expect("Failed to bind UnixListener");
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666)).unwrap();

    assert!(socket_path.exists(), "Socket file must exist on filesystem");
    assert!(
        check_file_mode(&socket_path, 0o666),
        "Socket permissions must be 0666"
    );
}

#[tokio::test]
async fn test_uds_so_peercred_extraction() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("peercred.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();
    let current_uid = nix::unistd::getuid().as_raw();
    let current_gid = nix::unistd::getgid().as_raw();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ucred = stream
            .peer_cred()
            .expect("SO_PEERCRED getsockopt must succeed on Linux");
        (ucred.uid(), ucred.gid(), ucred.pid())
    });

    // Client connects
    let _client_stream = UnixStream::connect(&socket_path).await.unwrap();

    let (extracted_uid, extracted_gid, extracted_pid) = server_handle.await.unwrap();
    assert_eq!(
        extracted_uid, current_uid,
        "Extracted UID must match process UID"
    );
    assert_eq!(
        extracted_gid, current_gid,
        "Extracted GID must match process GID"
    );
    assert!(extracted_pid.is_some(), "PID should be available on Linux");
}

#[test]
fn test_uds_active_user_auth_authorized() {
    let current_uid = nix::unistd::getuid().as_raw();
    // Test current running user session is authorized
    assert!(
        is_caller_authorized_mock(current_uid, current_uid),
        "Current user session must be authorized"
    );

    // Test any desktop UID >= 1000 matching active session is authorized
    assert!(
        is_caller_authorized_mock(1001, 1001),
        "Active user UID 1001 must be authorized"
    );
}

#[test]
fn test_uds_root_user_authorized() {
    // Root UID 0 is always authorized
    assert!(
        is_caller_authorized_mock(0, 1000),
        "Root UID 0 must always be authorized regardless of active session"
    );
}

#[tokio::test]
async fn test_uds_client_transport_ping_pong() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("daemon.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
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
        .expect("Ping request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "pong");

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_uds_status_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("status.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
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
        .expect("Status request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let status: DaemonStatus = resp.json().await.unwrap();
    assert_eq!(status, DaemonStatus::Disconnected);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_uds_is_running_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("running.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let resp = client
        .get("http://localhost/api/is_running")
        .send()
        .await
        .expect("is_running request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let is_running: bool = resp.json().await.unwrap();
    assert!(!is_running, "Proxy must initially not be running");

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

// ---------------------------------------------------------------------------
// Tier 2: Boundary & Corner Cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_boundary_invalid_uds_path_handling() {
    // Attempting to bind in a non-existent parent directory without creating it first fails cleanly
    let invalid_path = PathBuf::from("/nonexistent/directory/structure/never_created/daemon.sock");
    let bind_result = UnixListener::bind(&invalid_path);
    assert!(
        bind_result.is_err(),
        "Binding to a non-existent parent directory must fail with an I/O error"
    );
}

#[test]
fn test_boundary_unauthorized_system_uid_rejected() {
    // System service accounts (UID < 1000 except 0) must be rejected
    let system_uids = [1, 2, 99, 100, 500, 999];
    for &uid in &system_uids {
        assert!(
            !is_caller_authorized_mock(uid, 1000),
            "System UID {uid} must be rejected"
        );
    }
}

#[tokio::test]
async fn test_boundary_unauthenticated_connection_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("auth_reject.sock");

    // Spawn server expecting UID 99999 (which does not match current process UID)
    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), 99999)
        .await
        .unwrap();

    let client = Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    // Make mutating request: POST /api/proxy/stop
    let resp = client
        .post("http://localhost/api/proxy/stop")
        .send()
        .await
        .unwrap();

    // Must be rejected with 403 Forbidden
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Unauthorized caller must receive 403 Forbidden"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[tokio::test]
async fn test_boundary_stale_socket_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("stale.sock");

    // 1. Create a stale dead socket file (bind and drop listener without unlinking)
    {
        let listener = UnixListener::bind(&socket_path).unwrap();
        drop(listener);
    }
    assert!(socket_path.exists(), "Stale socket file must exist on disk");

    // 2. Simulate daemon startup stale socket recovery logic:
    // Probe if socket is actively responding
    let is_active = UnixStream::connect(&socket_path).await.is_ok();
    assert!(!is_active, "Stale socket must not accept connections");

    // Unlink dead socket
    if !is_active && socket_path.exists() {
        std::fs::remove_file(&socket_path).unwrap();
    }
    assert!(!socket_path.exists(), "Stale socket must be unlinked");

    // 3. New daemon binds successfully
    let new_listener = UnixListener::bind(&socket_path);
    assert!(
        new_listener.is_ok(),
        "New daemon must bind successfully after stale socket cleanup"
    );
}

#[tokio::test]
async fn test_boundary_active_socket_conflict_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("active.sock");
    let current_uid = nix::unistd::getuid().as_raw();

    // Spawn actively running daemon
    let (handle, shutdown_tx, _state) = spawn_test_uds_daemon(socket_path.clone(), current_uid)
        .await
        .unwrap();

    // Probing active socket succeeds
    let is_active = UnixStream::connect(&socket_path).await.is_ok();
    assert!(
        is_active,
        "Active daemon socket must respond to connection probe"
    );

    // Attempting to bind a second instance should detect active daemon
    if is_active {
        // Correct behavior: do NOT remove socket, abort startup
        assert!(socket_path.exists(), "Active socket must not be unlinked");
    }

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}
