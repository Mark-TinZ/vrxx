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
    assert_eq!(manager.get_status().await, "Disconnected");
    assert!(!manager.is_running().await);

    let res = manager.stop_proxy().await;
    assert!(res.is_ok());
    assert_eq!(manager.get_status().await, "Disconnected");
    assert!(!manager.is_running().await);
}
