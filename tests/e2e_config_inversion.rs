/* tests/e2e_config_inversion.rs
 *
 * Requirement R2: Core Architecture & Configuration Generation Inversion
 * Tiers 1 & 2: Feature Coverage & Boundary / Corner Cases
 */

mod common;

use common::*;
use std::process::Command;
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::domain::singbox_config::build_singbox_config;

// ---------------------------------------------------------------------------
// Tier 1: Feature Coverage (Happy Paths)
// ---------------------------------------------------------------------------

#[test]
fn test_typed_start_proxy_request_serialization() {
    let url = valid_vless_reality_url();
    let profile = parse_vpn_key(url).expect("Failed to parse valid VLESS URL");
    let options = DaemonConfigOptions::default();

    let req = StartProxyRequest {
        core_type: CoreType::SingBox,
        profile,
        options,
    };

    let serialized = serde_json::to_string(&req).expect("Failed to serialize StartProxyRequest");
    assert!(serialized.contains("\"core_type\":\"sing-box\""));
    assert!(serialized.contains("\"socks_port\":10808"));
    assert!(serialized.contains("\"http_port\":10809"));

    let deserialized: StartProxyRequest =
        serde_json::from_str(&serialized).expect("Failed to deserialize StartProxyRequest");
    assert_eq!(deserialized.core_type, CoreType::SingBox);
    assert_eq!(deserialized.options.socks_port, 10808);
    assert_eq!(deserialized.options.http_port, 10809);
    assert_eq!(deserialized.profile.protocol, "VLESS");
}

#[test]
fn test_profile_validation_valid_vless_reality() {
    let url = valid_vless_reality_url();
    let profile = parse_vpn_key(url).expect("Valid VLESS URL should parse");
    assert!(
        profile.validate().is_ok(),
        "Profile with valid UUID, server, port, and Reality pbk must pass validation"
    );
}

#[test]
fn test_profile_validation_valid_vmess() {
    let url = valid_vmess_url();
    let profile = parse_vpn_key(url).expect("Valid VMess URL should parse");
    assert!(
        profile.validate().is_ok(),
        "Profile with valid VMess parameters must pass validation"
    );
}

#[test]
fn test_profile_validation_valid_shadowsocks() {
    let url = valid_shadowsocks_url();
    let profile = parse_vpn_key(url).expect("Valid Shadowsocks URL should parse");
    assert!(
        profile.validate().is_ok(),
        "Profile with valid Shadowsocks parameters must pass validation"
    );
}

#[test]
fn test_options_validation_valid_ports() {
    let options = DaemonConfigOptions {
        socks_port: 20808,
        http_port: 20809,
        enable_mux: true,
        mux_concurrency: 16,
        ..Default::default()
    };
    assert!(
        options.validate().is_ok(),
        "Valid options must pass validation"
    );
}

#[test]
fn test_daemon_singbox_config_synthesis() {
    let url = valid_vless_reality_url();
    let parsed = parse_vpn_key(url).unwrap();
    let settings = sample_app_settings();

    // Inverted daemon configuration generation
    let config_json = build_singbox_config(&parsed, &settings);
    assert!(!config_json.is_empty(), "Generated config cannot be empty");

    let val: serde_json::Value = serde_json::from_str(&config_json)
        .expect("Generated sing-box config must be syntactically valid JSON");

    assert!(
        val.get("inbounds").is_some(),
        "Config must contain inbounds"
    );
    assert!(
        val.get("outbounds").is_some(),
        "Config must contain outbounds"
    );
    assert!(val.get("route").is_some(), "Config must contain route");
    assert!(val.get("dns").is_some(), "Config must contain dns");
    assert!(
        val.get("experimental").is_some(),
        "Config must contain experimental"
    );
}

#[test]
fn test_daemon_preflight_check_with_singbox() {
    let url = valid_vless_reality_url();
    let parsed = parse_vpn_key(url).unwrap();
    let settings = sample_app_settings();
    let config_json = build_singbox_config(&parsed, &settings);

    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    std::fs::write(temp_file.path(), config_json).expect("Failed to write config");

    // Execute sing-box check if sing-box binary exists
    let singbox_path = "/usr/local/bin/sing-box";
    if std::path::Path::new(singbox_path).exists() {
        let output = Command::new(singbox_path)
            .arg("check")
            .arg("-c")
            .arg(temp_file.path())
            .output()
            .expect("Failed to execute sing-box check");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "Pre-flight sing-box check failed: {}",
            stderr
        );
    }
}

#[test]
fn test_typed_daemon_status_and_log_levels() {
    let status_json = serde_json::to_string(&DaemonStatus::Connected).unwrap();
    assert_eq!(status_json, "\"connected\"");

    let level_json = serde_json::to_string(&LogLevel::Warning).unwrap();
    assert_eq!(level_json, "\"warning\"");

    let strategy_json = serde_json::to_string(&DomainStrategy::PreferIpv4).unwrap();
    assert_eq!(strategy_json, "\"prefer-ipv4\"");

    let mode_json = serde_json::to_string(&RoutingMode::Proxy).unwrap();
    assert_eq!(mode_json, "\"proxy\"");
}

// ---------------------------------------------------------------------------
// Tier 2: Boundary & Corner Cases
// ---------------------------------------------------------------------------

#[test]
fn test_boundary_invalid_uuid_in_parsed_key_rejected() {
    let url = invalid_uuid_url();
    let parsed = parse_vpn_key(url).expect("Key parser parses structure");
    let validation_result = parsed.validate();
    assert!(
        validation_result.is_err(),
        "ParsedKey with invalid UUID must fail validation"
    );
}

#[test]
fn test_boundary_invalid_zero_port_in_parsed_key_rejected() {
    let url = invalid_port_url();
    let parsed = parse_vpn_key(url).expect("Key parser parses structure");
    let validation_result = parsed.validate();
    assert!(
        validation_result.is_err(),
        "ParsedKey with port 0 must fail validation"
    );
}

#[test]
fn test_boundary_conflicting_ports_rejected() {
    let options = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 10808, // Conflicting port!
        ..Default::default()
    };
    let validation_result = options.validate();
    assert!(
        validation_result.is_err(),
        "Conflicting SOCKS and HTTP ports must be rejected"
    );
    assert!(validation_result.unwrap_err().contains("identical"));
}

#[test]
fn test_boundary_zero_port_rejected_in_options() {
    let options_zero_socks = DaemonConfigOptions {
        socks_port: 0,
        ..Default::default()
    };
    assert!(
        options_zero_socks.validate().is_err(),
        "Port 0 must be rejected"
    );

    let options_zero_http = DaemonConfigOptions {
        socks_port: 10808,
        http_port: 0,
        ..Default::default()
    };
    assert!(
        options_zero_http.validate().is_err(),
        "Port 0 must be rejected"
    );
}

#[test]
fn test_boundary_invalid_mux_concurrency_rejected() {
    let options_zero_mux = DaemonConfigOptions {
        enable_mux: true,
        mux_concurrency: 0,
        ..Default::default()
    };
    assert!(
        options_zero_mux.validate().is_err(),
        "Mux concurrency 0 must be rejected"
    );

    let options_high_mux = DaemonConfigOptions {
        enable_mux: true,
        mux_concurrency: 129,
        ..Default::default()
    };
    assert!(
        options_high_mux.validate().is_err(),
        "Mux concurrency > 128 must be rejected"
    );
}

#[test]
fn test_boundary_malformed_json_start_proxy_rejected() {
    // Empty JSON
    let empty_res = serde_json::from_str::<StartProxyRequest>("");
    assert!(
        empty_res.is_err(),
        "Empty string cannot deserialize into StartProxyRequest"
    );

    // Corrupt JSON syntax
    let corrupt_res =
        serde_json::from_str::<StartProxyRequest>("{\"core_type\": \"sing-box\", invalid");
    assert!(
        corrupt_res.is_err(),
        "Corrupt JSON must fail deserialization"
    );

    // Missing profile field
    let missing_field = serde_json::from_str::<StartProxyRequest>("{\"core_type\": \"sing-box\"}");
    assert!(
        missing_field.is_err(),
        "Missing required fields must fail deserialization"
    );
}

#[test]
fn test_boundary_preflight_check_invalid_config_rejected() {
    let singbox_path = "/usr/local/bin/sing-box";
    if std::path::Path::new(singbox_path).exists() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        // Broken JSON configuration with unknown inbound type
        let broken_config = r#"{
            "inbounds": [{"type": "non_existent_protocol_type", "listen": "127.0.0.1"}],
            "outbounds": [{"type": "direct"}]
        }"#;
        std::fs::write(temp_file.path(), broken_config).unwrap();

        let output = Command::new(singbox_path)
            .arg("check")
            .arg("-c")
            .arg(temp_file.path())
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Invalid configuration must be rejected by sing-box check"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.is_empty(),
            "stderr should contain validation error details"
        );
    }
}
