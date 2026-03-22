use crate::settings::AppSettings;
use crate::domain::key_parser::ParsedKey;
use serde_json::json;

pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    let mut actual_http_port = settings.http_port;
    if actual_http_port == settings.socks_port {
        actual_http_port += 1;
    }

    let mut inbounds = vec![
        json!({
            "type": "socks",
            "tag": "socks-in",
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": settings.socks_port,
            "sniff": settings.enable_sniffing,
            "sniff_override_destination": settings.enable_sniffing
        }),
        json!({
            "type": "http",
            "tag": "http-in",
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": actual_http_port
        })
    ];

    if settings.tun_mode {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "tun0",
            "address": [
                "172.19.0.1/30",
                "fdfe:dcba:9876::1/126"
            ],
            "auto_route": true,
            "strict_route": true,
            "stack": "gvisor",
            "sniff": settings.enable_sniffing,
            "sniff_override_destination": settings.enable_sniffing
        }));
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

    if settings.disable_ipv6 {
        proxy_outbound["domain_strategy"] = json!("ipv4_only");
    }

    let mut rules = vec![];

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
    }

    let mut rule_sets = vec![];
    let mut active_rule_sets = vec![];

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
                    "domain": [rule.value],
                    "outbound": action_tag
                }));
            } else if rule.type_ == "ip" {
                rules.push(json!({
                    "ip_cidr": [rule.value],
                    "outbound": action_tag
                }));
            } else if rule.type_ == "srs_url" {
                let srs_tag = format!("srs-{}", rule.name.replace(" ", "-").to_lowercase());
                rule_sets.push(json!({
                    "tag": srs_tag.clone(),
                    "type": "remote",
                    "format": "binary",
                    "url": rule.value,
                    "download_detour": "direct"
                }));
                rules.push(json!({
                    "rule_set": [srs_tag],
                    "outbound": action_tag
                }));
            }
        }
    }

    let outbounds = vec![
        proxy_outbound,
        json!({
            "type": "direct",
            "tag": "direct",
            "domain_strategy": if settings.disable_ipv6 { "ipv4_only" } else { "" }
        }),
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

    if !rule_sets.is_empty() {
        route_config["rule_set"] = json!(rule_sets);
    }

    let dns_config = json!({
        "servers": [
            {
                "tag": "remote-dns",
                "type": "https",
                "server": "1.1.1.1",
                "detour": "proxy"
            },
            {
                "tag": "local-dns",
                "type": "local",
                "detour": "direct"
            }
        ],
        "rules": [
            {
                "outbound": "any",
                "server": "local-dns"
            },
            {
                "rule_set": active_rule_sets.clone(),
                "server": "local-dns"
            }
        ],
        "final": "remote-dns",
        "independent_cache": true
    });

    let mut root = json!({
        "log": {
            "level": settings.log_level,
            "timestamp": true
        },
        "dns": dns_config,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route_config
    });

    if active_rule_sets.is_empty() {
        root["dns"]["rules"] = json!([
            {
                "outbound": "any",
                "server": "local-dns"
            }
        ]);
    }

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
}