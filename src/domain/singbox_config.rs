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

/// Генерирует JSON-конфигурацию для sing-box на основе выбранного ключа и настроек приложения.
///
/// Основные возможности:
/// - Поддержка протоколов VLESS, VMess, Trojan.
/// - Настройка TUN-интерфейса с автоматической маршрутизацией.
/// - Региональная маршрутизация (RU, CN, IR) через SRS-файлы.
/// - Блокировка IPv6 и рекламы.
/// - Тултипы и сниффинг трафика.
pub fn build_singbox_config(parsed_key: &ParsedKey, settings: &AppSettings) -> String {
    // Гарантируем, что HTTP и SOCKS порты не совпадают.
    let mut actual_http_port = settings.http_port;
    if actual_http_port == settings.socks_port {
        actual_http_port += 1;
    }

    let sb_version = get_singbox_version();
    // Начиная с версии 1.11 sing-box изменил механизм сниффинга.
    let is_1_11_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 11);
    // Версия 1.12 принесла изменения в DNS и domain_resolver.
    let is_1_12_or_newer = sb_version.0 > 1 || (sb_version.0 == 1 && sb_version.1 >= 12);

    // Настройка входящих соединений (Inbounds).
    let mut socks_inbound = json!({
        "type": "socks",
        "tag": "socks-in",
        "listen": if settings.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
        "listen_port": settings.socks_port,
    });

    // Для старых версий сниффинг настраивается во входящем соединении.
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

    // Настройка TUN интерфейса, если режим включен.
    if settings.tun_mode {
        let mut tun_inbound = json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "vrxx-tun",
            "address": [
                "172.19.0.1/30",
                "fdfe:dcba:9876::1/126"
            ],
            "auto_route": true, // Позволяет демону автоматически настраивать маршруты.
            "strict_route": true, // Предотвращает утечки трафика вне туннеля.
            "stack": "gvisor",
        });

        if !is_1_11_or_newer {
            tun_inbound["sniff"] = json!(settings.enable_sniffing);
            tun_inbound["sniff_override_destination"] = json!(settings.enable_sniffing);
        }

        inbounds.push(tun_inbound);
    }

    // Настройка основного исходящего соединения (Proxy Outbound).
    let qp = &parsed_key.query_params;
    let security = qp.get("security").map(|s| s.as_str()).unwrap_or("none");
    let net = qp.get("type").map(|s| s.as_str()).unwrap_or("tcp");

    let mut proxy_outbound = json!({
        "type": parsed_key.protocol.to_lowercase(),
        "tag": "proxy",
        "server": parsed_key.host,
        "server_port": parsed_key.port,
    });

    // Для sing-box 1.12+ указываем удаленный DNS для резолва домена сервера.
    if is_1_12_or_newer && std::net::IpAddr::from_str(&parsed_key.host).is_err() {
        proxy_outbound["domain_resolver"] = json!("remote-dns");
    }

    // Специфичные настройки для VLESS/VMess/Trojan.
    if parsed_key.protocol.to_lowercase() == "vless"
        || parsed_key.protocol.to_lowercase() == "vmess"
    {
        proxy_outbound["uuid"] = json!(parsed_key.uuid);
        if parsed_key.protocol.to_lowercase() == "vmess" {
            proxy_outbound["alter_id"] = json!(0);
            proxy_outbound["security"] = json!("auto");
            proxy_outbound["packet_encoding"] = json!("xudp");
        } else {
            let flow = qp.get("flow").map(|s| s.as_str()).unwrap_or("");
            if !flow.is_empty() {
                proxy_outbound["flow"] = json!(flow);
            }
        }
    } else if parsed_key.protocol.to_lowercase() == "trojan" {
        proxy_outbound["password"] = json!(parsed_key.uuid);
    }

    // Настройка TLS (TLS, Reality).
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

    // Настройка транспорта (gRPC, WebSocket).
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

    // Мультиплексирование.
    if settings.enable_mux && security != "reality" {
        proxy_outbound["multiplex"] = json!({
            "enabled": true,
            "protocol": "smux"
        });
    }

    if !is_1_12_or_newer && settings.disable_ipv6 {
        proxy_outbound["domain_strategy"] = json!("ipv4_only");
    }

    // --- Раздел: Правила маршрутизации ---
    let mut rules = vec![];

    // Сниффинг в новых версиях настраивается через правила.
    if is_1_11_or_newer && settings.enable_sniffing {
        rules.push(json!({
            "action": "sniff"
        }));
    }

    // Блокировка всего IPv6 трафика.
    if settings.disable_ipv6 {
        rules.push(json!({
            "ip_cidr": ["::/0"],
            "outbound": "block"
        }));
    }

    // Обход локальной сети (LAN).
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

    // Блокировка рекламы через удаленные наборы правил.
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

    // Региональные правила (Россия, Китай, Иран).
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

        // Пользовательские правила (домены, IP, SRS).
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

    // Настройка исходящих соединений (Outbounds).
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

    // Конфигурация маршрутизатора (Route).
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

    // Настройка DNS.
    let remote_dns = json!({
        "tag": "remote-dns",
        "type": "https",
        "server": "1.1.1.1",
        "detour": "proxy" // DNS-запросы идут через прокси для предотвращения утечек.
    });

    let local_dns = json!({
        "tag": "local-dns",
        "type": "local",
        "detour": "direct"
    });

    let mut dns_rules = vec![];

    if is_1_12_or_newer && settings.disable_ipv6 {
        // Отклоняем AAAA запросы в новых версиях.
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

    // Сборка финального корневого объекта JSON.
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
