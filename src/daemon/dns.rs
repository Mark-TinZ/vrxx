/* dns.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Управление DNS через systemd-resolved (D-Bus DNS Manager)
//!
//! Модуль взаимодействует с D-Bus сервисом `org.freedesktop.resolve1` для:
//! - Назначения эксклюзивных DNS-серверов интерфейсу `vrxx-tun`
//! - Маршрутизации всех доменных запросов (`~.`) в туннель для предотвращения утечек DNS (DNS Leak Protection)
//! - Сброса настроек DNS при отключении прокси

use anyhow::Result;
use std::net::IpAddr;
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.resolve1.Manager",
    default_service = "org.freedesktop.resolve1",
    default_path = "/org/freedesktop/resolve1"
)]
trait Resolve1Manager {
    fn set_link_dns(&self, ifindex: i32, addresses: &[(i32, Vec<u8>)]) -> zbus::Result<()>;
    fn set_link_domains(&self, ifindex: i32, domains: &[(&str, bool)]) -> zbus::Result<()>;
}

/// Менеджер DNS, работающий через системную шину D-Bus.
pub struct DnsManager {
    connection: Connection,
}

impl DnsManager {
    /// Создает новый экземпляр DnsManager с подключением к системной шине.
    pub async fn new() -> Result<Self> {
        let connection = zbus::Connection::system().await?;
        Ok(Self { connection })
    }

    /// Назначает DNS-серверы и поисковые домены для заданного индекса сетевого интерфейса.
    pub async fn set_dns(&self, iface_index: i32, dns_servers: Vec<String>) -> Result<()> {
        let proxy = Resolve1ManagerProxy::new(&self.connection).await?;

        let mut addrs = Vec::new();
        for server in dns_servers {
            if let Ok(ip) = server.parse::<IpAddr>() {
                match ip {
                    IpAddr::V4(v4) => {
                        addrs.push((2, v4.octets().to_vec()));
                    }
                    IpAddr::V6(v6) => {
                        addrs.push((10, v6.octets().to_vec()));
                    }
                }
            }
        }

        proxy.set_link_dns(iface_index, &addrs).await?;
        proxy.set_link_domains(iface_index, &[("~.", true)]).await?;

        Ok(())
    }

    /// Сбрасывает DNS-настройки интерфейса.
    pub async fn reset_dns(&self, iface_index: i32) -> Result<()> {
        let proxy = Resolve1ManagerProxy::new(&self.connection).await?;
        proxy.set_link_dns(iface_index, &[]).await?;
        proxy.set_link_domains(iface_index, &[]).await?;
        Ok(())
    }
}
