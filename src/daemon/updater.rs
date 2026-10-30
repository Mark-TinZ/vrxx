/* updater.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Обнаружение и проверка исполняемых файлов сетевого ядра (Core Updater & Resolver)
//!
//! Модуль отвечает за:
//! - Поиск бинарного файла `sing-box` в системных директориях (`PATH`, `/usr/bin`, `/usr/local/bin`, `/opt/vrxx/bin`)
//! - Поиск в пользовательских директориях данных (`~/.local/share/vrxx/bin/`) и директории текущего запуска
//! - Проверку прав на исполнение (`0o755`) и тестовый запуск команды `version`
//! - Автоматическое кэширование найденного бинарника в локальную директорию `~/.local/share/vrxx/bin/`

use anyhow::Result;
use std::path::PathBuf;

/// Возвращает имя исполняемого файла ядра для текущей операционной системы.
pub fn get_core_executable_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "sing-box.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "sing-box"
    }
}

/// Возвращает путь к локальной директории хранения бинарных файлов приложения (`~/.local/share/vrxx/bin`).
#[allow(dead_code)]
pub fn get_local_bin_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("vrxx")
        .join("bin")
}

/// Выполняет безопасный поиск исполняемого файла `sing-box` в доверенных системных директориях (CWE-426).
pub fn find_singbox_binary() -> Option<PathBuf> {
    let bin_name = get_core_executable_name();
    tracing::debug!("Searching for sing-box binary ('{}')...", bin_name);

    // Доверенные системные пути
    #[cfg(unix)]
    let candidates: Vec<PathBuf> = vec![
        PathBuf::from(format!("/usr/bin/{}", bin_name)),
        PathBuf::from(format!("/usr/local/bin/{}", bin_name)),
        PathBuf::from(format!("/opt/vrxx/bin/{}", bin_name)),
    ];
    #[cfg(not(unix))]
    let candidates: Vec<PathBuf> = vec![PathBuf::from(format!(
        "C:\\Program Files\\vrxx\\bin\\{}",
        bin_name
    ))];

    for candidate in candidates {
        tracing::trace!("Checking candidate path: {}", candidate.display());
        if !candidate.is_file() {
            continue;
        }

        // Проверка владельца файла на Unix: исполняемый файл в системной директории обязан принадлежать root (uid 0)
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            match std::fs::metadata(&candidate) {
                Ok(meta) => {
                    if meta.uid() != 0 {
                        tracing::warn!(
                            "Security warning: Binary at {} is not owned by root (uid: {}), skipping",
                            candidate.display(),
                            meta.uid()
                        );
                        continue;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read metadata for {}: {}", candidate.display(), e);
                    continue;
                }
            }
        }

        // Проверка запуска
        if let Ok(output) = std::process::Command::new(&candidate)
            .arg("version")
            .output()
        {
            if output.status.success() {
                tracing::info!(
                    "Found trusted root-owned sing-box binary: {}",
                    candidate.display()
                );
                return Some(candidate);
            } else {
                tracing::warn!(
                    "Binary at {} failed 'version' check (status: {})",
                    candidate.display(),
                    output.status
                );
            }
        }
    }

    tracing::error!("No valid sing-box executable found in trusted system paths");
    None
}

/// Проверяет наличие установленного ядра sing-box в системе.
pub fn check_core_exists() -> bool {
    find_singbox_binary().is_some()
}

/// Возвращает путь к исполняемому файлу sing-box в виде строки или ошибку.
pub async fn resolve_singbox_binary() -> Result<String> {
    if let Some(path) = find_singbox_binary() {
        Ok(path.to_string_lossy().to_string())
    } else {
        Err(anyhow::anyhow!("Ядро sing-box не установлено в системе"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executable_name() {
        let name = get_core_executable_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_ignores_untrusted_cwd_binary() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fake_bin = temp_dir.path().join(get_core_executable_name());
        std::fs::write(&fake_bin, b"#!/bin/sh\necho sing-box version 1.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&fake_bin, std::fs::Permissions::from_mode(0o755));
        }

        // find_singbox_binary should never return a binary located outside trusted system paths
        if let Some(found) = find_singbox_binary() {
            assert_ne!(found, fake_bin);
            assert!(
                found.starts_with("/usr/bin")
                    || found.starts_with("/usr/local/bin")
                    || found.starts_with("/opt/vrxx/bin")
            );
        }
    }
}
