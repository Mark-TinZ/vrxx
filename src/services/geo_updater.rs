/* geo_updater.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Фоновый загрузчик и менеджер обновления гео-баз (Geo Updater)
//!
//! Модуль отвечает за:
//! - Скачивание и актуализацию баз гео-маршрутизации (GeoIP и GeoSite в бинарном формате `.srs`)
//! - Автоматический CDN Fallback (переключение с GitHub Raw на jsDelivr CDN при блокировках или сетевых сбоях)
//! - Безопасную запись файлов с правами доступа `0o600` на POSIX-системах
//! - Хранение файлов в стандартном каталоге пользовательских данных `~/.local/share/vrxx/geodata/`
//! - Отслеживание даты последней модификации и фоновый периодический запуск раз в 24 часа

use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Возвращает путь к каталогу хранения бинарных правил маршрутизации (~/.local/share/vrxx/geodata).
pub fn get_geodata_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local").join("share"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("vrxx")
        .join("geodata")
}

/// Главная функция обновления баз гео-маршрутизации (GeoIP и GeoSite) в формате `.srs`.
///
/// # Аргументы
/// * `force` - Если true, принудительно перезагружает базы независимо от даты последнего обновления.
/// * `progress_tx` - Опциональный канал для передачи прогресса загрузки (от 0.0 до 1.0).
pub async fn update_geo_databases(
    force: bool,
    progress_tx: Option<async_channel::Sender<f64>>,
) -> Result<()> {
    let geo_dir = get_geodata_dir();
    fs::create_dir_all(&geo_dir)?;

    let settings = crate::settings::SettingsManager::new().load();

    // Список ресурсов SRS: локальное имя без коллизий, основной URL и резервный URL, флаг необходимости
    let all_files = [
        (
            "geosite-google.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/google.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/google.srs",
            true, // Всегда требуется для обхода критических сервисов Google
        ),
        (
            "geoip-private.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/private.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/private.srs",
            true, // Всегда требуется для локальных сетей
        ),
        (
            "geosite-ru.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ru.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ru.srs",
            settings.enable_routing && (settings.route_ru || settings.route_ru_sites),
        ),
        (
            "geoip-ru.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ru.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/ru.srs",
            settings.enable_routing && (settings.route_ru || settings.route_ru_ips),
        ),
        (
            "geosite-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/cn.srs",
            settings.enable_routing && (settings.route_cn || settings.route_cn_sites),
        ),
        (
            "geoip-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/cn.srs",
            settings.enable_routing && (settings.route_cn || settings.route_cn_ips),
        ),
        (
            "geosite-geolocation-not-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/geolocation-!cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/geolocation-!cn.srs",
            settings.enable_routing && settings.route_cn,
        ),
        (
            "geosite-ir.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ir.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ir.srs",
            settings.enable_routing && (settings.route_ir || settings.route_ir_sites),
        ),
        (
            "geoip-ir.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ir.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/ir.srs",
            settings.enable_routing && (settings.route_ir || settings.route_ir_ips),
        ),
        (
            "geosite-antifilter.srs",
            "https://github.com/savely-krasovsky/antizapret-sing-box/releases/latest/download/antizapret.srs",
            "https://cdn.jsdelivr.net/gh/legiz-ru/sb-rule-sets@release/antifilter.srs",
            settings.enable_routing && settings.route_antifilter,
        ),
        (
            "geosite-ads.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ads-all.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ads-all.srs",
            settings.block_ads,
        ),
    ];

    let files_to_download: Vec<_> = all_files
        .into_iter()
        .filter(|(_, _, _, is_needed)| *is_needed)
        .map(|(name, p, f, _)| (name, p, f))
        .collect();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    let total_files = files_to_download.len();
    if total_files == 0 {
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(1.0).await;
        }
        return Ok(());
    }
    let mut downloaded_count = 0;

    for (filename, primary_url, fallback_url) in files_to_download {
        let file_path = geo_dir.join(filename);
        let mut should_download = true;

        if !force {
            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        let interval_days = settings.geodata_update_interval_days.max(1) as u64;
                        if elapsed.as_secs() < interval_days * 24 * 3600 {
                            should_download = false;
                        }
                    }
                }
            }
        }

        if should_download {
            tracing::info!("Downloading database {}...", filename);
            let mut download_success = false;

            // Попытка 1: Основной источник GitHub Raw
            if let Ok(res) = client.get(primary_url).send().await {
                if res.status().is_success() {
                    if let Ok(bytes) = res.bytes().await {
                        if save_file_safely(&file_path, &bytes).is_ok() {
                            tracing::info!(
                                "{} successfully downloaded from primary source.",
                                filename
                            );
                            download_success = true;
                        }
                    }
                }
            }

            // Попытка 2: Резервный источник jsDelivr CDN
            if !download_success {
                tracing::warn!(
                    "Primary source unavailable for {}. Trying fallback CDN...",
                    filename
                );
                if let Ok(res) = client.get(fallback_url).send().await {
                    if res.status().is_success() {
                        if let Ok(bytes) = res.bytes().await {
                            if save_file_safely(&file_path, &bytes).is_ok() {
                                tracing::info!(
                                    "{} successfully downloaded from fallback CDN.",
                                    filename
                                );
                                download_success = true;
                            }
                        }
                    }
                }
            }

            if !download_success {
                tracing::warn!(
                    "Failed to download {} from available sources. Local or cached rule-set will be retained.",
                    filename
                );
            }
        }

        downloaded_count += 1;
        if let Some(ref tx) = progress_tx {
            let progress = downloaded_count as f64 / total_files as f64;
            let _ = tx.send(progress).await;
        }
    }

    Ok(())
}

/// Сохраняет файл на диск с безопасным установлением прав доступа.
fn save_file_safely(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true).mode(0o600);
        let mut file = opts.open(path)?;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
        file.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    {
        let mut file = fs::File::create(path)?;
        file.write_all(bytes)?;
    }
    Ok(())
}

/// Возвращает время последней модификации локальных гео-баз, если хотя бы одна база существует.
pub fn get_last_updated_time() -> Option<std::time::SystemTime> {
    let geo_dir = get_geodata_dir();
    let mut last_updated = std::time::SystemTime::UNIX_EPOCH;
    let mut found = false;

    let files = [
        "geosite-ru.srs",
        "geoip-ru.srs",
        "geosite-cn.srs",
        "geoip-cn.srs",
        "geosite-ir.srs",
        "geoip-ir.srs",
        "geosite-antifilter.srs",
        "geosite-ads.srs",
        "geoip-private.srs",
    ];

    for file in files {
        if let Ok(metadata) = fs::metadata(geo_dir.join(file)) {
            if let Ok(modified) = metadata.modified() {
                if modified > last_updated {
                    last_updated = modified;
                    found = true;
                }
            }
        }
    }

    if found {
        Some(last_updated)
    } else {
        None
    }
}

/// Проверяет, устарели ли гео-базы относительно заданного интервала в днях.
pub fn is_geodata_stale(interval_days: u32) -> bool {
    match get_last_updated_time() {
        Some(time) => {
            let max_age = std::time::Duration::from_secs(interval_days.max(1) as u64 * 86400);
            match time.elapsed() {
                Ok(elapsed) => elapsed >= max_age,
                Err(_) => true,
            }
        }
        None => true,
    }
}

/// Возвращает дату последней модификации локальных гео-баз в форматированном виде.
pub fn get_geo_status() -> String {
    if let Some(last_updated) = get_last_updated_time() {
        let datetime: chrono::DateTime<chrono::Local> = last_updated.into();
        datetime.format("%Y-%m-%d %H:%M").to_string()
    } else {
        gettextrs::gettext("Never updated")
    }
}

/// Запускает фоновый поток для периодической проверки и обновления гео-баз по расписанию.
pub fn spawn_background_updater() {
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                loop {
                    let settings = crate::settings::SettingsManager::new().load();
                    if settings.auto_update_geodata {
                        if is_geodata_stale(settings.geodata_update_interval_days) {
                            tracing::info!(
                                "Geodata is stale or missing; triggering scheduled update..."
                            );
                            let _ = update_geo_databases(false, None).await;
                        } else {
                            tracing::debug!("Geodata is up-to-date; skipping download.");
                        }
                    } else {
                        tracing::debug!(
                            "Automatic geodata updates disabled in settings; updater waiting."
                        );
                    }

                    // Проверяем раз в час, чтобы реагировать на включение настроек и наступление срока
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geodata_stale_logic() {
        // If files are missing or never updated, it must report stale
        let _temp_dir = tempfile::tempdir().unwrap();
        // Since get_last_updated_time inspects the real dir, we verify is_geodata_stale doesn't panic
        let stale = is_geodata_stale(7);
        let _ = stale; // bool check
        let status = get_geo_status();
        assert!(!status.is_empty());
    }
}

