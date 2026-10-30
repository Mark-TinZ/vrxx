/* tests/adversarial_challenger_m2_socket_lifecycle.rs
 *
 * Adversarial Challenge Suite for Milestone M2 (Unix Domain Socket & Caller Authentication)
 * Challenger 2: Empirical Verification of Socket Lifecycle, Stale Socket Recovery, and Signal Handling
 *
 * Empirical verification dimensions:
 * 1. Graceful Shutdown on SIGTERM (exit code 0, socket file cleanly unlinked).
 * 2. Graceful Shutdown on SIGINT (exit code 0, socket file cleanly unlinked).
 * 3. Stale Socket Recovery after SIGKILL crash (dead socket file left behind, new daemon cleans up & binds).
 * 4. Stale Socket Recovery from regular file at socket path.
 * 5. Conflict Detection against active daemon (second daemon fails safely, active daemon & socket unaffected).
 * 6. Concurrent Conflicting Launches (multiple simultaneous launches all fail safely, live daemon unaffected).
 * 7. Verification that TCP port 13337 is completely closed during daemon operation.
 * 8. Socket and directory permissions verification (0666 socket, 0755 directory).
 * 9. Adversarial Broken Symlink behavior at socket path (CWE-59 / path handling edge case).
 */

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::net::{TcpStream, UnixStream};

fn get_daemon_binary_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_vrxx") {
        PathBuf::from(path)
    } else {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("target/debug/vrxx")
    }
}

/// Spawns a real `vrxx --daemon` child process with `VRXX_DAEMON_SOCKET` set to `socket_path`.
fn spawn_daemon_subprocess(socket_path: &Path) -> Child {
    let binary = get_daemon_binary_path();
    Command::new(&binary)
        .arg("--daemon")
        .env("VRXX_DAEMON_SOCKET", socket_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn daemon process at {:?}: {e}", binary))
}

/// Polls until the daemon on `socket_path` accepts connections and responds to ping.
async fn wait_for_daemon_ready(socket_path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if socket_path.exists() {
            if let Ok(client) = reqwest::Client::builder()
                .no_proxy()
                .unix_socket(socket_path.to_str().unwrap())
                .build()
            {
                if let Ok(resp) = client.get("http://localhost/api/ping").send().await {
                    if resp.status().is_success() {
                        return true;
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn test_empirical_graceful_shutdown_sigterm() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_sigterm.sock");

    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready, "Daemon must become ready within 5 seconds");
    assert!(
        socket_path.exists(),
        "Socket file must exist while daemon is running"
    );

    // Send SIGTERM to the daemon
    let kill_res = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    assert!(kill_res.is_ok(), "Sending SIGTERM must succeed");

    // Await process exit
    let exit_status = tokio::task::spawn_blocking(move || daemon.wait())
        .await
        .unwrap()
        .expect("Failed to wait on daemon child");

    assert!(
        exit_status.success(),
        "Daemon must exit cleanly with code 0 on SIGTERM, got: {:?}",
        exit_status.code()
    );

    // Verify socket file was unlinked by SocketCleaner
    assert!(
        !socket_path.exists(),
        "Socket file must be unlinked after graceful SIGTERM shutdown"
    );
}

#[tokio::test]
async fn test_empirical_graceful_shutdown_sigint() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_sigint.sock");

    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready, "Daemon must become ready within 5 seconds");
    assert!(
        socket_path.exists(),
        "Socket file must exist while daemon is running"
    );

    // Send SIGINT to the daemon
    let kill_res = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
    assert!(kill_res.is_ok(), "Sending SIGINT must succeed");

    // Await process exit
    let exit_status = tokio::task::spawn_blocking(move || daemon.wait())
        .await
        .unwrap()
        .expect("Failed to wait on daemon child");

    assert!(
        exit_status.success(),
        "Daemon must exit cleanly with code 0 on SIGINT, got: {:?}",
        exit_status.code()
    );

    // Verify socket file was unlinked by SocketCleaner
    assert!(
        !socket_path.exists(),
        "Socket file must be unlinked after graceful SIGINT shutdown"
    );
}

#[tokio::test]
async fn test_empirical_stale_socket_recovery_after_sigkill() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_sigkill_stale.sock");

    // Step 1: Launch instance 1
    let mut daemon1 = spawn_daemon_subprocess(&socket_path);
    let pid1 = daemon1.id();

    let ready1 = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready1, "Daemon instance 1 must become ready");

    // Step 2: Abruptly kill instance 1 with SIGKILL (no userspace cleanup runs)
    let kill_res = kill(Pid::from_raw(pid1 as i32), Signal::SIGKILL);
    assert!(kill_res.is_ok(), "Sending SIGKILL must succeed");

    let _ = tokio::task::spawn_blocking(move || daemon1.wait()).await;

    // Verify socket file is still left on disk (stale socket)
    assert!(
        socket_path.exists(),
        "Socket file must still be present on disk after SIGKILL crash"
    );

    // Verify connecting to the dead socket fails
    let connect_dead = UnixStream::connect(&socket_path).await;
    assert!(
        connect_dead.is_err(),
        "Connecting to stale dead socket must fail with ECONNREFUSED"
    );

    // Step 3: Launch instance 2 on the SAME socket path
    let mut daemon2 = spawn_daemon_subprocess(&socket_path);
    let pid2 = daemon2.id();

    // Instance 2 should clean up the stale socket and successfully bind
    let ready2 = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(
        ready2,
        "Daemon instance 2 must successfully clean stale socket and become ready"
    );

    // Verify client can perform full round-trip queries
    let client = reqwest::Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let ping_resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(ping_resp, "pong");

    let status_resp = client
        .get("http://localhost/api/status")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status_resp, "disconnected");

    // Clean up instance 2 gracefully
    let _ = kill(Pid::from_raw(pid2 as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon2.wait()).await;
    assert!(
        !socket_path.exists(),
        "Socket file must be unlinked after shutdown"
    );
}

#[tokio::test]
async fn test_empirical_conflict_detection_preserves_active_daemon() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_conflict.sock");

    // Step 1: Launch instance 1 (active daemon)
    let mut daemon1 = spawn_daemon_subprocess(&socket_path);
    let pid1 = daemon1.id();

    let ready1 = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready1, "Instance 1 must become ready");

    // Step 2: Attempt to launch instance 2 on the same socket path
    let mut daemon2 = spawn_daemon_subprocess(&socket_path);

    // Instance 2 must detect active daemon and exit with failure
    let exit_status2 = tokio::task::spawn_blocking(move || daemon2.wait())
        .await
        .unwrap()
        .expect("Failed to wait on daemon 2");

    assert!(
        !exit_status2.success(),
        "Conflicting daemon launch must fail with non-zero exit code"
    );

    // Step 3: Crucial verification - Instance 1 must NOT be crashed, killed, or unlinked
    assert!(
        socket_path.exists(),
        "Active socket file must NOT be unlinked by conflicting launch"
    );

    // Verify instance 1 is still responsive
    let client = reqwest::Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let ping_resp = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .expect("Active daemon must remain responsive after conflict attempt")
        .text()
        .await
        .unwrap();
    assert_eq!(ping_resp, "pong");

    // Step 4: Gracefully terminate instance 1
    let _ = kill(Pid::from_raw(pid1 as i32), Signal::SIGTERM);
    let exit_status1 = tokio::task::spawn_blocking(move || daemon1.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(exit_status1.success());
    assert!(!socket_path.exists());
}

#[tokio::test]
async fn test_empirical_multiple_concurrent_conflicts() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_multi_conflict.sock");

    // Launch active daemon
    let mut daemon1 = spawn_daemon_subprocess(&socket_path);
    let pid1 = daemon1.id();

    let ready1 = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready1, "Instance 1 must become ready");

    // Attempt 5 simultaneous conflicting launches
    let mut handles = Vec::new();
    for _ in 0..5 {
        let sp = socket_path.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            let mut conflict_child = spawn_daemon_subprocess(&sp);
            conflict_child.wait()
        }));
    }

    for handle in handles {
        let status = handle.await.unwrap().expect("Failed to wait on child");
        assert!(
            !status.success(),
            "All concurrent conflicting launches must fail with non-zero exit code"
        );
    }

    // Active daemon must still be healthy and running
    assert!(socket_path.exists());
    let client = reqwest::Client::builder()
        .no_proxy()
        .unix_socket(socket_path.to_str().unwrap())
        .build()
        .unwrap();

    let ping = client
        .get("http://localhost/api/ping")
        .send()
        .await
        .unwrap();
    assert_eq!(ping.status(), reqwest::StatusCode::OK);

    // Clean up
    let _ = kill(Pid::from_raw(pid1 as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon1.wait()).await;
}

#[tokio::test]
async fn test_empirical_tcp_port_13337_closed_during_execution() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_tcp_check.sock");

    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready, "Daemon must become ready");

    // Attempt direct TCP connection to 127.0.0.1:13337
    let tcp_conn = TcpStream::connect("127.0.0.1:13337").await;
    assert!(
        tcp_conn.is_err(),
        "TCP port 127.0.0.1:13337 must be closed, but connection succeeded!"
    );

    // Check system sockets via ss command
    let output = Command::new("ss")
        .args(["-tulpn"])
        .output()
        .expect("Failed to run ss command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("13337"),
        "ss output must not contain port 13337, found:\n{stdout}"
    );

    // Gracefully shut down
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon.wait()).await;
}

#[tokio::test]
async fn test_empirical_socket_and_directory_permissions() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let nested_dir = temp_dir.path().join("nested_run_dir");
    let socket_path = nested_dir.join("daemon.sock");

    // Ensure parent directory does not exist yet
    assert!(!nested_dir.exists());

    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(ready, "Daemon must become ready");

    // Check parent directory permissions (must be 0755)
    let parent_meta = std::fs::metadata(&nested_dir).expect("Parent metadata");
    let parent_mode = parent_meta.permissions().mode() & 0o777;
    assert_eq!(
        parent_mode, 0o755,
        "Parent directory permissions must be 0755, got {:o}",
        parent_mode
    );

    // Check socket file permissions (must be 0666)
    let socket_meta = std::fs::metadata(&socket_path).expect("Socket metadata");
    let socket_mode = socket_meta.permissions().mode() & 0o777;
    assert_eq!(
        socket_mode, 0o666,
        "Socket permissions must be 0666, got {:o}",
        socket_mode
    );

    // Gracefully shut down
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon.wait()).await;
}

#[tokio::test]
async fn test_empirical_stale_regular_file_recovery() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_regular_file.sock");

    // Create a regular file at the socket path
    std::fs::write(&socket_path, b"stale dummy text content").unwrap();
    assert!(socket_path.is_file());

    // Launch daemon
    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(
        ready,
        "Daemon must remove stale regular file and successfully bind"
    );

    // Verify it is now a valid responding socket
    let client = reqwest::Client::builder()
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

    // Clean up
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon.wait()).await;
}

#[tokio::test]
async fn test_adversarial_dangling_symlink_behavior() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let socket_path = temp_dir.path().join("daemon_broken_symlink.sock");
    let non_existent_target = temp_dir.path().join("does_not_exist_xyz");

    // Create dangling symlink pointing to non-existent target
    std::os::unix::fs::symlink(&non_existent_target, &socket_path)
        .expect("Failed to create symlink");

    // socket_path.exists() is FALSE for dangling symlinks
    assert!(
        !socket_path.exists(),
        "std::path::Path::exists() returns false for dangling symlink"
    );
    // But symlink metadata succeeds
    assert!(socket_path.symlink_metadata().is_ok());

    // Launch daemon: with symlink_metadata().is_ok() recovery, the daemon detects
    // the stale dangling symlink, unlinks it, and binds successfully.
    let mut daemon = spawn_daemon_subprocess(&socket_path);
    let pid = daemon.id();

    let ready = wait_for_daemon_ready(&socket_path, Duration::from_secs(5)).await;
    assert!(
        ready,
        "Daemon must remove dangling symlink and successfully bind"
    );

    let client = reqwest::Client::builder()
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

    // Clean up
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    let _ = tokio::task::spawn_blocking(move || daemon.wait()).await;
}
