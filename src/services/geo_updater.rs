use std::path::PathBuf;
use anyhow::Result;
use std::fs;
use std::io::Write;

// --- Раздел: Работа с гео-базами ---
pub async fn update_geo_databases(force: bool) -> Result<()> {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx");
    fs::create_dir_all(&config_dir)?;

    let files_to_download = [
        ("geosite.dat", "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geosite.dat"),
        ("geoip.dat", "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geoip.dat"),
        ("geosite_ru.dat", "https://github.com/runet-geodata/runet-geodata/releases/latest/download/geosite.dat"),
        ("geoip_ru.dat", "https://github.com/runet-geodata/runet-geodata/releases/latest/download/geoip.dat"),
        ("geosite_cn.dat", "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geosite.dat"),
        ("geoip_cn.dat", "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geoip.dat"),
        ("geosite_antifilter.dat", "https://github.com/1andrevich/antifilter-domain/releases/latest/download/geosite.dat"),
        ("geosite_ir.dat", "https://github.com/Chocolate4U/Iran-v2ray-rules/releases/latest/download/geosite.dat"),
        ("geoip_ir.dat", "https://github.com/Chocolate4U/Iran-v2ray-rules/releases/latest/download/geoip.dat"),
    ];

    let client = reqwest::Client::new();

    for (filename, url) in files_to_download {
        let file_path = config_dir.join(filename);
        let mut should_download = true;
        
        if !force {
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

pub fn get_geo_status() -> String {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("vrxx");
    let mut last_updated = std::time::SystemTime::UNIX_EPOCH;
    let mut found = false;

    let files = [
        "geosite.dat", "geoip.dat",
        "geosite_ru.dat", "geoip_ru.dat",
        "geosite_cn.dat", "geoip_cn.dat",
        "geosite_antifilter.dat",
        "geosite_ir.dat", "geoip_ir.dat",
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
        "Never updated".to_string()
    }
}

// ================================

// --- Раздел: Фоновое обновление ---
pub fn spawn_background_updater() {
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                // FIXME: При первом запуске может возникнуть гонка, если ядро стартует раньше скачивания баз.
                let _ = update_geo_databases(false).await;
                
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600));
                loop {
                    interval.tick().await;
                    let _ = update_geo_databases(false).await;
                }
            });
        }
    });
}
