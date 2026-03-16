use crate::settings::AppSettings;
use crate::key_parser::ParsedKey;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
pub struct LogConfig {
    pub loglevel: String,
    pub access: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct ApiConfig {
    pub services: Vec<String>,
    pub tag: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PolicyConfig {
    pub system: SystemPolicy,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct SystemPolicy {
    pub statsInboundUplink: bool,
    pub statsInboundDownlink: bool,
    pub statsOutboundUplink: bool,
    pub statsOutboundDownlink: bool,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct RoutingConfig {
    pub domainStrategy: String,
    pub rules: Vec<Value>,
}

#[derive(Serialize)]
pub struct XrayConfig {
    pub log: LogConfig,
    pub api: ApiConfig,
    pub stats: Value,
    pub policy: PolicyConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<Value>,
    pub inbounds: Vec<Value>,
    pub outbounds: Vec<Value>,
    pub routing: RoutingConfig,
}

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
            },
            "sniffing": if settings.enable_sniffing {
                json!({
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic"]
                })
            } else {
                json!({ "enabled": false })
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
            },
            "sniffing": if settings.enable_sniffing {
                json!({
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic"],
                    "routeOnly": true
                })
            } else {
                json!({ "enabled": false })
            }
        }));
    }

    inbounds.push(json!({
        "tag": "api",
        "listen": "127.0.0.1",
        "port": 10085,
        "protocol": "dokodemo-door",
        "settings": {
            "address": "127.0.0.1"
        }
    }));

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
        // TCP settings
    }

    // Mux and Fragment
    let mut proxy_outbound = json!({
        "tag": "proxy",
        "protocol": parsed_key.protocol.to_lowercase(),
        "streamSettings": stream_settings
    });

    if settings.enable_mux {
        proxy_outbound["mux"] = json!({
            "enabled": true,
            "concurrency": settings.mux_concurrency,
            "xudpConcurrency": settings.mux_concurrency,
            "xudpProxyUDP443": "reject"
        });
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

    proxy_outbound["settings"] = outbound_settings;

    let mut outbounds = vec![
        proxy_outbound,
        json!({
            "tag": "direct",
            "protocol": "freedom",
            "settings": {}
        }),
        json!({
            "tag": "block",
            "protocol": "blackhole",
            "settings": {}
        })
    ];

    if settings.enable_fragment {
        let first_outbound = outbounds[0].clone();
        outbounds[0] = json!({
            "tag": "proxy",
            "protocol": "freedom",
            "settings": {
                "fragment": {
                    "packets": "tlshello",
                    "length": "100-200",
                    "interval": "10-20"
                }
            },
            "streamSettings": {
                "sockopt": {
                    "dialerProxy": "fragment-proxy"
                }
            }
        });
        
        let mut frag_proxy = first_outbound;
        frag_proxy["tag"] = json!("fragment-proxy");
        outbounds.push(frag_proxy);
    }

    let mut rules = vec![];

    if settings.bypass_lan {
        rules.push(json!({
            "type": "field",
            "outboundTag": "direct",
            "ip": ["geoip:private"]
        }));
    }

    // Whitelist rules handling
    if settings.enable_routing && !settings.whitelist.is_empty() {
        let mut domains = vec![];
        for d in &settings.whitelist {
            if d.contains('*') {
                let pattern = d.replace(".", "\\.").replace("*", ".*");
                domains.push(format!("regexp:{}$", pattern));
            } else {
                domains.push(format!("domain:{}", d));
            }
        }
        
        let target_tag = if settings.routing_mode == "proxy" { "proxy" } else { "direct" };
        rules.push(json!({
            "type": "field",
            "outboundTag": target_tag,
            "domain": domains
        }));
        
        // If mode is proxy, we also want to ensure default traffic goes direct if no rules matched
        // But Xray's default rule handling depends on the first outbound. The first outbound is "proxy".
        // Wait, if it's "proxy", and rule says proxy, what about the rest? 
        // We probably should append a rule for direct if we want others to bypass, but standard VPN defaults all to proxy.
        // If mode is proxy (Включения), it means ONLY those domains go through VPN, others go direct.
        // To do this, we need a catch-all rule or swap the first outbound. Let's keep it simple:
        // By default all traffic goes to the first outbound ("proxy").
        // If routing_mode is "bypass" (Исключения), we add a rule to route `domains` to "direct". The rest goes to "proxy" (default).
        // If routing_mode is "proxy" (Включения), we want ONLY `domains` to go to "proxy". Then the rest should go to "direct".
        if settings.routing_mode == "proxy" {
            rules.push(json!({
                "type": "field",
                "outboundTag": "direct",
                "network": "tcp,udp"
            }));
        }
    }

    rules.push(json!({
        "type": "field",
        "inboundTag": ["api"],
        "outboundTag": "api"
    }));

    let mut dns_config = None;
    if settings.enable_fake_dns {
        dns_config = Some(json!({
            "servers": [
                "fakedns"
            ]
        }));
        // Note: Full fakedns implementation requires configuring fakedns inbound/sniffing.
    }

    let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
    std::fs::create_dir_all(&log_dir).ok();
    let access_log = log_dir.join("access.log").to_string_lossy().to_string();
    let error_log = log_dir.join("error.log").to_string_lossy().to_string();

    let root_config = XrayConfig {
        log: LogConfig {
            loglevel: settings.log_level.clone(),
            access: access_log,
            error: error_log,
        },
        api: ApiConfig {
            services: vec![
                "HandlerService".to_string(),
                "LoggerService".to_string(),
                "StatsService".to_string()
            ],
            tag: "api".to_string(),
        },
        stats: json!({}),
        policy: PolicyConfig {
            system: SystemPolicy {
                statsInboundUplink: true,
                statsInboundDownlink: true,
                statsOutboundUplink: true,
                statsOutboundDownlink: true,
            }
        },
        dns: dns_config,
        inbounds,
        outbounds,
        routing: RoutingConfig {
            domainStrategy: settings.domain_strategy.clone(),
            rules,
        },
    };

    serde_json::to_string_pretty(&root_config).unwrap_or_else(|_| "{}".to_string())
}
