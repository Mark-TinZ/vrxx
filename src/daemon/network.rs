/* network.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Сетевой стек TUN-интерфейса и процедуры самовосстановления (Network & Self-Healing)
//!
//! Модуль отвечает за:
//! - Создание и настройку виртуального TUN-устройства `vrxx-tun` (IPv4 `172.19.0.1/30`) через `rtnetlink`
//! - Настройку таблицы маршрутизации 100 и правил `ip rule` с исключением маркированного трафика (`fwmark 0x255`)
//! - Процедуры самовосстановления (Self-Healing) при старте демона:
//!   - Удаление зависших сиротских интерфейсов `vrxx-tun`
//!   - Очистка устаревших правил `ip rule table 100`
//!   - Сброс системных настроек GNOME proxy в `none` при аварийном завершении

use anyhow::{Context, Result};
use futures_util::stream::TryStreamExt;
use rtnetlink::{new_connection, Handle, LinkUnspec, RouteMessageBuilder};
use std::net::{IpAddr, Ipv4Addr};
use tun_rs::{AsyncDevice, DeviceBuilder};

/// Менеджер виртуального сетевого устройства TUN.
pub struct TunManager {
    handle: Handle,
    connection_task: tokio::task::JoinHandle<()>,
    tun: Option<AsyncDevice>,
    pub if_index: u32,
}

impl TunManager {
    /// Создает новый экземпляр TunManager с подключением к Netlink.
    pub async fn new() -> Result<Self> {
        let (connection, handle, _) = new_connection()?;
        let connection_task = tokio::spawn(connection);
        Ok(Self {
            handle,
            connection_task,
            tun: None,
            if_index: 0,
        })
    }

    /// Выполняет создание TUN-устройства `vrxx-tun`, назначение IP и настройку маршрутизации.
    pub async fn setup(&mut self) -> Result<()> {
        tracing::info!("Настройка TUN интерфейса vrxx-tun (172.19.0.1/30, table 100)...");

        // 1. Создание TUN устройства
        let tun = DeviceBuilder::new()
            .name("vrxx-tun")
            .build_async()
            .context("Не удалось создать TUN устройство")?;
        self.tun = Some(tun);
        tracing::debug!("TUN устройство vrxx-tun успешно создано");

        // 2. Получение индекса сетевого интерфейса
        let mut links = self
            .handle
            .link()
            .get()
            .match_name("vrxx-tun".to_string())
            .execute();
        let link = links
            .try_next()
            .await?
            .context("Интерфейс vrxx-tun не найден")?;
        self.if_index = link.header.index;
        tracing::debug!("Интерфейс vrxx-tun найден, if_index={}", self.if_index);

        // 3. Установка IPv4 адреса 172.19.0.1/30
        let addr = Ipv4Addr::new(172, 19, 0, 1);
        let prefix = 30;
        self.handle
            .address()
            .add(self.if_index, IpAddr::V4(addr), prefix)
            .execute()
            .await
            .context("Не удалось назначить IP адрес интерфейсу")?;
        tracing::debug!("Интерфейсу vrxx-tun назначен адрес 172.19.0.1/30");

        // 4. Перевод интерфейса в состояние UP
        self.handle
            .link()
            .set(LinkUnspec::new_with_index(self.if_index).up().build())
            .execute()
            .await
            .context("Не удалось поднять интерфейс")?;
        tracing::debug!("Интерфейс vrxx-tun переведен в состояние UP");

        // 5. Создание таблицы маршрутизации 100 и добавление маршрута по умолчанию
        let route_msg = RouteMessageBuilder::<Ipv4Addr>::new()
            .destination_prefix(Ipv4Addr::new(0, 0, 0, 0), 0)
            .output_interface(self.if_index)
            .table_id(100)
            .build();
        self.handle
            .route()
            .add(route_msg)
            .execute()
            .await
            .context("Не удалось добавить маршрут по умолчанию в таблицу 100")?;
        tracing::debug!("Добавлен маршрут по умолчанию в таблицу 100 через vrxx-tun");

        // 6. Добавление ip rule для перенаправления трафика в таблицу 100
        let status = tokio::process::Command::new("ip")
            .args(["rule", "add", "not", "fwmark", "0x255", "table", "100"])
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("Не удалось добавить ip rule для таблицы 100");
        }
        tracing::info!("Настройка TUN интерфейса vrxx-tun успешно завершена");

        Ok(())
    }

    /// Корректно удаляет интерфейс и очищает правила маршрутизации.
    pub async fn teardown(&mut self) -> Result<()> {
        if self.if_index != 0 {
            tracing::info!(
                "Очистка и удаление TUN интерфейса vrxx-tun (if_index={})...",
                self.if_index
            );

            // Удаление ip rule
            if let Ok(st) = tokio::process::Command::new("ip")
                .args(["rule", "del", "not", "fwmark", "0x255", "table", "100"])
                .status()
                .await
            {
                if !st.success() {
                    tracing::warn!("Не удалось удалить ip rule для таблицы 100 при остановке");
                }
            }

            // Удаление записей из таблицы маршрутизации
            let route_msg = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(Ipv4Addr::new(0, 0, 0, 0), 0)
                .table_id(100)
                .build();
            if let Err(e) = self.handle.route().del(route_msg).execute().await {
                tracing::warn!(
                    "Не удалось удалить маршрут по умолчанию из таблицы 100: {}",
                    e
                );
            }
        }

        self.tun = None;
        self.connection_task.abort();
        tracing::debug!("Задача Netlink соединения остановлена");
        Ok(())
    }
}

/// Функция самовосстановления сети (Self-Healing).
/// Выполняется при старте демона `vrxx-daemon`:
/// 1. Удаление подвисших/сиротских интерфейсов `vrxx-tun`.
/// 2. Очистка подвисших правил таблицы маршрутизации (`ip rule del table 100`).
/// 3. Сброс `org.gnome.system.proxy mode` на `"none"`, если прокси не подключен.
pub async fn self_heal() -> Result<()> {
    tracing::info!("Запуск процедур сетевого самовосстановления (Self-Healing)...");

    // 1. Проверка наличия и удаление сиротских интерфейсов vrxx-tun
    let status_del_tun = tokio::process::Command::new("ip")
        .args(["link", "del", "vrxx-tun"])
        .status()
        .await;

    match status_del_tun {
        Ok(st) if st.success() => {
            tracing::info!("Self-Healing: Removed orphaned vrxx-tun interface.");
        }
        _ => {
            tracing::debug!("Self-Healing: No orphaned vrxx-tun interfaces found.");
        }
    }

    // 2. Очистка зависших правил таблицы маршрутизации (ip rule del table 100)
    let mut cleaned_rules = 0;
    loop {
        let status = tokio::process::Command::new("ip")
            .args(["rule", "del", "table", "100"])
            .status()
            .await;

        match status {
            Ok(st) if st.success() => {
                cleaned_rules += 1;
            }
            _ => break,
        }
    }
    if cleaned_rules > 0 {
        tracing::info!(
            "Self-Healing: Removed {} stale routing table 100 rules.",
            cleaned_rules
        );
    } else {
        tracing::debug!("Self-Healing: No stale routing table 100 rules found.");
    }

    // 3. Проверка org.gnome.system.proxy mode. Если демон не подключен, сброс режима на "none".
    reset_gnome_proxy_if_disconnected().await;

    Ok(())
}

/// Сбрасывает режим системного прокси GNOME в 'none', если отсутствует активное соединение.
pub async fn reset_gnome_proxy_if_disconnected() {
    // Проверяем доступность сессионной шины D-Bus
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        tracing::debug!("Self-Healing: DBUS_SESSION_BUS_ADDRESS is not set, skipping GNOME proxy reset in headless context");
        return;
    }

    let output = tokio::process::Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
        .await;

    if let Ok(out) = output {
        let mode_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !mode_str.contains("'none'") && !mode_str.is_empty() {
            tracing::warn!(
                "Self-Healing: GNOME proxy mode was set to {}, resetting to 'none'",
                mode_str
            );
            let set_res = tokio::process::Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy", "mode", "none"])
                .status()
                .await;
            if let Err(e) = set_res {
                tracing::error!("Self-Healing: Failed to reset GNOME proxy mode: {}", e);
            }
        }
    }
}
