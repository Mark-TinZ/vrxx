/* tests/challenger_m3_migration_stress.rs
 *
 * Empirical Challenge & Stress Test Suite for Milestone M3
 * (Cryptographic Storage & Keystore Hardening)
 * Challenger 2: Legacy Migration, Concurrent Startup, and Corrupt Data Recovery
 *
 * Requirements & Dimensions:
 * 1. Legacy Auto-Migration:
 *    - Create legacy VRXXDAT1 container with `encrypt_keys_with_id`
 *    - Load via `SettingsManager::load_keys()`
 *    - Verify immediate on-disk conversion to valid VRXXDAT2 container with 0600 permissions
 *    - Verify data.dat.bak also updated with VRXXDAT2 and 0600 permissions
 *    - Verify parent directory is 0700
 *    - Verify full fidelity of all VPN profile fields and UTF-8 data
 * 2. Concurrent Key Creation & Startup:
 *    - High thread contention (64 threads) simultaneously generating key file
 *    - High process contention (16 separate OS processes) simultaneously generating key file
 *    - Verify zero race conditions, zero 0-byte files, zero crashes
 *    - Verify all threads and processes converge on the identical 256-bit key
 *    - Verify concurrent load_keys() during legacy migration under multi-thread load
 * 3. Corrupt Data Recovery & Backup Auto-Migration:
 *    - data.dat corrupted (garbage, 0-byte, truncated, tampered tag), backup is VRXXDAT2 -> restored
 *    - data.dat corrupted (garbage, 0-byte, truncated, tampered tag), backup is VRXXDAT1 -> restored AND auto-migrated
 *    - Both data.dat and backup corrupted -> safe fallback to empty Vec, zero panics
 *    - Missing data.dat, backup is VRXXDAT1 -> restored AND auto-migrated
 * 4. Keystore Container Direct Inspection & Low-Level API Verification:
 *    - decrypt_keys dual-version container dispatch
 *    - migrate_legacy_container functional verification
 *    - Permission boundaries and symlink rejection
 */

#![allow(unused_imports, dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use tempfile::TempDir;

use vrxx::crypto::keystore::{
    container_version, decrypt_keys, decrypt_v2_container, derive_user_key_v2, encrypt_keys,
    encrypt_keys_with_id, encrypt_v2_container, get_machine_id, get_or_create_user_key_at,
    is_legacy_v1_container, is_v2_container, migrate_legacy_container, MAGIC_HEADER_V1,
    MAGIC_HEADER_V2, MIN_CONTAINER_LENGTH,
};
use vrxx::settings::{SettingsManager, VpnKeyData};

/// Process-wide lock for tests that mutate process environment variables (`VRXX_KEY_PATH`).
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn create_sample_keys() -> Vec<VpnKeyData> {
    vec![
        VpnKeyData {
            name: "Frankfurt Reality 01".to_string(),
            protocol: "VLESS".to_string(),
            is_active: false,
            traffic_down: String::new(),
            traffic_up: String::new(),
            time_connected: String::new(),
            ping: "28 ms".to_string(),
            location: "DE".to_string(),
            country_code: "DE".to_string(),
            city: "Frankfurt".to_string(),
            isp: "Hetzner Online GmbH".to_string(),
            timezone: "Europe/Berlin".to_string(),
            url: "vless://11111111-2222-3333-4444-555555555555@198.51.100.10:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sid=1234&sni=de.example.com&flow=xtls-rprx-vision#DE-01".to_string(),
            group: "Europe-Tier1".to_string(),
        },
        VpnKeyData {
            name: "Amsterdam Shadowsocks 02".to_string(),
            protocol: "Shadowsocks".to_string(),
            is_active: false,
            traffic_down: String::new(),
            traffic_up: String::new(),
            time_connected: String::new(),
            ping: "35 ms".to_string(),
            location: "NL".to_string(),
            country_code: "NL".to_string(),
            city: "Amsterdam".to_string(),
            isp: "DigitalOcean LLC".to_string(),
            timezone: "Europe/Amsterdam".to_string(),
            url: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZDEyMw@198.51.100.20:8388#NL-02".to_string(),
            group: "Europe-Tier1".to_string(),
        },
        VpnKeyData {
            name: "Tokyo VMess 03 (Unicode: 東京-高速サーバー 🚀)".to_string(),
            protocol: "VMess".to_string(),
            is_active: false,
            traffic_down: String::new(),
            traffic_up: String::new(),
            time_connected: String::new(),
            ping: "180 ms".to_string(),
            location: "JP".to_string(),
            country_code: "JP".to_string(),
            city: "Tokyo".to_string(),
            isp: "Linode".to_string(),
            timezone: "Asia/Tokyo".to_string(),
            url: "vmess://eyJhZGQiOiIxOTguNTEuMTAwLjMwIiwiYWlkIjowLCJpZCI6ImQzYjA3Mzg0LWQxMTMtNDI2Yi04N2I2LTk2M2QwYzMyNjA4NCIsIm5ldCI6IndzIiwicGF0aCI6Ii92bWVzcyIsInBvcnQiOjQ0MywicHMiOiJKUC0wMyIsInRscyI6InRscyIsInYiOjJ9".to_string(),
            group: "Asia-Pacific".to_string(),
        },
    ]
}

// ===========================================================================
// Multi-Process Child Worker Function
// ===========================================================================

#[test]
fn helper_child_process_worker() {
    let target = match std::env::var("VRXX_CHALLENGER_CHILD_KEY_PATH") {
        Ok(val) => val,
        Err(_) => return, // Not running as child worker, exit test immediately
    };
    let path = PathBuf::from(target);
    match get_or_create_user_key_at(&path) {
        Ok(key) => {
            let hex = key.iter().map(|b| format!("{:02x}", b)).collect::<String>();
            println!("KEY_HEX_OUTPUT:{}", hex);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("KEY_ERROR:{}", e);
            std::process::exit(1);
        }
    }
}

// ===========================================================================
// Dimension 1: Legacy Auto-Migration Empirical Verification
// ===========================================================================

#[test]
fn test_legacy_auto_migration_converts_disk_to_vrxxdat2_with_0600() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("isolated_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("config_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    // 1. Generate legacy VRXXDAT1 container encrypted with current machine-id
    let machine_id = get_machine_id().expect("Machine ID must be available on Linux");
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id)
        .expect("Legacy encryption with machine-id should succeed");

    assert!(is_legacy_v1_container(&legacy_encrypted));
    assert_eq!(&legacy_encrypted[0..8], MAGIC_HEADER_V1);

    // Write legacy container to disk with loose permissions 0644
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &legacy_encrypted).unwrap();
    fs::set_permissions(&data_path, fs::Permissions::from_mode(0o644)).unwrap();

    // Verify initial state
    let initial_bytes = fs::read(&data_path).unwrap();
    assert_eq!(&initial_bytes[0..8], MAGIC_HEADER_V1);
    assert_eq!(container_version(&initial_bytes).unwrap(), 1);

    // 2. Trigger load_keys()
    let loaded_keys = manager.load_keys();

    // 3. Verify returned keys match verbatim
    assert_eq!(loaded_keys.len(), initial_keys.len());
    for (loaded, initial) in loaded_keys.iter().zip(initial_keys.iter()) {
        assert_eq!(loaded.name, initial.name);
        assert_eq!(loaded.protocol, initial.protocol);
        assert_eq!(loaded.url, initial.url);
        assert_eq!(loaded.group, initial.group);
        assert_eq!(loaded.location, initial.location);
        assert_eq!(loaded.country_code, initial.country_code);
        assert_eq!(loaded.city, initial.city);
        assert_eq!(loaded.isp, initial.isp);
        assert_eq!(loaded.timezone, initial.timezone);
        assert_eq!(loaded.ping, initial.ping);
    }

    // 4. Verify data.dat on disk is IMMEDIATELY converted to VRXXDAT2
    assert!(data_path.exists());
    let migrated_bytes = fs::read(&data_path).unwrap();
    assert_eq!(
        &migrated_bytes[0..8],
        MAGIC_HEADER_V2,
        "File on disk must have been automatically converted to VRXXDAT2"
    );
    assert_eq!(
        container_version(&migrated_bytes).unwrap(),
        2,
        "Container version must be 2"
    );

    // 5. Verify file permissions are strictly 0600 and parent directory 0700
    let mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "data.dat permissions must be hardened to 0600");

    let dir_mode = fs::metadata(&config_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "Parent directory permissions must be 0700");

    // 6. Verify backup data.dat.bak was created as VRXXDAT2 with 0600 mode
    assert!(bak_path.exists(), "data.dat.bak should exist");
    let bak_bytes = fs::read(&bak_path).unwrap();
    assert_eq!(
        &bak_bytes[0..8],
        MAGIC_HEADER_V2,
        "Backup file must also be converted to VRXXDAT2"
    );
    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600, "Backup file permissions must be 0600");

    // 7. Verify isolated user key was created with 0600 mode
    assert!(
        isolated_key_path.exists(),
        "User key must have been created"
    );
    let key_mode = fs::metadata(&isolated_key_path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(key_mode, 0o600, "User key file must have 0600 permissions");

    // 8. Re-load from disk: verify subsequent loads succeed cleanly without legacy migration
    let second_load = manager.load_keys();
    assert_eq!(second_load.len(), initial_keys.len());
    assert_eq!(second_load[0].name, initial_keys[0].name);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_legacy_auto_migration_preserves_empty_profile_list() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("empty_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("empty_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let machine_id = get_machine_id().unwrap();
    let empty_keys: Vec<VpnKeyData> = Vec::new();
    let legacy_encrypted = encrypt_keys_with_id(&empty_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &legacy_encrypted).unwrap();

    let loaded = manager.load_keys();
    assert!(loaded.is_empty(), "Empty profile list must remain empty");

    let on_disk = fs::read(&data_path).unwrap();
    assert_eq!(&on_disk[0..8], MAGIC_HEADER_V2);
    assert_eq!(container_version(&on_disk).unwrap(), 2);

    let mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    assert!(bak_path.exists());
    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_legacy_auto_migration_decrypted_with_specific_user_key() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("specific_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("specific_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &legacy_encrypted).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());

    // Read the generated user key directly
    let raw_secret = get_or_create_user_key_at(&isolated_key_path).unwrap();
    let derived_aes_key = derive_user_key_v2(&raw_secret).unwrap();

    // Verify the file on disk can be directly decrypted using this exact derived key
    let disk_bytes = fs::read(&data_path).unwrap();
    let decrypted_payload = decrypt_v2_container(&disk_bytes, &derived_aes_key)
        .expect("Disk bytes must decrypt cleanly with the user key derived from isolated_key_path");
    let parsed: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_payload).unwrap();
    assert_eq!(parsed.len(), initial_keys.len());
    assert_eq!(parsed[0].name, initial_keys[0].name);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_migrate_legacy_container_low_level_api() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("lowlevel_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_bytes = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    // Direct low-level migrate
    let migrated = migrate_legacy_container(&legacy_bytes, &machine_id)
        .expect("migrate_legacy_container should succeed");
    assert!(is_v2_container(&migrated));
    assert_eq!(&migrated[0..8], MAGIC_HEADER_V2);
    assert_eq!(container_version(&migrated).unwrap(), 2);

    // Passing a container that is already V2 should return unmodified
    let second_migrate = migrate_legacy_container(&migrated, &machine_id).unwrap();
    assert_eq!(second_migrate, migrated);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_decrypt_keys_dual_dispatch() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("dual_dispatch.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let initial_keys = create_sample_keys();

    // 1. Decrypt legacy container
    let machine_id = get_machine_id().unwrap();
    let legacy_bytes = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();
    let decrypted_from_legacy = decrypt_keys(&legacy_bytes).expect("decrypt_keys on legacy");
    assert_eq!(decrypted_from_legacy.len(), initial_keys.len());

    // 2. Decrypt modern V2 container
    let v2_bytes = encrypt_keys(&initial_keys).expect("encrypt_keys v2");
    assert!(is_v2_container(&v2_bytes));
    let decrypted_from_v2 = decrypt_keys(&v2_bytes).expect("decrypt_keys on v2");
    assert_eq!(decrypted_from_v2.len(), initial_keys.len());

    // 3. Truncated container (< MIN_CONTAINER_LENGTH)
    let short_bytes = vec![0u8; MIN_CONTAINER_LENGTH - 1];
    let err_short = decrypt_keys(&short_bytes);
    assert!(err_short.is_err());
    assert!(err_short.unwrap_err().to_string().contains("too small"));

    // 4. Invalid header
    let mut bad_header = vec![0u8; 64];
    bad_header[0..8].copy_from_slice(b"UNKNOWN1");
    let err_bad = decrypt_keys(&bad_header);
    assert!(err_bad.is_err());
    assert!(err_bad
        .unwrap_err()
        .to_string()
        .contains("Неверная сигнатура"));

    std::env::remove_var("VRXX_KEY_PATH");
}

// ===========================================================================
// Dimension 2: Concurrent Key Creation Empirical Stress Testing
// ===========================================================================

#[test]
fn test_concurrent_threads_generating_key_file_simultaneously() {
    let temp_dir = TempDir::new().unwrap();
    let sub_dir = temp_dir.path().join("concurrent_threads").join("vrxx");
    let key_path = sub_dir.join(".key");

    assert!(!key_path.exists());
    assert!(!sub_dir.exists());

    let num_threads = 64;
    let barrier = Arc::new(Barrier::new(num_threads));
    let path_arc = Arc::new(key_path.clone());
    let results = Arc::new(Mutex::new(Vec::with_capacity(num_threads)));

    let mut handles = Vec::with_capacity(num_threads);
    for _ in 0..num_threads {
        let b = Arc::clone(&barrier);
        let p = Arc::clone(&path_arc);
        let r = Arc::clone(&results);

        let handle = std::thread::spawn(move || {
            // Wait for all threads to align
            b.wait();
            // All threads simultaneously invoke get_or_create_user_key_at
            let res = get_or_create_user_key_at(&p);
            r.lock().unwrap().push(res);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let collected = results.lock().unwrap();
    assert_eq!(collected.len(), num_threads);

    // Check that every single thread succeeded without error
    for (i, res) in collected.iter().enumerate() {
        assert!(
            res.is_ok(),
            "Thread {} failed to get or create user key: {:?}",
            i,
            res.as_ref().err()
        );
    }

    // Check that every single thread returned the exact same key
    let first_key = collected[0].as_ref().unwrap();
    for (i, res) in collected.iter().enumerate() {
        let key = res.as_ref().unwrap();
        assert_eq!(
            key, first_key,
            "Thread {} returned divergent key {:?} vs first key {:?}",
            i, key, first_key
        );
    }

    // Inspect file on disk
    assert!(key_path.exists(), "Key file must exist on disk");
    let mode = fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "Key file must have 0600 permissions");

    let dir_mode = fs::metadata(&sub_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "Key directory must have 0700 permissions");

    let disk_raw = fs::read(&key_path).unwrap();
    assert!(
        !disk_raw.is_empty(),
        "Key file must NOT be empty (zero-byte corruption!)"
    );
    let disk_str = String::from_utf8(disk_raw).expect("Key file must contain valid UTF-8 hex");
    let trimmed = disk_str.trim();
    assert_eq!(
        trimmed.len(),
        64,
        "Key hex string must be exactly 64 characters"
    );

    // Verify no leftover .key.tmp files in directory
    for entry in fs::read_dir(&sub_dir).unwrap() {
        let entry = entry.unwrap();
        let fname = entry.file_name().to_string_lossy().to_string();
        assert!(
            !fname.starts_with(".key.tmp"),
            "Leftover temporary file detected: {}",
            fname
        );
    }
}

#[test]
fn test_concurrent_threads_rapid_read_after_creation() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("rapid").join(".key");

    // Initialize key
    let initial_key = get_or_create_user_key_at(&key_path).unwrap();

    let num_threads = 32;
    let iterations_per_thread = 25;
    let barrier = Arc::new(Barrier::new(num_threads));
    let path_arc = Arc::new(key_path.clone());
    let mismatch_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let b = Arc::clone(&barrier);
        let p = Arc::clone(&path_arc);
        let mismatches = Arc::clone(&mismatch_count);
        let expected = initial_key;

        let handle = std::thread::spawn(move || {
            b.wait();
            for _ in 0..iterations_per_thread {
                match get_or_create_user_key_at(&p) {
                    Ok(k) => {
                        if k != expected {
                            mismatches.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Err(_) => {
                        mismatches.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        mismatch_count.load(Ordering::SeqCst),
        0,
        "Rapid read contention must produce zero errors or mismatches"
    );
}

#[test]
fn test_concurrent_processes_generating_key_file_simultaneously() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("multiprocess").join(".key");
    let current_exe = std::env::current_exe().expect("Failed to get current_exe");

    let num_processes = 16;
    let mut children = Vec::with_capacity(num_processes);

    // Launch 16 separate OS child processes simultaneously
    for _ in 0..num_processes {
        let child = std::process::Command::new(&current_exe)
            .arg("helper_child_process_worker")
            .arg("--nocapture")
            .env("VRXX_CHALLENGER_CHILD_KEY_PATH", &key_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");
        children.push(child);
    }

    let mut keys = Vec::new();
    for (i, child) in children.into_iter().enumerate() {
        let output = child
            .wait_with_output()
            .expect("Failed to wait on child process");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success(),
            "Child process {} failed (status {:?}): stderr: {}, stdout: {}",
            i,
            output.status.code(),
            stderr,
            stdout
        );

        let key_line = stdout
            .lines()
            .find(|l| l.starts_with("KEY_HEX_OUTPUT:"))
            .expect("Child process did not emit KEY_HEX_OUTPUT");
        let hex = key_line.trim_start_matches("KEY_HEX_OUTPUT:").trim();
        assert_eq!(hex.len(), 64, "Key hex must be exactly 64 characters");
        keys.push(hex.to_string());
    }

    assert_eq!(keys.len(), num_processes);
    let first_key = &keys[0];
    for (i, key) in keys.iter().enumerate() {
        assert_eq!(
            key, first_key,
            "Process {} produced divergent key {} vs first key {}",
            i, key, first_key
        );
    }

    // Verify key on disk
    assert!(key_path.exists());
    let mode = fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "Key file permissions must be 0600");
}

#[test]
fn test_concurrent_startup_settings_manager_load_keys_legacy_migration() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("concurrent_migration_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("concurrent_migration_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &legacy_encrypted).unwrap();

    let num_threads = 24;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mgr_arc = Arc::new(SettingsManager::with_paths(config_path, data_path.clone()));
    let thread_keys = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let b = Arc::clone(&barrier);
        let m = Arc::clone(&mgr_arc);
        let tk = Arc::clone(&thread_keys);

        let handle = std::thread::spawn(move || {
            b.wait();
            let keys = m.load_keys();
            tk.lock().unwrap().push(keys);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let all_keys = thread_keys.lock().unwrap();
    assert_eq!(all_keys.len(), num_threads);

    for (i, keys) in all_keys.iter().enumerate() {
        assert_eq!(
            keys.len(),
            initial_keys.len(),
            "Thread {} loaded incorrect number of keys",
            i
        );
        assert_eq!(keys[0].name, initial_keys[0].name);
    }

    // Verify disk state
    let final_bytes = fs::read(&data_path).unwrap();
    assert_eq!(
        &final_bytes[0..8],
        MAGIC_HEADER_V2,
        "File must be migrated to VRXXDAT2"
    );
    let mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    std::env::remove_var("VRXX_KEY_PATH");
}

// ===========================================================================
// Dimension 3: Corrupt Data Recovery & Backup Auto-Migration
// ===========================================================================

#[test]
fn test_corrupt_data_recovery_backup_is_vrxxdat2() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("backup_v2_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("backup_v2_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let initial_keys = create_sample_keys();
    let user_secret = get_or_create_user_key_at(&isolated_key_path).unwrap();
    let aes_key = derive_user_key_v2(&user_secret).unwrap();
    let valid_v2_container =
        encrypt_v2_container(&serde_json::to_vec(&initial_keys).unwrap(), &aes_key).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    // Primary is corrupted with random garbage
    fs::write(
        &data_path,
        b"random corrupted non-crypto bytes that cannot decrypt",
    )
    .unwrap();
    // Backup is valid VRXXDAT2
    fs::write(&bak_path, &valid_v2_container).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());
    assert_eq!(loaded[0].name, initial_keys[0].name);

    // Verify data.dat was restored from backup and hardened
    assert!(data_path.exists());
    let restored_bytes = fs::read(&data_path).unwrap();
    assert_eq!(&restored_bytes[0..8], MAGIC_HEADER_V2);
    let mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_recovery_backup_is_legacy_vrxxdat1_auto_migrates() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("backup_v1_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("backup_v1_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    // Primary is corrupted
    fs::write(&data_path, b"corrupted bytes in primary container").unwrap();
    // Backup is legacy V1
    fs::write(&bak_path, &legacy_encrypted).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());
    assert_eq!(loaded[0].name, initial_keys[0].name);

    // Primary data.dat must now be migrated to VRXXDAT2
    let restored_primary = fs::read(&data_path).unwrap();
    assert_eq!(
        &restored_primary[0..8],
        MAGIC_HEADER_V2,
        "Restored primary must be upgraded to VRXXDAT2"
    );
    let primary_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(primary_mode, 0o600);

    // Backup data.dat.bak must also be updated to VRXXDAT2
    let restored_bak = fs::read(&bak_path).unwrap();
    assert_eq!(
        &restored_bak[0..8],
        MAGIC_HEADER_V2,
        "Backup must also be upgraded to VRXXDAT2"
    );
    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(bak_mode, 0o600);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_recovery_zero_byte_primary_file() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("zerobyte_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("zerobyte_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let initial_keys = create_sample_keys();
    let user_secret = get_or_create_user_key_at(&isolated_key_path).unwrap();
    let aes_key = derive_user_key_v2(&user_secret).unwrap();
    let valid_container =
        encrypt_v2_container(&serde_json::to_vec(&initial_keys).unwrap(), &aes_key).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    // 0-byte corrupted primary file
    fs::write(&data_path, b"").unwrap();
    // Valid backup
    fs::write(&bak_path, &valid_container).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());

    let restored = fs::read(&data_path).unwrap();
    assert_eq!(&restored[0..8], MAGIC_HEADER_V2);
    assert!(!restored.is_empty());

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_recovery_truncated_primary_file() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("trunc_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("trunc_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    // Truncated primary file (< 36 bytes)
    fs::write(&data_path, &legacy_encrypted[0..20]).unwrap();
    // Valid legacy backup
    fs::write(&bak_path, &legacy_encrypted).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());

    // Both files now VRXXDAT2
    let p_bytes = fs::read(&data_path).unwrap();
    assert_eq!(&p_bytes[0..8], MAGIC_HEADER_V2);

    let b_bytes = fs::read(&bak_path).unwrap();
    assert_eq!(&b_bytes[0..8], MAGIC_HEADER_V2);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_recovery_tampered_aead_tag_in_primary() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("tampered_tag_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("tampered_tag_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let initial_keys = create_sample_keys();
    let user_secret = get_or_create_user_key_at(&isolated_key_path).unwrap();
    let aes_key = derive_user_key_v2(&user_secret).unwrap();
    let mut tampered_container =
        encrypt_v2_container(&serde_json::to_vec(&initial_keys).unwrap(), &aes_key).unwrap();

    let valid_container = tampered_container.clone();

    // Corrupt last byte (AEAD tag)
    let last = tampered_container.len() - 1;
    tampered_container[last] ^= 0xAA;

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, &tampered_container).unwrap();
    fs::write(&bak_path, &valid_container).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());

    let restored = fs::read(&data_path).unwrap();
    assert_eq!(&restored, &valid_container);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_recovery_primary_missing_backup_is_legacy_vrxxdat1() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("missing_primary_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("missing_primary_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    let machine_id = get_machine_id().unwrap();
    let initial_keys = create_sample_keys();
    let legacy_encrypted = encrypt_keys_with_id(&initial_keys, &machine_id).unwrap();

    fs::create_dir_all(&config_dir).unwrap();
    // data.dat does NOT exist
    assert!(!data_path.exists());
    // data.dat.bak exists as legacy VRXXDAT1
    fs::write(&bak_path, &legacy_encrypted).unwrap();

    let loaded = manager.load_keys();
    assert_eq!(loaded.len(), initial_keys.len());

    // Both files now exist as VRXXDAT2
    assert!(data_path.exists());
    let primary_bytes = fs::read(&data_path).unwrap();
    assert_eq!(&primary_bytes[0..8], MAGIC_HEADER_V2);

    let bak_bytes = fs::read(&bak_path).unwrap();
    assert_eq!(&bak_bytes[0..8], MAGIC_HEADER_V2);

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_corrupt_data_both_primary_and_backup_corrupted_fails_gracefully() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("both_corrupt_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path, data_path.clone());

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&data_path, b"total junk primary").unwrap();
    fs::write(&bak_path, b"total junk backup").unwrap();

    // Must return empty vector without panicking
    let loaded = manager.load_keys();
    assert!(
        loaded.is_empty(),
        "Both corrupt must gracefully return empty Vec"
    );
}

// ===========================================================================
// Dimension 4: Permission Boundaries & Symlink Rejection
// ===========================================================================

#[test]
fn test_get_or_create_user_key_at_symlink_rejection() {
    let temp_dir = TempDir::new().unwrap();
    let real_file = temp_dir.path().join("real_target.key");
    fs::write(
        &real_file,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .unwrap();

    let symlink_path = temp_dir.path().join("symlink_attack.key");
    std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

    let result = get_or_create_user_key_at(&symlink_path);
    assert!(
        result.is_err(),
        "Symlink at key path must be strictly rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("символической ссылкой") || err_msg.contains("symlink"),
        "Error message should mention symlink: {}",
        err_msg
    );
}

#[test]
fn test_settings_manager_directory_and_file_permissions_hardening() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let isolated_key_path = temp_dir.path().join("hardening_user.key");
    std::env::set_var("VRXX_KEY_PATH", &isolated_key_path);

    let config_dir = temp_dir.path().join("hardening_vrxx");
    let config_path = config_dir.join("settings.json");
    let data_path = config_dir.join("data.dat");
    let bak_path = config_dir.join("data.dat.bak");

    let manager = SettingsManager::with_paths(config_path.clone(), data_path.clone());

    // Create directory with wide permissions 0777
    fs::create_dir_all(&config_dir).unwrap();
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o777)).unwrap();

    let initial_keys = create_sample_keys();
    manager.save_keys(&initial_keys);

    let dir_mode = fs::metadata(&config_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        dir_mode, 0o700,
        "Directory permissions must be hardened to 0700"
    );

    let data_mode = fs::metadata(&data_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        data_mode, 0o600,
        "data.dat permissions must be hardened to 0600"
    );

    let bak_mode = fs::metadata(&bak_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        bak_mode, 0o600,
        "data.dat.bak permissions must be hardened to 0600"
    );

    std::env::remove_var("VRXX_KEY_PATH");
}

#[test]
fn test_existing_key_file_with_loose_permissions_is_hardened() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("loose_key.key");
    let raw_key = [0x42u8; 32];
    fs::write(&key_path, raw_key).unwrap();
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o666)).unwrap();

    let read_key = get_or_create_user_key_at(&key_path).expect("Read loose key");
    assert_eq!(read_key, raw_key);

    let mode = fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "Permissions must be automatically hardened to 0600"
    );
}
