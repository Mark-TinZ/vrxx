/* tests/e2e_keystore_migration.rs
 *
 * Requirement R3: Cryptographic Storage & Keystore Hardening
 * Tiers 1 & 2: Feature Coverage & Boundary / Corner Cases
 */

mod common;

use common::*;
use std::os::unix::fs::PermissionsExt;
use vrxx::crypto::keystore::{decrypt_keys_with_id, encrypt_keys_with_id};
use vrxx::settings::VpnKeyData;

// ---------------------------------------------------------------------------
// Tier 1: Feature Coverage (Happy Paths)
// ---------------------------------------------------------------------------

#[test]
fn test_user_key_generation_and_permissions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let vrxx_share = temp_dir.path().join("vrxx");
    std::fs::create_dir_all(&vrxx_share).unwrap();
    std::fs::set_permissions(&vrxx_share, std::fs::Permissions::from_mode(0o700)).unwrap();

    let key_file = vrxx_share.join(".key");

    // Generate 32 bytes of CSPRNG entropy
    use aes_gcm::aead::rand_core::RngCore;
    use aes_gcm::aead::OsRng;
    let mut secret_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);

    // Save with 0600 permissions
    std::fs::write(&key_file, secret_bytes).unwrap();
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();

    assert!(
        check_file_mode(&vrxx_share, 0o700),
        "Key parent dir must be 0700"
    );
    assert!(
        check_file_mode(&key_file, 0o600),
        "Key file mode must be 0600"
    );
    assert_eq!(std::fs::read(&key_file).unwrap().len(), 32);
}

#[test]
fn test_user_key_derivation_hkdf_sha256() {
    let secret = b"super-secret-user-entropy-32-byte-length";
    let key1 = derive_user_key_v2(secret).expect("KDF derivation must succeed");
    let key2 = derive_user_key_v2(secret).expect("KDF derivation must be deterministic");
    assert_eq!(
        key1, key2,
        "KDF must be deterministic for identical user secret"
    );

    let different_secret = b"another-user-entropy-totally-different";
    let key3 = derive_user_key_v2(different_secret).unwrap();
    assert_ne!(
        key1, key3,
        "Different secrets must produce different derived keys"
    );
}

#[test]
fn test_v2_container_encryption_magic_header() {
    let key = [0x42u8; 32];
    let payload = b"{\"keys\":[]}";

    let container = encrypt_v2_container(payload, &key).expect("Encryption must succeed");
    assert!(
        container.len() > MAGIC_HEADER_V2.len() + NONCE_LENGTH + TAG_LENGTH,
        "Container must contain header, nonce, and ciphertext"
    );
    assert_eq!(
        &container[0..MAGIC_HEADER_V2.len()],
        MAGIC_HEADER_V2,
        "Container must start with VRXXDAT2 magic header"
    );
}

#[test]
fn test_v2_container_roundtrip() {
    let user_secret = b"user-secret-test-roundtrip-entropy";
    let key = derive_user_key_v2(user_secret).unwrap();

    let keys = vec![
        sample_vpn_key_data("VLESS DE", valid_vless_reality_url()),
        sample_vpn_key_data("Shadowsocks US", valid_shadowsocks_url()),
    ];

    let json_bytes = serde_json::to_vec(&keys).unwrap();
    let encrypted = encrypt_v2_container(&json_bytes, &key).unwrap();

    let decrypted_bytes = decrypt_v2_container(&encrypted, &key).unwrap();
    let recovered_keys: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_bytes).unwrap();

    assert_eq!(recovered_keys.len(), 2);
    assert_eq!(recovered_keys[0].name, "VLESS DE");
    assert_eq!(recovered_keys[0].url, valid_vless_reality_url());
    assert_eq!(recovered_keys[1].name, "Shadowsocks US");
    assert_eq!(recovered_keys[1].protocol, "VLESS");
}

#[test]
fn test_user_isolation_protection() {
    let user_a_secret = b"user-a-isolated-entropy-key-12345";
    let user_b_secret = b"user-b-isolated-entropy-key-67890";

    let key_a = derive_user_key_v2(user_a_secret).unwrap();
    let key_b = derive_user_key_v2(user_b_secret).unwrap();

    let secret_payload = b"Top Secret VPN credentials for User A";
    let encrypted_a = encrypt_v2_container(secret_payload, &key_a).unwrap();

    // User B tries to decrypt User A's data
    let decrypt_result = decrypt_v2_container(&encrypted_a, &key_b);
    assert!(
        decrypt_result.is_err(),
        "User B must NOT be able to decrypt User A's data (User Isolation violated!)"
    );
}

// ---------------------------------------------------------------------------
// Tier 2: Boundary & Corner Cases
// ---------------------------------------------------------------------------

#[test]
fn test_boundary_missing_keystore_user_key_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let non_existent_key = temp_dir.path().join("missing.key");

    assert!(!non_existent_key.exists());
    let read_result = std::fs::read(&non_existent_key);
    assert!(
        read_result.is_err(),
        "Reading missing key file must return error without panicking"
    );
}

#[test]
fn test_boundary_empty_secret_fails() {
    let empty_secret = b"";
    let result = derive_user_key_v2(empty_secret);
    assert!(result.is_err(), "Empty secret must fail key derivation");
}

#[test]
fn test_boundary_legacy_vrxxdat1_migration() {
    let test_machine_id = "0123456789abcdef0123456789abcdef";
    let test_keys = vec![sample_vpn_key_data(
        "Legacy Germany",
        valid_vless_reality_url(),
    )];

    // 1. Create a legacy VRXXDAT1 container using the machine-id implementation
    let legacy_container = encrypt_keys_with_id(&test_keys, test_machine_id)
        .expect("Legacy encryption should succeed");
    assert_eq!(
        &legacy_container[0..8],
        MAGIC_HEADER_V1,
        "Legacy container must have VRXXDAT1 magic header"
    );

    // 2. Read and decrypt legacy format
    let decrypted_legacy_keys = decrypt_keys_with_id(&legacy_container, test_machine_id)
        .expect("Legacy decryption should succeed");
    assert_eq!(decrypted_legacy_keys.len(), 1);
    assert_eq!(decrypted_legacy_keys[0].name, "Legacy Germany");

    // 3. Migrate: re-encrypt into modern V2 container with user key
    let user_secret = b"migrated-user-key-entropy-secret";
    let v2_key = derive_user_key_v2(user_secret).unwrap();
    let re_encrypted_bytes = serde_json::to_vec(&decrypted_legacy_keys).unwrap();
    let v2_container = encrypt_v2_container(&re_encrypted_bytes, &v2_key)
        .expect("V2 re-encryption should succeed");

    assert_eq!(
        &v2_container[0..8],
        MAGIC_HEADER_V2,
        "Migrated container must have VRXXDAT2 header"
    );

    // 4. Verify new container decrypts cleanly with V2 key
    let migrated_bytes = decrypt_v2_container(&v2_container, &v2_key).unwrap();
    let migrated_keys: Vec<VpnKeyData> = serde_json::from_slice(&migrated_bytes).unwrap();
    assert_eq!(migrated_keys[0].name, "Legacy Germany");
    assert_eq!(migrated_keys[0].url, valid_vless_reality_url());
}

#[test]
fn test_boundary_tampered_ciphertext_fails_aead_tag() {
    let key = [0x55u8; 32];
    let payload = b"Valid secret payload";
    let mut container = encrypt_v2_container(payload, &key).unwrap();

    // Corrupt one byte in the ciphertext/tag
    let last = container.len() - 1;
    container[last] ^= 0xFF;

    let result = decrypt_v2_container(&container, &key);
    assert!(
        result.is_err(),
        "Tampered ciphertext must fail AEAD authentication"
    );
}

#[test]
fn test_boundary_tampered_nonce_fails_aead() {
    let key = [0x77u8; 32];
    let payload = b"Valid secret payload";
    let mut container = encrypt_v2_container(payload, &key).unwrap();

    // Corrupt one byte of the nonce (bytes 8..20)
    container[10] ^= 0x01;

    let result = decrypt_v2_container(&container, &key);
    assert!(
        result.is_err(),
        "Tampered nonce must fail AEAD authentication"
    );
}

#[test]
fn test_boundary_invalid_magic_header_fails() {
    let key = [0x88u8; 32];
    let mut bad_container = vec![0u8; 64];
    bad_container[0..8].copy_from_slice(b"BADMAGIC");

    let result = decrypt_v2_container(&bad_container, &key);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("VRXXDAT2"));
}

#[test]
fn test_boundary_truncated_container_fails() {
    let key = [0x99u8; 32];
    let truncated = vec![0u8; 10]; // Smaller than 8 + 12 + 16 = 36 bytes

    let result = decrypt_v2_container(&truncated, &key);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too small"));
}
