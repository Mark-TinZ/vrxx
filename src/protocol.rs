use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum ProtocolSettings {
    Vless(VlessSettings),
    Vmess(VmessSettings),
    Trojan(TrojanSettings),
    Shadowsocks(SsSettings),
    Wireguard(WireguardSettings),
    Socks(SocksSettings),
    Http(HttpSettings),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VlessSettings {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub security: String,
    pub sni: String,
    pub fingerprint: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmessSettings {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub alter_id: u32,
    pub security: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrojanSettings {
    pub password: String,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SsSettings {
    pub method: String,
    pub password: String,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WireguardSettings {
    pub private_key: String,
    pub address: Vec<String>,
    pub endpoint: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SocksSettings {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpSettings {
    pub address: String,
    pub port: u16,
}

impl ProtocolSettings {
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        match self {
            Self::Vless(_) => "VLESS",
            Self::Vmess(_) => "VMess",
            Self::Trojan(_) => "Trojan",
            Self::Shadowsocks(_) => "Shadowsocks",
            Self::Wireguard(_) => "WireGuard",
            Self::Socks(_) => "SOCKS",
            Self::Http(_) => "HTTP",
        }
    }
}
