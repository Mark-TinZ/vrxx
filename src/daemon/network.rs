use anyhow::{Context, Result};
use futures_util::stream::TryStreamExt;
use rtnetlink::{new_connection, Handle, LinkUnspec, RouteMessageBuilder};
use std::net::{IpAddr, Ipv4Addr};
use tun_rs::{AsyncDevice, DeviceBuilder};

pub struct TunManager {
    handle: Handle,
    connection_task: tokio::task::JoinHandle<()>,
    tun: Option<AsyncDevice>,
    pub if_index: u32,
}

impl TunManager {
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

    pub async fn setup(&mut self) -> Result<()> {
        // --- Раздел: Настройка TUN интерфейса ---
        // REVIEW: Мы объединяем все операции в одну последовательность, чтобы минимизировать запросы прав

        // 1. Create TUN
        let tun = DeviceBuilder::new()
            .name("vrxx-tun")
            .build_async()
            .context("Failed to create TUN device")?;
        self.tun = Some(tun);

        // 2. Get interface index
        let mut links = self
            .handle
            .link()
            .get()
            .match_name("vrxx-tun".to_string())
            .execute();
        let link = links
            .try_next()
            .await?
            .context("Interface vrxx-tun not found")?;
        self.if_index = link.header.index;

        // 3. Set IPv4 172.19.0.1/30
        let addr = Ipv4Addr::new(172, 19, 0, 1);
        let prefix = 30;
        self.handle
            .address()
            .add(self.if_index, IpAddr::V4(addr), prefix)
            .execute()
            .await
            .context("Failed to set IP address")?;

        // 4. Set interface UP
        self.handle
            .link()
            .set(LinkUnspec::new_with_index(self.if_index).up().build())
            .execute()
            .await
            .context("Failed to bring interface up")?;

        // 5. Create a new routing table 100 and add default route through vrxx-tun
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
            .context("Failed to add default route to table 100")?;

        // 6. Add ip rule to direct traffic to table 100 (except marked)
        // NOTE: Используем одну команду для настройки правил, соблюдая паттерн минимизации привилегированных вызовов
        let status = tokio::process::Command::new("ip")
            .args(["rule", "add", "not", "fwmark", "0x255", "table", "100"])
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("Failed to add ip rule");
        }

        Ok(())
    }

    pub async fn teardown(&mut self) -> Result<()> {
        if self.if_index != 0 {
            // Delete ip rule
            let _ = tokio::process::Command::new("ip")
                .args(["rule", "del", "not", "fwmark", "0x255", "table", "100"])
                .status()
                .await;

            // Delete routing table entries
            let route_msg = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(Ipv4Addr::new(0, 0, 0, 0), 0)
                .table_id(100)
                .build();
            let _ = self.handle.route().del(route_msg).execute().await;
        }

        self.tun = None;
        self.connection_task.abort();
        Ok(())
    }
}

/// Функция самовосстановления сети (Self-Healing).
/// Выполняется при старте демона `vrxx-daemon`:
/// 1. Удаление подвисших/сиротских интерфейсов `vrxx-tun`.
/// 2. Очистка подвисших правил таблицы маршрутизации (`ip rule del table 100`).
/// 3. Сброс `org.gnome.system.proxy mode` на `"none"`, если прокси не подключен.
pub async fn self_heal() -> Result<()> {
    tracing::info!("Running network self-healing checks on daemon startup...");

    // 1. Проверка наличия и удаление сиротских интерфейсов vrxx-tun
    let status_del_tun = tokio::process::Command::new("ip")
        .args(["link", "del", "vrxx-tun"])
        .status()
        .await;

    match status_del_tun {
        Ok(st) if st.success() => {
            tracing::info!("Self-Healing: Removed orphan interface vrxx-tun.");
        }
        _ => {
            tracing::debug!("Self-Healing: No orphan vrxx-tun interface found.");
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
            "Self-Healing: Removed {} dangling table 100 routing rule(s).",
            cleaned_rules
        );
    } else {
        tracing::debug!("Self-Healing: No dangling table 100 routing rules found.");
    }

    // 3. Проверка org.gnome.system.proxy mode. Если демон не подключен, сброс режима на "none".
    reset_gnome_proxy_if_disconnected().await;

    Ok(())
}

pub async fn reset_gnome_proxy_if_disconnected() {
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
