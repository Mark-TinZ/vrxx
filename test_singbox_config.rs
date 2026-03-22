use vrxx::domain::singbox_config::build_singbox_config;
use vrxx::domain::key_parser::parse_vpn_key;
use vrxx::settings::SettingsManager;

fn main() {
    let key = "vless://a3482e88-6860-4a1c-914c-4b4ea5c49f87@1.2.3.4:443?security=reality&sni=google.com&fp=chrome&pbk=pubkey123&sid=shortid&type=tcp&flow=xtls-rprx-vision#MyVLESS";
    let parsed = parse_vpn_key(key).unwrap();
    let mut s = SettingsManager::new().load();
    s.tun_mode = false;
    let json = build_singbox_config(&parsed, &s);
    println!("{}", json);
}
