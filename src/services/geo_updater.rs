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

    // Список ресурсов SRS: локальное имя без коллизий, основной URL GitHub Raw и резервный jsDelivr CDN URL
    let files_to_download = [
        (
            "geosite-ru.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ru.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ru.srs",
        ),
        (
            "geoip-ru.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ru.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/ru.srs",
        ),
        (
            "geosite-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/cn.srs",
        ),
        (
            "geoip-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/cn.srs",
        ),
        (
            "geosite-ir.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ir.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ir.srs",
        ),
        (
            "geoip-ir.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/ir.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/ir.srs",
        ),
        (
            "geosite-antifilter.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ru-antifilter.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ru-antifilter.srs",
        ),
        (
            "geosite-ads.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/category-ads-all.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/category-ads-all.srs",
        ),
        (
            "geosite-google.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/google.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/google.srs",
        ),
        (
            "geosite-geolocation-not-cn.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/geolocation-!cn.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geosite/geolocation-!cn.srs",
        ),
        (
            "geoip-private.srs",
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/private.srs",
            "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/private.srs",
        ),
    ];

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    let total_files = files_to_download.len();
    let mut downloaded_count = 0;

    for (filename, primary_url, fallback_url) in files_to_download {
        let file_path = geo_dir.join(filename);
        let mut should_download = true;

        if !force {
            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        // Обновляем только если файлы старше 3 дней
                        if elapsed.as_secs() < 3 * 24 * 3600 {
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
                tracing::error!(
                    "Failed to download {} from all available sources.",
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

/// Возвращает дату последней модификации локальных гео-баз в форматированном виде.
pub fn get_geo_status() -> String {
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
        let datetime: chrono::DateTime<chrono::Local> = last_updated.into();
        datetime.format("%Y-%m-%d %H:%M").to_string()
    } else {
        gettextrs::gettext("Never updated")
    }
}

/// Запускает фоновый поток для периодической проверки и обновления гео-баз каждые 24 часа.
pub fn spawn_background_updater() {
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let _ = update_geo_databases(false, None).await;
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600));
                loop {
                    interval.tick().await;
                    let _ = update_geo_databases(false, None).await;
                }
            });
        }
    });
}
