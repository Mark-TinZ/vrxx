use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Основная функция для обновления баз данных гео-локации (GeoIP и GeoSite).
///
/// # Аргументы
/// * `force` - если true, принудительно скачивает файлы, даже если они свежие.
/// * `progress_tx` - опциональный канал для передачи прогресса загрузки (от 0.0 до 1.0).
pub async fn update_geo_databases(
    force: bool,
    progress_tx: Option<async_channel::Sender<f64>>,
) -> Result<()> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vrxx");
    crate::utils::secure_create_dir_all(&config_dir)?;

    // Список ресурсов для скачивания.
    // Используются актуальные источники для глобальных и региональных правил.
    let files_to_download = [
        (
            "geosite.dat",
            "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geosite.dat",
        ),
        (
            "geoip.dat",
            "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geoip.dat",
        ),
        (
            "geosite_ru.dat",
            "https://github.com/runet-geodata/runet-geodata/releases/latest/download/geosite.dat",
        ),
        (
            "geoip_ru.dat",
            "https://github.com/runet-geodata/runet-geodata/releases/latest/download/geoip.dat",
        ),
        (
            "geosite_antifilter.dat",
            "https://github.com/1andrevich/antifilter-domain/releases/latest/download/geosite.dat",
        ),
    ];

    let client = reqwest::Client::new();
    let total_files = files_to_download.len();
    let mut downloaded_count = 0;

    for (filename, url) in files_to_download {
        let file_path = config_dir.join(filename);
        let mut should_download = true;

        // Проверяем дату последнего изменения, если не задан флаг force.
        if !force {
            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        // Обновляем только если файлы старше 3 дней.
                        if elapsed.as_secs() < 3 * 24 * 3600 {
                            should_download = false;
                        }
                    }
                }
            }
        }

        if should_download {
            tracing::info!("Downloading {}...", filename);
            match client.get(url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(bytes) = response.bytes().await {
                            // --- Раздел: Безопасное сохранение файлов ---
                            // На Unix-системах ограничиваем права до 0600 для предотвращения доступа других пользователей.
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                                let mut opts = std::fs::OpenOptions::new();
                                opts.create(true).write(true).truncate(true).mode(0o600);
                                if let Ok(mut file) = opts.open(&file_path) {
                                    let _ = file
                                        .set_permissions(std::fs::Permissions::from_mode(0o600));
                                    let _ = file.write_all(&bytes);
                                    tracing::info!("{} updated successfully.", filename);
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                if let Ok(mut file) = fs::File::create(&file_path) {
                                    let _ = file.write_all(&bytes);
                                    tracing::info!("{} updated successfully.", filename);
                                }
                            }
                        }
                    } else {
                        tracing::warn!(
                            "Failed to download {}: HTTP {}",
                            filename,
                            response.status()
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to download {}: {}", filename, e);
                }
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

/// Возвращает дату последнего обновления Geo-ресурсов в виде строки.
pub fn get_geo_status() -> String {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vrxx");
    let mut last_updated = std::time::SystemTime::UNIX_EPOCH;
    let mut found = false;

    let files = [
        "geosite.dat",
        "geoip.dat",
        "geosite_ru.dat",
        "geoip_ru.dat",
        "geosite_antifilter.dat",
    ];

    for file in files {
        if let Ok(metadata) = fs::metadata(config_dir.join(file)) {
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

/// Запускает фоновый поток для периодического обновления баз данных.
pub fn spawn_background_updater() {
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                // Выполняем проверку обновлений при запуске.
                let _ = update_geo_databases(false, None).await;

                // Повторяем каждые 24 часа.
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
