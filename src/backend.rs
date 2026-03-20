use std::process::{Command, Child, Stdio};
use std::sync::{Arc, Mutex};
use std::io::{BufRead, BufReader, Write};
use crate::settings::SettingsManager;
use tempfile::NamedTempFile;
use anyhow::{Result, Context, anyhow};
use std::path::Path;

/// Вспомогательная функция для ротации логов, если они превышают 5 МБ
fn rotate_log_if_needed(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > 5 * 1024 * 1024 { // 5 MB
            let mut backup_path = path.to_path_buf();
            backup_path.set_extension("log.old");
            let _ = std::fs::rename(path, &backup_path);
        }
    }
}



#[derive(Debug)]
pub struct XrayBackend {
    /// Защищенный мьютексом процесс ядра
    process: Arc<Mutex<Option<Child>>>,
    /// Защищенный мьютексом временный файл конфигурации
    config_file: Arc<Mutex<Option<NamedTempFile>>>,
}

impl Default for XrayBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl XrayBackend {
    pub fn new() -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
            config_file: Arc::new(Mutex::new(None)),
        }
    }

    /// Запускает ядро Xray/Sing-box с переданной конфигурацией
    pub fn start(&self, config_json: &str) -> Result<()> {
        // Останавливаем предыдущий процесс
        self.stop().unwrap_or_else(|e| tracing::error!("{}", &format!("Failed to stop previous process: {e}")));

        // Сохраняем конфиг во временный файл безопасно
        let mut named_temp_file = tempfile::Builder::new()
            .prefix("vrxx_config_")
            .suffix(".json")
            .tempfile()
            .context("Failed to create temporary file")?;
            
        named_temp_file.write_all(config_json.as_bytes())
            .context("Failed to write configuration")?;
        let temp_path = named_temp_file.path().to_path_buf();
        
        let mut config_file_guard = self.config_file.lock()
            .map_err(|e| anyhow!("Mutex lock failed: {e}"))?;
        *config_file_guard = Some(named_temp_file);

        // Получаем настройки для выбора ядра
        let settings = SettingsManager::new().load();
        
        let bin_name = match settings.core.as_str() {
            "sing-box" => "sing-box",
            _ => "xray",
        };

        // Проверяем наличие бинарника
        let which_check = Command::new("which").arg(bin_name).output();
        let binary_missing = match which_check {
            Ok(output) => !output.status.success(),
            Err(_) => true,
        };
        
        if binary_missing {
            return Err(anyhow!("Core {bin_name} not found in the system.\n\nPlease install it (e.g., via your package manager) or select another core in Settings."));
        }

        if settings.tun_mode {
            let version_check = Command::new(bin_name).arg("version").output();
            if let Ok(out) = version_check {
                let v_out = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if bin_name == "sing-box" {
                    // sing-box uses Tags to show compilation features
                    if !v_out.contains("with_tun") && !v_out.contains("with_gvisor") && v_out.contains("tags:") {
                        tracing::warn!("Sing-box might be compiled without TUN support.");
                        // We don't hard fail, let it try and crash if so, but warn the user in logs.
                    }
                }
            }

            let which_core = Command::new("which").arg(bin_name).output();
            let core_path = match which_core {
                Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                Err(_) => bin_name.to_string(),
            };

            // Проверяем права cap_net_admin для TUN режима
            let cap_check = Command::new("getcap").arg(&core_path).output();
            let has_caps = match cap_check {
                Ok(out) => String::from_utf8_lossy(&out.stdout).contains("cap_net_admin"),
                Err(_) => false,
            };
            
            if !has_caps {
                return Err(anyhow!("TUN mode is enabled, but the core {bin_name} lacks necessary permissions (cap_net_admin).\n\nRun in terminal:\nsudo setcap cap_net_admin=ep {core_path}"));
            }
        }

        // Выполняем ротацию логов ядра Xray
        let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx").join("logs");
        std::fs::create_dir_all(&log_dir).ok();
        let log_path = log_dir.join("core.log");
        let error_log_path = log_dir.join("error.log");
        let access_log_path = log_dir.join("access.log");
        
        rotate_log_if_needed(&log_path);
        rotate_log_if_needed(&error_log_path);
        rotate_log_if_needed(&access_log_path);

        let mut cmd = Command::new(bin_name);

        // Унифицированный запуск (run -c config.json)
        cmd.arg("run").arg("-c").arg(&temp_path);

        // Перехватываем потоки вывода для SSD-безопасного логирования
        tracing::info!("{}", &format!("Starting core {bin_name}..."));

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to start {bin_name}"))?;

        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stderr = child.stderr.take().context("Failed to capture stderr")?;

        // Фоновый поток для stdout
        let log_path_out = log_path.clone();
        std::thread::spawn(move || {
            let mut log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path_out);
            let mut reader = BufReader::new(stdout);
            let mut buffer = String::new();
            let mut last_flush = std::time::Instant::now();

            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line).unwrap_or(0);
                if bytes_read > 0 {
                    buffer.push_str(&line);
                }

                if (last_flush.elapsed().as_secs() >= 5 || buffer.len() > 8192) && !buffer.is_empty() {
                    rotate_log_if_needed(&log_path_out);
                    // Переоткрываем файл на случай ротации
                    log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path_out);
                    
                    if let Ok(ref mut f) = log_file {
                        let _ = f.write_all(buffer.as_bytes());
                        let _ = f.flush();
                    }
                    buffer.clear();
                    last_flush = std::time::Instant::now();
                }

                if bytes_read == 0 {
                    if let Ok(ref mut f) = log_file {
                        if !buffer.is_empty() {
                            let _ = f.write_all(buffer.as_bytes());
                            let _ = f.flush();
                        }
                    }
                    break;
                }
            }
        });

        // Фоновый поток для stderr
        let log_path_err = log_path.clone();
        std::thread::spawn(move || {
            let mut log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path_err);
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();
            let mut err_buffer = String::new();
            let mut last_flush = std::time::Instant::now();

            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line).unwrap_or(0);
                if bytes_read > 0 {
                    buffer.push_str(&line);
                    err_buffer.push_str(&line);
                }

                if (last_flush.elapsed().as_secs() >= 5 || buffer.len() > 8192) && !buffer.is_empty() {
                    rotate_log_if_needed(&log_path_err);
                    log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path_err);

                    if let Ok(ref mut f) = log_file {
                        let _ = f.write_all(buffer.as_bytes());
                        let _ = f.flush();
                    }
                    buffer.clear();
                    last_flush = std::time::Instant::now();
                }

                if bytes_read == 0 {
                    if let Ok(ref mut f) = log_file {
                        if !buffer.is_empty() {
                            let _ = f.write_all(buffer.as_bytes());
                            let _ = f.flush();
                        }
                    }
                    if !err_buffer.trim().is_empty() {
                        tracing::error!("Core crashed with message: {}", err_buffer);
                    }
                    break;
                }
            }
        });

        let mut process_guard = self.process.lock().map_err(|e| anyhow!("Mutex lock failed: {e}"))?;
        *process_guard = Some(child);
        Ok(())
    }

    /// Останавливает ядро, ожидая завершения процесса
    pub fn stop(&self) -> Result<()> {
        let mut process_guard = self.process.lock().map_err(|e| anyhow!("Mutex lock failed: {e}"))?;
        if let Some(mut child) = process_guard.take() {
            tracing::info!("Stopping core process...");
            
            // Отправляем сигнал завершения (SIGKILL через std::process::Child::kill)
            let _ = child.kill();
            
            // Ожидаем завершения процесса
            let _ = child.wait();
            tracing::info!("Core process terminated.");
        }
        
        let mut config_guard = self.config_file.lock().map_err(|e| anyhow!("Mutex lock failed: {e}"))?;
        *config_guard = None; // Автоматически удалит временный файл при Drop
        
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        if let Ok(mut process_guard) = self.process.lock() {
            if let Some(ref mut child) = *process_guard {
                match child.try_wait() {
                    Ok(None) => return true, // Всё ещё работает
                    _ => return false, // Завершился или ошибка
                }
            }
        }
        false
    }
}

impl Drop for XrayBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
