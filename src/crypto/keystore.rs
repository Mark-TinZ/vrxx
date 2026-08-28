/* keystore.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Защищенное хранилище профилей (Host-Bound Keystore)
//!
//! Реализует криптографическую защиту ключей и профилей подключения с привязкой к хосту:
//! - Чтение системного 128-битного UUID (`/etc/machine-id` или `/var/lib/dbus/machine-id`)
//! - Деривация 256-битного ключа шифрования: `HKDF-SHA256(HMAC-SHA256(AppID, machine-id), Salt)`
//! - Аутентифицированное симметричное шифрование AES-256-GCM (AEAD) с валидацией целостности
//! - Контейнерный бинарный формат `VRXXDAT1` с 12-байтовым случайным Nonce

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fs;

use crate::settings::VpnKeyData;

type HmacSha256 = Hmac<Sha256>;

/// Магический заголовок контейнера зашифрованных данных (версия 1)
pub const MAGIC_HEADER: &[u8; 8] = b"VRXXDAT1";

/// Длина вектора инициализации (Nonce) для AES-256-GCM
pub const NONCE_LENGTH: usize = 12;

/// Длина криптографического тега аутентификации Poly1305 / GCM
pub const TAG_LENGTH: usize = 16;

/// Уникальный идентификатор приложения для изоляции ключа
const APP_UNIQUE_ID: &[u8] = b"ru.mark.vrxx.secure-keystore.v1";

/// Константная соль приложения для деривации ключа
const APP_SALT: &[u8] = b"vrxx-host-bound-salt-2026";

/// Контекст расширения ключа HKDF
const HKDF_INFO: &[u8] = b"vrxx-aes-256-gcm-data-key";

/// Считывает системный идентификатор машины без необходимости прав суперпользователя (root).
///
/// Источники:
/// 1. `/etc/machine-id` (стандартный 128-битный UUID systemd, права 0444)
/// 2. `/var/lib/dbus/machine-id` (резервный путь для D-Bus)
pub fn get_machine_id() -> anyhow::Result<String> {
    let candidate_paths = ["/etc/machine-id", "/var/lib/dbus/machine-id"];

    for path in &candidate_paths {
        if let Ok(content) = fs::read_to_string(path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    anyhow::bail!("Не удалось получить machine-id: файлы /etc/machine-id и /var/lib/dbus/machine-id отсутствуют или пусты")
}

/// Вычисляет 256-битный ключ шифрования для переданного `machine_id` с использованием HMAC-SHA256 и HKDF.
pub fn derive_key_from_machine_id(machine_id: &str) -> anyhow::Result<[u8; 32]> {
    let trimmed_id = machine_id.trim();
    if trimmed_id.is_empty() {
        anyhow::bail!("Идентификатор machine-id не может быть пустым");
    }

    // 1. Извлечение псевдослучайного ключа (PRK) через HMAC-SHA256
    let mut mac = <HmacSha256 as Mac>::new_from_slice(APP_UNIQUE_ID)
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации HMAC: {e}"))?;
    mac.update(trimmed_id.as_bytes());
    let prk = mac.finalize().into_bytes();

    // 2. Растяжение ключа через HKDF-SHA256 в 32-байтовый ключ AES-256
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(APP_SALT), &prk);
    let mut derived_key = [0u8; 32];
    hkdf.expand(HKDF_INFO, &mut derived_key)
        .map_err(|e| anyhow::anyhow!("Ошибка расширения ключа HKDF: {e}"))?;

    Ok(derived_key)
}

/// Вычисляет 256-битный ключ шифрования на основе текущего системного `machine-id`.
pub fn derive_host_key() -> anyhow::Result<[u8; 32]> {
    let machine_id = get_machine_id()?;
    derive_key_from_machine_id(&machine_id)
}

/// Шифрует список VPN-профилей с использованием указанного `machine_id`.
pub fn encrypt_keys_with_id(keys: &[VpnKeyData], machine_id: &str) -> anyhow::Result<Vec<u8>> {
    use aes_gcm::aead::rand_core::RngCore;

    let key_bytes = derive_key_from_machine_id(machine_id)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Ошибка создания шифра AES-GCM: {e}"))?;

    let json_bytes = serde_json::to_vec(keys)
        .map_err(|e| anyhow::anyhow!("Ошибка сериализации ключей в JSON: {e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, json_bytes.as_ref())
        .map_err(|e| anyhow::anyhow!("Ошибка симметричного шифрования AES-GCM: {e}"))?;

    // Сборка бинарного контейнера: Magic (8 байт) + Nonce (12 байт) + Ciphertext + Tag (16 байт)
    let mut container = Vec::with_capacity(MAGIC_HEADER.len() + NONCE_LENGTH + ciphertext.len());
    container.extend_from_slice(MAGIC_HEADER);
    container.extend_from_slice(&nonce_bytes);
    container.extend_from_slice(&ciphertext);

    Ok(container)
}

/// Шифрует список VPN-профилей с привязкой к текущей системе.
pub fn encrypt_keys(keys: &[VpnKeyData]) -> anyhow::Result<Vec<u8>> {
    let machine_id = get_machine_id()?;
    encrypt_keys_with_id(keys, &machine_id)
}

/// Расшифровывает бинарный контейнер `data.dat` с использованием указанного `machine_id`.
pub fn decrypt_keys_with_id(raw_data: &[u8], machine_id: &str) -> anyhow::Result<Vec<VpnKeyData>> {
    let min_expected_len = MAGIC_HEADER.len() + NONCE_LENGTH + TAG_LENGTH;
    if raw_data.len() < min_expected_len {
        anyhow::bail!(
            "Файл data.dat поврежден: размер данных ({} байт) меньше минимального ({min_expected_len} байт)",
            raw_data.len()
        );
    }

    // Проверка магического заголовка
    if &raw_data[0..MAGIC_HEADER.len()] != MAGIC_HEADER {
        anyhow::bail!("Неверная сигнатура контейнера data.dat (ожидалась 'VRXXDAT1')");
    }

    let nonce_start = MAGIC_HEADER.len();
    let nonce_end = nonce_start + NONCE_LENGTH;
    let nonce = Nonce::from_slice(&raw_data[nonce_start..nonce_end]);
    let ciphertext = &raw_data[nonce_end..];

    let key_bytes = derive_key_from_machine_id(machine_id)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Ошибка создания шифра AES-GCM: {e}"))?;

    let decrypted_bytes = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        anyhow::anyhow!(
            "Не удалось расшифровать ключи: нарушена целостность данных или файл перенесен с другой системы (не совпадает machine-id)"
        )
    })?;

    let keys: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_bytes)
        .map_err(|e| anyhow::anyhow!("Ошибка десериализации расшифрованного JSON: {e}"))?;

    Ok(keys)
}

/// Расшифровывает бинарный контейнер `data.dat` с использованием текущего системного `machine-id`.
pub fn decrypt_keys(raw_data: &[u8]) -> anyhow::Result<Vec<VpnKeyData>> {
    let machine_id = get_machine_id()?;
    decrypt_keys_with_id(raw_data, &machine_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_keys() -> Vec<VpnKeyData> {
        vec![
            VpnKeyData {
                name: "Germany High-Speed".to_string(),
                protocol: "VLESS".to_string(),
                is_active: false,
                traffic_down: "0 MB".to_string(),
                traffic_up: "0 MB".to_string(),
                time_connected: "00:00:00".to_string(),
                ping: "45 ms".to_string(),
                location: "DE".to_string(),
                timezone: "UTC+1".to_string(),
                url: "vless://test-uuid@1.2.3.4:443?security=reality&sni=example.com#Germany"
                    .to_string(),
            },
            VpnKeyData {
                name: "Netherlands Backup".to_string(),
                protocol: "Shadowsocks".to_string(),
                is_active: true,
                traffic_down: "10 MB".to_string(),
                traffic_up: "2 MB".to_string(),
                time_connected: "01:23:45".to_string(),
                ping: "38 ms".to_string(),
                location: "NL".to_string(),
                timezone: "UTC+1".to_string(),
                url: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNz@5.6.7.8:8388#Netherlands"
                    .to_string(),
            },
        ]
    }

    #[test]
    fn test_keystore_encrypt_decrypt_roundtrip() {
        let keys = create_test_keys();
        let test_machine_id = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

        let encrypted =
            encrypt_keys_with_id(&keys, test_machine_id).expect("Шифрование должно пройти успешно");
        assert!(encrypted.len() > MAGIC_HEADER.len() + NONCE_LENGTH + TAG_LENGTH);
        assert_eq!(&encrypted[0..8], MAGIC_HEADER);

        let decrypted = decrypt_keys_with_id(&encrypted, test_machine_id)
            .expect("Дешифрование должно пройти успешно");
        assert_eq!(decrypted.len(), 2);
        assert_eq!(decrypted[0].name, "Germany High-Speed");
        assert_eq!(decrypted[0].url, keys[0].url);
        assert_eq!(decrypted[1].name, "Netherlands Backup");
        assert_eq!(decrypted[1].protocol, "Shadowsocks");
    }

    #[test]
    fn test_keystore_wrong_machine_id_fails() {
        let keys = create_test_keys();
        let original_machine_id = "11111111111111111111111111111111";
        let foreign_machine_id = "22222222222222222222222222222222";

        let encrypted = encrypt_keys_with_id(&keys, original_machine_id).unwrap();
        let decrypt_result = decrypt_keys_with_id(&encrypted, foreign_machine_id);

        assert!(
            decrypt_result.is_err(),
            "Попытка расшифровать данные с чужим machine-id должна возвращать ошибку"
        );
    }

    #[test]
    fn test_keystore_tampered_payload_fails() {
        let keys = create_test_keys();
        let test_machine_id = "33333333333333333333333333333333";

        let mut encrypted = encrypt_keys_with_id(&keys, test_machine_id).unwrap();
        let last_idx = encrypted.len() - 1;
        encrypted[last_idx] ^= 0xFF; // Искажаем 1 байт в зашифрованных данных или теге

        let decrypt_result = decrypt_keys_with_id(&encrypted, test_machine_id);
        assert!(
            decrypt_result.is_err(),
            "Поврежденные данные не должны проходить валидацию AEAD-тега"
        );
    }

    #[test]
    fn test_keystore_invalid_magic_fails() {
        let mut bad_data = vec![0u8; 64];
        bad_data[0..8].copy_from_slice(b"BADMAGIC");

        let result = decrypt_keys_with_id(&bad_data, "any-id");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("VRXXDAT1"));
    }
}
