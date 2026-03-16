use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VpnKeyData {
    pub name: String,
    pub protocol: String,
    #[serde(skip, default)]
    pub is_active: bool,
    #[serde(skip, default)]
    pub traffic_down: String,
    #[serde(skip, default)]
    pub traffic_up: String,
    #[serde(skip, default)]
    pub time_connected: String,
    #[serde(skip, default)]
    pub ping: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub theme: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_core")]
    pub core: String,
    #[serde(default)]
    pub tun_mode: bool,
    
    // App Settings
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    #[serde(default = "default_connect_startup")]
    pub connect_on_startup: bool,
    #[serde(default = "default_notifications")]
    pub notifications: bool,
    #[serde(default = "default_streamer_mode")]
    pub streamer_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,

    // Proxy Settings
    #[serde(default = "default_system_proxy")]
    pub set_system_proxy: bool,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default)]
    pub allow_lan: bool,

    // Advanced Routing & Net Settings
    #[serde(default)]
    pub enable_sniffing: bool,
    #[serde(default = "default_domain_strategy")]
    pub domain_strategy: String,
    #[serde(default)]
    pub bypass_lan: bool,
    #[serde(default)]
    pub enable_fake_dns: bool,
    #[serde(default)]
    pub enable_local_dns: bool,
    #[serde(default)]
    pub enable_mux: bool,
    #[serde(default = "default_mux_concurrency")]
    pub mux_concurrency: i32,
    #[serde(default)]
    pub enable_fragment: bool,

    pub keys: Vec<VpnKeyData>,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub enable_routing: bool,
    #[serde(default)]
    pub routing_mode: String, // "bypass" or "proxy"
}

fn default_language() -> String { "system".to_string() }
fn default_core() -> String { "xray".to_string() }
fn default_system_proxy() -> bool { true }
fn default_socks_port() -> u16 { 10808 }
fn default_http_port() -> u16 { 10809 }
fn default_autostart() -> bool { true }
fn default_connect_startup() -> bool { false }
fn default_notifications() -> bool { true }
fn default_streamer_mode() -> bool { false }
fn default_log_level() -> String { "info".to_string() }
fn default_domain_strategy() -> String { "AsIs".to_string() }
fn default_mux_concurrency() -> i32 { 8 }

impl AppSettings {
    pub fn new() -> Self {
        Self {
            theme: "default".to_string(),
            language: default_language(),
            core: default_core(),
            tun_mode: false,
            autostart: default_autostart(),
            connect_on_startup: default_connect_startup(),
            notifications: default_notifications(),
            streamer_mode: default_streamer_mode(),
            log_level: default_log_level(),
            set_system_proxy: default_system_proxy(),
            socks_port: default_socks_port(),
            http_port: default_http_port(),
            allow_lan: false,
            enable_sniffing: false,
            domain_strategy: default_domain_strategy(),
            bypass_lan: false,
            enable_fake_dns: false,
            enable_local_dns: false,
            enable_mux: false,
            mux_concurrency: default_mux_concurrency(),
            enable_fragment: false,
            keys: vec![],
            whitelist: vec![],
            enable_routing: false,
            routing_mode: "bypass".to_string(),
        }
    }
}

pub struct SettingsManager {
    config_path: PathBuf,
}

impl SettingsManager {
    pub fn new() -> Self {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("vrxx");
        fs::create_dir_all(&path).ok();
        path.push("settings.json");
        Self { config_path: path }
    }

    pub fn load(&self) -> AppSettings {
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            if let Ok(settings) = serde_json::from_str(&content) {
                return settings;
            }
        }
        AppSettings::new()
    }

    pub fn save(&self, settings: &AppSettings) {
        if let Ok(content) = serde_json::to_string_pretty(settings) {
            fs::write(&self.config_path, content).ok();
        }
    }

    // Backwards compatibility wrappers
    pub fn load_keys(&self) -> Vec<VpnKeyData> {
        self.load().keys
    }

    pub fn save_keys(&self, keys: &[VpnKeyData]) {
        let mut current = self.load();
        current.keys = keys.to_vec();
        self.save(&current);
    }
}
