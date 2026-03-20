use crate::settings::AppSettings;
use crate::domain::key_parser::ParsedKey;
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
    let mut actual_http_port = settings.http_port;
    if actual_http_port == settings.socks_port {
        actual_http_port += 1;
    }

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
            "port": actual_http_port,
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
    
    // Разбор параметров строки запроса для транспорта и безопасности
    let qp = &parsed_key.query_params;
    let net = qp.get("type").map(|s| s.as_str()).unwrap_or("tcp");
    let security = qp.get("security").map(|s| s.as_str()).unwrap_or("none");
    
    stream_settings["network"] = json!(net);
    stream_settings["security"] = json!(security);

    let default_chrome = "chrome".to_string();
    let default_host = parsed_key.host.clone();

    if security == "tls" {
        stream_settings["tlsSettings"] = json!({
            "serverName": qp.get("sni").unwrap_or(&default_host),
            "fingerprint": qp.get("fp").unwrap_or(&default_chrome),
            "alpn": qp.get("alpn").map(|s| s.split(',').collect::<Vec<&str>>()).unwrap_or_else(|| vec!["h2", "http/1.1"])
        });
    } else if security == "reality" {
        stream_settings["realitySettings"] = json!({
            "serverName": qp.get("sni").unwrap_or(&default_host),
            "fingerprint": qp.get("fp").unwrap_or(&default_chrome),
            "publicKey": qp.get("pbk").unwrap_or(&"".to_string()),
            "shortId": qp.get("sid").unwrap_or(&"".to_string()),
            "spiderX": qp.get("spx").unwrap_or(&"/".to_string()),
        });
    }

    // Специфичные настройки транспорта
    if net == "ws" {
        stream_settings["wsSettings"] = json!({
            "path": qp.get("path").unwrap_or(&"/".to_string()),
            "headers": {
                "Host": qp.get("host").unwrap_or(&default_host)
            }
        });
    } else if net == "grpc" {
        stream_settings["grpcSettings"] = json!({
            "serviceName": qp.get("serviceName").unwrap_or(&"".to_string()),
            "multiMode": qp.get("mode").map(|m| m == "multi").unwrap_or(false)
        });
    } else if net == "tcp" {
        // Settings TCP
    }

    // Mux и фрагментация
    let mut proxy_outbound = json!({
        "tag": "proxy",
        "protocol": parsed_key.protocol.to_lowercase(),
        "streamSettings": stream_settings
    });

    let is_vless_reality = parsed_key.protocol.to_lowercase() == "vless" && security == "reality";

    if settings.enable_mux && !is_vless_reality {
        proxy_outbound["mux"] = json!({
            "enabled": true,
            "concurrency": settings.mux_concurrency,
            "xudpConcurrency": settings.mux_concurrency,
            "xudpProxyUDP443": "reject"
        });
    } else if is_vless_reality {
        // Отключаем mux для VLESS+Reality или задаем низкую конкурентность для обхода ТСПУ
        proxy_outbound["mux"] = json!({
            "enabled": false,
            "concurrency": -1
        });
    }

    let outbound_settings = if parsed_key.protocol.to_lowercase() == "vless" {
        let default_flow = if security == "reality" {
            "xtls-rprx-vision".to_string()
        } else {
            "".to_string()
        };
        json!({
            "vnext": [{
                "address": parsed_key.host,
                "port": parsed_key.port,
                "users": [{
                    "id": parsed_key.uuid,
                    "encryption": "none",
                    "flow": qp.get("flow").unwrap_or(&default_flow)
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
        rules.push(json!({
            "type": "field",
            "outboundTag": "direct",
            "domain": ["geosite:private"]
        }));
    }

    // Обработка правил белого/черного списка с поддержкой geosite/geoip
    if settings.enable_routing {
        let mut domains = vec![];
        let mut ips = vec![];
        
        if settings.route_ru {
            domains.push("ext:geosite_ru.dat:ru".to_string());
            domains.push("geosite:ru".to_string());
            ips.push("ext:geoip_ru.dat:ru".to_string());
            ips.push("geoip:ru".to_string());
        }
        if settings.route_cn {
            domains.push("ext:geosite_cn.dat:cn".to_string());
            domains.push("geosite:cn".to_string());
            ips.push("ext:geoip_cn.dat:cn".to_string());
            ips.push("geoip:cn".to_string());
        }
        if settings.route_ir {
            domains.push("ext:geosite_ir.dat:ir".to_string());
            domains.push("geosite:ir".to_string());
            ips.push("ext:geoip_ir.dat:ir".to_string());
            ips.push("geoip:ir".to_string());
        }
        if settings.route_antifilter {
            domains.push("ext:geosite_antifilter.dat:antifilter".to_string());
            domains.push("geosite:antifilter".to_string());
        }

        if !domains.is_empty() || !ips.is_empty() {
            let target_tag = if settings.routing_mode == "proxy" { "proxy" } else { "direct" };
            let mut rule = json!({
                "type": "field",
                "outboundTag": target_tag,
            });
            
            if !domains.is_empty() {
                rule["domain"] = json!(domains);
            }
            if !ips.is_empty() {
                rule["ip"] = json!(ips);
            }
            rules.push(rule);
            
            // Если режим proxy (Включения), то ТОЛЬКО указанные домены/IP идут через proxy.
            // Остальной трафик должен идти напрямую (direct).
            if settings.routing_mode == "proxy" {
                rules.push(json!({
                    "type": "field",
                    "outboundTag": "direct",
                    "network": "tcp,udp"
                }));
            }
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
        // Примечание: Полная реализация fakedns требует настройки fakedns inbound/sniffing.
    }

    let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx").join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
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
            domainStrategy: if settings.disable_ipv6 { "UseIPv4".to_string() } else { settings.domain_strategy.clone() },
            rules,
        },
    };

    serde_json::to_string_pretty(&root_config).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;
    use crate::domain::key_parser::ParsedKey;
    use std::collections::HashMap;

    #[test]
    fn test_build_xray_config_generates_valid_json() {
        let key = ParsedKey {
            protocol: "VLESS".to_string(),
            name: "TestXray".to_string(),
            host: "example.com".to_string(),
            port: 443,
            uuid: "uuid-456".to_string(),
            query_params: HashMap::new(),
            raw_url: "vless://...".to_string(),
        };
        let mut settings = AppSettings::new();
        settings.socks_port = 1080;
        
        let json_str = build_xray_config(&key, &settings);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("Should be valid JSON for Xray");
        
        let proxy_outbound = parsed["outbounds"].as_array().unwrap().get(0).unwrap();
        assert_eq!(proxy_outbound["protocol"], "vless");
    }
}
