use anyhow::Result;
use futures_util::StreamExt;
use reqwest::redirect::Policy;
use std::io::Write;
use std::path::{Path, PathBuf};

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

pub fn get_local_bin_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vrxx")
        .join("bin")
}

pub fn check_core_exists() -> bool {
    let bin_name = get_core_executable_name();

    // 1. PATH check
    if std::process::Command::new(bin_name)
        .arg("version")
        .output()
        .is_ok()
    {
        return true;
    }

    // 2. Local dir check
    let bin_path = get_local_bin_dir().join(bin_name);
    if bin_path.exists()
        && std::process::Command::new(&bin_path)
            .arg("version")
            .output()
            .is_ok()
    {
        return true;
    }

    false
}

pub async fn resolve_singbox_binary() -> Result<String> {
    let bin_name = get_core_executable_name();

    if std::process::Command::new(bin_name)
        .arg("version")
        .output()
        .is_ok()
    {
        return Ok(bin_name.to_string());
    }

    let bin_path = get_local_bin_dir().join(bin_name);
    if bin_path.exists() {
        return Ok(bin_path.to_string_lossy().to_string());
    }

    Err(anyhow::anyhow!("sing-box core is not installed"))
}

fn get_os_arch_strings() -> (&'static str, &'static str) {
    #[cfg(target_os = "linux")]
    let os = "linux";
    #[cfg(target_os = "windows")]
    let os = "windows";
    #[cfg(target_os = "macos")]
    let os = "darwin";
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let os = "linux"; // fallback

    #[cfg(target_arch = "x86_64")]
    let arch = "amd64";
    #[cfg(target_arch = "aarch64")]
    let arch = "arm64";
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let arch = "amd64"; // fallback

    (os, arch)
}

pub async fn download_core(progress_tx: Option<async_channel::Sender<f64>>) -> Result<String> {
    tracing::info!("Downloading the latest sing-box version...");

    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()?;

    let res = client
        .get("https://github.com/SagerNet/sing-box/releases/latest")
        .send()
        .await?;

    let mut version = "1.11.1".to_string();
    if res.status().is_redirection() {
        if let Some(loc) = res.headers().get(reqwest::header::LOCATION) {
            let loc_str = loc.to_str().unwrap_or("");
            if let Some(tag) = loc_str.split('/').last() {
                version = tag.trim_start_matches('v').to_string();
            }
        }
    }

    let (os, arch) = get_os_arch_strings();
    let ext = if os == "windows" { "zip" } else { "tar.gz" };
    let archive_name = format!("sing-box-{}-{}-{}.{}", version, os, arch, ext);
    let download_url = format!(
        "https://github.com/SagerNet/sing-box/releases/download/v{}/{}",
        version, archive_name
    );

    tracing::info!("Downloading sing-box from {}", download_url);

    let data_dir = get_local_bin_dir();
    crate::utils::secure_create_dir_all(&data_dir)?;
    let archive_path = data_dir.join(&archive_name);

    let response = reqwest::Client::new().get(&download_url).send().await?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download sing-box. HTTP status: {}",
            response.status()
        ));
    }

    let total_size = response.content_length().unwrap_or(0) as f64;
    let mut file = std::fs::File::create(&archive_path)?;
    let mut downloaded: f64 = 0.0;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as f64;

        if total_size > 0.0 {
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(downloaded / total_size).await;
            }
        }
    }
    drop(file);

    tracing::info!("Extracting downloaded archive...");
    let bin_path = extract_and_install(&archive_path, &data_dir)?;

    // Clean up
    let _ = std::fs::remove_file(&archive_path);

    Ok(bin_path)
}

pub fn install_from_archive(archive_path: &Path) -> Result<String> {
    let data_dir = get_local_bin_dir();
    crate::utils::secure_create_dir_all(&data_dir)?;
    extract_and_install(archive_path, &data_dir)
}

fn extract_and_install(archive_path: &Path, dest_dir: &Path) -> Result<String> {
    let archive_str = archive_path.to_string_lossy().to_lowercase();
    let bin_name = get_core_executable_name();
    let bin_path = dest_dir.join(bin_name);

    if archive_str.ends_with(".tar.gz") {
        let status = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(dest_dir)
            .status()?;

        if !status.success() {
            return Err(anyhow::anyhow!("Failed to extract tar.gz archive"));
        }
    } else if archive_str.ends_with(".zip") {
        let status = std::process::Command::new("unzip")
            .arg("-o")
            .arg(archive_path)
            .arg("-d")
            .arg(dest_dir)
            .status()?;

        if !status.success() {
            return Err(anyhow::anyhow!("Failed to extract zip archive"));
        }
    } else {
        return Err(anyhow::anyhow!(
            "Unsupported archive format. Use .tar.gz or .zip"
        ));
    }

    // Since tar/unzip extracts into a subfolder, we need to find the binary
    let output = std::process::Command::new("find")
        .arg(dest_dir)
        .arg("-name")
        .arg(bin_name)
        .arg("-type")
        .arg("f")
        .output()?;

    let mut found_it = false;
    if let Ok(out_str) = String::from_utf8(output.stdout) {
        if let Some(found_path_str) = out_str.lines().next() {
            let found_path = PathBuf::from(found_path_str);
            if found_path != bin_path {
                if bin_path.exists() {
                    let _ = std::fs::remove_file(&bin_path);
                }
                std::fs::rename(&found_path, &bin_path)?;

                // Try to clean up the extracted folder
                if let Some(parent) = found_path.parent() {
                    if parent != dest_dir {
                        let _ = std::fs::remove_dir_all(parent);
                    }
                }
            }
            found_it = true;
        }
    }

    if !found_it {
        return Err(anyhow::anyhow!(
            "Executable '{}' not found in archive",
            bin_name
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&bin_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&bin_path, perms);
        }
    }

    // Verify it works
    if !std::process::Command::new(&bin_path)
        .arg("version")
        .output()
        .is_ok()
    {
        return Err(anyhow::anyhow!(
            "Installed binary fails to run (maybe wrong architecture?)"
        ));
    }

    Ok(bin_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_os_arch() {
        let (os, arch) = get_os_arch_strings();
        assert!(!os.is_empty());
        assert!(!arch.is_empty());
    }
}
