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

//! # Защищенное хранилище профилей (User-Isolated Keystore)
//!
//! Реализует криптографическую защиту ключей и профилей подключения с изоляцией пользователей:
//! - Генерация и хранение 256-битного криптостойкого секрета в `~/.local/share/vrxx/.key` (права 0600, директория 0700)
//! - Деривация ключа KDF v2: `HKDF-SHA256(HMAC-SHA256(AppID_v2, user_secret), Salt_v2, Info_v2)`
//! - Аутентифицированное симметричное шифрование AES-256-GCM (AEAD) с валидацией целостности
//! - Контейнерный бинарный формат `VRXXDAT2` с 12-байтовым случайным Nonce
//! - Обратная совместимость и прозрачная дешифрация устаревших контейнеров `VRXXDAT1` (на базе `machine-id`)

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::VpnKeyData;

type HmacSha256 = Hmac<Sha256>;

/// Магический заголовок устаревшего контейнера зашифрованных данных (версия 1)
pub const MAGIC_HEADER_V1: &[u8; 8] = b"VRXXDAT1";

/// Магический заголовок современного контейнера зашифрованных данных (версия 2)
pub const MAGIC_HEADER_V2: &[u8; 8] = b"VRXXDAT2";

/// Магический заголовок по умолчанию для новых контейнеров
pub const MAGIC_HEADER: &[u8; 8] = MAGIC_HEADER_V2;

/// Длина вектора инициализации (Nonce) для AES-256-GCM
pub const NONCE_LENGTH: usize = 12;

/// Длина криптографического тега аутентификации Poly1305 / GCM
pub const TAG_LENGTH: usize = 16;

/// Минимальная длина валидного контейнера (Заголовок 8B + Nonce 12B + Tag 16B)
pub const MIN_CONTAINER_LENGTH: usize = 36;

// ---------------------------------------------------------------------------
// Константы KDF v2 (User-Isolated)
// ---------------------------------------------------------------------------

/// Уникальный идентификатор приложения для изоляции ключа пользователя (v2)
const APP_UNIQUE_ID_V2: &[u8] = b"ru.mark.vrxx.user-keystore.v2";

/// Соль приложения для деривации пользовательского ключа (v2)
const APP_SALT_V2: &[u8] = b"vrxx-user-isolated-salt-2026";

/// Контекст расширения ключа HKDF (v2)
const HKDF_INFO_V2: &[u8] = b"vrxx-aes-256-gcm-data-key-v2";

// ---------------------------------------------------------------------------
// Константы KDF v1 (Legacy Host-Bound)
// ---------------------------------------------------------------------------

const APP_UNIQUE_ID: &[u8] = b"ru.mark.vrxx.secure-keystore.v1";
const APP_SALT: &[u8] = b"vrxx-host-bound-salt-2026";
const HKDF_INFO: &[u8] = b"vrxx-aes-256-gcm-data-key";

// ---------------------------------------------------------------------------
// Управление файлом ключа пользователя (~/.local/share/vrxx/.key)
// ---------------------------------------------------------------------------

/// Возвращает канонический путь к файлу секретного ключа пользователя `~/.local/share/vrxx/.key`.
///
/// Поддерживает переопределение через переменную окружения `VRXX_KEY_PATH` для изолированного тестирования.
pub fn user_key_path() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("VRXX_KEY_PATH") {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let data_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| anyhow::anyhow!("Не удалось определить директорию данных пользователя"))?;

    Ok(data_dir.join("vrxx").join(".key"))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

fn hex_to_bytes(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        anyhow::bail!("Hex string length is {}, expected 64", s.len());
    }
    let mut out = [0u8; 32];
    let bytes = s.as_bytes();
    for i in 0..32 {
        let h1 = match bytes[i * 2] {
            b'0'..=b'9' => bytes[i * 2] - b'0',
            b'a'..=b'f' => bytes[i * 2] - b'a' + 10,
            b'A'..=b'F' => bytes[i * 2] - b'A' + 10,
            c => anyhow::bail!("Invalid hex character: {c}"),
        };
        let h2 = match bytes[i * 2 + 1] {
            b'0'..=b'9' => bytes[i * 2 + 1] - b'0',
            b'a'..=b'f' => bytes[i * 2 + 1] - b'a' + 10,
            b'A'..=b'F' => bytes[i * 2 + 1] - b'A' + 10,
            c => anyhow::bail!("Invalid hex character: {c}"),
        };
        out[i] = (h1 << 4) | h2;
    }
    Ok(out)
}

fn read_user_key_file(path: &Path) -> anyhow::Result<[u8; 32]> {
    let last_err = anyhow::anyhow!("Файл ключа пользователя {:?} пуст", path);
    // До 10 попыток чтения с паузой 10мс на случай параллельной записи другим потоком/процессом
    for _ in 0..10 {
        if let Ok(raw) = fs::read(path) {
            if !raw.is_empty() {
                // Try parsing as 64-char hex string first
                if let Ok(s) = std::str::from_utf8(&raw) {
                    let trimmed = s.trim();
                    if trimmed.len() == 64 {
                        if let Ok(key) = hex_to_bytes(trimmed) {
                            return Ok(key);
                        }
                    }
                }

                // Fallback: 32 raw bytes (or 32 raw bytes + newline)
                if raw.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&raw);
                    return Ok(key);
                } else if raw.len() == 33 && raw.ends_with(b"\n") {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&raw[..32]);
                    return Ok(key);
                }

                return Err(anyhow::anyhow!(
                    "Неверный формат или длина файла ключа {:?}: {} байт (ожидалась 64-символьная hex-строка или 32 сырых байта)",
                    path,
                    raw.len()
                ));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Err(last_err)
}

/// Считывает или атомарно создает 256-битный ключ пользователя по указанному пути `path`.
///
/// Гарантирует:
/// - Директория создается с правами `0700` (`rwx------`).
/// - Файл ключа создается с правами `0600` (`rw-------`).
/// - Генерация 32 криптостойких байтов из CSPRNG (`OsRng`).
/// - Защита от symlink-атак (символические ссылки отвергаются).
/// - Защита от состояний гонки при параллельном запуске через `create_new(true)` и hard_link.
pub fn get_or_create_user_key_at(path: &Path) -> anyhow::Result<[u8; 32]> {
    // 1. Проверка на символические ссылки
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            anyhow::bail!(
                "Файл ключа {:?} является символической ссылкой, что небезопасно",
                path
            );
        }
    }

    // 2. Если файл существует — читаем и проверяем права
    if path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                let mode = meta.permissions().mode() & 0o777;
                if mode != 0o600 {
                    tracing::warn!(
                        "Небезопасные права доступа к файлу ключа {:?}: {:o}. Устанавливаем 0600.",
                        path,
                        mode
                    );
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                }
            }
        }

        return read_user_key_file(path);
    }

    // 3. Создаем родительскую директорию с правами 0700
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Не удалось создать директорию {:?}: {e}", parent))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = fs::metadata(parent) {
                    let mode = meta.permissions().mode() & 0o777;
                    if mode != 0o700 {
                        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
                    }
                }
            }
        }
    }

    // 4. Генерируем 32 байта CSPRNG и форматируем как 64-символьную hex-строку
    let mut new_key = [0u8; 32];
    OsRng.fill_bytes(&mut new_key);
    let hex_key = bytes_to_hex(&new_key);

    // 5. Атомарно создаем файл с правами 0600
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let tmp_file_name = format!(".key.tmp.{}.{}", std::process::id(), OsRng.next_u64());
        let tmp_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(tmp_file_name);

        let mut opts = fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true).mode(0o600);

        let write_tmp = (|| -> std::io::Result<()> {
            let mut file = opts.open(&tmp_path)?;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
            file.write_all(hex_key.as_bytes())?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_tmp {
            let _ = fs::remove_file(&tmp_path);
            return Err(anyhow::anyhow!(
                "Не удалось записать временный файл ключа {:?}: {e}",
                tmp_path
            ));
        }

        match fs::hard_link(&tmp_path, path) {
            Ok(()) => {
                let _ = fs::remove_file(&tmp_path);
                Ok(new_key)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&tmp_path);
                read_user_key_file(path)
            }
            Err(_) => {
                let _ = fs::remove_file(&tmp_path);
                let mut opts = fs::OpenOptions::new();
                opts.create_new(true).write(true).mode(0o600);
                match opts.open(path) {
                    Ok(mut file) => {
                        file.write_all(hex_key.as_bytes()).map_err(|e| {
                            anyhow::anyhow!("Ошибка записи в файл ключа {:?}: {e}", path)
                        })?;
                        file.sync_all().map_err(|e| {
                            anyhow::anyhow!("Ошибка сброса буферов ключа {:?}: {e}", path)
                        })?;
                        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
                        Ok(new_key)
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        read_user_key_file(path)
                    }
                    Err(e) => Err(anyhow::anyhow!(
                        "Не удалось создать файл ключа {:?}: {e}",
                        path
                    )),
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        fs::write(path, hex_key.as_bytes())
            .map_err(|e| anyhow::anyhow!("Ошибка записи в файл ключа {:?}: {e}", path))?;
        Ok(new_key)
    }
}

/// Считывает или создает 256-битный ключ текущего пользователя в `~/.local/share/vrxx/.key`.
pub fn get_or_create_user_key() -> anyhow::Result<[u8; 32]> {
    let path = user_key_path()?;
    get_or_create_user_key_at(&path)
}

// ---------------------------------------------------------------------------
// Деривация ключей шифрования (KDF v2 & v1)
// ---------------------------------------------------------------------------

/// Вычисляет 256-битный ключ AES-256-GCM из пользовательского секрета через HMAC-SHA256 и HKDF-SHA256 (KDF v2).
pub fn derive_user_key_v2(secret: &[u8]) -> anyhow::Result<[u8; 32]> {
    if secret.is_empty() {
        anyhow::bail!("Секрет пользователя не может быть пустым (empty secret)");
    }

    // 1. Извлечение псевдослучайного ключа (PRK) через HMAC-SHA256
    let mut mac = <HmacSha256 as Mac>::new_from_slice(APP_UNIQUE_ID_V2)
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации HMAC v2: {e}"))?;
    mac.update(secret);
    let prk = mac.finalize().into_bytes();

    // 2. Растяжение ключа через HKDF-SHA256 в 32-байтовый ключ AES-256
    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(APP_SALT_V2), &prk);
    let mut derived_key = [0u8; 32];
    hkdf.expand(HKDF_INFO_V2, &mut derived_key)
        .map_err(|e| anyhow::anyhow!("Ошибка расширения ключа HKDF v2: {e}"))?;

    Ok(derived_key)
}

/// Алиас для `derive_user_key_v2`.
pub fn derive_user_key(secret: &[u8]) -> anyhow::Result<[u8; 32]> {
    derive_user_key_v2(secret)
}

/// Вычисляет 256-битный ключ шифрования для текущего пользователя на базе `~/.local/share/vrxx/.key`.
pub fn derive_current_user_key() -> anyhow::Result<[u8; 32]> {
    let secret = get_or_create_user_key()?;
    derive_user_key_v2(&secret)
}

// ---------------------------------------------------------------------------
// Контейнеры AES-256-GCM v2
// ---------------------------------------------------------------------------

/// Определяет версию контейнера (1 или 2).
pub fn container_version(raw_data: &[u8]) -> anyhow::Result<u32> {
    if raw_data.len() < MIN_CONTAINER_LENGTH {
        anyhow::bail!(
            "Container too small: {} bytes (expected at least {MIN_CONTAINER_LENGTH} bytes)",
            raw_data.len()
        );
    }
    if raw_data.starts_with(MAGIC_HEADER_V2) {
        Ok(2)
    } else if raw_data.starts_with(MAGIC_HEADER_V1) {
        Ok(1)
    } else {
        anyhow::bail!(
            "Неверная сигнатура контейнера data.dat (ожидалась 'VRXXDAT2' или 'VRXXDAT1')"
        );
    }
}

/// Проверяет, является ли контейнер устаревшим форматом `VRXXDAT1`.
pub fn is_legacy_v1_container(raw_data: &[u8]) -> bool {
    raw_data.starts_with(MAGIC_HEADER_V1)
}

/// Проверяет, является ли контейнер современным форматом `VRXXDAT2`.
pub fn is_v2_container(raw_data: &[u8]) -> bool {
    raw_data.starts_with(MAGIC_HEADER_V2)
}

/// Упаковывает произвольные байты в бинарный контейнер `VRXXDAT2` с шифрованием AES-256-GCM.
pub fn encrypt_v2_container(data: &[u8], key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Ошибка создания шифра AES-GCM: {e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("Ошибка симметричного шифрования AES-GCM v2: {e}"))?;

    let mut container = Vec::with_capacity(MAGIC_HEADER_V2.len() + NONCE_LENGTH + ciphertext.len());
    container.extend_from_slice(MAGIC_HEADER_V2);
    container.extend_from_slice(&nonce_bytes);
    container.extend_from_slice(&ciphertext);

    Ok(container)
}

/// Распаковывает и валидирует бинарный контейнер `VRXXDAT2` с использованием переданного ключа.
pub fn decrypt_v2_container(raw_data: &[u8], key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    if raw_data.len() < MIN_CONTAINER_LENGTH {
        anyhow::bail!(
            "Container too small: {} bytes (expected at least {MIN_CONTAINER_LENGTH} bytes)",
            raw_data.len()
        );
    }

    if !raw_data.starts_with(MAGIC_HEADER_V2) {
        anyhow::bail!("Invalid magic header, expected VRXXDAT2");
    }

    let nonce_start = MAGIC_HEADER_V2.len();
    let nonce_end = nonce_start + NONCE_LENGTH;
    let nonce = Nonce::from_slice(&raw_data[nonce_start..nonce_end]);
    let ciphertext = &raw_data[nonce_end..];

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Ошибка создания шифра AES-GCM: {e}"))?;

    let decrypted = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed: integrity compromised or wrong key"))?;

    Ok(decrypted)
}

/// Шифрует список профилей VPN в контейнер `VRXXDAT2` с использованием ключа пользователя.
pub fn encrypt_keys_with_user_key(
    keys: &[VpnKeyData],
    user_key: &[u8; 32],
) -> anyhow::Result<Vec<u8>> {
    let json_bytes = serde_json::to_vec(keys)
        .map_err(|e| anyhow::anyhow!("Ошибка сериализации ключей в JSON: {e}"))?;
    encrypt_v2_container(&json_bytes, user_key)
}

/// Расшифровывает список профилей VPN из контейнера `VRXXDAT2` с использованием ключа пользователя.
pub fn decrypt_keys_with_user_key(
    raw_data: &[u8],
    user_key: &[u8; 32],
) -> anyhow::Result<Vec<VpnKeyData>> {
    let decrypted_bytes = decrypt_v2_container(raw_data, user_key)?;
    let keys: Vec<VpnKeyData> = serde_json::from_slice(&decrypted_bytes)
        .map_err(|e| anyhow::anyhow!("Ошибка десериализации расшифрованного JSON: {e}"))?;
    Ok(keys)
}

// ---------------------------------------------------------------------------
// Универсальные интерфейсы & Прозрачная обратная совместимость
// ---------------------------------------------------------------------------

/// Шифрует список VPN-профилей с использованием изолированного ключа текущего пользователя (`VRXXDAT2`).
pub fn encrypt_keys(keys: &[VpnKeyData]) -> anyhow::Result<Vec<u8>> {
    let key = derive_current_user_key()?;
    encrypt_keys_with_user_key(keys, &key)
}

/// Расшифровывает бинарный контейнер `data.dat`, автоматически распознавая `VRXXDAT2` или legacy `VRXXDAT1`.
pub fn decrypt_keys(raw_data: &[u8]) -> anyhow::Result<Vec<VpnKeyData>> {
    if raw_data.len() < MIN_CONTAINER_LENGTH {
        anyhow::bail!(
            "Файл data.dat поврежден: размер данных ({} байт) too small, меньше минимального ({MIN_CONTAINER_LENGTH} байт)",
            raw_data.len()
        );
    }

    if is_v2_container(raw_data) {
        let key = derive_current_user_key()?;
        decrypt_keys_with_user_key(raw_data, &key)
    } else if is_legacy_v1_container(raw_data) {
        let machine_id = get_machine_id()?;
        decrypt_keys_with_id(raw_data, &machine_id)
    } else {
        anyhow::bail!(
            "Неверная сигнатура контейнера data.dat (ожидалась 'VRXXDAT2' или 'VRXXDAT1')"
        );
    }
}

/// Выполняет дешифрование устаревшего контейнера `VRXXDAT1` и перешифрование в `VRXXDAT2`.
pub fn migrate_legacy_container(raw_data: &[u8], machine_id: &str) -> anyhow::Result<Vec<u8>> {
    if is_v2_container(raw_data) {
        return Ok(raw_data.to_vec());
    }

    let keys = decrypt_keys_with_id(raw_data, machine_id)?;
    encrypt_keys(&keys)
}

// ---------------------------------------------------------------------------
// Устаревшие функции (Legacy V1 Machine-ID) — для обратной совместимости
// ---------------------------------------------------------------------------

/// Считывает системный идентификатор машины без необходимости прав root.
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

/// Вычисляет 256-битный ключ шифрования для указанного `machine_id` (KDF v1).
pub fn derive_key_from_machine_id(machine_id: &str) -> anyhow::Result<[u8; 32]> {
    let trimmed_id = machine_id.trim();
    if trimmed_id.is_empty() {
        anyhow::bail!("Идентификатор machine-id не может быть пустым");
    }

    let mut mac = <HmacSha256 as Mac>::new_from_slice(APP_UNIQUE_ID)
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации HMAC: {e}"))?;
    mac.update(trimmed_id.as_bytes());
    let prk = mac.finalize().into_bytes();

    let hkdf = hkdf::Hkdf::<Sha256>::new(Some(APP_SALT), &prk);
    let mut derived_key = [0u8; 32];
    hkdf.expand(HKDF_INFO, &mut derived_key)
        .map_err(|e| anyhow::anyhow!("Ошибка расширения ключа HKDF: {e}"))?;

    Ok(derived_key)
}

/// Вычисляет 256-битный ключ шифрования на базе текущего системного `machine-id` (KDF v1).
pub fn derive_host_key() -> anyhow::Result<[u8; 32]> {
    let machine_id = get_machine_id()?;
    derive_key_from_machine_id(&machine_id)
}

/// Шифрует список VPN-профилей в устаревший формат `VRXXDAT1` с использованием указанного `machine_id`.
pub fn encrypt_keys_with_id(keys: &[VpnKeyData], machine_id: &str) -> anyhow::Result<Vec<u8>> {
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

    let mut container = Vec::with_capacity(MAGIC_HEADER_V1.len() + NONCE_LENGTH + ciphertext.len());
    container.extend_from_slice(MAGIC_HEADER_V1);
    container.extend_from_slice(&nonce_bytes);
    container.extend_from_slice(&ciphertext);

    Ok(container)
}

/// Расшифровывает бинарный контейнер устаревшего формата `VRXXDAT1` с указанным `machine_id`.
pub fn decrypt_keys_with_id(raw_data: &[u8], machine_id: &str) -> anyhow::Result<Vec<VpnKeyData>> {
    if raw_data.len() < MIN_CONTAINER_LENGTH {
        anyhow::bail!(
            "Файл data.dat поврежден: размер данных ({} байт) too small, меньше минимального ({MIN_CONTAINER_LENGTH} байт)",
            raw_data.len()
        );
    }

    if !raw_data.starts_with(MAGIC_HEADER_V1) {
        anyhow::bail!("Неверная сигнатура контейнера data.dat (ожидалась 'VRXXDAT1')");
    }

    let nonce_start = MAGIC_HEADER_V1.len();
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
                country_code: "DE".to_string(),
                city: "Frankfurt".to_string(),
                isp: "Hetzner".to_string(),
                timezone: "UTC+1".to_string(),
                url: "vless://test-uuid@1.2.3.4:443?security=reality&sni=example.com#Germany"
                    .to_string(),
                group: String::new(),
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
                country_code: "NL".to_string(),
                city: "Amsterdam".to_string(),
                isp: "DigitalOcean".to_string(),
                timezone: "UTC+1".to_string(),
                url: "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNz@5.6.7.8:8388#Netherlands"
                    .to_string(),
                group: String::new(),
            },
        ]
    }

    #[test]
    fn test_keystore_v1_encrypt_decrypt_roundtrip() {
        let keys = create_test_keys();
        let test_machine_id = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

        let encrypted =
            encrypt_keys_with_id(&keys, test_machine_id).expect("Шифрование должно пройти успешно");
        assert!(encrypted.len() > MAGIC_HEADER_V1.len() + NONCE_LENGTH + TAG_LENGTH);
        assert_eq!(&encrypted[0..8], MAGIC_HEADER_V1);

        let decrypted = decrypt_keys_with_id(&encrypted, test_machine_id)
            .expect("Дешифрование должно пройти успешно");
        assert_eq!(decrypted.len(), 2);
        assert_eq!(decrypted[0].name, "Germany High-Speed");
        assert_eq!(decrypted[0].url, keys[0].url);
        assert_eq!(decrypted[1].name, "Netherlands Backup");
        assert_eq!(decrypted[1].protocol, "Shadowsocks");
    }

    #[test]
    fn test_keystore_v2_encrypt_decrypt_user_key_roundtrip() {
        let keys = create_test_keys();
        let user_key = [0x42u8; 32];

        let encrypted = encrypt_keys_with_user_key(&keys, &user_key)
            .expect("V2 шифрование должно пройти успешно");
        assert!(encrypted.len() > MAGIC_HEADER_V2.len() + NONCE_LENGTH + TAG_LENGTH);
        assert_eq!(&encrypted[0..8], MAGIC_HEADER_V2);

        let decrypted = decrypt_keys_with_user_key(&encrypted, &user_key)
            .expect("V2 дешифрование должно пройти успешно");
        assert_eq!(decrypted.len(), 2);
        assert_eq!(decrypted[0].name, "Germany High-Speed");
        assert_eq!(decrypted[1].name, "Netherlands Backup");
    }

    #[test]
    fn test_keystore_wrong_user_key_fails() {
        let keys = create_test_keys();
        let key_a = [0x11u8; 32];
        let key_b = [0x22u8; 32];

        let encrypted = encrypt_keys_with_user_key(&keys, &key_a).unwrap();
        let result = decrypt_keys_with_user_key(&encrypted, &key_b);
        assert!(
            result.is_err(),
            "Чужой ключ пользователя должен вызывать ошибку AEAD"
        );
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

        let result_v2 = decrypt_v2_container(&bad_data, &[0u8; 32]);
        assert!(result_v2.is_err());
        assert!(result_v2.unwrap_err().to_string().contains("VRXXDAT2"));
    }

    #[test]
    fn test_container_version() {
        let mut v1_data = vec![0u8; 40];
        v1_data[0..8].copy_from_slice(MAGIC_HEADER_V1);
        assert_eq!(container_version(&v1_data).unwrap(), 1);

        let mut v2_data = vec![0u8; 40];
        v2_data[0..8].copy_from_slice(MAGIC_HEADER_V2);
        assert_eq!(container_version(&v2_data).unwrap(), 2);

        let short_data = vec![0u8; 20];
        assert!(container_version(&short_data).is_err());

        let bad_magic = vec![0x99u8; 40];
        assert!(container_version(&bad_magic).is_err());
    }

    #[test]
    fn test_get_or_create_user_key_permissions_and_idempotence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sub = temp_dir.path().join("sub/vrxx");
        let key_path = sub.join(".key");

        let key1 = get_or_create_user_key_at(&key_path).unwrap();
        assert_eq!(key1.len(), 32);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sub_mode = std::fs::metadata(&sub).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                sub_mode, 0o700,
                "Родительская директория должна иметь права 0700"
            );
            let key_mode = std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(key_mode, 0o600, "Файл ключа должен иметь права 0600");
        }

        // Повторный вызов должен вернуть абсолютно тот же ключ
        let key2 = get_or_create_user_key_at(&key_path).unwrap();
        assert_eq!(
            key1, key2,
            "Повторное чтение должно возвращать идентичный ключ"
        );
    }

    #[test]
    fn test_symlink_rejection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let real_file = temp_dir.path().join("real_key");
        std::fs::write(
            &real_file,
            b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        let symlink_file = temp_dir.path().join("symlink_key");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real_file, &symlink_file).unwrap();
            let res = get_or_create_user_key_at(&symlink_file);
            assert!(res.is_err(), "Символическая ссылка должна быть отклонена");
            assert!(res
                .unwrap_err()
                .to_string()
                .contains("символической ссылкой"));
        }
    }

    #[test]
    fn test_existing_key_permission_hardening() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key_path = temp_dir.path().join(".key");
        std::fs::write(&key_path, [0x55u8; 32]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let key = get_or_create_user_key_at(&key_path).unwrap();
        assert_eq!(key, [0x55u8; 32]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Права должны быть ужесточены до 0600");
        }
    }

    #[test]
    fn test_hex_string_key_file_support() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key_path = temp_dir.path().join(".key");
        let raw_key = [0x7au8; 32];
        let hex_str = bytes_to_hex(&raw_key);
        std::fs::write(&key_path, format!("{hex_str}\n")).unwrap();

        let key = get_or_create_user_key_at(&key_path).unwrap();
        assert_eq!(key, raw_key);
    }
}
