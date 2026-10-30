/* protocol.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Описание протоколов и параметров IPC (Protocol & IPC Definitions)
//!
//! Модуль содержит:
//! - Структуры настроек VPN-протоколов ([`ProtocolSettings`], [`VlessSettings`], [`VmessSettings`] и др.)
//! - Строго типизированные структуры и перечисления для межпроцессного взаимодействия (IPC)
//!   между клиентами (GUI, TUI) и системным демоном VRXX:
//!   - [`CoreType`]: Поддерживаемые движки прокси (sing-box)
//!   - [`DaemonStatus`]: Статусы жизненного цикла демона
//!   - [`LogLevel`]: Уровни логирования
//!   - [`DomainStrategy`]: Стратегии разрешения доменных имен DNS
//!   - [`RoutingMode`]: Глобальные режимы маршрутизации
//!   - [`DaemonConfigOptions`]: Типизированные параметры конфигурации прокси
//!   - [`StartProxyRequest`]: Полезная нагрузка запроса запуска ядра
//!   - [`ConfigValidationError`]: Ошибки валидации параметров конфигурации

use crate::domain::key_parser::ParsedKey;
use crate::settings::{AppSettings, RoutingRule};
use serde::{Deserialize, Serialize};

/// Поддерживаемые движки прокси-ядра.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CoreType {
    #[default]
    SingBox,
}

impl CoreType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingBox => "sing-box",
        }
    }
}

impl std::fmt::Display for CoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for CoreType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "sing-box" | "singbox" => Ok(Self::SingBox),
            other => Err(format!("Unsupported core type: '{other}'")),
        }
    }
}

/// Операционный статус демона управления ядром прокси.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DaemonStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

impl DaemonStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Disconnecting => "disconnecting",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected => write!(f, "Connected"),
            Self::Disconnecting => write!(f, "Disconnecting"),
            Self::Error => write!(f, "Error"),
        }
    }
}

impl std::str::FromStr for DaemonStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disconnected" => Ok(Self::Disconnected),
            "connecting" => Ok(Self::Connecting),
            "connected" => Ok(Self::Connected),
            "disconnecting" => Ok(Self::Disconnecting),
            "error" => Ok(Self::Error),
            other => Err(format!("Unknown daemon status: '{other}'")),
        }
    }
}

/// Уровни детализации системных и консольных логов.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    #[serde(alias = "warn")]
    Warning,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Возвращает канонический идентификатор уровня для конфигурации sing-box.
    pub fn singbox_level(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for LogLevel {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" => Self::Error,
            "warning" | "warn" => Self::Warning,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }
}

/// Стратегия разрешения доменных имен DNS в sing-box.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DomainStrategy {
    #[default]
    AsIs,
    PreferIpv4,
    PreferIpv6,
    Ipv4Only,
    Ipv6Only,
}

impl DomainStrategy {
    pub fn singbox_strategy(&self) -> &'static str {
        match self {
            Self::AsIs => "",
            Self::PreferIpv4 => "prefer_ipv4",
            Self::PreferIpv6 => "prefer_ipv6",
            Self::Ipv4Only => "ipv4_only",
            Self::Ipv6Only => "ipv6_only",
        }
    }
}

impl From<&str> for DomainStrategy {
    fn from(s: &str) -> Self {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "preferipv4" => Self::PreferIpv4,
            "preferipv6" => Self::PreferIpv6,
            "ipv4only" => Self::Ipv4Only,
            "ipv6only" => Self::Ipv6Only,
            _ => Self::AsIs,
        }
    }
}

/// Глобальный режим маршрутизации трафика.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    #[default]
    Bypass,
    Proxy,
}

impl RoutingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bypass => "bypass",
            Self::Proxy => "proxy",
        }
    }
}

impl From<&str> for RoutingMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "proxy" => Self::Proxy,
            _ => Self::Bypass,
        }
    }
}

/// Ошибки валидации структуры опций конфигурации демона [`DaemonConfigOptions`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("SOCKS port cannot be 0")]
    ZeroSocksPort,
    #[error("HTTP port cannot be 0")]
    ZeroHttpPort,
    #[error("SOCKS port and HTTP port cannot be identical (socks: {socks}, http: {http})")]
    PortConflict { socks: u16, http: u16 },
    #[error("Multiplex concurrency ({concurrency}) must be between 1 and 128")]
    InvalidMuxConcurrency { concurrency: i32 },
    #[error("Routing rule '{name}' contains empty value")]
    EmptyRuleValue { name: String },
    #[error("Routing rule '{name}' contains invalid action '{action}'. Expected 'proxy', 'direct' or 'block'.")]
    InvalidRuleAction { name: String, action: String },
}

fn default_socks_port() -> u16 {
    10808
}
fn default_http_port() -> u16 {
    10809
}
fn default_mux_concurrency() -> i32 {
    8
}
fn default_true() -> bool {
    true
}

/// Строго типизированные параметры конфигурации, передаваемые клиентом в демон.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DaemonConfigOptions {
    #[serde(default)]
    pub tun_mode: bool,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default)]
    pub allow_lan: bool,
    #[serde(default)]
    pub enable_mux: bool,
    #[serde(default = "default_mux_concurrency")]
    pub mux_concurrency: i32,
    #[serde(default)]
    pub enable_fragment: bool,
    #[serde(default)]
    pub block_quic: bool,
    #[serde(default = "default_true")]
    pub enable_sniffing: bool,
    #[serde(default)]
    pub block_ads: bool,
    #[serde(default = "default_true")]
    pub bypass_lan: bool,
    #[serde(default)]
    pub disable_ipv6: bool,
    #[serde(default)]
    pub enable_fake_dns: bool,
    #[serde(default)]
    pub domain_strategy: DomainStrategy,
    #[serde(default)]
    pub routing_mode: RoutingMode,
    #[serde(default = "default_true")]
    pub enable_routing: bool,
    #[serde(default)]
    pub route_ru_sites: bool,
    #[serde(default)]
    pub route_ru_ips: bool,
    #[serde(default)]
    pub route_cn_sites: bool,
    #[serde(default)]
    pub route_cn_ips: bool,
    #[serde(default)]
    pub route_ir_sites: bool,
    #[serde(default)]
    pub route_ir_ips: bool,
    #[serde(default)]
    pub route_antifilter: bool,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRule>,
    #[serde(default)]
    pub log_level: LogLevel,
}

impl Default for DaemonConfigOptions {
    fn default() -> Self {
        Self {
            tun_mode: false,
            socks_port: default_socks_port(),
            http_port: default_http_port(),
            allow_lan: false,
            enable_mux: false,
            mux_concurrency: default_mux_concurrency(),
            enable_fragment: false,
            block_quic: false,
            enable_sniffing: true,
            block_ads: false,
            bypass_lan: true,
            disable_ipv6: false,
            enable_fake_dns: false,
            domain_strategy: DomainStrategy::AsIs,
            routing_mode: RoutingMode::Bypass,
            enable_routing: true,
            route_ru_sites: false,
            route_ru_ips: false,
            route_cn_sites: false,
            route_cn_ips: false,
            route_ir_sites: false,
            route_ir_ips: false,
            route_antifilter: false,
            routing_rules: Vec::new(),
            log_level: LogLevel::Info,
        }
    }
}

impl DaemonConfigOptions {
    /// Создает опции демона из пользовательских настроек UI с явным указанием TUN-режима.
    pub fn from_settings(settings: &AppSettings, tun_mode: bool) -> Self {
        let mut opts = Self::from(settings);
        opts.tun_mode = tun_mode;
        opts
    }

    /// Выполняет статическую валидацию диапазонов портов, параметров мультиплексирования и правил.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.socks_port == 0 {
            return Err(ConfigValidationError::ZeroSocksPort);
        }
        if self.http_port == 0 {
            return Err(ConfigValidationError::ZeroHttpPort);
        }
        if self.socks_port == self.http_port {
            return Err(ConfigValidationError::PortConflict {
                socks: self.socks_port,
                http: self.http_port,
            });
        }
        if self.enable_mux && !(1..=128).contains(&self.mux_concurrency) {
            return Err(ConfigValidationError::InvalidMuxConcurrency {
                concurrency: self.mux_concurrency,
            });
        }
        for rule in &self.routing_rules {
            if rule.value.trim().is_empty() {
                return Err(ConfigValidationError::EmptyRuleValue {
                    name: rule.name.clone(),
                });
            }
            let action_lower = rule.action.trim().to_lowercase();
            if !matches!(action_lower.as_str(), "proxy" | "direct" | "block") {
                return Err(ConfigValidationError::InvalidRuleAction {
                    name: rule.name.clone(),
                    action: rule.action.clone(),
                });
            }
        }
        Ok(())
    }
}

impl From<&AppSettings> for DaemonConfigOptions {
    fn from(s: &AppSettings) -> Self {
        // Поддержка обратной совместимости с legacy route_ru полем
        let (route_ru_sites, route_ru_ips) = if s.route_ru && !s.route_ru_sites && !s.route_ru_ips {
            (true, true)
        } else {
            (s.route_ru_sites, s.route_ru_ips)
        };

        Self {
            tun_mode: s.tun_mode,
            socks_port: s.socks_port,
            http_port: s.http_port,
            allow_lan: s.allow_lan,
            enable_mux: s.enable_mux,
            mux_concurrency: s.mux_concurrency,
            enable_fragment: s.enable_fragment,
            block_quic: s.block_quic,
            enable_sniffing: s.enable_sniffing,
            block_ads: s.block_ads,
            bypass_lan: s.bypass_lan,
            disable_ipv6: s.disable_ipv6,
            enable_fake_dns: s.enable_fake_dns,
            domain_strategy: DomainStrategy::from(s.domain_strategy.as_str()),
            routing_mode: RoutingMode::from(s.routing_mode.as_str()),
            enable_routing: s.enable_routing,
            route_ru_sites,
            route_ru_ips,
            route_cn_sites: s.route_cn_sites,
            route_cn_ips: s.route_cn_ips,
            route_ir_sites: s.route_ir_sites,
            route_ir_ips: s.route_ir_ips,
            route_antifilter: s.route_antifilter,
            routing_rules: s.routing_rules.clone(),
            log_level: LogLevel::from(s.log_level.as_str()),
        }
    }
}

/// Строго типизированная полезная нагрузка запроса запуска ядра VPN.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct StartProxyRequest {
    #[serde(default)]
    pub core_type: CoreType,
    pub profile: ParsedKey,
    #[serde(default)]
    pub options: DaemonConfigOptions,
}

/// Структурированный ответ ошибки от демона при сбое предстартовой валидации.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DaemonErrorResponse {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// =========================================================================
// Настройки протоколов (Protocol Settings)
// =========================================================================

/// Перечисление поддерживаемых VPN протоколов с их индивидуальными параметрами.
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

/// Настройки протокола VLESS.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VlessSettings {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub security: String,
    pub sni: String,
    pub fingerprint: String,
}

/// Настройки протокола VMess.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmessSettings {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub alter_id: u32,
    pub security: String,
}

/// Настройки протокола Trojan.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrojanSettings {
    pub password: String,
    pub address: String,
    pub port: u16,
}

/// Настройки протокола Shadowsocks.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SsSettings {
    pub method: String,
    pub password: String,
    pub address: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_opts: Option<String>,
}

/// Настройки протокола WireGuard.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WireguardSettings {
    pub private_key: String,
    pub address: Vec<String>,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preshared_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u16>,
}

/// Настройки протокола SOCKS5.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SocksSettings {
    pub address: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Настройки HTTP-прокси.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpSettings {
    pub address: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
}

impl ProtocolSettings {
    /// Возвращает каноническое строковое имя протокола.
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
