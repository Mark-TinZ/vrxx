/* tests.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Интеграционные и модульные тесты для подсистемы демона

use crate::daemon::dns::DnsManager;
use crate::daemon::network::TunManager;

/// Проверяет создание и удаление виртуального сетевого устройства TUN.
#[tokio::test]
#[ignore = "Требует прав суперпользователя (root)"]
async fn test_tun_creation() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();
    assert!(tun_mgr.if_index > 0);
    tun_mgr.teardown().await.unwrap();
}

/// Проверяет создание и сброс правил маршрутизации.
#[tokio::test]
#[ignore = "Требует прав суперпользователя (root)"]
async fn test_routing_rules() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();
    tun_mgr.teardown().await.unwrap();
}

/// Проверяет настройку и сброс DNS через systemd-resolved.
#[tokio::test]
#[ignore = "Требует прав суперпользователя (root) и работающего systemd-resolved"]
async fn test_dns_protection() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();

    let dns_mgr = DnsManager::new().await.unwrap();
    dns_mgr
        .set_dns(tun_mgr.if_index as i32, vec!["172.19.0.1".to_string()])
        .await
        .unwrap();
    dns_mgr.reset_dns(tun_mgr.if_index as i32).await.unwrap();

    tun_mgr.teardown().await.unwrap();
}

/// Проверяет, что процедура самовосстановления сети выполняется без паник и сбоев.
#[tokio::test]
async fn test_self_healing_execution() {
    let res = crate::daemon::network::self_heal().await;
    assert!(res.is_ok());
}

/// Проверяет безопасную обработку вызова остановки прокси, когда процесс не запущен.
#[tokio::test]
async fn test_proxy_manager_stop_proxy_when_not_running() {
    let (event_manager, _) = crate::daemon::events::EventManager::new(10);
    let manager = crate::daemon::core::ProxyManager::new(std::sync::Arc::new(event_manager));
    assert_eq!(
        manager.get_status().await,
        crate::protocol::DaemonStatus::Disconnected
    );
    assert!(!manager.is_running().await);

    let res = manager.stop_proxy().await;
    assert!(res.is_ok());
    assert_eq!(
        manager.get_status().await,
        crate::protocol::DaemonStatus::Disconnected
    );
    assert!(!manager.is_running().await);
}

#[tokio::test]
async fn test_proxy_manager_stop_broadcasts_zero_speed() {
    let (event_manager, mut rx) = crate::daemon::events::EventManager::new(10);
    let manager = crate::daemon::core::ProxyManager::new(std::sync::Arc::new(event_manager));
    manager.stop_proxy().await.unwrap();

    let mut found_zero_speed = false;
    while let Ok(event) = rx.try_recv() {
        if let crate::daemon::events::DaemonEvent::Traffic {
            down_speed,
            up_speed,
            ..
        } = event
        {
            if down_speed == 0 && up_speed == 0 {
                found_zero_speed = true;
            }
        }
    }
    assert!(
        found_zero_speed,
        "Expected zero-speed Traffic event on stop_proxy"
    );
}

#[tokio::test]
async fn test_start_proxy_tier1_validation_failure() {
    let (event_manager, _) = crate::daemon::events::EventManager::new(10);
    let manager = crate::daemon::core::ProxyManager::new(std::sync::Arc::new(event_manager));

    let mut profile = crate::domain::key_parser::ParsedKey::parse(
        "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#test",
    )
    .unwrap();
    // Invalidate profile: empty host
    profile.host = String::new();

    let request = crate::protocol::StartProxyRequest {
        core_type: crate::protocol::CoreType::SingBox,
        profile,
        options: crate::protocol::DaemonConfigOptions::default(),
    };

    let result = manager.start_proxy(request).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Tier 1 profile validation failed"),
        "Unexpected error: {err_msg}"
    );
    assert_eq!(
        manager.get_status().await,
        crate::protocol::DaemonStatus::Error
    );
}

#[tokio::test]
async fn test_start_proxy_tier2_validation_failure() {
    let (event_manager, _) = crate::daemon::events::EventManager::new(10);
    let manager = crate::daemon::core::ProxyManager::new(std::sync::Arc::new(event_manager));

    let profile = crate::domain::key_parser::ParsedKey::parse(
        "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp#test",
    )
    .unwrap();

    let options = crate::protocol::DaemonConfigOptions {
        socks_port: 0,
        ..Default::default()
    };

    let request = crate::protocol::StartProxyRequest {
        core_type: crate::protocol::CoreType::SingBox,
        profile,
        options,
    };

    let result = manager.start_proxy(request).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Tier 2 configuration options validation failed"),
        "Unexpected error: {err_msg}"
    );
    assert_eq!(
        manager.get_status().await,
        crate::protocol::DaemonStatus::Error
    );
}

#[tokio::test]
async fn test_auth_root_authorized() {
    let id = super::auth::CallerIdentity {
        is_authenticated: true,
        uid: 0,
        gid: 0,
        pid: None,
    };
    assert!(super::auth::is_caller_authorized(&id).await);
}

#[tokio::test]
async fn test_auth_self_authorized() {
    let self_uid = nix::unistd::getuid().as_raw();
    let id = super::auth::CallerIdentity {
        is_authenticated: true,
        uid: self_uid,
        gid: 0,
        pid: None,
    };
    assert!(super::auth::is_caller_authorized(&id).await);
}

#[tokio::test]
async fn test_auth_system_accounts_rejected() {
    for uid in [1, 2, 42, 99, 100, 500, 999] {
        let id = super::auth::CallerIdentity {
            is_authenticated: true,
            uid,
            gid: 0,
            pid: None,
        };
        assert!(!super::auth::is_caller_authorized(&id).await);
    }
}

#[tokio::test]
async fn test_auth_nonexistent_user_rejected() {
    let invalid_uid = 987654;
    let id = super::auth::CallerIdentity {
        is_authenticated: true,
        uid: invalid_uid,
        gid: 0,
        pid: None,
    };
    assert!(!super::auth::is_caller_authorized(&id).await);
}

#[tokio::test]
async fn test_auth_unauthenticated_rejected() {
    let unauth_id = super::auth::CallerIdentity {
        is_authenticated: false,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(!super::auth::is_caller_authorized(&unauth_id).await);
}

#[tokio::test]
async fn test_auth_u32_max_sentinel_rejected() {
    let id = super::auth::CallerIdentity {
        is_authenticated: true,
        uid: u32::MAX,
        gid: u32::MAX,
        pid: None,
    };
    assert!(!super::auth::is_caller_authorized(&id).await);
}

// Process-wide mutex to serialize tests that mutate the VRXX_DAEMON_SOCKET environment variable
static SOCKET_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn test_get_socket_path_env_override() {
    let _lock = SOCKET_ENV_LOCK.lock().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let expected = temp_dir.path().join("test.sock");
    std::env::set_var("VRXX_DAEMON_SOCKET", &expected);
    assert_eq!(super::get_socket_path(), expected);
    std::env::remove_var("VRXX_DAEMON_SOCKET");
    assert_eq!(
        super::get_socket_path(),
        std::path::PathBuf::from(super::DEFAULT_SOCKET_PATH)
    );
}

#[tokio::test]
async fn test_uds_daemon_lifecycle_and_client_roundtrip() {
    let _lock = SOCKET_ENV_LOCK.lock().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("lifecycle_test.sock");
    std::env::set_var("VRXX_DAEMON_SOCKET", &socket_path);

    let (event_manager, _) = crate::daemon::events::EventManager::new(10);
    let event_manager = std::sync::Arc::new(event_manager);

    let daemon_task = tokio::spawn(async move {
        let _ = super::run_with_manager(event_manager).await;
    });

    let start = tokio::time::Instant::now();
    let mut bound = false;
    while start.elapsed() < std::time::Duration::from_secs(3) {
        if socket_path.exists() {
            bound = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(bound, "Daemon socket must be created");

    let client = crate::ipc::DaemonClient::with_socket_path(socket_path.to_str().unwrap());
    let ping_result = client.ping().await;
    assert_eq!(ping_result.unwrap(), "pong");

    let status = client.status().await.unwrap();
    assert_eq!(status, crate::protocol::DaemonStatus::Disconnected);

    let is_running = client.is_running().await.unwrap();
    assert!(!is_running);

    daemon_task.abort();
    let _ = daemon_task.await;
    std::env::remove_var("VRXX_DAEMON_SOCKET");
}
