/* tests/challenger_m1_protocol_stress.rs
 *
 * EMPIRICAL CHALLENGER TEST SUITE: Milestone M1 (Core Protocol & Config Inversion)
 * Adversarial stress testing, boundary condition verification, and corrupt payload fuzzing
 * for vrxx::protocol and IPC payload handling.
 */

use std::collections::HashMap;
use std::sync::Arc;
use vrxx::daemon::api::{create_router, ApiState};
use vrxx::daemon::core::ProxyManager;
use vrxx::daemon::events::EventManager;
use vrxx::domain::key_parser::{is_valid_uuid, KeyValidationError, ParsedKey};
use vrxx::protocol::{
    ConfigValidationError, CoreType, DaemonConfigOptions, DaemonStatus, StartProxyRequest,
};
use vrxx::settings::RoutingRule;

// =========================================================================
// Category A: Port Boundary Values & Port Collisions
// =========================================================================

#[test]
fn test_port_boundaries_zero_socks_port() {
    let opts = DaemonConfigOptions {
        socks_port: 0,
        http_port: 10809,
        ..Default::default()
    };
    let err = opts.validate().unwrap_err();
    assert_eq!(err, ConfigValidationError::ZeroSocksPort);
    assert_eq!(err.to_string(), "SOCKS port cannot be 0");
}

#[test]
fn test_port_boundaries_zero_http_port() {
    let opts = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 0,
        ..Default::default()
    };
    let err = opts.validate().unwrap_err();
    assert_eq!(err, ConfigValidationError::ZeroHttpPort);
    assert_eq!(err.to_string(), "HTTP port cannot be 0");
}

#[test]
fn test_port_boundaries_both_zero_ports() {
    let opts = DaemonConfigOptions {
        socks_port: 0,
        http_port: 0,
        ..Default::default()
    };
    // socks_port is checked first
    let err = opts.validate().unwrap_err();
    assert_eq!(err, ConfigValidationError::ZeroSocksPort);
}

#[test]
fn test_port_boundaries_min_valid_ports() {
    let opts = DaemonConfigOptions {
        socks_port: 1,
        http_port: 2,
        ..Default::default()
    };
    assert!(opts.validate().is_ok(), "Ports 1 and 2 must be valid");
}

#[test]
fn test_port_boundaries_max_valid_ports() {
    let opts = DaemonConfigOptions {
        socks_port: 65534,
        http_port: 65535,
        ..Default::default()
    };
    assert!(
        opts.validate().is_ok(),
        "Ports 65534 and 65535 must be valid"
    );
}

#[test]
fn test_port_collisions_default_ports() {
    let opts = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 10808,
        ..Default::default()
    };
    let err = opts.validate().unwrap_err();
    assert_eq!(
        err,
        ConfigValidationError::PortConflict {
            socks: 10808,
            http: 10808
        }
    );
    assert!(err.to_string().contains("cannot be identical"));
}

#[test]
fn test_port_collisions_min_and_max_extremes() {
    // Port 1 collision
    let opts_min = DaemonConfigOptions {
        socks_port: 1,
        http_port: 1,
        ..Default::default()
    };
    assert_eq!(
        opts_min.validate().unwrap_err(),
        ConfigValidationError::PortConflict { socks: 1, http: 1 }
    );

    // Port 65535 collision
    let opts_max = DaemonConfigOptions {
        socks_port: 65535,
        http_port: 65535,
        ..Default::default()
    };
    assert_eq!(
        opts_max.validate().unwrap_err(),
        ConfigValidationError::PortConflict {
            socks: 65535,
            http: 65535
        }
    );
}

#[test]
fn test_parsed_key_port_boundaries() {
    let mut key = ParsedKey {
        protocol: "VLESS".to_string(),
        name: "test".to_string(),
        host: "1.2.3.4".to_string(),
        port: 0,
        uuid: "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0".to_string(),
        query_params: HashMap::new(),
        raw_url: String::new(),
    };
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::InvalidPort(0)
    );

    // Port 1 (min valid)
    key.port = 1;
    assert!(key.validate().is_ok());

    // Port 65535 (max valid)
    key.port = 65535;
    assert!(key.validate().is_ok());
}

// =========================================================================
// Category B: Corrupt / Malformed UUIDs & Cryptographic Credentials
// =========================================================================

#[test]
fn test_adversarial_uuid_validation() {
    // Standard RFC 4122 (36 chars)
    assert!(is_valid_uuid("a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0"));
    assert!(is_valid_uuid("A0A0A0A0-B1B1-C2C2-D3D3-E4E4E4E4E4E4"));

    // Standard 32-hex format
    assert!(is_valid_uuid("a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0"));
    assert!(is_valid_uuid("A0A0A0A0B1B1C2C2D3D3E4E4E4E4E4E4"));

    // Edge cases and corrupt variants
    let corrupt_uuids = [
        "",                                                                     // Empty string
        "   ",                                                                  // Whitespace
        "not-a-valid-uuid",                                                     // Non-hex words
        "g0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0",                                 // 'g' is not hex
        "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a", // 35 chars (one short)
        "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0a", // 37 chars (one extra)
        "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0",      // 31 hex chars
        "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",   // 33 hex chars
        "a0a0a0a0_a0a0_a0a0_a0a0_a0a0a0a0a0a0", // Underscores instead of dashes
        "a0a0a0a0:a0a0:a0a0:a0a0:a0a0a0a0a0a0", // Colons instead of dashes
        "a0a0a0a-0a0a0-a0a0-a0a0-a0a0a0a0a0a0", // Dash at wrong index 7 instead of 8
        "a0a0a0a0-a0a00a0a0-a0a0-a0a0a0a0a0a0", // Missing dash at 13
        "a0a0a0a0-a0a0-a0a00a0a0-a0a0a0a0a0a0", // Missing dash at 18
        "a0a0a0a0-a0a0-a0a0-a0a00a0a0a0a0a0a0", // Missing dash at 23
        "а0а0а0а0-а0а0-а0а0-а0а0-а0а0а0а0а0а0", // Cyrillic lookalike 'а' (UTF-8 multi-byte)
        "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0🦀",  // Multi-byte emoji at end
        "'; DROP TABLE vpn_keys; --",          // SQL injection attempt
        "$(reboot)",                           // Command injection attempt
        "\0\0\0\0\0\0\0\0-\0\0\0\0-\0\0\0\0-\0\0\0\0-\0\0\0\0\0\0\0\0\0\0\0\0", // Null bytes
    ];

    for &corrupt in &corrupt_uuids {
        assert!(
            !is_valid_uuid(corrupt),
            "Corrupt UUID '{corrupt}' should fail is_valid_uuid"
        );

        let key = ParsedKey {
            protocol: "VLESS".to_string(),
            name: "corrupt".to_string(),
            host: "1.2.3.4".to_string(),
            port: 443,
            uuid: corrupt.to_string(),
            query_params: HashMap::new(),
            raw_url: String::new(),
        };
        let res = key.validate();
        assert!(
            matches!(res, Err(KeyValidationError::InvalidUuid(_))),
            "Key with corrupt UUID '{corrupt}' must yield KeyValidationError::InvalidUuid"
        );
    }
}

#[test]
fn test_adversarial_reality_credentials() {
    let mut qp = HashMap::new();
    qp.insert("security".to_string(), "reality".to_string());
    // Missing pbk and sni
    let mut key = ParsedKey {
        protocol: "VLESS".to_string(),
        name: "test".to_string(),
        host: "1.2.3.4".to_string(),
        port: 443,
        uuid: "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0".to_string(),
        query_params: qp,
        raw_url: String::new(),
    };

    // Missing pbk
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::InvalidRealityPublicKey
    );

    // Empty pbk
    key.query_params.insert("pbk".to_string(), "".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::InvalidRealityPublicKey
    );

    // Whitespace pbk
    key.query_params
        .insert("pbk".to_string(), "   \t\n".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::InvalidRealityPublicKey
    );

    // Valid pbk, but missing sni
    key.query_params.insert(
        "pbk".to_string(),
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
    );
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingRealitySni
    );

    // Empty sni
    key.query_params.insert("sni".to_string(), "".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingRealitySni
    );

    // Whitespace sni
    key.query_params
        .insert("sni".to_string(), "   ".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingRealitySni
    );

    // Valid pbk and valid sni
    key.query_params
        .insert("sni".to_string(), "example.com".to_string());
    assert!(key.validate().is_ok());
}

#[test]
fn test_adversarial_wireguard_credentials() {
    let qp = HashMap::new();
    let mut key = ParsedKey {
        protocol: "WireGuard".to_string(),
        name: "wg-test".to_string(),
        host: "1.2.3.4".to_string(),
        port: 51820,
        uuid: String::new(), // Empty private key
        query_params: qp,
        raw_url: String::new(),
    };

    // Empty uuid (private key)
    assert!(matches!(
        key.validate().unwrap_err(),
        KeyValidationError::EmptyCredentials(_)
    ));

    // Whitespace uuid
    key.uuid = "   \t ".to_string();
    assert!(matches!(
        key.validate().unwrap_err(),
        KeyValidationError::EmptyCredentials(_)
    ));

    // Valid uuid, but missing public_key in query_params
    key.uuid = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=".to_string();
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingWireGuardPublicKey
    );

    // Empty public_key
    key.query_params
        .insert("public_key".to_string(), "".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingWireGuardPublicKey
    );

    // Whitespace public_key
    key.query_params
        .insert("public_key".to_string(), "   ".to_string());
    assert_eq!(
        key.validate().unwrap_err(),
        KeyValidationError::MissingWireGuardPublicKey
    );

    // Valid public_key
    key.query_params.insert(
        "public_key".to_string(),
        "YmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmI=".to_string(),
    );
    assert!(key.validate().is_ok());
}

#[test]
fn test_adversarial_trojan_shadowsocks_credentials() {
    for proto in &["Trojan", "Shadowsocks", "Hysteria2", "TUIC"] {
        let mut key = ParsedKey {
            protocol: proto.to_string(),
            name: "test".to_string(),
            host: "1.2.3.4".to_string(),
            port: 443,
            uuid: String::new(),
            query_params: HashMap::new(),
            raw_url: String::new(),
        };

        // Empty password/token
        assert!(matches!(
            key.validate().unwrap_err(),
            KeyValidationError::EmptyCredentials(_)
        ));

        // Whitespace password/token
        key.uuid = "   \n\t ".to_string();
        assert!(matches!(
            key.validate().unwrap_err(),
            KeyValidationError::EmptyCredentials(_)
        ));

        // Valid password
        key.uuid = "super-secret-password".to_string();
        assert!(key.validate().is_ok());
    }
}

// =========================================================================
// Category C: Multiplex Concurrency Boundaries (< 1, > 128)
// =========================================================================

#[test]
fn test_mux_concurrency_boundaries_when_enabled() {
    let test_cases = [
        (0, false),
        (-1, false),
        (-100, false),
        (i32::MIN, false),
        (1, true), // Min allowed
        (2, true),
        (8, true), // Default
        (64, true),
        (128, true),  // Max allowed
        (129, false), // Boundary fail
        (130, false),
        (1000, false),
        (i32::MAX, false),
    ];

    for (concurrency, should_pass) in test_cases {
        let opts = DaemonConfigOptions {
            enable_mux: true,
            mux_concurrency: concurrency,
            ..Default::default()
        };
        let res = opts.validate();
        if should_pass {
            assert!(
                res.is_ok(),
                "Concurrency {concurrency} should be valid when mux is enabled"
            );
        } else {
            assert!(
                matches!(res, Err(ConfigValidationError::InvalidMuxConcurrency { .. })),
                "Concurrency {concurrency} should fail with InvalidMuxConcurrency when mux is enabled"
            );
            if let Err(ConfigValidationError::InvalidMuxConcurrency { concurrency: c }) = res {
                assert_eq!(c, concurrency);
            }
        }
    }
}

#[test]
fn test_mux_concurrency_when_disabled_is_bypassed() {
    // When enable_mux is false, concurrency value is ignored
    for concurrency in [0, -1, 129, i32::MIN, i32::MAX] {
        let opts = DaemonConfigOptions {
            enable_mux: false,
            mux_concurrency: concurrency,
            ..Default::default()
        };
        assert!(
            opts.validate().is_ok(),
            "When enable_mux is false, concurrency {concurrency} should be bypassed"
        );
    }
}

// =========================================================================
// Category D: Empty or Invalid Routing Rule Actions
// =========================================================================

#[test]
fn test_routing_rules_empty_or_whitespace_value() {
    let empty_values = ["", "   ", "\t\r\n", " "];

    for &val in &empty_values {
        let rule = RoutingRule {
            name: "empty-val-rule".to_string(),
            type_: "domain".to_string(),
            value: val.to_string(),
            action: "proxy".to_string(),
        };
        let opts = DaemonConfigOptions {
            routing_rules: vec![rule],
            ..Default::default()
        };
        let err = opts.validate().unwrap_err();
        assert_eq!(
            err,
            ConfigValidationError::EmptyRuleValue {
                name: "empty-val-rule".to_string()
            }
        );
        assert!(err.to_string().contains("contains empty value"));
    }
}

#[test]
fn test_routing_rules_invalid_actions() {
    let invalid_actions = [
        "",
        "   ",
        "drop",
        "reject",
        "allow",
        "forward",
        "drop_packet",
        "deny",
        "pass",
        "123",
        "proxy!",
        "direct;rm -rf /",
    ];

    for &act in &invalid_actions {
        let rule = RoutingRule {
            name: format!("rule-{act}"),
            type_: "domain".to_string(),
            value: "example.com".to_string(),
            action: act.to_string(),
        };
        let opts = DaemonConfigOptions {
            routing_rules: vec![rule],
            ..Default::default()
        };
        let err = opts.validate().unwrap_err();
        assert_eq!(
            err,
            ConfigValidationError::InvalidRuleAction {
                name: format!("rule-{act}"),
                action: act.to_string(),
            }
        );
        assert!(err.to_string().contains("invalid action"));
    }
}

#[test]
fn test_routing_rules_valid_actions_case_insensitivity() {
    let valid_actions = [
        "proxy",
        "direct",
        "block",
        "Proxy",
        "PROXY",
        "Direct",
        "DIRECT",
        "Block",
        "BLOCK",
        "  proxy  ",
        " direct\t",
    ];

    for &act in &valid_actions {
        let rule = RoutingRule {
            name: "valid-action-rule".to_string(),
            type_: "domain".to_string(),
            value: "example.com".to_string(),
            action: act.to_string(),
        };
        let opts = DaemonConfigOptions {
            routing_rules: vec![rule],
            ..Default::default()
        };
        assert!(
            opts.validate().is_ok(),
            "Action '{act}' should pass case-insensitive validation"
        );
    }
}

#[test]
fn test_routing_rules_multiple_rules_fail_fast() {
    let valid_rule = RoutingRule {
        name: "valid-rule".to_string(),
        type_: "domain".to_string(),
        value: "valid.com".to_string(),
        action: "proxy".to_string(),
    };
    let invalid_rule = RoutingRule {
        name: "bad-rule".to_string(),
        type_: "ip_cidr".to_string(),
        value: "10.0.0.0/8".to_string(),
        action: "unsupported".to_string(),
    };

    let opts = DaemonConfigOptions {
        routing_rules: vec![valid_rule, invalid_rule],
        ..Default::default()
    };
    let err = opts.validate().unwrap_err();
    assert_eq!(
        err,
        ConfigValidationError::InvalidRuleAction {
            name: "bad-rule".to_string(),
            action: "unsupported".to_string(),
        }
    );
}

// =========================================================================
// Category E: IPC Payload Serialization & Deserialization Stress
// =========================================================================

#[test]
fn test_start_proxy_request_corrupt_json_payloads() {
    // Malformed JSON strings
    let malformed_payloads = [
        "",
        "{",
        "{}}",
        "null",
        "123",
        "\"a string\"",
        "{\"core_type\": }",
        "{\"core_type\": \"sing-box\", \"profile\": {",
    ];

    for &payload in &malformed_payloads {
        let res = serde_json::from_str::<StartProxyRequest>(payload);
        assert!(
            res.is_err(),
            "Malformed payload '{payload}' must fail deserialization cleanly without panics"
        );
    }
}

#[test]
fn test_start_proxy_request_type_mismatches() {
    // Missing required profile
    let missing_profile = r#"{"core_type": "sing-box"}"#;
    assert!(serde_json::from_str::<StartProxyRequest>(missing_profile).is_err());

    // Integer given for string field in profile
    let int_host = r#"{
        "profile": {
            "protocol": "VLESS",
            "name": "test",
            "host": 12345,
            "port": 443,
            "uuid": "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0"
        }
    }"#;
    assert!(serde_json::from_str::<StartProxyRequest>(int_host).is_err());

    // String given for port
    let string_port = r#"{
        "profile": {
            "protocol": "VLESS",
            "name": "test",
            "host": "1.2.3.4",
            "port": "not-a-port",
            "uuid": "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0"
        }
    }"#;
    assert!(serde_json::from_str::<StartProxyRequest>(string_port).is_err());

    // Port exceeds u16 limit (70000)
    let overflow_port = r#"{
        "profile": {
            "protocol": "VLESS",
            "name": "test",
            "host": "1.2.3.4",
            "port": 70000,
            "uuid": "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0"
        }
    }"#;
    assert!(serde_json::from_str::<StartProxyRequest>(overflow_port).is_err());
}

#[test]
fn test_start_proxy_request_extra_fields_tolerance() {
    // Forward-compatibility: unknown fields should be ignored gracefully
    let extra_fields_json = r#"{
        "core_type": "sing-box",
        "profile": {
            "protocol": "VLESS",
            "name": "test",
            "host": "1.2.3.4",
            "port": 443,
            "uuid": "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0",
            "raw_url": "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@1.2.3.4:443",
            "extra_unrecognized_profile_field": 42
        },
        "options": {
            "socks_port": 10808,
            "http_port": 10809,
            "extra_future_option": "future_feature"
        },
        "completely_unknown_top_level_field": [1, 2, 3]
    }"#;

    let res = serde_json::from_str::<StartProxyRequest>(extra_fields_json);
    assert!(
        res.is_ok(),
        "Extra unknown fields should not break deserialization: {:?}",
        res.err()
    );
    let req = res.unwrap();
    assert_eq!(req.core_type, CoreType::SingBox);
    assert_eq!(req.profile.host, "1.2.3.4");
    assert_eq!(req.options.socks_port, 10808);
}

#[test]
fn test_parsed_key_omitted_raw_url_fails_deserialization() {
    // Empirical observation: ParsedKey requires raw_url because #[serde(default)] is missing on raw_url.
    let json_missing_raw_url = r#"{
        "core_type": "sing-box",
        "profile": {
            "protocol": "VLESS",
            "name": "test",
            "host": "1.2.3.4",
            "port": 443,
            "uuid": "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0"
        }
    }"#;

    let res = serde_json::from_str::<StartProxyRequest>(json_missing_raw_url);
    assert!(
        res.is_err(),
        "Omitting raw_url fails deserialization cleanly with missing field error"
    );
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("missing field `raw_url`"),
        "Expected missing field `raw_url`, got: {err_str}"
    );
}

#[test]
fn test_large_payload_stress() {
    // Generate 1,000 routing rules to stress-test deserialization and validation
    let mut rules = Vec::with_capacity(1000);
    for i in 0..1000 {
        rules.push(RoutingRule {
            name: format!("rule-{i}"),
            type_: "domain".to_string(),
            value: format!("sub-{i}.domain.example.com"),
            action: if i % 2 == 0 { "proxy" } else { "direct" }.to_string(),
        });
    }

    let opts = DaemonConfigOptions {
        routing_rules: rules,
        ..Default::default()
    };

    assert!(opts.validate().is_ok());

    let json_str = serde_json::to_string(&opts).expect("Failed to serialize large options");
    let deserialized: DaemonConfigOptions =
        serde_json::from_str(&json_str).expect("Failed to deserialize large options");
    assert_eq!(deserialized.routing_rules.len(), 1000);
}

// =========================================================================
// Category F: End-to-End Daemon REST API Stress Verification
// =========================================================================

#[tokio::test]
async fn test_daemon_rest_api_validation_rejections() {
    let (event_manager, _) = EventManager::new(50);
    let event_manager = Arc::new(event_manager);
    let proxy_manager = Arc::new(ProxyManager::new(event_manager.clone()));
    let state = Arc::new(ApiState {
        proxy_manager: proxy_manager.clone(),
        event_manager: event_manager.clone(),
    });

    let app = create_router(state);
    let self_uid = nix::unistd::getuid().as_raw();
    let auth_id = vrxx::daemon::auth::CallerIdentity {
        is_authenticated: true,
        uid: self_uid,
        gid: nix::unistd::getgid().as_raw(),
        pid: None,
    };
    let app = app.layer(axum::middleware::from_fn(
        move |mut req: axum::extract::Request, next: axum::middleware::Next| {
            let id = auth_id.clone();
            async move {
                req.extensions_mut().insert(axum::extract::ConnectInfo(id));
                next.run(req).await
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind TCP listener for test");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let start_url = format!("http://127.0.0.1:{port}/api/proxy/start");

    // 1. Corrupt JSON body syntax -> must return 400 Bad Request or 422 Unprocessable Entity
    let res = client
        .post(&start_url)
        .header("Content-Type", "application/json")
        .body("{\"core_type\": invalid_json")
        .send()
        .await
        .unwrap();
    assert!(
        res.status().is_client_error(),
        "Corrupt JSON syntax must be rejected with 4xx"
    );

    // 2. Tier 1 Profile Validation Failure (corrupt UUID)
    let corrupt_profile_req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: ParsedKey {
            protocol: "VLESS".to_string(),
            name: "corrupt".to_string(),
            host: "1.2.3.4".to_string(),
            port: 443,
            uuid: "not-a-valid-uuid".to_string(),
            query_params: HashMap::new(),
            raw_url: String::new(),
        },
        options: DaemonConfigOptions::default(),
    };
    let res = client
        .post(&start_url)
        .json(&corrupt_profile_req)
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "Tier 1 failure must return 400 Bad Request"
    );
    let err_body: serde_json::Value = res.json().await.unwrap();
    let err_str = err_body["error"].as_str().unwrap();
    assert!(
        err_str.contains("Tier 1 profile validation failed"),
        "Error message must mention Tier 1 failure: {err_str}"
    );
    assert_eq!(proxy_manager.get_status().await, DaemonStatus::Error);

    // 3. Tier 2 Options Validation Failure (conflicting ports)
    let conflicting_ports_req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: ParsedKey {
            protocol: "VLESS".to_string(),
            name: "valid".to_string(),
            host: "1.2.3.4".to_string(),
            port: 443,
            uuid: "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0".to_string(),
            query_params: HashMap::new(),
            raw_url: String::new(),
        },
        options: DaemonConfigOptions {
            socks_port: 10808,
            http_port: 10808, // Conflict!
            ..Default::default()
        },
    };
    let res = client
        .post(&start_url)
        .json(&conflicting_ports_req)
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "Tier 2 failure must return 400 Bad Request"
    );
    let err_body: serde_json::Value = res.json().await.unwrap();
    let err_str = err_body["error"].as_str().unwrap();
    assert!(
        err_str.contains("Tier 2 configuration options validation failed"),
        "Error message must mention Tier 2 failure: {err_str}"
    );
    assert_eq!(proxy_manager.get_status().await, DaemonStatus::Error);

    // 4. Tier 2 Options Validation Failure (invalid routing rule action)
    let bad_rule_req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile: ParsedKey {
            protocol: "VLESS".to_string(),
            name: "valid".to_string(),
            host: "1.2.3.4".to_string(),
            port: 443,
            uuid: "a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0".to_string(),
            query_params: HashMap::new(),
            raw_url: String::new(),
        },
        options: DaemonConfigOptions {
            routing_rules: vec![RoutingRule {
                name: "malicious-action".to_string(),
                type_: "domain".to_string(),
                value: "example.com".to_string(),
                action: "DROP_AND_EXEC".to_string(),
            }],
            ..Default::default()
        },
    };
    let res = client
        .post(&start_url)
        .json(&bad_rule_req)
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "Bad routing rule action must return 400 Bad Request"
    );
    let err_body: serde_json::Value = res.json().await.unwrap();
    let err_str = err_body["error"].as_str().unwrap();
    assert!(
        err_str.contains("InvalidRuleAction") || err_str.contains("invalid action"),
        "Error message must specify rule action failure: {err_str}"
    );
    assert_eq!(proxy_manager.get_status().await, DaemonStatus::Error);
}
