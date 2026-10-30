/* tests/e2e_telemetry.rs
 *
 * Requirement R4: Clash API Telemetry, Event Broadcasting & Dynamic Bandwidth
 * Tiers 1 & 2: Feature Coverage & Boundary / Corner Cases
 */

mod common;

use common::*;
use reqwest::Client;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Tier 1: Feature Coverage (Happy Paths)
// ---------------------------------------------------------------------------

#[test]
fn test_clash_traffic_api_json_parsing() {
    let raw_json = r#"{"up": 1048576, "down": 5242880}"#;
    let parsed: ClashTrafficResponse =
        serde_json::from_str(raw_json).expect("Failed to parse Clash traffic JSON");

    assert_eq!(parsed.up, 1_048_576, "Upload speed mismatch");
    assert_eq!(parsed.down, 5_242_880, "Download speed mismatch");
}

#[test]
fn test_clash_connections_api_json_parsing() {
    let raw_json = r#"{"downloadTotal": 104857600, "uploadTotal": 52428800}"#;
    let parsed: ClashConnectionsResponse =
        serde_json::from_str(raw_json).expect("Failed to parse Clash connections JSON");

    assert_eq!(parsed.download_total, 104_857_600);
    assert_eq!(parsed.upload_total, 52_428_800);
}

#[tokio::test]
async fn test_daemon_event_traffic_broadcast() {
    let (tx, mut rx) = broadcast::channel::<DaemonEventWire>(16);

    let event = DaemonEventWire::Traffic {
        down_speed: 2_097_152,
        up_speed: 1_048_576,
        down_total: 200_000_000,
        up_total: 100_000_000,
    };

    tx.send(event).expect("Broadcast must succeed");

    let received = rx.recv().await.expect("Receive must succeed");
    match received {
        DaemonEventWire::Traffic {
            down_speed,
            up_speed,
            down_total,
            up_total,
        } => {
            assert_eq!(down_speed, 2_097_152);
            assert_eq!(up_speed, 1_048_576);
            assert_eq!(down_total, 200_000_000);
            assert_eq!(up_total, 100_000_000);
        }
        _ => panic!("Expected Traffic event variant"),
    }
}

#[tokio::test]
async fn test_telemetry_collector_polling_loop() {
    // Spawn mock Clash server on ephemeral port
    let (handle, shutdown_tx, port) =
        spawn_mock_clash_server(0, 512_000, 1_024_000, 50_000_000, 100_000_000)
            .await
            .unwrap();

    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{port}");

    // Poll /traffic
    let traffic_resp = client
        .get(format!("{base_url}/traffic"))
        .send()
        .await
        .unwrap()
        .json::<ClashTrafficResponse>()
        .await
        .unwrap();
    assert_eq!(traffic_resp.up, 512_000);
    assert_eq!(traffic_resp.down, 1_024_000);

    // Poll /connections
    let conn_resp = client
        .get(format!("{base_url}/connections"))
        .send()
        .await
        .unwrap()
        .json::<ClashConnectionsResponse>()
        .await
        .unwrap();
    assert_eq!(conn_resp.upload_total, 50_000_000);
    assert_eq!(conn_resp.download_total, 100_000_000);

    let _ = shutdown_tx.send(());
    let _ = handle.await;
}

#[test]
fn test_dynamic_bandwidth_unit_formatting() {
    assert_eq!(format_dynamic_bandwidth(500), "500 B/s");
    assert_eq!(format_dynamic_bandwidth(1_024), "1.0 KB/s");
    assert_eq!(format_dynamic_bandwidth(153_600), "150.0 KB/s");
    assert_eq!(format_dynamic_bandwidth(10_485_760), "10.0 MB/s");
    assert_eq!(format_dynamic_bandwidth(1_073_741_824), "1.0 GB/s");
}

// ---------------------------------------------------------------------------
// Tier 2: Boundary & Corner Cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_boundary_clash_api_unreachable() {
    // Pick an unassigned port that is closed
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(100))
        .build()
        .unwrap();

    let resp = client.get("http://127.0.0.1:49999/traffic").send().await;
    assert!(
        resp.is_err(),
        "Connection to closed Clash API port must return Err without panicking"
    );
}

#[test]
fn test_boundary_clash_api_corrupt_response() {
    let corrupt_json = r#"{"up": "not-a-number", "invalid": true}"#;
    let res = serde_json::from_str::<ClashTrafficResponse>(corrupt_json);
    assert!(
        res.is_err(),
        "Corrupt Clash API response must fail deserialization cleanly"
    );
}

#[test]
fn test_boundary_zero_traffic_when_disconnected() {
    let zero_traffic = DaemonEventWire::Traffic {
        down_speed: 0,
        up_speed: 0,
        down_total: 0,
        up_total: 0,
    };
    match zero_traffic {
        DaemonEventWire::Traffic {
            down_speed,
            up_speed,
            ..
        } => {
            assert_eq!(down_speed, 0);
            assert_eq!(up_speed, 0);
            assert_eq!(format_dynamic_bandwidth(down_speed), "0 B/s");
        }
        _ => panic!("Expected Traffic event"),
    }
}

#[test]
fn test_boundary_traffic_overflow_resilience() {
    let extreme_speed = u64::MAX;
    let formatted = format_dynamic_bandwidth(extreme_speed);
    assert!(formatted.ends_with("GB/s"));
    assert!(!formatted.is_empty());
}
