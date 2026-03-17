use crate::settings::AppSettings;
use crate::key_parser::ParsedKey;
use serde_json::json;

pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
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
            "listen_port": settings.http_port
        })
    ];

    if settings.tun_mode {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "tun0",
            "inet4_address": "172.19.0.1/30",
            "auto_route": true,
            "strict_route": true,
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

    let mut rules = vec![];
    if settings.bypass_lan {
        rules.push(json!({
            "ip_is_private": true,
            "outbound": "direct"
        }));
    }

    if settings.enable_routing && !settings.whitelist.is_empty() {
        let mut domains = vec![];
        let mut domain_suffix = vec![];
        for d in &settings.whitelist {
            if let Some(stripped) = d.strip_prefix("*.") {
                domain_suffix.push(stripped.to_string());
            } else {
                domains.push(d.to_string());
            }
        }
        
        let target_tag = if settings.routing_mode == "proxy" { "proxy" } else { "direct" };
        let mut rule = json!({ "outbound": target_tag });
        if !domains.is_empty() { rule["domain"] = json!(domains); }
        if !domain_suffix.is_empty() { rule["domain_suffix"] = json!(domain_suffix); }
        rules.push(rule);
        
        if settings.routing_mode == "proxy" {
            rules.push(json!({
                "outbound": "direct"
            }));
        }
    }

    let outbounds = vec![
        proxy_outbound,
        json!({
            "type": "direct",
            "tag": "direct"
        }),
        json!({
            "type": "block",
            "tag": "block"
        })
    ];
    
    let root = json!({
        "log": {
            "level": settings.log_level,
            "timestamp": true
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "rules": rules,
            "auto_detect_interface": true
        }
    });

    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
}