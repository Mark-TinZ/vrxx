use std::path::PathBuf;
use anyhow::Result;
use std::fs;
use std::io::Write;

// --- Раздел: Работа с гео-базами ---
pub async fn update_geo_databases() -> Result<()> {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx");
    fs::create_dir_all(&config_dir)?;

    let files_to_download = [
        ("geosite.dat", "https://github.com/v2fly/domain-list-community/releases/latest/download/dlc.dat"),
        ("geoip.dat", "https://github.com/v2fly/geoip/releases/latest/download/geoip.dat"),
        ("geosite_ru.dat", "https://github.com/Tech-X-Labs/domain-list-community-ru/releases/latest/download/geosite.dat"),
        ("geoip_ru.dat", "https://github.com/Tech-X-Labs/geoip-ru/releases/latest/download/geoip.dat"),
        ("geosite_cn.dat", "https://github.com/v2fly/domain-list-community/releases/latest/download/dlc.dat"),
        ("geoip_cn.dat", "https://github.com/v2fly/geoip/releases/latest/download/geoip.dat"),
        ("geosite_antifilter.dat", "https://github.com/1andrevich/geosite-antifilter/releases/latest/download/geosite.dat"),
    ];

    let client = reqwest::Client::new();

    for (filename, url) in files_to_download {
        let file_path = config_dir.join(filename);
        let mut should_download = true;
        
        if let Ok(metadata) = fs::metadata(&file_path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    // Update if older than 3 days
                    if elapsed.as_secs() < 3 * 24 * 3600 {
                        should_download = false;
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
                            if let Ok(mut file) = fs::File::create(&file_path) {
                                let _ = file.write_all(&bytes);
                                tracing::info!("{} updated successfully.", filename);
                            }
                        }
                    } else {
                        tracing::warn!("Failed to download {}: HTTP {}", filename, response.status());
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to download {}: {}", filename, e);
                }
            }
        }
    }

    Ok(())
}

// ================================

// --- Раздел: Фоновое обновление ---
pub fn spawn_background_updater() {
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                // FIXME: При первом запуске может возникнуть гонка, если ядро стартует раньше скачивания баз.
                let _ = update_geo_databases().await;
                
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600));
                loop {
                    interval.tick().await;
                    let _ = update_geo_databases().await;
                }
            });
        }
    });
}
