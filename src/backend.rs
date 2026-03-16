use std::process::{Command, Child, Stdio};
use std::sync::{Arc, Mutex};
use std::io::{BufRead, BufReader, Write};
use crate::settings::SettingsManager;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tempfile::NamedTempFile;

pub fn log_app_event(level: &str, message: &str) {
    let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("core.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "[APP] [{}] [{}] {}", now, level.to_uppercase(), message);
    }
}

#[derive(Debug)]
pub struct XrayBackend {
    process: Arc<Mutex<Option<Child>>>,
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

    pub fn start(&self, config_json: &str) -> Result<(), String> {
        let mut process_guard = self.process.lock().map_err(|e| e.to_string())?;
        
        // Останавливаем предыдущий процесс gracefully
        if let Some(mut child) = process_guard.take() {
            let pid = Pid::from_raw(child.id() as i32);
            let _ = kill(pid, Signal::SIGTERM);
            
            // Ждем завершения с таймаутом, если не завершился - убиваем жестко
            let mut max_wait = 50; // 50 * 100ms = 5s
            while max_wait > 0 {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    _ => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        max_wait -= 1;
                    }
                }
            }
            if max_wait == 0 {
                let _ = child.kill();
            }
            let _ = child.wait();
        }

        // Сохраняем конфиг во временный файл безопасно
        let mut named_temp_file = tempfile::Builder::new()
            .prefix("vrxx_config_")
            .suffix(".json")
            .tempfile()
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        named_temp_file.write_all(config_json.as_bytes()).map_err(|e| format!("Failed to write config: {}", e))?;
        let temp_path = named_temp_file.path().to_path_buf();
        
        let mut config_file_guard = self.config_file.lock().map_err(|e| e.to_string())?;
        *config_file_guard = Some(named_temp_file);

        // Получаем настройки
        let settings = SettingsManager::new().load();
        
        let bin_name = match settings.core.as_str() {
            "sing-box" => "sing-box",
            _ => "xray",
        };

        // Check if binary exists
        let which_check = Command::new("which").arg(bin_name).output();
        let binary_missing = match which_check {
            Ok(output) => !output.status.success(),
            Err(_) => true,
        };
        if binary_missing {
            return Err(format!("Ядро {} не найдено в системе.\n\nПожалуйста, установите его (например, через ваш пакетный менеджер: pacman/apt/apt-get install {}) или выберите другое ядро в Настройках.", bin_name, bin_name));
        }

        if settings.tun_mode {
            let which_core = Command::new("which").arg(&bin_name).output();
            let core_path = match which_core {
                Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                Err(_) => bin_name.to_string(),
            };

            // Check for capabilities to avoid requiring root/pkexec
            let cap_check = Command::new("getcap").arg(&core_path).output();
            let has_caps = match cap_check {
                Ok(out) => String::from_utf8_lossy(&out.stdout).contains("cap_net_admin"),
                Err(_) => false,
            };
            
            if !has_caps {
                return Err(format!("Режим TUN включен, но ядро {} не имеет необходимых прав (cap_net_admin).\n\nВыполните в терминале:\nsudo setcap cap_net_admin=ep {}", bin_name, core_path));
            }
        }

        let mut cmd = Command::new(bin_name);

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
                let stdout = child.stdout.take().ok_or_else(|| "Failed to capture stdout".to_string())?;
                let stderr = child.stderr.take().ok_or_else(|| "Failed to capture stderr".to_string())?;
                
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
            let pid = Pid::from_raw(child.id() as i32);
            let _ = kill(pid, Signal::SIGTERM);
            
            std::thread::spawn(move || {
                let mut max_wait = 50; // 5s
                while max_wait > 0 {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        _ => {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            max_wait -= 1;
                        }
                    }
                }
                if max_wait == 0 {
                    let _ = child.kill();
                }
                let _ = child.wait();
                println!("Core process exited.");
            });
        }
        
        let mut config_guard = self.config_file.lock().map_err(|e| e.to_string())?;
        *config_guard = None; // Delete temp file
        
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
