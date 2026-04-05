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
        // 1. Create TUN
        let tun = DeviceBuilder::new()
            .name("vrxx-tun")
            .build_async()
            .context("Failed to create TUN device")?;
        self.tun = Some(tun);

        // 2. Get interface index
        let mut links = self.handle.link().get().match_name("vrxx-tun".to_string()).execute();
        let link = links.try_next().await?.context("Interface vrxx-tun not found")?;
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
        // We will try using command for ip rule since rtnetlink rule API is less stable/documented for fwmark mask
        let status = tokio::process::Command::new("ip")
            .args(&["rule", "add", "not", "fwmark", "0x255", "table", "100"])
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
                .args(&["rule", "del", "not", "fwmark", "0x255", "table", "100"])
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
