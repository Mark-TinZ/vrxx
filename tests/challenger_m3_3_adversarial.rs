/* tests/challenger_m3_3_adversarial.rs
 *
 * Empirical Challenge & Adversarial Stress Test Suite for Milestone M3
 * Focus: User-Isolated Keystore, AEAD Cryptographic Integrity & DAC Boundaries
 *
 * Test Dimensions:
 * 1. User Isolation & Domain Separation
 *    - Cross-user decryption failure (User A ciphertext rejected by User B, User C, and zero key)
 *    - Nonce uniqueness & avalanche effect across distinct user secrets
 *    - Domain separation between KDF v1 (machine-id) and KDF v2 (user secret)
 * 2. Boundary Corruption & AEAD Integrity
 *    - Truncated containers at all boundary lengths (<36 bytes: 0, 1, 2, 7, 8, 19, 20, 35)
 *    - Invalid magic headers (VRXXDAT0, VRXXDAT3, bad ASCII, all-zeros, etc.)
 *    - Exhaustive nonce byte tampering (bit-flipping each byte 8..20)
 *    - Exhaustive ciphertext and authentication tag bit-flipping
 *    - Trailing garbage byte injection
 *    - Malformed JSON payload after valid AEAD encryption
 * 3. File System & DAC Permission Boundaries
 *    - Strict 0600 mode on key file and 0700 mode on parent directory
 *    - Automatic permission hardening on existing 0644 / 0666 / 0777 files
 *    - Parent directory permission verification
 * 4. Symlink Attack Resistance
 *    - Direct symlink pointing to an existing file
 *    - Dangling symlink pointing to non-existent file
 * 5. Concurrency & Race-Free Key Initialization
 *    - 20 concurrent threads calling get_or_create_user_key_at simultaneously
 * 6. File Format Robustness
 *    - Lowercase hex, uppercase hex, trailing newline, 32 raw bytes
 *    - Malformed hex length and non-hex characters
 * 7. Settings Manager Automatic Migration & Backup Recovery
 *    - Auto-migration from VRXXDAT1 to VRXXDAT2 on load_keys()
 *    - Permissions 0600 on migrated data.dat and data.dat.bak
 *    - Corrupted primary fallback to backup
 */

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

use vrxx::crypto::keystore::{
    container_version, decrypt_keys, decrypt_keys_with_user_key, decrypt_v2_container,
    derive_key_from_machine_id, derive_user_key_v2, encrypt_keys_with_id,
    encrypt_keys_with_user_key, encrypt_v2_container, get_or_create_user_key_at,
    is_legacy_v1_container, is_v2_container, MAGIC_HEADER_V1,
};
use vrxx::settings::{SettingsManager, VpnKeyData};

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn check_mode(path: &Path, expected: u32) -> bool {
    if let Ok(meta) = fs::metadata(path) {
        (meta.permissions().mode() & 0o777) == expected
    } else {
        false
    }
}

fn sample_keys() -> Vec<VpnKeyData> {
    vec![
        VpnKeyData {
            name: "Frankfurt Reality".to_string(),
            protocol: "VLESS".to_string(),
            is_active: true,
            traffic_down: "1.2 GB".to_string(),
            traffic_up: "450 MB".to_string(),
            time_connected: "03:12:44".to_string(),
            ping: "28 ms".to_string(),
            location: "DE".to_string(),
            country_code: "DE".to_string(),
            city: "Frankfurt".to_string(),
            isp: "Hetzner Online GmbH".to_string(),
            timezone: "Europe/Berlin".to_string(),
            url: "vless://a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0@198.51.100.1:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=abcd&sni=gateway.example.com&flow=xtls-rprx-vision#Frankfurt".to_string(),
            group: "Primary".to_string(),
        },
        VpnKeyData {
            name: "Amsterdam Shadowsocks".to_string(),
            protocol: "Shadowsocks".to_string(),
            is_active: false,
            traffic_down: "0 MB".to_string(),
            traffic_up: "0 MB".to_string(),
            time_connected: "00:00:00".to_string(),
            ping: "35 ms".to_string(),
            location: "NL".to_string(),
            country_code: "NL".to_string(),
            city: "Amsterdam".to_string(),
            isp: "DigitalOcean LLC".to_string(),
            timezone: "Europe/Amsterdam".to_string(),
            url: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZDEyMw@198.51.100.2:8388#Amsterdam".to_string(),
            group: "Backup".to_string(),
        },
    ]
}

// ===========================================================================
// Dimension 1: User Isolation & Domain Separation
// ===========================================================================

#[test]
fn test_user_isolation_cross_decryption_impossible() {
    let user_a_secret = b"alice-entropy-seed-secret-token-32";
    let user_b_secret = b"bob-entropy-seed-secret-token-32";
    let user_c_secret = b"eve-entropy-seed-secret-token-32";

    let key_a = derive_user_key_v2(user_a_secret).unwrap();
    let key_b = derive_user_key_v2(user_b_secret).unwrap();
    let key_c = derive_user_key_v2(user_c_secret).unwrap();
    let zero_key = [0u8; 32];

    assert_ne!(
        key_a, key_b,
        "Distinct user secrets must produce distinct keys"
    );
    assert_ne!(
        key_b, key_c,
        "Distinct user secrets must produce distinct keys"
    );

    let keys = sample_keys();
    let encrypted_a =
        encrypt_keys_with_user_key(&keys, &key_a).expect("Encryption with User A key must succeed");

    // Verify User A can decrypt
    let decrypted_a = decrypt_keys_with_user_key(&encrypted_a, &key_a)
        .expect("User A must be able to decrypt their own data");
    assert_eq!(decrypted_a.len(), 2);
    assert_eq!(decrypted_a[0].name, "Frankfurt Reality");

    // Verify User B fails
    let res_b = decrypt_keys_with_user_key(&encrypted_a, &key_b);
    assert!(
        res_b.is_err(),
        "User B must NOT be able to decrypt User A's data"
    );

    // Verify User C fails
    let res_c = decrypt_keys_with_user_key(&encrypted_a, &key_c);
    assert!(
        res_c.is_err(),
        "User C must NOT be able to decrypt User A's data"
    );

    // Verify zero key fails
    let res_zero = decrypt_keys_with_user_key(&encrypted_a, &zero_key);
    assert!(
        res_zero.is_err(),
        "Zero key must NOT be able to decrypt User A's data"
    );
}

#[test]
fn test_domain_separation_kdf_v1_vs_kdf_v2() {
    // Even if machine-id is identical to user secret string, KDF v1 and KDF v2 must diverge
    let common_id = "0123456789abcdef0123456789abcdef";
    let key_v1 = derive_key_from_machine_id(common_id).unwrap();
    let key_v2 = derive_user_key_v2(common_id.as_bytes()).unwrap();

    assert_ne!(
        key_v1, key_v2,
        "KDF v1 and KDF v2 must have cryptographic domain separation"
    );

    let keys = sample_keys();
    let legacy_container = encrypt_keys_with_id(&keys, common_id).unwrap();

    // Trying to decrypt legacy container with v2 container decryptor must fail magic check
    let v2_dec_res = decrypt_v2_container(&legacy_container, &key_v2);
    assert!(v2_dec_res.is_err());
    assert!(v2_dec_res.unwrap_err().to_string().contains("VRXXDAT2"));
}

#[test]
fn test_avalanche_effect_on_user_secret() {
    let secret_1 = b"user-secret-sample-string-version-A";
    let secret_2 = b"user-secret-sample-string-version-B"; // 1 character difference

    let key_1 = derive_user_key_v2(secret_1).unwrap();
    let key_2 = derive_user_key_v2(secret_2).unwrap();

    // Count bit differences between key_1 and key_2
    let mut bit_diffs = 0;
    for i in 0..32 {
        bit_diffs += (key_1[i] ^ key_2[i]).count_ones();
    }

    // A good PRF/KDF should flip roughly 128 out of 256 bits (~50% avalanche)
    assert!(
        (90..=166).contains(&bit_diffs),
        "Avalanche effect failed: {bit_diffs} bits differ out of 256"
    );
}

// ===========================================================================
// Dimension 2: Boundary Corruption & AEAD Integrity
// ===========================================================================

#[test]
fn test_boundary_truncated_containers_all_lengths() {
    let key = [0x33u8; 32];
    let payload = b"hello world secure payload";
    let valid_container = encrypt_v2_container(payload, &key).unwrap();

    // Test every length strictly less than MIN_CONTAINER_LENGTH (36)
    let test_lengths = [0, 1, 2, 7, 8, 9, 12, 19, 20, 21, 30, 35];
    for len in test_lengths {
        let truncated = &valid_container[..len.min(valid_container.len())];

        // decrypt_v2_container must fail cleanly without panic
        let res1 = decrypt_v2_container(truncated, &key);
        assert!(
            res1.is_err(),
            "Length {len} should fail decrypt_v2_container"
        );
        assert!(
            res1.unwrap_err().to_string().contains("too small"),
            "Error for length {len} should mention 'too small'"
        );

        // container_version must fail cleanly without panic
        let res2 = container_version(truncated);
        assert!(res2.is_err(), "Length {len} should fail container_version");

        // decrypt_keys must fail cleanly without panic
        let res3 = decrypt_keys(truncated);
        assert!(res3.is_err(), "Length {len} should fail decrypt_keys");
    }
}

#[test]
fn test_boundary_invalid_magic_headers() {
    let key = [0x55u8; 32];
    let bad_magics: [&[u8; 8]; 7] = [
        b"VRXXDAT0",
        b"VRXXDAT1", // Legacy header passed to v2 decryptor
        b"VRXXDAT3",
        b"VRXXDATX",
        b"vrxxdat2", // Wrong case
        b"\x00\x00\x00\x00\x00\x00\x00\x00",
        b"BADMAGIC",
    ];

    let valid_container = encrypt_v2_container(b"test data", &key).unwrap();

    for bad_magic in bad_magics {
        let mut corrupted = valid_container.clone();
        corrupted[0..8].copy_from_slice(bad_magic);

        let res = decrypt_v2_container(&corrupted, &key);
        assert!(
            res.is_err(),
            "Bad magic {:?} should fail v2 decryption",
            bad_magic
        );

        if bad_magic != MAGIC_HEADER_V1 {
            let ver_res = container_version(&corrupted);
            assert!(
                ver_res.is_err(),
                "Bad magic {:?} should fail container_version",
                bad_magic
            );
        }
    }
}

#[test]
fn test_boundary_tampered_nonce_exhaustive() {
    let key = [0x77u8; 32];
    let valid_container = encrypt_v2_container(b"classified proxy credentials", &key).unwrap();

    // Nonce is at bytes 8..20 (12 bytes)
    for nonce_offset in 8..20 {
        let mut tampered = valid_container.clone();
        tampered[nonce_offset] ^= 0x01; // single bit flip

        let res = decrypt_v2_container(&tampered, &key);
        assert!(
            res.is_err(),
            "Tampered nonce byte at offset {nonce_offset} must fail AEAD verification"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("Decryption failed") || err_msg.contains("integrity"),
            "Error must indicate integrity failure: {err_msg}"
        );
    }
}

#[test]
fn test_boundary_tampered_ciphertext_and_tag_exhaustive() {
    let key = [0x88u8; 32];
    let valid_container = encrypt_v2_container(b"short payload", &key).unwrap();

    // Bytes 20..end are ciphertext + tag
    for offset in 20..valid_container.len() {
        let mut tampered = valid_container.clone();
        tampered[offset] ^= 0x80; // MSB flip

        let res = decrypt_v2_container(&tampered, &key);
        assert!(
            res.is_err(),
            "Tampered ciphertext/tag at offset {offset} must fail AEAD verification"
        );
    }
}

#[test]
fn test_boundary_trailing_garbage_injection() {
    let key = [0x99u8; 32];
    let valid_container = encrypt_v2_container(b"sample secrets", &key).unwrap();

    let garbage_sizes = [1, 2, 8, 16, 64, 256];
    for garbage_size in garbage_sizes {
        let mut corrupted = valid_container.clone();
        corrupted.extend(vec![0xAA; garbage_size]);

        let res = decrypt_v2_container(&corrupted, &key);
        assert!(
            res.is_err(),
            "Injecting {garbage_size} garbage bytes at container tail must fail AEAD verification"
        );
    }
}

#[test]
fn test_malformed_json_after_valid_encryption() {
    let key = [0xABu8; 32];
    // Encrypt bytes that are valid ciphertext but not valid JSON
    let bad_json_payload = b"{\"keys\": [unclosed json object...";
    let container = encrypt_v2_container(bad_json_payload, &key).unwrap();

    let res = decrypt_keys_with_user_key(&container, &key);
    assert!(
        res.is_err(),
        "Decrypting invalid JSON must return error without panicking"
    );
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("JSON") || err_msg.contains("десериализации"),
        "Error message should mention JSON deserialization: {err_msg}"
    );
}

// ===========================================================================
// Dimension 3: File System & DAC Permission Boundaries
// ===========================================================================

#[test]
fn test_permission_boundaries_strict_0600_and_0700() {
    let temp_dir = TempDir::new().unwrap();
    let deep_path = temp_dir.path().join("nested/secure/store/.key");

    let key = get_or_create_user_key_at(&deep_path).expect("Key creation must succeed");
    assert_eq!(key.len(), 32);

    #[cfg(unix)]
    {
        let parent_dir = deep_path.parent().unwrap();
        assert!(
            check_mode(parent_dir, 0o700),
            "Parent directory {:?} must have mode 0700 (rwx------), got {:o}",
            parent_dir,
            fs::metadata(parent_dir).unwrap().permissions().mode() & 0o777
        );

        assert!(
            check_mode(&deep_path, 0o600),
            "Key file {:?} must have mode 0600 (rw-------), got {:o}",
            deep_path,
            fs::metadata(&deep_path).unwrap().permissions().mode() & 0o777
        );
    }
}

#[test]
fn test_permission_boundaries_auto_hardening_existing_files() {
    let temp_dir = TempDir::new().unwrap();
    let parent = temp_dir.path().join("insecure_dir");
    fs::create_dir_all(&parent).unwrap();

    #[cfg(unix)]
    {
        // Set parent to 0777 (world-readable/writable)
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
    }

    let key_path = parent.join(".key");
    let initial_key = [0x55u8; 32];
    fs::write(&key_path, initial_key).unwrap();

    #[cfg(unix)]
    {
        // Set key file to 0666 (world-readable/writable)
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(check_mode(&key_path, 0o666));
    }

    // Calling get_or_create_user_key_at must harden existing permissions to 0600
    let key = get_or_create_user_key_at(&key_path).expect("Reading key must succeed");
    assert_eq!(key, initial_key);

    #[cfg(unix)]
    {
        assert!(
            check_mode(&key_path, 0o600),
            "Key file permissions must be hardened to 0600"
        );
    }
}

// ===========================================================================
// Dimension 4: Symlink Attack Resistance
// ===========================================================================

#[test]
fn test_symlink_rejection_direct_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let target_key = temp_dir.path().join("real_target.key");
    fs::write(
        &target_key,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .unwrap();

    let symlink_path = temp_dir.path().join("symlink.key");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target_key, &symlink_path).unwrap();

        let res = get_or_create_user_key_at(&symlink_path);
        assert!(
            res.is_err(),
            "get_or_create_user_key_at must reject symlink"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("символической ссылкой"),
            "Error must specify symlink rejection: {err_msg}"
        );
    }
}

#[test]
fn test_symlink_rejection_dangling_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let non_existent_target = temp_dir.path().join("non_existent_file.key");
    let dangling_symlink = temp_dir.path().join("dangling.key");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&non_existent_target, &dangling_symlink).unwrap();

        let res = get_or_create_user_key_at(&dangling_symlink);
        assert!(
            res.is_err(),
            "get_or_create_user_key_at must reject dangling symlink"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("символической ссылкой"),
            "Error must specify symlink rejection: {err_msg}"
        );
    }
}

// ===========================================================================
// Dimension 5: Concurrency & Race-Free Key Initialization
// ===========================================================================

#[test]
fn test_concurrent_race_condition_20_threads() {
    let temp_dir = TempDir::new().unwrap();
    let shared_key_path = Arc::new(temp_dir.path().join("concurrent/vrxx/.key"));

    let thread_count = 20;
    let mut handles = Vec::with_capacity(thread_count);

    for _ in 0..thread_count {
        let path = Arc::clone(&shared_key_path);
        handles.push(std::thread::spawn(move || get_or_create_user_key_at(&path)));
    }

    let mut keys = Vec::new();
    for handle in handles {
        let key = handle
            .join()
            .expect("Thread should not panic")
            .expect("Key derivation should succeed");
        keys.push(key);
    }

    // All threads must return the identical 32-byte key
    let first_key = keys[0];
    for (idx, key) in keys.iter().enumerate() {
        assert_eq!(
            &first_key, key,
            "Thread {idx} produced a different key during race condition"
        );
    }

    // Verify resulting permissions
    #[cfg(unix)]
    {
        assert!(check_mode(&shared_key_path, 0o600));
        assert!(check_mode(shared_key_path.parent().unwrap(), 0o700));
    }
}

// ===========================================================================
// Dimension 6: File Format Robustness
// ===========================================================================

#[test]
fn test_key_file_formats_hex_and_raw_bytes() {
    let temp_dir = TempDir::new().unwrap();
    let raw_key = [0x5au8; 32];
    let hex_lower = "5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";
    let hex_upper = "5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A";

    // 1. Lowercase hex with newline
    let path_1 = temp_dir.path().join("key1");
    fs::write(&path_1, format!("{hex_lower}\n")).unwrap();
    assert_eq!(get_or_create_user_key_at(&path_1).unwrap(), raw_key);

    // 2. Uppercase hex without newline
    let path_2 = temp_dir.path().join("key2");
    fs::write(&path_2, hex_upper).unwrap();
    assert_eq!(get_or_create_user_key_at(&path_2).unwrap(), raw_key);

    // 3. Raw 32 bytes
    let path_3 = temp_dir.path().join("key3");
    fs::write(&path_3, raw_key).unwrap();
    assert_eq!(get_or_create_user_key_at(&path_3).unwrap(), raw_key);

    // 4. Raw 32 bytes + newline (33 bytes)
    let path_4 = temp_dir.path().join("key4");
    let mut raw_with_nl = raw_key.to_vec();
    raw_with_nl.push(b'\n');
    fs::write(&path_4, raw_with_nl).unwrap();
    assert_eq!(get_or_create_user_key_at(&path_4).unwrap(), raw_key);
}

#[test]
fn test_corrupted_key_file_rejected() {
    let temp_dir = TempDir::new().unwrap();

    // 1. 63 hex chars (too short)
    let path_short = temp_dir.path().join("short.key");
    fs::write(
        &path_short,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde",
    )
    .unwrap();
    assert!(get_or_create_user_key_at(&path_short).is_err());

    // 2. 64 non-hex characters
    let path_non_hex = temp_dir.path().join("non_hex.key");
    fs::write(
        &path_non_hex,
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    )
    .unwrap();
    assert!(get_or_create_user_key_at(&path_non_hex).is_err());

    // 3. 10 random bytes
    let path_random = temp_dir.path().join("random.key");
    fs::write(&path_random, [0x42u8; 10]).unwrap();
    assert!(get_or_create_user_key_at(&path_random).is_err());
}

// ===========================================================================
// Dimension 7: Settings Manager Auto-Migration & Backup Recovery
// ===========================================================================

#[test]
fn test_settings_auto_migration_e2e_lifecycle() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join(".config/vrxx");
    let share_dir = temp_dir.path().join(".local/share/vrxx");
    let custom_key_path = share_dir.join(".key");

    // Force VRXX_KEY_PATH for isolated execution
    std::env::set_var("VRXX_KEY_PATH", custom_key_path.to_str().unwrap());

    let settings_file = config_dir.join("settings.json");
    let data_file = config_dir.join("data.dat");
    let bak_file = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(settings_file, data_file.clone());

    // 1. Create a legacy VRXXDAT1 container encrypted with machine-id
    let legacy_machine_id = vrxx::crypto::keystore::get_machine_id()
        .expect("machine-id should be readable on test system");
    let original_keys = sample_keys();
    let legacy_container = encrypt_keys_with_id(&original_keys, &legacy_machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_file, &legacy_container).unwrap();
    assert!(is_legacy_v1_container(&fs::read(&data_file).unwrap()));

    // 2. load_keys() should automatically decrypt legacy container and re-encrypt as VRXXDAT2
    let loaded_keys = manager.load_keys();
    assert_eq!(loaded_keys.len(), 2);
    assert_eq!(loaded_keys[0].name, "Frankfurt Reality");
    assert_eq!(loaded_keys[1].name, "Amsterdam Shadowsocks");

    // 3. Verify data.dat has been upgraded to VRXXDAT2
    let migrated_bytes = fs::read(&data_file).unwrap();
    assert!(
        is_v2_container(&migrated_bytes),
        "data.dat must be upgraded to VRXXDAT2"
    );
    assert!(!is_legacy_v1_container(&migrated_bytes));

    // 4. Verify data.dat.bak was created as VRXXDAT2
    assert!(
        bak_file.exists(),
        "data.dat.bak must be created during save_keys"
    );
    let bak_bytes = fs::read(&bak_file).unwrap();
    assert!(is_v2_container(&bak_bytes), "data.dat.bak must be VRXXDAT2");

    // 5. Verify file permissions
    #[cfg(unix)]
    {
        assert!(
            check_mode(&data_file, 0o600),
            "data.dat must have mode 0600"
        );
        assert!(
            check_mode(&bak_file, 0o600),
            "data.dat.bak must have mode 0600"
        );
        assert!(
            check_mode(&custom_key_path, 0o600),
            "Key file must have mode 0600"
        );
    }

    // 6. Verify subsequent load_keys() loads from VRXXDAT2 seamlessly
    let reload_keys = manager.load_keys();
    assert_eq!(reload_keys.len(), 2);
    assert_eq!(reload_keys[0].url, original_keys[0].url);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_settings_corrupted_primary_fallback_to_backup() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join(".config/vrxx");
    let custom_key_path = temp_dir.path().join(".local/share/vrxx/.key");
    std::env::set_var("VRXX_KEY_PATH", custom_key_path.to_str().unwrap());

    let settings_file = config_dir.join("settings.json");
    let data_file = config_dir.join("data.dat");
    let bak_file = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(settings_file, data_file.clone());

    // Save valid keys first
    let original_keys = sample_keys();
    manager.save_keys(&original_keys);
    assert!(data_file.exists());
    assert!(bak_file.exists());

    // Corrupt primary data.dat completely
    fs::write(&data_file, b"TOTAL_GARBAGE_DATA_CORRUPTION_HEADER").unwrap();

    // load_keys should detect corruption, fall back to bak_file, restore data.dat
    let recovered_keys = manager.load_keys();
    assert_eq!(recovered_keys.len(), 2);
    assert_eq!(recovered_keys[0].name, "Frankfurt Reality");

    // Verify data.dat was restored from backup and has mode 0600
    let restored_bytes = fs::read(&data_file).unwrap();
    assert!(is_v2_container(&restored_bytes));
    #[cfg(unix)]
    {
        assert!(check_mode(&data_file, 0o600));
    }

    std::env::remove_var("VRXX_KEY_PATH");
}
