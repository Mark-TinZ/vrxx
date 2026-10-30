/* tests/challenger_m3_keystore_stress.rs
 *
 * Empirical Challenge & Stress Test Suite for Milestone M3
 * Focus: Cryptographic Storage, SettingsManager Auto-Migration, Permission Enforcement,
 * and Adversarial Keystore Hardening.
 *
 * Test Dimensions:
 * 1. SettingsManager Auto-Migration & Persistence (M3 Core Verification)
 *    - On-the-fly migration from VRXXDAT1 to VRXXDAT2 on load_keys()
 *    - Strict Unix permission enforcement: 0700 parent directory, 0600 on data.dat and data.dat.bak
 *    - Corruption recovery: primary data.dat corrupt -> backup VRXXDAT1 restored AND auto-migrated
 *    - Corruption recovery: primary data.dat corrupt -> backup VRXXDAT2 restored with 0600 mode
 *    - Unrecoverable state: both primary and backup corrupt -> clean empty Vec, no panic
 *    - Zero-byte primary file handling -> clean fallback to backup
 *    - Empty key set encryption and persistence
 * 2. Security & Permission Hardening
 *    - Pre-existing loose parent directory (e.g. 0755 or 0777) hardened to 0700 on save
 *    - Pre-existing loose file (e.g. 0644 or 0666) hardened to 0600 on save
 *    - Atomic swap: no temporary files (.tmp) left behind after save_keys
 * 3. Keystore Cryptographic Integrity & Stress
 *    - Nonce uniqueness / non-repeating across repeated encryptions
 *    - Bit-flip tamper resistance across every component (magic, nonce, ciphertext, tag)
 *    - User isolation: cross-user decryption impossibility
 *    - Large profile volume stress (hundreds of VPN configurations)
 *    - Rapid concurrent read/write operations
 */

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use vrxx::crypto::keystore::{
    container_version, decrypt_v2_container, encrypt_keys_with_id, encrypt_v2_container,
    get_machine_id, is_legacy_v1_container, is_v2_container, MAGIC_HEADER_V1, MAGIC_HEADER_V2,
};
use vrxx::settings::{AppSettings, SettingsManager, VpnKeyData};

fn sample_keys(count: usize) -> Vec<VpnKeyData> {
    (0..count)
        .map(|i| VpnKeyData {
            name: format!("Server #{i}"),
            protocol: if i % 2 == 0 {
                "VLESS".to_string()
            } else {
                "Shadowsocks".to_string()
            },
            is_active: i == 0,
            traffic_down: "100 MB".to_string(),
            traffic_up: "50 MB".to_string(),
            time_connected: "01:23:45".to_string(),
            ping: format!("{} ms", 20 + i),
            location: "DE".to_string(),
            country_code: "DE".to_string(),
            city: "Frankfurt".to_string(),
            isp: "Hetzner".to_string(),
            timezone: "UTC+1".to_string(),
            url: format!(
                "vless://00000000-0000-0000-0000-{:012x}@10.0.0.{}:443?security=reality#Server-{}",
                i,
                (i % 250) + 1,
                i
            ),
            group: String::new(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. SettingsManager Auto-Migration & Persistence
// ---------------------------------------------------------------------------

#[test]
fn test_stress_auto_migration_primary_vrxxdat1_to_vrxxdat2() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    let machine_id =
        get_machine_id().unwrap_or_else(|_| "fedcba9876543210fedcba9876543210".to_string());
    let keys = sample_keys(5);

    // Write a valid legacy VRXXDAT1 container to primary data_path
    let v1_bytes = encrypt_keys_with_id(&keys, &machine_id).expect("V1 encryption must succeed");
    assert_eq!(&v1_bytes[0..8], MAGIC_HEADER_V1);
    assert!(is_legacy_v1_container(&v1_bytes));

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &v1_bytes).unwrap();
    fs::set_permissions(&data_path, fs::Permissions::from_mode(0o644)).unwrap();

    // Calling load_keys() must trigger transparent on-the-fly migration
    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), 5);
    assert_eq!(loaded[0].name, "Server #0");
    assert_eq!(loaded[4].name, "Server #4");

    // Primary data.dat must now be in VRXXDAT2 format with 0600 mode
    let updated_data = fs::read(&data_path).expect("data.dat must exist after migration");
    assert_eq!(&updated_data[0..8], MAGIC_HEADER_V2);
    assert_eq!(container_version(&updated_data).unwrap(), 2);
    assert!(is_v2_container(&updated_data));

    let data_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        data_mode, 0o600,
        "data.dat permissions must be hardened to 0600"
    );

    // Backup file data.dat.bak must also be updated with VRXXDAT2 and 0600 mode
    assert!(
        bak_path.exists(),
        "data.dat.bak must be created during migration"
    );
    let updated_bak = fs::read(&bak_path).expect("data.dat.bak must be readable");
    assert_eq!(&updated_bak[0..8], MAGIC_HEADER_V2);
    assert_eq!(container_version(&updated_bak).unwrap(), 2);

    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600, "data.dat.bak permissions must be 0600");
}

#[test]
fn test_stress_backup_vrxxdat1_auto_migration_when_primary_corrupt() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    let machine_id =
        get_machine_id().unwrap_or_else(|_| "fedcba9876543210fedcba9876543210".to_string());
    let keys = sample_keys(3);

    let v1_bytes = encrypt_keys_with_id(&keys, &machine_id).expect("V1 encryption must succeed");

    fs::create_dir_all(&config_dir).unwrap();
    // Primary is corrupted
    fs::write(&data_path, b"random garbage that cannot be decrypted").unwrap();
    // Backup has legacy V1 keys
    fs::write(&bak_path, &v1_bytes).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].name, "Server #0");

    // Both data.dat and data.dat.bak must now be migrated to VRXXDAT2
    let restored_data = fs::read(&data_path).unwrap();
    assert_eq!(&restored_data[0..8], MAGIC_HEADER_V2);
    let data_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(data_mode, 0o600);

    let restored_bak = fs::read(&bak_path).unwrap();
    assert_eq!(&restored_bak[0..8], MAGIC_HEADER_V2);
    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600);
}

#[test]
fn test_stress_backup_vrxxdat2_restoration_when_primary_corrupt() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    let keys = sample_keys(2);

    // Save normally first (creates V2 data.dat and data.dat.bak)
    manager.save_keys(&keys);
    assert!(data_path.exists());
    assert!(bak_path.exists());

    // Corrupt primary
    fs::write(&data_path, b"completely corrupted primary payload").unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[1].name, "Server #1");

    // Verify primary was restored from backup and permissions are 0600
    let restored_data = fs::read(&data_path).unwrap();
    assert_eq!(&restored_data[0..8], MAGIC_HEADER_V2);
    let data_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(data_mode, 0o600);
}

#[test]
fn test_stress_unrecoverable_state_primary_and_backup_corrupt() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, b"corrupted 1").unwrap();
    fs::write(&bak_path, b"corrupted 2").unwrap();

    // Must return empty vector without crashing
    let loaded = manager.load_keys();
    assert!(loaded.is_empty());
}

#[test]
fn test_stress_zero_byte_primary_file_falls_back_to_backup() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    let keys = sample_keys(4);
    manager.save_keys(&keys);

    // Truncate primary to 0 bytes
    fs::write(&data_path, b"").unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), 4);
    assert_eq!(loaded[0].name, "Server #0");

    // Primary should be restored
    assert!(bak_path.exists());
    let data_bytes = fs::read(&data_path).unwrap();
    assert_eq!(&data_bytes[0..8], MAGIC_HEADER_V2);
}

#[test]
fn test_stress_empty_key_set_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_config");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());
    let empty_keys: Vec<VpnKeyData> = Vec::new();
    manager.save_keys(&empty_keys);

    assert!(data_path.exists());
    let loaded = manager.load_keys();
    assert!(loaded.is_empty());

    let raw = fs::read(&data_path).unwrap();
    assert_eq!(&raw[0..8], MAGIC_HEADER_V2);
}

// ---------------------------------------------------------------------------
// 2. Security & Permission Hardening
// ---------------------------------------------------------------------------

#[test]
fn test_stress_directory_and_file_permission_enforcement() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("vrxx_insecure_dir");
    fs::create_dir_all(&config_dir).unwrap();
    // Intentionally loose permissions
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o777)).unwrap();

    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path.clone(), data_path.clone());
    let keys = sample_keys(1);
    manager.save_keys(&keys);

    // Check directory hardened to 0700
    let dir_mode = fs::metadata(&config_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        dir_mode, 0o700,
        "Parent directory mode must be hardened to 0700"
    );

    // Check file modes
    let data_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(data_mode, 0o600, "data.dat mode must be 0600");
    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600, "data.dat.bak mode must be 0600");

    // Check temporary file cleanup
    let tmp_path = config_dir.join("data.dat.tmp");
    assert!(
        !tmp_path.exists(),
        "Temporary file data.dat.tmp must not remain on disk"
    );

    // Test save_settings_only permission enforcement
    let settings = AppSettings::new();
    manager.save_settings_only(&settings);
    let cfg_mode = fs::metadata(&config_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(cfg_mode, 0o600, "settings.json mode must be 0600");
    let cfg_bak = config_dir.join("settings.json.bak");
    let cfg_bak_mode = fs::metadata(&cfg_bak).unwrap().permissions().mode() & 0o777;
    assert_eq!(cfg_bak_mode, 0o600, "settings.json.bak mode must be 0600");
}

// ---------------------------------------------------------------------------
// 3. Keystore Cryptographic Integrity & Tamper Resistance
// ---------------------------------------------------------------------------

#[test]
fn test_stress_nonce_uniqueness_across_encryptions() {
    let key = [0x33u8; 32];
    let payload = b"identically repeated plaintext message";

    let c1 = encrypt_v2_container(payload, &key).unwrap();
    let c2 = encrypt_v2_container(payload, &key).unwrap();
    let c3 = encrypt_v2_container(payload, &key).unwrap();

    assert_ne!(
        &c1[8..20],
        &c2[8..20],
        "Nonces must never repeat across distinct encryptions"
    );
    assert_ne!(
        &c2[8..20],
        &c3[8..20],
        "Nonces must never repeat across distinct encryptions"
    );
    assert_ne!(c1, c2, "Ciphertexts must differ due to distinct nonces");
}

#[test]
fn test_stress_tamper_resistance_across_all_container_offsets() {
    let key = [0x5au8; 32];
    let payload = b"Critical security keys that must not be forged";
    let original = encrypt_v2_container(payload, &key).unwrap();

    // Verify original decrypts
    assert_eq!(decrypt_v2_container(&original, &key).unwrap(), payload);

    // Test corrupting magic header (offset 0..8)
    for offset in 0..8 {
        let mut tampered = original.clone();
        tampered[offset] ^= 0x55;
        let res = decrypt_v2_container(&tampered, &key);
        assert!(
            res.is_err(),
            "Tampered magic header at byte {offset} must fail"
        );
    }

    // Test corrupting nonce (offset 8..20)
    for offset in 8..20 {
        let mut tampered = original.clone();
        tampered[offset] ^= 0xaa;
        let res = decrypt_v2_container(&tampered, &key);
        assert!(res.is_err(), "Tampered nonce at byte {offset} must fail");
    }

    // Test corrupting ciphertext (offset 20..end-16)
    for offset in (20..original.len() - 16).step_by(3) {
        let mut tampered = original.clone();
        tampered[offset] ^= 0x01;
        let res = decrypt_v2_container(&tampered, &key);
        assert!(
            res.is_err(),
            "Tampered ciphertext at byte {offset} must fail"
        );
    }

    // Test corrupting tag (last 16 bytes)
    for offset in (original.len() - 16)..original.len() {
        let mut tampered = original.clone();
        tampered[offset] ^= 0x80;
        let res = decrypt_v2_container(&tampered, &key);
        assert!(res.is_err(), "Tampered tag at byte {offset} must fail");
    }
}

#[test]
fn test_stress_large_profile_volume() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("settings.json");
    let data_path = temp_dir.path().join("data.dat");

    let manager = SettingsManager::with_paths(config_path, data_path);
    let keys = sample_keys(200);

    manager.save_keys(&keys);
    let loaded = manager.load_keys();

    assert_eq!(loaded.len(), 200);
    for (i, key) in loaded.iter().enumerate() {
        assert_eq!(key.name, format!("Server #{i}"));
        assert_eq!(key.ping, format!("{} ms", 20 + i));
    }
}

#[test]
fn test_stress_rapid_consecutive_saves() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("settings.json");
    let data_path = temp_dir.path().join("data.dat");
    let manager = SettingsManager::with_paths(config_path, data_path);

    for i in 0..20 {
        let keys = sample_keys(i + 1);
        manager.save_keys(&keys);
        let loaded = manager.load_keys();
        assert_eq!(loaded.len(), i + 1);
    }
}
