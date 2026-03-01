use std::process::{Command, Child, Stdio};
use std::sync::{Arc, Mutex};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use crate::settings::SettingsManager;

#[derive(Debug)]
pub struct XrayBackend {
    process: Arc<Mutex<Option<Child>>>,
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
        }
    }

    pub fn start(&self, config_json: &str) -> Result<(), String> {
        let mut process_guard = self.process.lock().map_err(|e| e.to_string())?;
        
        // Останавливаем предыдущий процесс
        if let Some(mut child) = process_guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Сохраняем конфиг во временный файл
        let mut temp_path = std::env::temp_dir();
        temp_path.push("vrxx_core_config.json");
        fs::write(&temp_path, config_json).map_err(|e| format!("Failed to write config: {}", e))?;

        // Получаем настройки
        let settings = SettingsManager::new().load();
        
        let bin_name = match settings.core.as_str() {
            "sing-box" => "sing-box",
            _ => "xray",
        };

        // Check if binary exists
        let which_check = Command::new("which").arg(bin_name).output();
        if which_check.is_err() || !which_check.unwrap().status.success() {
            return Err(format!("Ядро {} не найдено в системе.\n\nПожалуйста, установите его (например, через ваш пакетный менеджер: pacman/apt/apt-get install {}) или выберите другое ядро в Настройках.", bin_name, bin_name));
        }

        let mut cmd = if settings.tun_mode {
            let mut c = Command::new("pkexec");
            c.arg(bin_name);
            c
        } else {
            Command::new(bin_name)
        };

        if bin_name == "xray" {
            cmd.arg("run").arg("-c").arg(&temp_path);
        } else if bin_name == "sing-box" {
            cmd.arg("run").arg("-c").arg(&temp_path);
        }

        // SSD Protection: Use pipes and manual buffering instead of direct file writing
        match cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn() 
        {
            Ok(mut child) => {
                let stdout = child.stdout.take().unwrap();
                let stderr = child.stderr.take().unwrap();
                
                // Spawn a thread to handle logs economically
                std::thread::spawn(move || {
                    let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
                    std::fs::create_dir_all(&log_dir).ok();
                    let log_path = log_dir.join("core.log");

                    let mut log_file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path);

                    let mut reader_out = BufReader::new(stdout);
                    let mut reader_err = BufReader::new(stderr);
                    let mut buffer = String::new();
                    let mut last_flush = std::time::Instant::now();

                    loop {
                        let mut line = String::new();
                        // Non-blocking approach would be better, but for now we alternate
                        if reader_out.read_line(&mut line).unwrap_or(0) > 0 {
                            buffer.push_str(&line);
                        }
                        line.clear();
                        if reader_err.read_line(&mut line).unwrap_or(0) > 0 {
                            buffer.push_str(&line);
                        }

                        // Flush to disk only every 5 seconds or if buffer is large (> 8KB)
                        if (last_flush.elapsed().as_secs() >= 5 || buffer.len() > 8192) && !buffer.is_empty() {
                            if let Ok(ref mut f) = log_file {
                                let _ = f.write_all(buffer.as_bytes());
                                let _ = f.flush();
                            }
                            buffer.clear();
                            last_flush = std::time::Instant::now();
                        }

                        if line.is_empty() && buffer.is_empty() {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                });

                *process_guard = Some(child);
                Ok(())
            }
            Err(e) => Err(format!("Failed to start {} (is it installed?): {}", bin_name, e))
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut process_guard = self.process.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = process_guard.take() {
            println!("Stopping core process...");
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
                println!("Core process exited.");
            });
        }
        Ok(())
    }
}

impl Drop for XrayBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
