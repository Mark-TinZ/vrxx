use crate::domain::key_parser::ParsedKey;
use crate::settings::AppSettings;
use serde_json::json;
use std::str::FromStr;

/// Определяет версию установленного в системе sing-box.
/// Возвращает кортеж (Major, Minor, Patch). По умолчанию (1, 8, 0).
fn get_singbox_version() -> (u32, u32, u32) {
    let mut bin_path = std::path::PathBuf::from("sing-box");
    if let Some(local_dir) = dirs::data_local_dir() {
        let local_bin = local_dir.join("vrxx").join("bin").join("sing-box");
        if local_bin.exists() {
            bin_path = local_bin;
        }
    }

    if let Ok(output) = std::process::Command::new(&bin_path)
        .arg("version")
        .output()
    {
        if let Ok(ver_str) = String::from_utf8(output.stdout) {
            if let Some(version_line) = ver_str.lines().next() {
                if let Some(v_str) = version_line.strip_prefix("sing-box version ") {
                    let parts: Vec<&str> = v_str.trim().split('.').collect();
                    if parts.len() >= 2 {
                        let major = parts[0].parse().unwrap_or(1);
                        let minor = parts[1].parse().unwrap_or(8);
                        let patch = parts
                            .get(2)
                            .and_then(|p| p.split('-').next())
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(0);
                        return (major, minor, patch);
                    }
                }
            }
        }
    }
    (1, 8, 0)
}

/// Генерирует JSON-конфигурацию для sing-box на основе выбранного ключа и настроек приложения с автоматическим определением версии ядра.
pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    let version = get_singbox_version();
    build_singbox_config_with_version(parsed_key, settings, version)
}

/// Генерирует JSON-конфигурацию для sing-box с явно заданной версией ядра (используется для генерации и версионных тестов).
pub fn build_singbox_config_with_version(
    parsed_key: &ParsedKey,
    settings: &AppSettings,
    sb_version: (u32, u32, u32),
) -> String {
    let mut actual_http_port = settings.http_port;
    if actual_http_port == settings.socks_port {
        actual_http_port += 1;
    }

    let is_1_11_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 11);
    let is_1_12_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 12);
    let is_1_13_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 13);

    // 1. Inbounds
    let mut socks_inbound = json!({
        "type": "socks",
        "tag": "socks-in",
        "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
        "listen_port": settings.socks_port,
    });

    if !is_1_11_or_newer {
        socks_inbound["sniff"] = json!(settings.enable_sniffing);
        socks_inbound["sniff_override_destination"] = json!(settings.enable_sniffing);
    }

    let mut inbounds = vec![
        socks_inbound,
        json!({
            "type": "http",
            "tag": "http-in",
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": actual_http_port
        }),
    ];

    if settings.tun_mode {
        let mut tun_inbound = json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "vrxx-tun",
            "auto_route": true,
            "strict_route": true,
            "stack": "gvisor",
        });

        if is_1_12_or_newer {
            tun_inbound["address"] = json!(["172.19.0.1/30", "fdfe:dcba:9876::1/126"]);
        } else {
            tun_inbound["inet4_address"] = json!("172.19.0.1/30");
            tun_inbound["inet6_address"] = json!("fdfe:dcba:9876::1/126");
        }

        if !is_1_11_or_newer {
            tun_inbound["sniff"] = json!(settings.enable_sniffing);
            tun_inbound["sniff_override_destination"] = json!(settings.enable_sniffing);
        }

        inbounds.push(tun_inbound);
    }

    // 2. Outbounds & Endpoints
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
                "packet_encoding": "xudp"
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

            if is_1_13_or_newer {
                endpoints.push(json!({
                    "type": "wireguard",
                    "tag": "wg-ep",
                    "system_interface": false,
                    "interface_name": "wg-vrxx",
                    "local_address": [local_ip],
                    "private_key": parsed_key.uuid,
                    "peers": [{
                        "server": parsed_key.host,
                        "server_port": parsed_key.port,
                        "public_key": peer_pub,
                        "allowed_ips": ["0.0.0.0/0", "::/0"]
                    }]
                }));
                json!({
                    "type": "direct",
                    "tag": "proxy",
                    "detour": "wg-ep"
                })
            } else {
                json!({
                    "type": "wireguard",
                    "tag": "proxy",
                    "server": parsed_key.host,
                    "server_port": parsed_key.port,
                    "system_interface": false,
                    "interface_name": "wg-vrxx",
                    "local_address": [local_ip],
                    "private_key": parsed_key.uuid,
                    "peer_public_key": peer_pub,
                })
            }
        }
        _ => json!({
            "type": "direct",
            "tag": "proxy"
        }),
    };

    if is_1_12_or_newer && std::net::IpAddr::from_str(&parsed_key.host).is_err() {
        proxy_outbound["domain_resolver"] = json!("remote-dns");
    }

    // TLS configuration
    if security == "tls"
        || security == "reality"
        || proto_lower == "hysteria2"
        || proto_lower == "tuic"
    {
        let mut tls = json!({
            "enabled": true,
            "server_name": qp.get("sni").unwrap_or(&parsed_key.host),
            "alpn": qp.get("alpn").map(|s| s.split(',').collect::<Vec<&str>>()).unwrap_or_else(|| vec!["h2", "http/1.1"])
        });

        let fp = qp
            .get("fp")
            .or_else(|| qp.get("fingerprint"))
            .map(|s| s.as_str())
            .unwrap_or("chrome");
        if !fp.is_empty() {
            tls["utls"] = json!({
                "enabled": true,
                "fingerprint": fp
            });
        }

        if security == "reality" {
            tls["reality"] = json!({
                "enabled": true,
                "public_key": qp.get("pbk").map(|s| s.as_str()).unwrap_or(""),
                "short_id": qp.get("sid").map(|s| s.as_str()).unwrap_or("")
            });
        }

        proxy_outbound["tls"] = tls;
    }

    // Transports
    if net == "grpc" {
        proxy_outbound["transport"] = json!({
            "type": "grpc",
            "service_name": qp.get("serviceName").map(|s| s.as_str()).unwrap_or("")
        });
    } else if net == "ws" {
        proxy_outbound["transport"] = json!({
            "type": "ws",
            "path": qp.get("path").map(|s| s.as_str()).unwrap_or("/"),
            "headers": {
                "Host": qp.get("host").unwrap_or(&parsed_key.host)
            }
        });
    }

    // Multiplexing
    if settings.enable_mux
        && security != "reality"
        && proto_lower != "hysteria2"
        && proto_lower != "tuic"
    {
        proxy_outbound["multiplex"] = json!({
            "enabled": true,
            "protocol": "smux"
        });
    }

    if !is_1_12_or_newer && settings.disable_ipv6 {
        proxy_outbound["domain_strategy"] = json!("ipv4_only");
    }

    // 3. Routing rules
    let mut rules = vec![];

    if is_1_11_or_newer && settings.enable_sniffing {
        rules.push(json!({
            "action": "sniff"
        }));
    }

    if is_1_13_or_newer {
        rules.push(json!({
            "protocol": "dns",
            "action": "hijack-dns"
        }));
    }

    if settings.disable_ipv6 {
        rules.push(json!({
            "ip_cidr": ["::/0"],
            "outbound": "block"
        }));
    }

    if settings.bypass_lan {
        rules.push(json!({
            "ip_is_private": true,
            "outbound": "direct"
        }));
        rules.push(json!({
            "ip_cidr": ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"],
            "outbound": "direct"
        }));
    }

    let mut rule_sets = vec![];
    let mut active_rule_sets = vec![];

    if settings.block_ads {
        let tag = "geosite-category-ads-all";
        rule_sets.push(json!({
            "tag": tag,
            "type": "remote",
            "format": "binary",
            "url": "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geosite/geosite-category-ads-all.srs",
            "download_detour": "direct"
        }));
        rules.push(json!({
            "rule_set": [tag],
            "outbound": "block"
        }));
    }

    if settings.enable_routing {
        let mut add_region = |tag: &str, url: &str| {
            active_rule_sets.push(tag.to_string());
            rule_sets.push(json!({
                "tag": tag,
                "type": "remote",
                "format": "binary",
                "url": url,
                "download_detour": "direct"
            }));
        };

        if settings.route_ru {
            add_region("geosite-ru", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geosite/geosite-ru.srs");
            add_region("geoip-ru", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geoip/geoip-ru.srs");
        }
        if settings.route_cn {
            add_region("geosite-cn", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geosite/geosite-cn.srs");
            add_region("geoip-cn", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geoip/geoip-cn.srs");
        }
        if settings.route_ir {
            add_region("geosite-ir", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geosite/geosite-ir.srs");
            add_region("geoip-ir", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geoip/geoip-ir.srs");
        }
        if settings.route_antifilter {
            add_region("geosite-antifilter", "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/refs/heads/sing/geo/geosite/geosite-antifilter.srs");
        }

        if !active_rule_sets.is_empty() {
            let target_tag = if settings.routing_mode == "proxy" {
                "proxy"
            } else {
                "direct"
            };
            rules.push(json!({
                "rule_set": active_rule_sets,
                "outbound": target_tag
            }));

            if settings.routing_mode == "proxy" {
                rules.push(json!({
                    "outbound": "direct"
                }));
            }
        }

        for rule in &settings.routing_rules {
            let action_tag = if rule.action == "direct" {
                "direct"
            } else if rule.action == "block" {
                "block"
            } else {
                "proxy"
            };

            if rule.type_ == "domain" {
                rules.push(json!({
                    "domain": [rule.value.clone()],
                    "outbound": action_tag
                }));
            } else if rule.type_ == "ip" {
                rules.push(json!({
                    "ip_cidr": [rule.value.clone()],
                    "outbound": action_tag
                }));
            } else if rule.type_ == "srs_url" {
                let srs_tag = format!("srs-{}", rule.name.replace(' ', "-").to_lowercase());
                rule_sets.push(json!({
                    "tag": srs_tag.clone(),
                    "type": "remote",
                    "format": "binary",
                    "url": rule.value.clone(),
                    "download_detour": "direct"
                }));
                rules.push(json!({
                    "rule_set": [srs_tag],
                    "outbound": action_tag
                }));
            }
        }
    }

    let mut direct_outbound = json!({
        "type": "direct",
        "tag": "direct"
    });
    if !is_1_12_or_newer && settings.disable_ipv6 {
        direct_outbound["domain_strategy"] = json!("ipv4_only");
    }

    let outbounds = vec![
        proxy_outbound,
        direct_outbound,
        json!({
            "type": "block",
            "tag": "block"
        }),
    ];

    let mut route_config = json!({
        "rules": rules,
        "auto_detect_interface": true,
        "final": "proxy"
    });

    if is_1_12_or_newer {
        route_config["default_domain_resolver"] = json!("remote-dns");
    }

    if !rule_sets.is_empty() {
        route_config["rule_set"] = json!(rule_sets);
    }

    // 4. DNS config
    let remote_dns = json!({
        "tag": "remote-dns",
        "type": "https",
        "server": "1.1.1.1",
        "detour": "proxy"
    });

    let local_dns = json!({
        "tag": "local-dns",
        "type": "local",
        "detour": "direct"
    });

    let mut dns_rules = vec![];

    if is_1_12_or_newer && settings.disable_ipv6 {
        dns_rules.push(json!({
            "query_type": ["AAAA"],
            "action": "reject",
            "method": "drop"
        }));
    }

    if !active_rule_sets.is_empty() {
        dns_rules.push(json!({
            "rule_set": active_rule_sets.clone(),
            "server": "local-dns"
        }));
    }

    if !is_1_12_or_newer && active_rule_sets.is_empty() {
        dns_rules.push(json!({
            "outbound": "any",
            "server": "local-dns"
        }));
    }

    let dns_config = json!({
        "servers": [
            remote_dns,
            local_dns
        ],
        "rules": dns_rules,
        "final": "remote-dns",
        "independent_cache": true
    });

    // 5. Final Root Assembly
    let mut root = json!({
        "log": {
            "level": settings.log_level,
            "timestamp": true
        },
        "dns": dns_config,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route_config,
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:9090"
            }
        }
    });

    if is_1_13_or_newer && !endpoints.is_empty() {
        root["endpoints"] = json!(endpoints);
    }

    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::key_parser::parse_vpn_key;

    #[test]
    fn test_singbox_config_validity() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443?security=reality&pbk=pubkey&sid=sid&sni=google.com&flow=xtls-rprx-vision#TestVLESS")
            .expect("Valid key");
        let settings = AppSettings::default();

        let json_str = build_singbox_config(&key, &settings);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");

        assert!(parsed.get("inbounds").is_some());
        assert!(parsed.get("outbounds").is_some());
        assert!(parsed.get("route").is_some());
        assert!(parsed.get("dns").is_some());
    }

    #[test]
    fn test_singbox_version_1_13_adaptation() {
        let key = parse_vpn_key(
            "wg://my-priv-key@1.1.1.1:51820?public_key=peer_pub&ip=10.0.0.2/32#TestWG",
        )
        .expect("Valid WG key");
        let mut settings = AppSettings::default();
        settings.tun_mode = true;
        settings.disable_ipv6 = true;

        let json_str = build_singbox_config_with_version(&key, &settings, (1, 13, 0));
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");

        // Verify endpoints array for 1.13+ WireGuard
        assert!(
            parsed.get("endpoints").is_some(),
            "Should contain endpoints in 1.13+"
        );

        // Verify TUN address array in 1.13+
        let inbounds = parsed
            .get("inbounds")
            .and_then(|i| i.as_array())
            .expect("Inbounds array");
        let tun = inbounds
            .iter()
            .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("tun"))
            .expect("TUN inbound");
        assert!(
            tun.get("address").is_some(),
            "Should use address array in 1.13+"
        );

        // Verify route rules hijack-dns in 1.13+
        let rules = parsed
            .get("route")
            .and_then(|r| r.get("rules"))
            .and_then(|r| r.as_array())
            .expect("Route rules");
        let hijack_rule = rules
            .iter()
            .find(|r| r.get("action").and_then(|a| a.as_str()) == Some("hijack-dns"));
        assert!(
            hijack_rule.is_some(),
            "Should contain hijack-dns rule in 1.13+"
        );

        // Verify DNS reject for IPv6 AAAA in 1.13+
        let dns_rules = parsed
            .get("dns")
            .and_then(|d| d.get("rules"))
            .and_then(|r| r.as_array())
            .expect("DNS rules");
        let reject_rule = dns_rules
            .iter()
            .find(|r| r.get("action").and_then(|a| a.as_str()) == Some("reject"));
        assert!(
            reject_rule.is_some(),
            "Should contain reject rule for AAAA in 1.13+"
        );
    }

    #[test]
    fn test_singbox_version_1_8_adaptation() {
        let key = parse_vpn_key("vless://my-uuid@1.1.1.1:443#TestOld").expect("Valid key");
        let mut settings = AppSettings::default();
        settings.tun_mode = true;

        let json_str = build_singbox_config_with_version(&key, &settings, (1, 8, 0));
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");

        // Verify TUN inet4_address / inet6_address in < 1.12
        let inbounds = parsed
            .get("inbounds")
            .and_then(|i| i.as_array())
            .expect("Inbounds array");
        let tun = inbounds
            .iter()
            .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("tun"))
            .expect("TUN inbound");
        assert!(
            tun.get("inet4_address").is_some(),
            "Should use inet4_address in < 1.12"
        );
        assert!(
            tun.get("address").is_none(),
            "Should not use address array in < 1.12"
        );
    }

    #[test]
    fn test_all_protocols_config_generation() {
        let settings = AppSettings::default();

        let protocols = vec![
            "vless://uuid@1.1.1.1:443?security=reality&pbk=key&sid=id#VLESS",
            "vmess://eyJ2IjoiMiIsInBzIjoiVk1lc3MiLCJhZGQiOiIxLjEuMS4xIiwicG9ydCI6NDQzLCJpZCI6InV1aWQiLCJuZXQiOiJ3cyJ9",
            "trojan://pass@1.1.1.1:443#Trojan",
            "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.1.1.1:8388#SS",
            "hy2://pass@1.1.1.1:8443?up=100&down=500&obfs=salamander&obfs-password=123#HY2",
            "tuic://uuid:pass@1.1.1.1:8443?congestion_control=bbr#TUIC",
            "wg://privkey@1.1.1.1:51820?public_key=pubkey#WG",
        ];

        for proto_url in protocols {
            let key = parse_vpn_key(proto_url).expect("Key parse");
            let json_1_13 = build_singbox_config_with_version(&key, &settings, (1, 13, 0));
            let parsed_1_13: serde_json::Value =
                serde_json::from_str(&json_1_13).expect("Valid 1.13 JSON");
            assert!(
                parsed_1_13.get("outbounds").is_some(),
                "Protocol {} failed in 1.13",
                key.protocol
            );

            let json_1_8 = build_singbox_config_with_version(&key, &settings, (1, 8, 0));
            let parsed_1_8: serde_json::Value =
                serde_json::from_str(&json_1_8).expect("Valid 1.8 JSON");
            assert!(
                parsed_1_8.get("outbounds").is_some(),
                "Protocol {} failed in 1.8",
                key.protocol
            );
        }
    }
}
