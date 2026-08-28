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

//! # Описание протоколов и их параметров (Protocol Definitions)
//!
//! Модуль содержит структуры и перечисления для строгой типизации настроек VPN-протоколов:
//! - VLESS (Reality, TLS, Vision flow)
//! - VMess (AlterID, AEAD, WebSocket)
//! - Trojan (TLS)
//! - Shadowsocks (AEAD шифры, 2022-blake3)
//! - WireGuard (Private key, Peer public key, Allowed IPs)
//! - Локальные SOCKS5 / HTTP прокси

use serde::{Deserialize, Serialize};

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
}

/// Настройки протокола WireGuard.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WireguardSettings {
    pub private_key: String,
    pub address: Vec<String>,
    pub endpoint: String,
}

/// Настройки протокола SOCKS5.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SocksSettings {
    pub address: String,
    pub port: u16,
}

/// Настройки HTTP-прокси.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpSettings {
    pub address: String,
    pub port: u16,
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
