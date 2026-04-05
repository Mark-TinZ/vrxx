use crate::settings::AppSettings;
use crate::domain::key_parser::ParsedKey;
use serde_json::json;
use std::str::FromStr;

fn get_singbox_version() -> (u32, u32, u32) {
    if let Ok(output) = std::process::Command::new("sing-box").arg("version").output() {
        if let Ok(ver_str) = String::from_utf8(output.stdout) {
            if let Some(version_line) = ver_str.lines().next() {
                if let Some(v_str) = version_line.strip_prefix("sing-box version ") {
                    let parts: Vec<&str> = v_str.trim().split('.').collect();
                    if parts.len() >= 2 {
                        let major = parts[0].parse().unwrap_or(1);
                        let minor = parts[1].parse().unwrap_or(8);
                        let patch = parts.get(2).and_then(|p| p.split('-').next()).and_then(|p| p.parse().ok()).unwrap_or(0);
                        return (major, minor, patch);
                    }
                }
            }
        }
    }
    (1, 8, 0)
}

pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    let mut actual_http_port = settings.http_port;
    if actual_http_port == settings.socks_port {
        actual_http_port += 1;
    }

    let sb_version = get_singbox_version();
    let is_1_11_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 11);
    let is_1_12_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 12);

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
        })
    ];

    if settings.tun_mode {
        let mut tun_inbound = json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "vrxx-tun",
            "address": [
                "172.19.0.1/30",
                "fdfe:dcba:9876::1/126"
            ],
            "auto_route": true,
            "strict_route": true,
            "stack": "gvisor",
        });
        
        if !is_1_11_or_newer {
            tun_inbound["sniff"] = json!(settings.enable_sniffing);
            tun_inbound["sniff_override_destination"] = json!(settings.enable_sniffing);
        }
        
        inbounds.push(tun_inbound);
    }

    let qp = &parsed_key.query_params;
    let security = qp.get("security").map(|s| s.as_str()).unwrap_or("none");
    let net = qp.get("type").map(|s| s.as_str()).unwrap_or("tcp");

    let mut proxy_outbound = json!({
        "type": parsed_key.protocol.to_lowercase(),
        "tag": "proxy",
        "server": parsed_key.host,
        "server_port": parsed_key.port,
    });
    
    // Add domain_resolver for domain servers in sing-box 1.12+
    if is_1_12_or_newer && std::net::IpAddr::from_str(&parsed_key.host).is_err() {
        proxy_outbound["domain_resolver"] = json!("remote-dns");
    }

    if parsed_key.protocol.to_lowercase() == "vless" || parsed_key.protocol.to_lowercase() == "vmess" {
        proxy_outbound["uuid"] = json!(parsed_key.uuid);
        if parsed_key.protocol.to_lowercase() == "vmess" {
            proxy_outbound["alter_id"] = json!(0);
            proxy_outbound["security"] = json!("auto");
        } else {
            let flow = qp.get("flow").map(|s| s.as_str()).unwrap_or("");
            if !flow.is_empty() {
                proxy_outbound["flow"] = json!(flow);
            }
        }
    } else if parsed_key.protocol.to_lowercase() == "trojan" {
        proxy_outbound["password"] = json!(parsed_key.uuid);
    }

    if security == "tls" || security == "reality" {
        let mut tls = json!({
            "enabled": true,
            "server_name": qp.get("sni").unwrap_or(&parsed_key.host),
            "alpn": qp.get("alpn").map(|s| s.split(',').collect::<Vec<&str>>()).unwrap_or_else(|| vec!["h2", "http/1.1"])
        });
        
        let fp = qp.get("fp").map(|s| s.as_str()).unwrap_or("chrome");
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

    if settings.enable_mux && security != "reality" {
        proxy_outbound["multiplex"] = json!({
            "enabled": true,
            "protocol": "smux"
        });
    }

    if !is_1_12_or_newer && settings.disable_ipv6 {
        proxy_outbound["domain_strategy"] = json!("ipv4_only");
    }

    let mut rules = vec![];
    
    if is_1_11_or_newer && settings.enable_sniffing {
        rules.push(json!({
            "action": "sniff"
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
            "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/main/rule-set/geosite-category-ads-all.srs",
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
            add_region("geosite-ru", "https://raw.githubusercontent.com/SagerNet/sing-geosite/main/rule-set/geosite-ru.srs");
            add_region("geoip-ru", "https://raw.githubusercontent.com/SagerNet/sing-geoip/main/rule-set/geoip-ru.srs");
        }
        if settings.route_cn {
            add_region("geosite-cn", "https://raw.githubusercontent.com/SagerNet/sing-geosite/main/rule-set/geosite-cn.srs");
            add_region("geoip-cn", "https://raw.githubusercontent.com/SagerNet/sing-geoip/main/rule-set/geoip-cn.srs");
        }
        if settings.route_ir {
            add_region("geosite-ir", "https://raw.githubusercontent.com/SagerNet/sing-geosite/main/rule-set/geosite-ir.srs");
            add_region("geoip-ir", "https://raw.githubusercontent.com/SagerNet/sing-geoip/main/rule-set/geoip-ir.srs");
        }
        if settings.route_antifilter {
            add_region("geosite-antifilter", "https://raw.githubusercontent.com/SagerNet/sing-geosite/main/rule-set/geosite-antifilter.srs");
        }

        if !active_rule_sets.is_empty() {
            let target_tag = if settings.routing_mode == "proxy" { "proxy" } else { "direct" };
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

        // Add custom rules
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
                let srs_tag = format!("srs-{}", rule.name.replace(" ", "-").to_lowercase());
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
        })
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

    let mut remote_dns = json!({
        "tag": "remote-dns",
        "type": "https",
        "server": "1.1.1.1",
        "detour": "proxy"
    });
    
    let mut local_dns = json!({
        "tag": "local-dns",
        "type": "local",
        "detour": "direct"
    });

    if is_1_12_or_newer && settings.disable_ipv6 {
        // dns rules reject ipv6
    }

    let mut dns_rules = vec![];

    if is_1_12_or_newer && settings.disable_ipv6 {
        dns_rules.push(json!({
            "ip_version": 6,
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

    let root = json!({
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

    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;
    use crate::domain::key_parser::ParsedKey;
    use std::collections::HashMap;

    #[test]
    fn test_build_singbox_config_generates_valid_json() {
        let key = ParsedKey {
            protocol: "VLESS".to_string(),
            name: "Test".to_string(),
            host: "example.com".to_string(),
            port: 443,
            uuid: "uuid-123".to_string(),
            query_params: HashMap::new(),
            raw_url: "vless://...".to_string(),
        };
        let mut settings = AppSettings::new();
        settings.socks_port = 1080;
        
        let json_str = build_singbox_config(&key, &settings);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Should be valid JSON");
        
        let proxy_outbound = parsed["outbounds"].as_array().unwrap().first().unwrap();
        assert_eq!(proxy_outbound["type"], "vless");
        assert_eq!(proxy_outbound["server"], "example.com");
        assert_eq!(proxy_outbound["uuid"], "uuid-123");
    }

    #[test]
    fn test_singbox_config_validity_permutations() {
        use std::process::Command;
        use std::fs::File;
        use std::io::Write;

        // Skip test if sing-box is not installed or lacks v2ray_api
        let version_out = Command::new("sing-box").arg("version").output();
        if version_out.is_err() {
            println!("sing-box not installed, skipping test");
            return;
        }
        let v_out = String::from_utf8_lossy(&version_out.unwrap().stdout).to_lowercase();
        if !v_out.contains("with_v2ray_api") && v_out.contains("tags:") {
            println!("sing-box lacks with_v2ray_api, skipping test");
            return;
        }

        let key = ParsedKey {
            protocol: "VLESS".to_string(),
            name: "Test".to_string(),
            host: "example.com".to_string(),
            port: 443,
            uuid: "uuid-123".to_string(),
            query_params: HashMap::new(),
            raw_url: "vless://...".to_string(),
        };

        let toggles = [false, true];
        for &ipv6 in &toggles {
            for &ru in &toggles {
                for &cn in &toggles {
                    for &ir in &toggles {
                        let mut settings = AppSettings::new();
                        settings.socks_port = 1080;
                        settings.disable_ipv6 = ipv6;
                        settings.route_ru = ru;
                        settings.route_cn = cn;
                        settings.route_ir = ir;
                        settings.enable_routing = ru || cn || ir;
                        
                        let json_str = build_singbox_config(&key, &settings);
                        
                        let temp_file_path = format!("/tmp/vrxx_singbox_test_{}_{}_{}_{}.json", ipv6, ru, cn, ir);
                        let mut file = File::create(&temp_file_path).unwrap();
                        file.write_all(json_str.as_bytes()).unwrap();
                        
                        let output = Command::new("sing-box")
                            .args(["check", "-c", &temp_file_path])
                            .output()
                            .expect("Failed to execute sing-box check");
                        
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        assert!(output.status.success(), "Sing-box check failed for toggles (IPv6: {}, RU: {}, CN: {}, IR: {}):\n{}", ipv6, ru, cn, ir, stderr);
                        
                        std::fs::remove_file(temp_file_path).unwrap();
                    }
                }
            }
        }
    }
}
