use url::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
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

pub fn parse_vpn_key(url_str: &str) -> Result<ParsedKey, String> {
    if url_str.starts_with("vmess://") {
        return parse_vmess(url_str);
    }
    
    let parsed_url = Url::parse(url_str).map_err(|e| e.to_string())?;
    
    let protocol = match parsed_url.scheme() {
        "vless" => "VLESS",
        "trojan" => "Trojan",
        "ss" => "Shadowsocks",
        other => return Err(format!("Unsupported protocol: {other}")),
    };

    let uuid = parsed_url.username().to_string();
    let host = parsed_url.host_str().unwrap_or("").to_string();
    let port = parsed_url.port().unwrap_or(443);
    
    // Fragment is often used for the name in these URI schemes
    let name = parsed_url.fragment()
        .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
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

// vmess usually is base64 encoded JSON
fn parse_vmess(url_str: &str) -> Result<ParsedKey, String> {
    let base64_str = url_str.trim_start_matches("vmess://");
    use base64::{Engine as _, engine::general_purpose};
    
    let decoded = general_purpose::STANDARD.decode(base64_str).map_err(|e| format!("Base64 decode error: {e}"))?;
    let json_str = String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 sequence: {e}"))?;
    
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
        let name = json.get("ps").and_then(|v| v.as_str()).unwrap_or("VMess Key").to_string();
        let host = json.get("add").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let port = json.get("port").map(|v| {
            if v.is_string() {
                v.as_str().unwrap_or("443").parse::<u16>().unwrap_or(443)
            } else {
                v.as_u64().unwrap_or(443) as u16
            }
        }).unwrap_or(443);
        let uuid = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut query_params = HashMap::new();
        if let Some(obj) = json.as_object() {
            for (k, v) in obj {
                if k != "ps" && k != "add" && k != "port" && k != "id" {
                    let val_str = if v.is_string() { v.as_str().unwrap_or("").to_string() } else { v.to_string() };
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
    if parsed.protocol.to_lowercase() == "vmess" {
        let mut map = serde_json::Map::new();
        map.insert("v".to_string(), serde_json::Value::String("2".to_string()));
        map.insert("ps".to_string(), serde_json::Value::String(parsed.name.clone()));
        map.insert("add".to_string(), serde_json::Value::String(parsed.host.clone()));
        map.insert("port".to_string(), serde_json::Value::Number(serde_json::Number::from(parsed.port)));
        map.insert("id".to_string(), serde_json::Value::String(parsed.uuid.clone()));
        
        for (k, v) in &parsed.query_params {
            map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
        
        let json_str = serde_json::to_string(&map).unwrap_or_default();
        use base64::{Engine as _, engine::general_purpose};
        let encoded = general_purpose::STANDARD.encode(json_str);
        return format!("vmess://{encoded}");
    }

    let scheme = match parsed.protocol.to_lowercase().as_str() {
        "vless" => "vless",
        "trojan" => "trojan",
        "shadowsocks" | "ss" => "ss",
        _ => "unknown"
    };

    if let Ok(mut url) = Url::parse(&format!("{}://{}@{}:{}", scheme, parsed.uuid, parsed.host, parsed.port)) {
        if !parsed.query_params.is_empty() {
            let mut query = url.query_pairs_mut();
            for (k, v) in &parsed.query_params {
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
        assert_eq!(parsed.query_params.get("security").map(|s| s.as_str()), Some("reality"));
        assert_eq!(parsed.query_params.get("flow").map(|s| s.as_str()), Some("xtls-rprx-vision"));
    }

    #[test]
    fn test_parse_vmess_url() {
        // {"v":"2","ps":"VMess Key","add":"1.1.1.1","port":443,"id":"my-uuid","net":"ws"}
        // eyJ2IjoiMiIsInBzIjoiVk1lc3MgS2V5IiwiYWRkIjoiMS4xLjEuMSIsInBvcnQiOjQ0MywiaWQiOiJteS11dWlkIiwibmV0Ijoid3MifQ==
        let url = "vmess://eyJ2IjoiMiIsInBzIjoiVk1lc3MgS2V5IiwiYWRkIjoiMS4xLjEuMSIsInBvcnQiOjQ0MywiaWQiOiJteS11dWlkIiwibmV0Ijoid3MifQ==";
        let parsed = parse_vpn_key(url).expect("Should parse vmess base64 url");
        assert_eq!(parsed.protocol, "VMess");
        assert_eq!(parsed.name, "VMess Key");
        assert_eq!(parsed.host, "1.1.1.1");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.uuid, "my-uuid");
        assert_eq!(parsed.query_params.get("net").map(|s| s.as_str()), Some("ws"));
    }

    #[test]
    fn test_parse_vmess_valid() {
        // {"add":"127.0.0.1","port":443,"id":"my-uuid","ps":"TestKey"}
        let valid_base64 = "vmess://eyJhZGQiOiIxMjcuMC4wLjEiLCJwb3J0Ijo0NDMsImlkIjoibXktdXVpZCIsInBzIjoiVGVzdEtleSJ9";
        
        let res = parse_vmess(valid_base64);
        assert!(res.is_ok(), "Parser should accept valid Base64 Vmess");
        let key = res.unwrap();
        
        assert_eq!(key.name, "TestKey");
        assert_eq!(key.host, "127.0.0.1");
        assert_eq!(key.port, 443);
        assert_eq!(key.uuid, "my-uuid");
    }

    #[test]
    fn test_parse_invalid_base64() {
        let res = parse_vmess("vmess://!!!invalid&&&");
        assert!(res.is_err(), "Parser should return Err on invalid Base64");
    }

    #[test]
    fn test_parse_vpn_key_garbage() {
        assert!(parse_vpn_key("not_a_url_at_all").is_err());
        assert!(parse_vpn_key("http://google.com").is_err()); // Unsupported protocol
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
        assert_eq!(parsed.uuid, "Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 8388);
        assert_eq!(parsed.name, "MySS");
    }
}