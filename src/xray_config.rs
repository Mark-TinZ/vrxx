use crate::settings::AppSettings;
use crate::key_parser::ParsedKey;
use serde_json::json;

pub fn build_xray_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    let mut inbounds = vec![
        json!({
            "tag": "socks-in",
            "port": settings.socks_port,
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "protocol": "socks",
            "settings": {
                "udp": true,
                "auth": "noauth"
            }
        }),
        json!({
            "tag": "http-in",
            "port": settings.http_port,
            "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "protocol": "http"
        })
    ];

    if settings.tun_mode {
        inbounds.push(json!({
            "tag": "tun-in",
            "protocol": "tun",
            "settings": {
                "network": "tcp,udp",
                "address": [
                    "172.19.0.1/30",
                    "fdfe:dcba:9876::1/126"
                ],
                "autoRoute": true,
                "strictRoute": true,
                "mtu": 9000
            }
        }));
    }

    let mut stream_settings = json!({});
    
    // Parse query parameters for transport and security
    let qp = &parsed_key.query_params;
    let net = qp.get("type").map(|s| s.as_str()).unwrap_or("tcp");
    let security = qp.get("security").map(|s| s.as_str()).unwrap_or("none");
    
    stream_settings["network"] = json!(net);
    stream_settings["security"] = json!(security);

    if security == "tls" {
        stream_settings["tlsSettings"] = json!({
            "serverName": qp.get("sni").unwrap_or(&parsed_key.host),
            "fingerprint": qp.get("fp").unwrap_or(&"chrome".to_string()),
            "alpn": qp.get("alpn").map(|s| s.split(',').collect::<Vec<&str>>()).unwrap_or_else(|| vec!["h2", "http/1.1"])
        });
    } else if security == "reality" {
        stream_settings["realitySettings"] = json!({
            "serverName": qp.get("sni").unwrap_or(&parsed_key.host),
            "fingerprint": qp.get("fp").unwrap_or(&"chrome".to_string()),
            "publicKey": qp.get("pbk").unwrap_or(&"".to_string()),
            "shortId": qp.get("sid").unwrap_or(&"".to_string()),
            "spiderX": qp.get("spx").unwrap_or(&"/".to_string()),
        });
    }

    // Transport specific settings
    if net == "ws" {
        stream_settings["wsSettings"] = json!({
            "path": qp.get("path").unwrap_or(&"/".to_string()),
            "headers": {
                "Host": qp.get("host").unwrap_or(&parsed_key.host)
            }
        });
    } else if net == "grpc" {
        stream_settings["grpcSettings"] = json!({
            "serviceName": qp.get("serviceName").unwrap_or(&"".to_string()),
            "multiMode": qp.get("mode").map(|m| m == "multi").unwrap_or(false)
        });
    } else if net == "tcp" {
        let empty_str = "".to_string();
        let flow = qp.get("flow").unwrap_or(&empty_str);
        if !flow.is_empty() {
            // Flow typically implies xtls or vision, set it on the outbound settings if it's VLESS
        }
    }

    let outbound_settings = if parsed_key.protocol.to_lowercase() == "vless" {
        json!({
            "vnext": [{
                "address": parsed_key.host,
                "port": parsed_key.port,
                "users": [{
                    "id": parsed_key.uuid,
                    "encryption": "none",
                    "flow": qp.get("flow").unwrap_or(&"".to_string())
                }]
            }]
        })
    } else if parsed_key.protocol.to_lowercase() == "vmess" {
         json!({
            "vnext": [{
                "address": parsed_key.host,
                "port": parsed_key.port,
                "users": [{
                    "id": parsed_key.uuid,
                    "alterId": 0,
                    "security": "auto"
                }]
            }]
        })
    } else if parsed_key.protocol.to_lowercase() == "trojan" {
        json!({
            "servers": [{
                "address": parsed_key.host,
                "port": parsed_key.port,
                "password": parsed_key.uuid
            }]
        })
    } else {
        json!({})
    };

    let mut rules = vec![];
    
    // Whitelist rules handling
    if !settings.whitelist.is_empty() {
        let mut domains = vec![];
        for d in &settings.whitelist {
            if d.contains('*') {
                // Convert *.google.com to regexp:.*\.google\.com$
                // Convert *.ru to regexp:.*\.ru$
                let pattern = d.replace(".", "\\.").replace("*", ".*");
                domains.push(format!("regexp:{}$", pattern));
            } else {
                domains.push(format!("domain:{}", d));
            }
        }
        
        // We will assume whitelist domains are to be bypassed (direct)
        rules.push(json!({
            "type": "field",
            "outboundTag": "direct",
            "domain": domains
        }));
    }

    let config = json!({
        "log": {
            "loglevel": settings.log_level
        },
        "inbounds": inbounds,
        "outbounds": [
            {
                "tag": "proxy",
                "protocol": parsed_key.protocol.to_lowercase(),
                "settings": outbound_settings,
                "streamSettings": stream_settings
            },
            {
                "tag": "direct",
                "protocol": "freedom",
                "settings": {}
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": {}
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": rules
        }
    });

    serde_json::to_string_pretty(&config).unwrap()
}
