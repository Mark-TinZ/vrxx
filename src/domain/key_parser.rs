use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ParsedKey {
    pub protocol: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub uuid: String,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    pub raw_url: String,
}

impl ParsedKey {
    pub fn parse(url_str: &str) -> Result<Self, String> {
        parse_vpn_key(url_str)
    }
}

pub fn parse_vpn_key(url_str: &str) -> Result<ParsedKey, String> {
    let trimmed = url_str.trim();

    if trimmed.starts_with("vmess://") {
        return parse_vmess(trimmed);
    }
    if trimmed.starts_with("ss://") || trimmed.starts_with("shadowsocks://") {
        return parse_shadowsocks(trimmed);
    }

    let parsed_url = Url::parse(trimmed).map_err(|e| e.to_string())?;

    let protocol = match parsed_url.scheme() {
        "vless" => "VLESS",
        "trojan" => "Trojan",
        "hy2" | "hysteria2" => "Hysteria2",
        "tuic" => "TUIC",
        "wg" | "wireguard" => "WireGuard",
        other => return Err(format!("Unsupported protocol: {other}")),
    };

    let user_info = parsed_url.username().to_string();
    let password_info = parsed_url.password().unwrap_or("").to_string();

    let uuid = if !password_info.is_empty() {
        format!("{user_info}:{password_info}")
    } else {
        user_info
    };

    let host = parsed_url.host_str().unwrap_or("").to_string();
    let port = parsed_url.port().unwrap_or(443);

    let name = parsed_url
        .fragment()
        .map(|s| {
            percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .to_string()
        })
        .unwrap_or_else(|| format!("{host}:{port}"));

    let mut query_params = HashMap::new();
    for (k, v) in parsed_url.query_pairs() {
        query_params.insert(k.into_owned(), v.into_owned());
    }

    Ok(ParsedKey {
        protocol: protocol.to_string(),
        name,
        host,
        port,
        uuid,
        query_params,
        raw_url: url_str.to_string(),
    })
}

// Shadowsocks URI parser supporting SIP002 Base64 and plain text userinfo
fn parse_shadowsocks(url_str: &str) -> Result<ParsedKey, String> {
    use base64::{engine::general_purpose, Engine as _};

    let trimmed = url_str.trim();
    let without_prefix = trimmed
        .strip_prefix("shadowsocks://")
        .or_else(|| trimmed.strip_prefix("ss://"))
        .unwrap_or(trimmed);

    // Separate fragment (#name) if present
    let (main_part, fragment) = match without_prefix.find('#') {
        Some(idx) => (&without_prefix[..idx], Some(&without_prefix[idx + 1..])),
        None => (without_prefix, None),
    };

    let name = fragment
        .map(|s| {
            percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .to_string()
        })
        .unwrap_or_else(|| "Shadowsocks Key".to_string());

    // Separate query parameters (?param=val)
    let (server_user_part, query_part) = match main_part.find('?') {
        Some(idx) => (&main_part[..idx], Some(&main_part[idx + 1..])),
        None => (main_part, None),
    };

    let mut query_params = HashMap::new();
    if let Some(qp) = query_part {
        for pair in qp.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                let decoded_k = percent_encoding::percent_decode_str(k)
                    .decode_utf8_lossy()
                    .to_string();
                let decoded_v = percent_encoding::percent_decode_str(v)
                    .decode_utf8_lossy()
                    .to_string();
                query_params.insert(decoded_k, decoded_v);
            }
        }
    }

    // Check if userinfo and host:port are separated by '@'
    let (method_pass, host_port) = match server_user_part.rfind('@') {
        Some(idx) => {
            let user_info = &server_user_part[..idx];
            let hp = &server_user_part[idx + 1..];

            let decoded_user = if let Ok(decoded) = general_purpose::STANDARD.decode(user_info) {
                String::from_utf8(decoded).unwrap_or_else(|_| user_info.to_string())
            } else if let Ok(decoded) = general_purpose::URL_SAFE_NO_PAD.decode(user_info) {
                String::from_utf8(decoded).unwrap_or_else(|_| user_info.to_string())
            } else {
                user_info.to_string()
            };

            (decoded_user, hp.to_string())
        }
        None => {
            // Entire string before query could be base64-encoded method:pass@host:port
            if let Ok(decoded) = general_purpose::STANDARD.decode(server_user_part) {
                let dec_str = String::from_utf8(decoded).unwrap_or_default();
                if let Some(idx) = dec_str.rfind('@') {
                    (dec_str[..idx].to_string(), dec_str[idx + 1..].to_string())
                } else {
                    return Err("Invalid Shadowsocks URL format".to_string());
                }
            } else {
                return Err("Invalid Shadowsocks URL format".to_string());
            }
        }
    };

    let mut host = host_port.clone();
    let mut port = 8388;
    if let Some(idx) = host_port.rfind(':') {
        host = host_port[..idx].to_string();
        if let Ok(p) = host_port[idx + 1..].parse::<u16>() {
            port = p;
        }
    }

    let mut method = "2022-blake3-aes-128-gcm".to_string();
    let mut password = method_pass.clone();

    if let Some(idx) = method_pass.find(':') {
        method = method_pass[..idx].to_string();
        password = method_pass[idx + 1..].to_string();
    }

    query_params.insert("method".to_string(), method);

    Ok(ParsedKey {
        protocol: "Shadowsocks".to_string(),
        name,
        host,
        port,
        uuid: password,
        query_params,
        raw_url: url_str.to_string(),
    })
}

// vmess usually is base64 encoded JSON
fn parse_vmess(url_str: &str) -> Result<ParsedKey, String> {
    let base64_str = url_str.trim_start_matches("vmess://");
    use base64::{engine::general_purpose, Engine as _};

    let decoded = general_purpose::STANDARD
        .decode(base64_str)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(base64_str))
        .map_err(|e| format!("Base64 decode error: {e}"))?;
    let json_str =
        String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 sequence: {e}"))?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
        let name = json
            .get("ps")
            .and_then(|v| v.as_str())
            .unwrap_or("VMess Key")
            .to_string();
        let host = json
            .get("add")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let port = json
            .get("port")
            .map(|v| {
                if v.is_string() {
                    v.as_str().unwrap_or("443").parse::<u16>().unwrap_or(443)
                } else {
                    v.as_u64().unwrap_or(443) as u16
                }
            })
            .unwrap_or(443);
        let uuid = json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut query_params = HashMap::new();
        if let Some(obj) = json.as_object() {
            for (k, v) in obj {
                if k != "ps" && k != "add" && k != "port" && k != "id" {
                    let val_str = if v.is_string() {
                        v.as_str().unwrap_or("").to_string()
                    } else {
                        v.to_string()
                    };
                    query_params.insert(k.clone(), val_str);
                }
            }
        }

        Ok(ParsedKey {
            protocol: "VMess".to_string(),
            name,
            host,
            port,
            uuid,
            query_params,
            raw_url: url_str.to_string(),
        })
    } else {
        Err("Invalid VMess JSON format".to_string())
    }
}

pub fn build_vpn_key(parsed: &ParsedKey) -> String {
    let proto_lower = parsed.protocol.to_lowercase();
    if proto_lower == "vmess" {
        let mut map = serde_json::Map::new();
        map.insert("v".to_string(), serde_json::Value::String("2".to_string()));
        map.insert(
            "ps".to_string(),
            serde_json::Value::String(parsed.name.clone()),
        );
        map.insert(
            "add".to_string(),
            serde_json::Value::String(parsed.host.clone()),
        );
        map.insert(
            "port".to_string(),
            serde_json::Value::Number(serde_json::Number::from(parsed.port)),
        );
        map.insert(
            "id".to_string(),
            serde_json::Value::String(parsed.uuid.clone()),
        );

        for (k, v) in &parsed.query_params {
            map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        let json_str = serde_json::to_string(&map).unwrap_or_default();
        use base64::{engine::general_purpose, Engine as _};
        let encoded = general_purpose::STANDARD.encode(json_str);
        return format!("vmess://{encoded}");
    }

    let scheme = match proto_lower.as_str() {
        "vless" => "vless",
        "trojan" => "trojan",
        "shadowsocks" | "ss" => "ss",
        "hysteria2" | "hy2" => "hy2",
        "tuic" => "tuic",
        "wireguard" | "wg" => "wg",
        _ => "unknown",
    };

    let userinfo = if scheme == "ss" {
        let method = parsed
            .query_params
            .get("method")
            .cloned()
            .unwrap_or_else(|| "2022-blake3-aes-128-gcm".to_string());
        use base64::{engine::general_purpose, Engine as _};
        general_purpose::STANDARD.encode(format!("{}:{}", method, parsed.uuid))
    } else {
        parsed.uuid.clone()
    };

    if let Ok(mut url) = Url::parse(&format!(
        "{}://{}@{}:{}",
        scheme, userinfo, parsed.host, parsed.port
    )) {
        if !parsed.query_params.is_empty() {
            let mut query = url.query_pairs_mut();
            for (k, v) in &parsed.query_params {
                if scheme == "ss" && k == "method" {
                    continue;
                }
                query.append_pair(k, v);
            }
        }

        url.set_fragment(Some(&parsed.name));
        url.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vless_reality_url() {
        let url = "vless://a3482e88-6860-4a1c-914c-4b4ea5c49f87@1.2.3.4:443?security=reality&sni=google.com&fp=chrome&pbk=pubkey123&sid=shortid&type=tcp&flow=xtls-rprx-vision#MyVLESS";
        let parsed = parse_vpn_key(url).expect("Should parse vless reality url");
        assert_eq!(parsed.protocol, "VLESS");
        assert_eq!(parsed.uuid, "a3482e88-6860-4a1c-914c-4b4ea5c49f87");
        assert_eq!(parsed.host, "1.2.3.4");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.name, "MyVLESS");
        assert_eq!(
            parsed.query_params.get("security").map(|s| s.as_str()),
            Some("reality")
        );
        assert_eq!(
            parsed.query_params.get("flow").map(|s| s.as_str()),
            Some("xtls-rprx-vision")
        );
    }

    #[test]
    fn test_parse_vmess_url() {
        let url = "vmess://eyJ2IjoiMiIsInBzIjoiVk1lc3MgS2V5IiwiYWRkIjoiMS4xLjEuMSIsInBvcnQiOjQ0MywiaWQiOiJteS11dWlkIiwibmV0Ijoid3MifQ==";
        let parsed = parse_vpn_key(url).expect("Should parse vmess base64 url");
        assert_eq!(parsed.protocol, "VMess");
        assert_eq!(parsed.name, "VMess Key");
        assert_eq!(parsed.host, "1.1.1.1");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.uuid, "my-uuid");
        assert_eq!(
            parsed.query_params.get("net").map(|s| s.as_str()),
            Some("ws")
        );
    }

    #[test]
    fn test_parse_trojan_url() {
        let url = "trojan://mypassword@example.com:443?security=tls&sni=example.com#MyTrojan";
        let parsed = parse_vpn_key(url).expect("Should parse trojan url");
        assert_eq!(parsed.protocol, "Trojan");
        assert_eq!(parsed.uuid, "mypassword");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.name, "MyTrojan");
    }

    #[test]
    fn test_parse_shadowsocks_url() {
        let url = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@example.com:8388#MySS";
        let parsed = parse_vpn_key(url).expect("Should parse ss url");
        assert_eq!(parsed.protocol, "Shadowsocks");
        assert_eq!(parsed.uuid, "password");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 8388);
        assert_eq!(parsed.name, "MySS");
        assert_eq!(
            parsed.query_params.get("method").map(|s| s.as_str()),
            Some("chacha20-ietf-poly1305")
        );
    }

    #[test]
    fn test_parse_hysteria2_url() {
        let url = "hy2://mypassword@hy2.example.com:8443?sni=hy2.example.com&obfs=salamander&obfs-password=123456&up=100&down=500#MyHy2";
        let parsed = parse_vpn_key(url).expect("Should parse hysteria2 url");
        assert_eq!(parsed.protocol, "Hysteria2");
        assert_eq!(parsed.uuid, "mypassword");
        assert_eq!(parsed.host, "hy2.example.com");
        assert_eq!(parsed.port, 8443);
        assert_eq!(parsed.name, "MyHy2");
        assert_eq!(
            parsed.query_params.get("obfs").map(|s| s.as_str()),
            Some("salamander")
        );
        assert_eq!(
            parsed.query_params.get("up").map(|s| s.as_str()),
            Some("100")
        );
    }

    #[test]
    fn test_parse_tuic_url() {
        let url = "tuic://my-uuid:my-pass@tuic.example.com:8443?congestion_control=bbr&udp_relay_mode=native&sni=tuic.example.com#MyTUIC";
        let parsed = parse_vpn_key(url).expect("Should parse tuic url");
        assert_eq!(parsed.protocol, "TUIC");
        assert_eq!(parsed.uuid, "my-uuid:my-pass");
        assert_eq!(parsed.host, "tuic.example.com");
        assert_eq!(parsed.port, 8443);
        assert_eq!(parsed.name, "MyTUIC");
        assert_eq!(
            parsed
                .query_params
                .get("congestion_control")
                .map(|s| s.as_str()),
            Some("bbr")
        );
    }

    #[test]
    fn test_parse_wireguard_url() {
        let url =
            "wg://my-priv-key@wg.example.com:51820?public_key=peer_pub_key&ip=10.0.0.2/32#MyWG";
        let parsed = parse_vpn_key(url).expect("Should parse wireguard url");
        assert_eq!(parsed.protocol, "WireGuard");
        assert_eq!(parsed.uuid, "my-priv-key");
        assert_eq!(parsed.host, "wg.example.com");
        assert_eq!(parsed.port, 51820);
        assert_eq!(parsed.name, "MyWG");
        assert_eq!(
            parsed.query_params.get("public_key").map(|s| s.as_str()),
            Some("peer_pub_key")
        );
    }

    #[test]
    fn test_build_vpn_key_roundtrip() {
        let url = "vless://a3482e88-6860-4a1c-914c-4b4ea5c49f87@1.2.3.4:443?security=reality&sni=google.com#MyVLESS";
        let parsed = parse_vpn_key(url).expect("Should parse vless url");
        let built = build_vpn_key(&parsed);
        assert!(built.contains("vless://a3482e88-6860-4a1c-914c-4b4ea5c49f87@1.2.3.4:443"));
        assert!(built.contains("MyVLESS"));
    }
}
