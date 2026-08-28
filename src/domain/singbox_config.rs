/* singbox_config.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Генератор конфигураций сетевого ядра sing-box (Sing-Box 1.13.18+ Config Builder)
//!
//! Модуль отвечает за:
//! - Формирование валидного JSON-документа конфигурации `sing-box` современной спецификации (1.13.18+)
//! - Использование актуального формата DNS (`type: "https"`, `type: "local"`, `type: "fakeip"`)
//! - Безопасную маршрутизацию доменов через `route.default_domain_resolver = "local-dns"`
//! - Перехват DNS-запросов через `action: "hijack-dns"` в `route.rules`
//! - Генерацию изолированных микро-конфигураций (L7 Probe) для проверки работоспособности ключей

use crate::domain::key_parser::ParsedKey;
use crate::settings::AppSettings;
use serde_json::json;
use std::str::FromStr;

/// Извлекает и формирует исходящий узел (Outbound) и endpoints (для WireGuard) для заданного ключа.
pub fn build_proxy_outbound_and_endpoints(
    parsed_key: &ParsedKey,
    enable_mux: bool,
    mux_concurrency: i32,
) -> (serde_json::Value, Vec<serde_json::Value>) {
    let qp = &parsed_key.query_params;
    let proto_lower = parsed_key.protocol.to_lowercase();
    let security = qp.get("security").map(|s| s.as_str()).unwrap_or("none");
    let net = qp
        .get("type")
        .or_else(|| qp.get("net"))
        .map(|s| s.as_str())
        .unwrap_or("tcp");

    let mut endpoints = vec![];

    let mut proxy_outbound = match proto_lower.as_str() {
        "vless" => {
            let mut outbound = json!({
                "type": "vless",
                "tag": "proxy",
                "server": parsed_key.host,
                "server_port": parsed_key.port,
                "uuid": parsed_key.uuid,
            });
            let flow = qp.get("flow").map(|s| s.as_str()).unwrap_or("");
            if !flow.is_empty() {
                outbound["flow"] = json!(flow);
            }
            outbound
        }
        "vmess" => json!({
            "type": "vmess",
            "tag": "proxy",
            "server": parsed_key.host,
            "server_port": parsed_key.port,
            "uuid": parsed_key.uuid,
            "alter_id": 0,
            "security": "auto",
            "packet_encoding": "xudp"
        }),
        "trojan" => json!({
            "type": "trojan",
            "tag": "proxy",
            "server": parsed_key.host,
            "server_port": parsed_key.port,
            "password": parsed_key.uuid,
        }),
        "shadowsocks" | "ss" => {
            let method = qp
                .get("method")
                .cloned()
                .unwrap_or_else(|| "2022-blake3-aes-128-gcm".to_string());
            json!({
                "type": "shadowsocks",
                "tag": "proxy",
                "server": parsed_key.host,
                "server_port": parsed_key.port,
                "method": method,
                "password": parsed_key.uuid,
            })
        }
        "hysteria2" | "hy2" => {
            let mut outbound = json!({
                "type": "hysteria2",
                "tag": "proxy",
                "server": parsed_key.host,
                "server_port": parsed_key.port,
                "password": parsed_key.uuid,
            });
            if let Some(up) = qp
                .get("up")
                .or_else(|| qp.get("up_mbps"))
                .and_then(|v| v.parse::<u32>().ok())
            {
                outbound["up_mbps"] = json!(up);
            }
            if let Some(down) = qp
                .get("down")
                .or_else(|| qp.get("down_mbps"))
                .and_then(|v| v.parse::<u32>().ok())
            {
                outbound["down_mbps"] = json!(down);
            }
            if let Some(obfs_type) = qp.get("obfs") {
                outbound["obfs"] = json!({
                    "type": obfs_type,
                    "password": qp.get("obfs-password").unwrap_or(&"".to_string())
                });
            }
            outbound
        }
        "tuic" => {
            let (user_id, pass) = if let Some((u, p)) = parsed_key.uuid.split_once(':') {
                (u.to_string(), p.to_string())
            } else {
                (
                    parsed_key.uuid.clone(),
                    qp.get("password").cloned().unwrap_or_default(),
                )
            };
            json!({
                "type": "tuic",
                "tag": "proxy",
                "server": parsed_key.host,
                "server_port": parsed_key.port,
                "uuid": user_id,
                "password": pass,
                "congestion_control": qp.get("congestion_control").or_else(|| qp.get("cc")).map(|s| s.as_str()).unwrap_or("bbr"),
                "udp_relay_mode": qp.get("udp_relay_mode").map(|s| s.as_str()).unwrap_or("native"),
            })
        }
        "wireguard" | "wg" => {
            let local_ip = qp
                .get("ip")
                .cloned()
                .unwrap_or_else(|| "10.0.0.2/32".to_string());
            let peer_pub = qp.get("public_key").cloned().unwrap_or_default();

            endpoints.push(json!({
                "type": "wireguard",
                "tag": "proxy",
                "address": [local_ip],
                "private_key": parsed_key.uuid,
                "peers": [{
                    "address": parsed_key.host,
                    "port": parsed_key.port,
                    "public_key": peer_pub,
                    "allowed_ips": ["0.0.0.0/0", "::/0"]
                }]
            }));
            json!({
                "type": "direct",
                "tag": "proxy-dummy"
            })
        }
        _ => json!({
            "type": "direct",
            "tag": "proxy"
        }),
    };

    // Настройка TLS и Reality
    let is_tls_proto = proto_lower == "hysteria2" || proto_lower == "hy2" || proto_lower == "tuic";
    if security == "tls" || security == "reality" || is_tls_proto {
        let mut tls = json!({
            "enabled": true,
            "server_name": qp.get("sni").or_else(|| qp.get("peer")).unwrap_or(&parsed_key.host),
            "insecure": qp.get("allowInsecure").map(|v| v == "1" || v == "true").unwrap_or(false),
        });

        // uTLS Fingerprint: если задан в ключе (fp=...), используем его; если нет — форсируем "chrome"
        let fp = qp
            .get("fp")
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("chrome");
        tls["utls"] = json!({
            "enabled": true,
            "fingerprint": fp
        });

        if security == "reality" {
            let mut reality = json!({
                "enabled": true,
                "public_key": qp.get("pbk").unwrap_or(&"".to_string()),
            });
            if let Some(sid) = qp.get("sid") {
                reality["short_id"] = json!(sid);
            }
            tls["reality"] = reality;
        }

        proxy_outbound["tls"] = tls;
    }

    // Настройка протоколов транспорта (WebSocket, gRPC, HTTPUpgrade)
    match net {
        "ws" => {
            let mut ws = json!({
                "type": "ws",
                "path": qp.get("path").unwrap_or(&"/".to_string()),
            });
            if let Some(host) = qp.get("host") {
                ws["headers"] = json!({ "Host": host });
            }
            proxy_outbound["transport"] = ws;
        }
        "grpc" => {
            proxy_outbound["transport"] = json!({
                "type": "grpc",
                "service_name": qp.get("serviceName").unwrap_or(&"".to_string()),
            });
        }
        "httpupgrade" => {
            let mut hu = json!({
                "type": "httpupgrade",
                "path": qp.get("path").unwrap_or(&"/".to_string()),
            });
            if let Some(host) = qp.get("host") {
                hu["host"] = json!(host);
            }
            proxy_outbound["transport"] = hu;
        }
        _ => {}
    }

    // Мультиплексирование (Mux)
    if enable_mux && proto_lower != "wireguard" && proto_lower != "wg" {
        proxy_outbound["multiplex"] = json!({
            "enabled": true,
            "protocol": "h2mux",
            "max_connections": mux_concurrency,
            "min_streams": 4,
            "padding": true
        });
    }

    (proxy_outbound, endpoints)
}

/// Генерирует полную JSON-конфигурацию для sing-box 1.13.18+ на основе ключа и настроек приложения.
pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    let socks_port = settings.socks_port;
    let http_port = if settings.http_port == settings.socks_port {
        settings.http_port + 1
    } else {
        settings.http_port
    };

    // =========================================================================
    // 1. ВХОДЯЩИЕ ПОДКЛЮЧЕНИЯ (INBOUNDS)
    // =========================================================================
    let mut inbounds = vec![
        json!({
            "type": "socks",
            "tag": "socks-in",
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": socks_port,
        }),
        json!({
            "type": "http",
            "tag": "http-in",
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": http_port
        }),
    ];

    if settings.tun_mode {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "vrxx-tun",
            "auto_route": true,
            "strict_route": true,
            "stack": "gvisor",
            "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"]
        }));
    }

    // =========================================================================
    // 2. ИСХОДЯЩИЕ ПОДКЛЮЧЕНИЯ И ЭНДПОИНТЫ (OUTBOUNDS & ENDPOINTS)
    // =========================================================================
    let proto_lower = parsed_key.protocol.to_lowercase();
    let (proxy_outbound, endpoints) = build_proxy_outbound_and_endpoints(
        parsed_key,
        settings.enable_mux,
        settings.mux_concurrency,
    );

    let mut outbounds = vec![
        proxy_outbound,
        json!({ "type": "direct", "tag": "direct" }),
        json!({ "type": "block", "tag": "block" }),
    ];

    if proto_lower == "wireguard" || proto_lower == "wg" {
        outbounds.retain(|o| o.get("tag").and_then(|t| t.as_str()) != Some("proxy-dummy"));
    }

    // =========================================================================
    // 3. ПРАВИЛА МАРШРУТИЗАЦИИ (ROUTING RULES)
    // =========================================================================
    let mut rules = vec![];
    let mut rule_sets = vec![];

    // Блокировка QUIC (UDP 443) для принудительного использования стабильного TCP/TLS
    if settings.block_quic {
        rules.push(json!({
            "action": "reject",
            "network": "udp",
            "port": [443]
        }));
    }

    // Перехват DNS через action: hijack-dns
    rules.push(json!({
        "action": "hijack-dns",
        "port": [53]
    }));

    // Сниффинг протоколов и доменов через action: sniff (sing-box 1.13.18+)
    if settings.enable_sniffing {
        rules.push(json!({
            "action": "sniff"
        }));
    }

    let geodata_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("vrxx")
        .join("geodata");

    let mut add_rule_set = |tag: &str, url: &str, file_name: &str| {
        let local_path = geodata_dir.join(file_name);
        if local_path.exists() {
            rule_sets.push(json!({
                "tag": tag,
                "type": "local",
                "format": "binary",
                "path": local_path.to_string_lossy()
            }));
        } else {
            rule_sets.push(json!({
                "tag": tag,
                "type": "remote",
                "format": "binary",
                "url": url,
                "download_detour": "direct"
            }));
        }
    };

    // Блокировка рекламы
    if settings.block_ads {
        add_rule_set(
            "geosite-ads",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ads-all.srs",
            "geosite-ads.srs",
        );
        rules.push(json!({
            "rule_set": ["geosite-ads"],
            "outbound": "block"
        }));
    }

    // Приоритетная защита Google и критических глобальных сервисов (исключение утечек через GGC)
    // Должно выполняться СТРОГО ДО региональных правил РФ
    add_rule_set(
        "geosite-google",
        "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/google.srs",
        "geosite-google.srs",
    );
    rules.push(json!({
        "rule_set": ["geosite-google"],
        "outbound": "proxy"
    }));

    // Дополнительный fallback по доменным суффиксам Google / YouTube / Telegram
    rules.push(json!({
        "domain_suffix": [
            "google.com",
            "google.ru",
            "gstatic.com",
            "googleapis.com",
            "googlevideo.com",
            "googleusercontent.com",
            "googleadservices.com",
            "googletagmanager.com",
            "youtube.com",
            "ytimg.com",
            "ggpht.com",
            "gvt1.com",
            "1e100.net",
            "t.me",
            "telegram.org"
        ],
        "outbound": "proxy"
    }));

    // Региональные правила маршрутизации (RU, CN, IR, Antifilter)
    if settings.enable_routing {
        // Сайты РФ (category-ru)
        if settings.route_ru_sites {
            add_rule_set(
                "geosite-ru",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ru.srs",
                "geosite-ru.srs",
            );
            rules.push(json!({
                "rule_set": ["geosite-ru"],
                "outbound": "direct"
            }));
        }

        // IP РФ (ru)
        if settings.route_ru_ips {
            add_rule_set(
                "geoip-ru",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ru.srs",
                "geoip-ru.srs",
            );
            rules.push(json!({
                "rule_set": ["geoip-ru"],
                "outbound": "direct"
            }));
        }

        // Сайты Китая (cn)
        if settings.route_cn_sites {
            add_rule_set(
                "geosite-cn",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/cn.srs",
                "geosite-cn.srs",
            );
            rules.push(json!({
                "rule_set": ["geosite-cn"],
                "outbound": "direct"
            }));
        }

        // IP Китая (cn)
        if settings.route_cn_ips {
            add_rule_set(
                "geoip-cn",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/cn.srs",
                "geoip-cn.srs",
            );
            rules.push(json!({
                "rule_set": ["geoip-cn"],
                "outbound": "direct"
            }));
        }

        // Сайты Ирана (category-ir)
        if settings.route_ir_sites {
            add_rule_set(
                "geosite-ir",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ir.srs",
                "geosite-ir.srs",
            );
            rules.push(json!({
                "rule_set": ["geosite-ir"],
                "outbound": "direct"
            }));
        }

        // IP Ирана (ir)
        if settings.route_ir_ips {
            add_rule_set(
                "geoip-ir",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ir.srs",
                "geoip-ir.srs",
            );
            rules.push(json!({
                "rule_set": ["geoip-ir"],
                "outbound": "direct"
            }));
        }

        // Antifilter (через proxy)
        if settings.route_antifilter {
            add_rule_set(
                "geosite-antifilter",
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/antifilter.srs",
                "geosite-antifilter.srs",
            );
            rules.push(json!({
                "rule_set": ["geosite-antifilter"],
                "outbound": "proxy"
            }));
        }
    }

    // Пользовательские правила маршрутизации
    for rule in &settings.routing_rules {
        match rule.type_.as_str() {
            "domain" | "Domain" | "domain_suffix" | "DomainSuffix" => {
                rules.push(json!({
                    "domain_suffix": [rule.value],
                    "outbound": rule.action
                }));
            }
            "domain_keyword" | "DomainKeyword" => {
                rules.push(json!({
                    "domain_keyword": [rule.value],
                    "outbound": rule.action
                }));
            }
            "ip_cidr" | "IP" => {
                rules.push(json!({
                    "ip_cidr": [rule.value],
                    "outbound": rule.action
                }));
            }
            "geoip" | "GeoIP" => {
                let tag = format!("geoip-{}", rule.value.to_lowercase());
                let file_name = format!("{tag}.srs");
                add_rule_set(&tag, &rule.value, &file_name);
                rules.push(json!({
                    "rule_set": [tag],
                    "outbound": rule.action
                }));
            }
            "geosite" | "GeoSite" => {
                let tag = format!("geosite-{}", rule.value.to_lowercase());
                let file_name = format!("{tag}.srs");
                add_rule_set(&tag, &rule.value, &file_name);
                rules.push(json!({
                    "rule_set": [tag],
                    "outbound": rule.action
                }));
            }
            "ruleset" | "RuleSet" => {
                let tag = if rule.name.is_empty() {
                    "custom-ruleset".to_string()
                } else {
                    format!(
                        "ruleset-{}",
                        rule.name
                            .to_lowercase()
                            .replace(|c: char| !c.is_alphanumeric(), "-")
                    )
                };
                let file_name = format!("{tag}.srs");
                add_rule_set(&tag, &rule.value, &file_name);
                rules.push(json!({
                    "rule_set": [tag],
                    "outbound": rule.action
                }));
            }
            _ => {}
        }
    }

    // Локальная сеть
    if settings.bypass_lan {
        add_rule_set(
            "geoip-private",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/private.srs",
            "geoip-private.srs",
        );
        rules.push(json!({
            "rule_set": ["geoip-private"],
            "outbound": "direct"
        }));
    }

    // Блокировка IPv6 на уровне маршрутизации
    if settings.disable_ipv6 {
        rules.push(json!({
            "ip_version": 6,
            "action": "reject"
        }));
    }

    // Обход самого адреса прокси-сервера (direct)
    if !parsed_key.host.is_empty() {
        if std::net::IpAddr::from_str(&parsed_key.host).is_ok() {
            rules.push(json!({
                "ip_cidr": [format!("{}/32", parsed_key.host)],
                "outbound": "direct"
            }));
        } else {
            rules.push(json!({
                "domain": [parsed_key.host],
                "outbound": "direct"
            }));
        }
    }

    let default_outbound = if settings.routing_mode == "bypass" {
        "proxy"
    } else {
        "direct"
    };

    let route_obj = json!({
        "rules": rules,
        "rule_set": rule_sets,
        "default_domain_resolver": "local-dns",
        "final": default_outbound,
        "auto_detect_interface": true
    });

    // =========================================================================
    // 4. КОНФИГУРАЦИЯ DNS (DNS SUB-CONFIG)
    // =========================================================================
    let mut dns_rules = vec![];

    // Прямой резолвинг хоста прокси через local-dns
    if !parsed_key.host.is_empty() && std::net::IpAddr::from_str(&parsed_key.host).is_err() {
        dns_rules.push(json!({
            "domain": [parsed_key.host],
            "server": "local-dns"
        }));
    }

    if settings.disable_ipv6 {
        dns_rules.push(json!({
            "query_type": ["AAAA"],
            "action": "reject"
        }));
    }

    // Принудительный резолвинг Google через remote-dns (или fake-dns)
    dns_rules.push(json!({
        "rule_set": ["geosite-google"],
        "server": if settings.enable_fake_dns { "fake-dns" } else { "remote-dns" }
    }));
    dns_rules.push(json!({
        "domain_suffix": [
            "google.com",
            "google.ru",
            "gstatic.com",
            "googleapis.com",
            "googlevideo.com",
            "youtube.com",
            "1e100.net"
        ],
        "server": if settings.enable_fake_dns { "fake-dns" } else { "remote-dns" }
    }));

    // Сайты РФ резолвим через local-dns
    if settings.route_ru_sites {
        dns_rules.push(json!({
            "rule_set": ["geosite-ru"],
            "server": "local-dns"
        }));
    }

    let mut dns_servers = vec![
        json!({
            "tag": "remote-dns",
            "type": "https",
            "server": "1.1.1.1",
            "detour": "proxy"
        }),
        json!({
            "tag": "local-dns",
            "type": "local"
        }),
    ];

    if settings.enable_fake_dns {
        dns_servers.push(json!({
            "tag": "fake-dns",
            "type": "fakeip",
            "inet4_range": "198.18.0.0/15",
            "inet6_range": "fc00::/18"
        }));
        dns_rules.push(json!({
            "inbound": ["tun-in", "socks-in", "http-in"],
            "server": "fake-dns"
        }));
    }

    let mut dns_obj = json!({
        "servers": dns_servers,
        "rules": dns_rules,
        "final": "remote-dns"
    });

    let strategy_lower = match settings
        .domain_strategy
        .to_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "preferipv4" | "prefer_ipv4" => "prefer_ipv4",
        "preferipv6" | "prefer_ipv6" => "prefer_ipv6",
        "ipv4only" | "ipv4_only" => "ipv4_only",
        "ipv6only" | "ipv6_only" => "ipv6_only",
        _ => "",
    };

    if !strategy_lower.is_empty() {
        dns_obj["strategy"] = json!(strategy_lower);
    }

    // =========================================================================
    // 5. ИТОГОВАЯ СБОРКА КОРНЕВОГО JSON ДОКУМЕНТА
    // =========================================================================
    let mut root = json!({
        "log": {
            "level": settings.log_level,
            "timestamp": true
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route_obj,
        "dns": dns_obj,
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:9090",
                "secret": ""
            }
        }
    });

    if !endpoints.is_empty() {
        root["endpoints"] = json!(endpoints);
    }

    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
}

/// Генерирует ультра-легковесный JSON-конфиг для изолированной L7 проверки (Sandbox Probe) отдельного ключа.
pub fn build_singbox_probe_config(parsed_key: &ParsedKey, probe_port: u16) -> String {
    let proto_lower = parsed_key.protocol.to_lowercase();
    let (proxy_outbound, endpoints) = build_proxy_outbound_and_endpoints(parsed_key, false, 8);

    let mut outbounds = vec![
        proxy_outbound,
        json!({ "type": "direct", "tag": "direct" }),
        json!({ "type": "block", "tag": "block" }),
    ];

    if proto_lower == "wireguard" || proto_lower == "wg" {
        outbounds.retain(|o| o.get("tag").and_then(|t| t.as_str()) != Some("proxy-dummy"));
    }

    let mut dns_rules = vec![];
    if !parsed_key.host.is_empty() && std::net::IpAddr::from_str(&parsed_key.host).is_err() {
        dns_rules.push(json!({
            "domain": [parsed_key.host],
            "server": "local-dns"
        }));
    }

    let mut route_rules = vec![json!({
        "action": "hijack-dns",
        "port": [53]
    })];

    if !parsed_key.host.is_empty() {
        if std::net::IpAddr::from_str(&parsed_key.host).is_ok() {
            route_rules.push(json!({
                "ip_cidr": [format!("{}/32", parsed_key.host)],
                "outbound": "direct"
            }));
        } else {
            route_rules.push(json!({
                "domain": [parsed_key.host],
                "outbound": "direct"
            }));
        }
    }

    let mut root = json!({
        "log": {
            "level": "warn",
            "timestamp": false
        },
        "inbounds": [
            {
                "type": "socks",
                "tag": "socks-probe",
                "listen": "127.0.0.1",
                "listen_port": probe_port
            }
        ],
        "outbounds": outbounds,
        "route": {
            "rules": route_rules,
            "default_domain_resolver": "local-dns",
            "final": "proxy",
            "auto_detect_interface": true
        },
        "dns": {
            "servers": [
                {
                    "tag": "remote-dns",
                    "type": "https",
                    "server": "1.1.1.1",
                    "detour": "proxy"
                },
                {
                    "tag": "local-dns",
                    "type": "local"
                }
            ],
            "rules": dns_rules,
            "final": "remote-dns"
        }
    });

    if !endpoints.is_empty() {
        root["endpoints"] = json!(endpoints);
    }

    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::key_parser::parse_vpn_key;

    #[test]
    fn test_build_config_vless_modern() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=google.com&flow=xtls-rprx-vision#TestVLESS")
            .expect("Валидный ключ");
        let settings = AppSettings::new();

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value =
            serde_json::from_str(&json_str).expect("Должен быть валидный JSON");

        // Проверка DNS
        let dns_servers = val["dns"]["servers"].as_array().expect("Массив серверов");
        let local_dns = dns_servers
            .iter()
            .find(|s| s["tag"] == "local-dns")
            .unwrap();
        assert_eq!(local_dns["type"], "local");
        assert!(local_dns.get("detour").is_none());

        let remote_dns = dns_servers
            .iter()
            .find(|s| s["tag"] == "remote-dns")
            .unwrap();
        assert_eq!(remote_dns["type"], "https");
        assert_eq!(remote_dns["server"], "1.1.1.1");
        assert_eq!(remote_dns["detour"], "proxy");

        // Проверка Route
        assert_eq!(val["route"]["default_domain_resolver"], "local-dns");
    }

    #[test]
    fn test_build_config_wireguard_endpoints() {
        let key =
            parse_vpn_key("wg://privkey@1.1.1.1:51820?public_key=pubkey&ip=10.0.0.2/32#TestWG")
                .expect("Валидный ключ");
        let settings = AppSettings::new();

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&json_str).expect("Валидный JSON");

        assert!(val.get("endpoints").is_some());
        let endpoints = val["endpoints"].as_array().unwrap();
        assert_eq!(endpoints[0]["type"], "wireguard");
        assert_eq!(endpoints[0]["tag"], "proxy");
    }

    #[test]
    fn test_build_config_fakedns() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443#TestFakeDns").expect("Валидный ключ");
        let mut settings = AppSettings::new();
        settings.enable_fake_dns = true;

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(val["dns"].get("fakeip").is_none());

        let servers = val["dns"]["servers"].as_array().unwrap();
        let fake_server = servers.iter().find(|s| s["tag"] == "fake-dns").unwrap();
        assert_eq!(fake_server["type"], "fakeip");
        assert_eq!(fake_server["inet4_range"], "198.18.0.0/15");
    }

    #[test]
    fn test_build_config_sniffing_modern() {
        let key = parse_vpn_key("vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@1.1.1.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=google.com&flow=xtls-rprx-vision#VLESS").unwrap();
        let mut settings = AppSettings::new();
        settings.enable_sniffing = true;

        let config_json = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&config_json).unwrap();

        // Проверяем, что route.sniff объект отсутствует
        assert!(val["route"].get("sniff").is_none());

        // Проверяем, что в route.rules есть правило action: sniff
        let rules = val["route"]["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|r| r.get("action").and_then(|a| a.as_str()) == Some("sniff")));
    }

    #[test]
    fn test_build_config_quic_blocked() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443#TestQuic").expect("Валидный ключ");
        let mut settings = AppSettings::new();
        settings.block_quic = true;

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let rules = val["route"]["rules"].as_array().unwrap();
        let quic_rule = rules.iter().find(|r| {
            r.get("network").and_then(|n| n.as_str()) == Some("udp")
                && r.get("port")
                    .and_then(|p| p.as_array())
                    .map(|arr| arr.iter().any(|v| v.as_i64() == Some(443)))
                    .unwrap_or(false)
        });
        assert!(
            quic_rule.is_some(),
            "Правило блокировки QUIC должно присутствовать в route.rules"
        );
    }

    #[test]
    fn test_build_config_utls_fallback_chrome() {
        // Ключ без Reality и без явного &fp=... должен получить fp="chrome" по умолчанию
        let key = parse_vpn_key("trojan://mypassword@1.1.1.1:443?security=tls#TestTrojanTls")
            .expect("Валидный ключ");
        let settings = AppSettings::new();

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let outbounds = val["outbounds"].as_array().unwrap();
        let proxy_out = outbounds.iter().find(|o| o["tag"] == "proxy").unwrap();
        assert_eq!(proxy_out["tls"]["utls"]["enabled"], true);
        assert_eq!(proxy_out["tls"]["utls"]["fingerprint"], "chrome");

        // Ключ с явным fp=firefox должен сохранить свой fingerprint
        let key_ff =
            parse_vpn_key("vless://my-uuid@1.1.1.1:443?security=tls&fp=firefox#TestFirefox")
                .unwrap();
        let json_str_ff = build_singbox_config(&key_ff, &settings);
        let val_ff: serde_json::Value = serde_json::from_str(&json_str_ff).unwrap();
        let out_ff = val_ff["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["tag"] == "proxy")
            .unwrap()
            .clone();
        assert_eq!(out_ff["tls"]["utls"]["fingerprint"], "firefox");
    }

    #[test]
    fn test_build_config_google_priority_rules() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443#TestGoogle").expect("Валидный ключ");
        let mut settings = AppSettings::new();
        settings.enable_routing = true;
        settings.route_ru = true;
        settings.route_ru_ips = true;

        let json_str = build_singbox_config(&key, &settings);
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let rules = val["route"]["rules"].as_array().unwrap();
        // Находим индекс правила google и индекс правила geoip-ru
        let google_idx = rules
            .iter()
            .position(|r| {
                r.get("rule_set")
                    .and_then(|rs| rs.as_array())
                    .map(|arr| arr.iter().any(|v| v == "geosite-google"))
                    .unwrap_or(false)
            })
            .expect("Правило geosite-google должно быть");

        let ru_idx = rules
            .iter()
            .position(|r| {
                r.get("rule_set")
                    .and_then(|rs| rs.as_array())
                    .map(|arr| arr.iter().any(|v| v == "geoip-ru"))
                    .unwrap_or(false)
            })
            .expect("Правило geoip-ru должно быть");

        assert!(
            google_idx < ru_idx,
            "Правило Google должно быть СТРОГО ДО правила GeoIP RU"
        );
    }

    #[test]
    fn test_build_probe_config() {
        let key =
            parse_vpn_key("trojan://pass@ams-447a0d.wb-cdn-global.com:443#TestTrojan").unwrap();
        let probe_json = build_singbox_probe_config(&key, 12345);
        let val: serde_json::Value = serde_json::from_str(&probe_json).unwrap();

        assert_eq!(val["inbounds"][0]["listen_port"], 12345);
        assert_eq!(val["inbounds"][0]["tag"], "socks-probe");
        assert_eq!(val["route"]["default_domain_resolver"], "local-dns");
        assert_eq!(val["dns"]["servers"][1]["tag"], "local-dns");
        assert!(val["dns"]["servers"][1].get("detour").is_none());

        // DNS правило для обхода домена
        let rules = val["dns"]["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|r| r["domain"][0] == "ams-447a0d.wb-cdn-global.com"));
    }

    #[test]
    fn test_build_singbox_config_all_protocols() {
        let protocols = vec![
            "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@1.1.1.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=google.com&flow=xtls-rprx-vision#VLESS",
            "vmess://eyJ2IjoiMiIsInBzIjoiVk1lc3MiLCJhZGQiOiIxLjEuMS4xIiwicG9ydCI6NDQzLCJpZCI6ImEwYTBhMGEwLWEwYTAtYTBhMC1hMGEwLWEwYTBhMGEwYTBhMCIsIm5ldCI6IndzIn0=",
            "trojan://pass@1.1.1.1:443#Trojan",
            "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.1.1.1:8388#SS",
            "hy2://pass@1.1.1.1:8443?up=100&down=500&obfs=salamander&obfs-password=123#HY2",
            "tuic://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0:pass@1.1.1.1:8443?congestion_control=bbr#TUIC",
            "wg://AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=@1.1.1.1:51820?public_key=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=&ip=10.0.0.2/32#WG",
        ];

        let settings = AppSettings::new();

        for url in protocols {
            let key = parse_vpn_key(url).expect("Парсинг должен быть успешным");
            let config_json = build_singbox_config(&key, &settings);
            let val: serde_json::Value =
                serde_json::from_str(&config_json).expect("Конфигурация должна быть валидным JSON");

            assert!(val.get("inbounds").is_some());
            assert!(val.get("outbounds").is_some());
            assert!(val.get("route").is_some());
            assert!(val.get("dns").is_some());
        }
    }

    #[test]
    fn test_singbox_check_with_binary_if_present() {
        if let Some(bin_path) = crate::daemon::updater::find_singbox_binary() {
            let protocols = vec![
                "trojan://password123@ams-447a0d.wb-cdn-global.com:443#TestTrojan",
                "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@1.1.1.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=google.com&flow=xtls-rprx-vision#TestVLESS",
                "vmess://eyJ2IjoiMiIsInBzIjoiVk1lc3MiLCJhZGQiOiIxLjEuMS4xIiwicG9ydCI6NDQzLCJpZCI6ImEwYTBhMGEwLWEwYTAtYTBhMC1hMGEwLWEwYTBhMGEwYTBhMCIsIm5ldCI6IndzIn0=",
                "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.1.1.1:8388#SS",
                "hy2://pass@1.1.1.1:8443?up=100&down=500&obfs=salamander&obfs-password=123#HY2",
                "tuic://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0:pass@1.1.1.1:8443?congestion_control=bbr#TUIC",
                "wg://AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=@1.1.1.1:51820?public_key=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=&ip=10.0.0.2/32#WG",
            ];

            let temp_dir = std::env::temp_dir();
            let temp_file = temp_dir.join("test_vrxx_singbox_check.json");

            for url in &protocols {
                let key = parse_vpn_key(url).expect("Парсинг ключа");

                // Проверка регулярного конфига
                for tun_mode in [false, true] {
                    for fake_dns in [false, true] {
                        for sniffing in [false, true] {
                            let mut settings = AppSettings::new();
                            settings.tun_mode = tun_mode;
                            settings.enable_fake_dns = fake_dns;
                            settings.enable_sniffing = sniffing;
                            settings.disable_ipv6 = true;

                            let config_json = build_singbox_config(&key, &settings);
                            std::fs::write(&temp_file, &config_json)
                                .expect("Запись тестового конфига");

                            let output = std::process::Command::new(&bin_path)
                                .arg("check")
                                .arg("-c")
                                .arg(&temp_file)
                                .output();

                            if let Ok(res) = output {
                                let stderr = String::from_utf8_lossy(&res.stderr);
                                assert!(
                                    res.status.success(),
                                    "sing-box check завершился ошибкой для URL {} (tun={}, fake_dns={}, sniffing={}):\nSTDERR:\n{}\nКонфиг:\n{}",
                                    url,
                                    tun_mode,
                                    fake_dns,
                                    sniffing,
                                    stderr,
                                    config_json
                                );
                            }
                        }
                    }
                }

                // Проверка probe конфига
                let probe_json = build_singbox_probe_config(&key, 19998);
                std::fs::write(&temp_file, &probe_json).expect("Запись probe конфига");
                let output = std::process::Command::new(&bin_path)
                    .arg("check")
                    .arg("-c")
                    .arg(&temp_file)
                    .output();
                if let Ok(res) = output {
                    let stderr = String::from_utf8_lossy(&res.stderr);
                    assert!(
                        res.status.success(),
                        "sing-box check для PROBE завершился ошибкой для URL {}:\nSTDERR:\n{}\nКонфиг:\n{}",
                        url,
                        stderr,
                        probe_json
                    );
                }
            }

            let _ = std::fs::remove_file(&temp_file);
        }
    }
}
