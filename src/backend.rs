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
                
                let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
                std::fs::create_dir_all(&log_dir).ok();
                let log_path = log_dir.join("core.log");

                // Spawn a thread for stdout
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
                            if let Ok(ref mut f) = log_file {
                                let _ = f.write_all(buffer.as_bytes());
                                let _ = f.flush();
                            }
                            buffer.clear();
                            last_flush = std::time::Instant::now();
                        }

                        if bytes_read == 0 {
                            // EOF
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

                // Spawn a thread for stderr
                let log_path_err = log_path.clone();
                std::thread::spawn(move || {
                    let mut log_file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path_err);
                    let mut reader = BufReader::new(stderr);
                    let mut buffer = String::new();
                    let mut last_flush = std::time::Instant::now();

                    loop {
                        let mut line = String::new();
                        let bytes_read = reader.read_line(&mut line).unwrap_or(0);
                        if bytes_read > 0 {
                            buffer.push_str(&line);
                        }

                        if (last_flush.elapsed().as_secs() >= 5 || buffer.len() > 8192) && !buffer.is_empty() {
                            if let Ok(ref mut f) = log_file {
                                let _ = f.write_all(buffer.as_bytes());
                                let _ = f.flush();
                            }
                            buffer.clear();
                            last_flush = std::time::Instant::now();
                        }

                        if bytes_read == 0 {
                            // EOF
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

    pub fn is_running(&self) -> bool {
        if let Ok(mut process_guard) = self.process.lock() {
            if let Some(ref mut child) = *process_guard {
                match child.try_wait() {
                    Ok(None) => return true, // Still running
                    _ => return false, // Exited or error
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
