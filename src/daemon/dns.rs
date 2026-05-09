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

pub struct DnsManager {
    connection: Connection,
}

impl DnsManager {
    pub async fn new() -> Result<Self> {
        let connection = zbus::Connection::system().await?;
        Ok(Self { connection })
    }

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

    pub async fn reset_dns(&self, iface_index: i32) -> Result<()> {
        let proxy = Resolve1ManagerProxy::new(&self.connection).await?;
        proxy.set_link_dns(iface_index, &[]).await?;
        proxy.set_link_domains(iface_index, &[]).await?;
        Ok(())
    }
}
