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

/// Выполняет исчерпывающий поиск исполняемого файла `sing-box` в системе.
pub fn find_singbox_binary() -> Option<PathBuf> {
    let bin_name = get_core_executable_name();
    tracing::debug!("Searching for sing-box binary ('{}')...", bin_name);

    // 1. Проверка системного PATH
    if let Ok(output) = std::process::Command::new(bin_name).arg("version").output() {
        if output.status.success() {
            tracing::info!("Sing-box binary found in system PATH: {}", bin_name);
            return Some(PathBuf::from(bin_name));
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    // 2. Локальная директория (~/.local/share/vrxx/bin/sing-box)
    candidates.push(get_local_bin_dir().join(bin_name));

    // 3. Директория пользователя под sudo (/home/$SUDO_USER/.local/share/vrxx/bin/sing-box)
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.trim().is_empty() {
            #[cfg(unix)]
            {
                if let Ok(Some(user)) = nix::unistd::User::from_name(&sudo_user) {
                    candidates.push(user.dir.join(".local/share/vrxx/bin").join(bin_name));
                } else {
                    candidates.push(PathBuf::from(format!(
                        "/home/{}/.local/share/vrxx/bin/{}",
                        sudo_user, bin_name
                    )));
                }
            }
            #[cfg(not(unix))]
            {
                candidates.push(PathBuf::from(format!(
                    "/home/{}/.local/share/vrxx/bin/{}",
                    sudo_user, bin_name
                )));
            }
        }
    }

    // 4. Текущая рабочая директория
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(bin_name));
    }

    // 5. Директория расположения запущенного исполняемого файла и родительские папки
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            candidates.push(parent.join(bin_name));
            let mut curr = parent.to_path_buf();
            for _ in 0..4 {
                if let Some(p) = curr.parent() {
                    candidates.push(p.join(bin_name));
                    curr = p.to_path_buf();
                }
            }
        }
    }

    // 6. Стандартные системные пути
    candidates.push(PathBuf::from(format!("/usr/bin/{}", bin_name)));
    candidates.push(PathBuf::from(format!("/usr/local/bin/{}", bin_name)));
    candidates.push(PathBuf::from(format!("/opt/vrxx/bin/{}", bin_name)));

    for candidate in candidates {
        tracing::trace!("Checking candidate path: {}", candidate.display());
        if candidate.exists() {
            // Проверка запуска
            if let Ok(output) = std::process::Command::new(&candidate)
                .arg("version")
                .output()
            {
                if output.status.success() {
                    // Установка прав 0o755 на Unix
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(metadata) = std::fs::metadata(&candidate) {
                            let mut perms = metadata.permissions();
                            if perms.mode() & 0o111 == 0 {
                                perms.set_mode(0o755);
                                if let Err(e) = std::fs::set_permissions(&candidate, perms) {
                                    tracing::warn!(
                                        "Failed to set 0755 permissions on {}: {}",
                                        candidate.display(),
                                        e
                                    );
                                }
                            }
                        }
                    }

                    // Если найден вне локальной директории, копируем для автономности
                    let local_dir = get_local_bin_dir();
                    let local_bin = local_dir.join(bin_name);
                    if !local_bin.exists() && candidate != local_bin {
                        if let Err(e) = std::fs::create_dir_all(&local_dir) {
                            tracing::warn!(
                                "Failed to create local bin directory {}: {}",
                                local_dir.display(),
                                e
                            );
                        } else if let Err(e) = std::fs::copy(&candidate, &local_bin) {
                            tracing::warn!(
                                "Failed to copy binary from {} to {}: {}",
                                candidate.display(),
                                local_bin.display(),
                                e
                            );
                        } else {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                if let Ok(metadata) = std::fs::metadata(&local_bin) {
                                    let mut perms = metadata.permissions();
                                    perms.set_mode(0o755);
                                    let _ = std::fs::set_permissions(&local_bin, perms);
                                }
                            }
                            tracing::info!(
                                "Sing-box binary copied to local directory: {}",
                                local_bin.display()
                            );
                        }
                    }

                    tracing::info!("Found valid sing-box binary: {}", candidate.display());
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
    }

    tracing::error!("No valid sing-box executable found among candidate search paths");
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
}
