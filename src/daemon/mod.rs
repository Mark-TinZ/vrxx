pub mod network;
pub mod dns;

use crate::ipc::{VrxxDaemon, VrxxDaemonSignals};
use zbus::connection::Builder;
use std::future::pending;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::process::{Child, Command};
use std::process::Stdio;
use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::time::{timeout, Duration};

pub enum DaemonEvent {
    StatusChanged(String),
    Log { level: String, message: String },
}

pub struct ProxyManager {
    // Менеджер прокси
    child: Arc<Mutex<Option<Child>>>,
    status: Arc<Mutex<String>>,
    event_sender: async_channel::Sender<DaemonEvent>,
    tun_manager: Arc<Mutex<Option<network::TunManager>>>,
    dns_manager: Arc<Mutex<Option<dns::DnsManager>>>,
}

impl ProxyManager {
    pub fn new(event_sender: async_channel::Sender<DaemonEvent>) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Disconnected".to_string())),
            event_sender,
            tun_manager: Arc::new(Mutex::new(None)),
            dns_manager: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_status(&self) -> String {
        self.status.lock().await.clone()
    }

    async fn set_status(&self, new_status: &str) {
        let mut status = self.status.lock().await;
        *status = new_status.to_string();
        let _ = self.event_sender.send(DaemonEvent::StatusChanged(new_status.to_string())).await;
    }

    pub async fn start_proxy(&self, core_type: &str, config_json: &str, tun_mode: bool) -> anyhow::Result<()> {
        // Stop previous if running
        self.stop_proxy().await?;

        self.set_status("Connecting").await;

        // --- Раздел: Сетевая настройка ---
        if tun_mode {
            tracing::info!("Setting up TUN interface and DNS for proxy");
            let mut tun_mgr = network::TunManager::new().await?;
            tun_mgr.setup().await?;
            let if_index = tun_mgr.if_index;
            let mut tun_guard = self.tun_manager.lock().await;
            *tun_guard = Some(tun_mgr);

            let dns_mgr = dns::DnsManager::new().await?;
            dns_mgr.set_dns(if_index as i32, vec!["172.19.0.1".to_string()]).await?;
            let mut dns_guard = self.dns_manager.lock().await;
            *dns_guard = Some(dns_mgr);
        }
        // ================================

        let bin_name = match core_type {
            "sing-box" => "sing-box",
            _ => "xray",
        };

        tracing::info!("Daemon starting core {} via tokio...", bin_name);

        let mut cmd = Command::new(bin_name);
        
        // --- Раздел: Конфигурация запуска ---
        // Pass config via stdin
        if bin_name == "xray" {
            cmd.arg("run").arg("-config").arg("stdin:");
        } else {
            // FIXME: В некоторых версиях sing-box /dev/stdin может не работать корректно.
            // XXX: Возможно стоит использовать временный файл.
            cmd.arg("run").arg("-c").arg("/dev/stdin");

            // Добавляем рабочую директорию, чтобы sing-box мог найти geo-файлы
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(parent) = exe_path.parent() {
                    cmd.current_dir(parent);
                }
            }
        }
        // ================================

        // We use piped stdin to pass config
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                let err_msg = format!("Failed to start {bin_name}: {e}");
                tokio::spawn({
                    let event_sender = self.event_sender.clone();
                    let status = self.status.clone();
                    let err_msg = err_msg.clone();
                    async move {
                        let mut status_guard = status.lock().await;
                        *status_guard = "Error".to_string();
                        let _ = event_sender.send(DaemonEvent::StatusChanged("Error".to_string())).await;
                        let _ = event_sender.send(DaemonEvent::Log {
                            level: "error".to_string(),
                            message: err_msg,
                        }).await;
                    }
                });
                anyhow::anyhow!(err_msg)
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(config_json.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin); // Close stdin to signal end of config to the core
        }

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let event_sender = self.event_sender.clone();
        
        let log_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("vrxx").join("logs");
        std::fs::create_dir_all(&log_dir).ok();
        let core_log_path = log_dir.join("core.log");

        // --- Раздел: Обработка логов ядра ---
        // Stdout reader task
        tokio::spawn({
            let event_sender = event_sender.clone();
            let core_log_path = core_log_path.clone();
            async move {
                let mut reader = BufReader::new(stdout).lines();
                // OPTIMIZE: Открываем файл один раз для записи всех логов текущей сессии
                #[cfg(unix)]
                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

                let mut opts = std::fs::OpenOptions::new();
                opts.create(true).append(true);
                #[cfg(unix)]
                opts.mode(0o600);

                let mut file = opts.open(&core_log_path).ok();
                #[cfg(unix)]
                if let Some(ref f) = file {
                    let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
                }

                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(ref mut f) = file {
                        use std::io::Write;
                        let _ = writeln!(f, "{}", line);
                    }

                    let _ = event_sender.send(DaemonEvent::Log {
                        level: "info".to_string(),
                        message: line,
                    }).await;
                }
            }
        });

        // Stderr reader task
        tokio::spawn({
            let event_sender = event_sender.clone();
            let core_log_path = core_log_path.clone();
            async move {
                let mut reader = BufReader::new(stderr).lines();
                // OPTIMIZE: Открываем файл один раз
                #[cfg(unix)]
                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

                let mut opts = std::fs::OpenOptions::new();
                opts.create(true).append(true);
                #[cfg(unix)]
                opts.mode(0o600);

                let mut file = opts.open(&core_log_path).ok();
                #[cfg(unix)]
                if let Some(ref f) = file {
                    let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
                }

                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(ref mut f) = file {
                        use std::io::Write;
                        let _ = writeln!(f, "{}", line);
                    }

                    let _ = event_sender.send(DaemonEvent::Log {
                        level: "error".to_string(),
                        message: line,
                    }).await;
                }
            }
        });
        // ================================

        let mut guard = self.child.lock().await;
        *guard = Some(child);

        // Monitor task
        let child_arc = self.child.clone();
        let status_arc = self.status.clone();
        let event_sender_clone = event_sender.clone();
        tokio::spawn(async move {
            let mut guard = child_arc.lock().await;
            if let Some(child) = guard.as_mut() {
                match child.wait().await {
                    Ok(exit_status) => {
                        let mut status = status_arc.lock().await;
                        if *status != "Disconnecting" && *status != "Disconnected" {
                            tracing::warn!("Proxy exited unexpectedly with status: {}", exit_status);
                            *status = "Error".to_string();
                            let _ = event_sender_clone.send(DaemonEvent::StatusChanged("Error".to_string())).await;
                            let _ = event_sender_clone.send(DaemonEvent::Log {
                                level: "error".to_string(),
                                message: format!("Proxy exited unexpectedly with status: {}", exit_status),
                            }).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error waiting for proxy: {}", e);
                    }
                }
            }
        });

        self.set_status("Connected").await;

        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    pub async fn stop_proxy(&self) -> anyhow::Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            self.set_status("Disconnecting").await;
            if let Some(pid_u32) = child.id() {
                let pid = Pid::from_raw(pid_u32 as i32);
                tracing::info!("Stopping proxy process (PID {})...", pid_u32);
                
                // Send SIGTERM
                let _ = signal::kill(pid, Signal::SIGTERM);
                
                // Wait with timeout
                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(_) => {
                        tracing::info!("Proxy stopped gracefully.");
                    }
                    Err(_) => {
                        tracing::warn!("Proxy stop timeout, killing (SIGKILL)...");
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                }
            }
            self.set_status("Disconnected").await;
        }

        // --- Раздел: Очистка сетевых настроек ---
        // Teardown DNS and TUN
        if let Some(dns_mgr) = self.dns_manager.lock().await.take() {
            if let Some(tun_mgr) = self.tun_manager.lock().await.as_mut() {
                tracing::info!("Resetting DNS settings for TUN interface");
                let _ = dns_mgr.reset_dns(tun_mgr.if_index as i32).await;
            }
        }
        
        if let Some(mut tun_mgr) = self.tun_manager.lock().await.take() {
            tracing::info!("Tearing down TUN interface");
            let _ = tun_mgr.teardown().await;
        }
        // ================================

        Ok(())
    }
}

pub async fn run() -> anyhow::Result<()> {
    tracing::info!("Starting vrxx privileged daemon...");

    let (event_sender, event_receiver) = async_channel::unbounded::<DaemonEvent>();
    let proxy_manager = Arc::new(ProxyManager::new(event_sender));
    let daemon = VrxxDaemon {
        proxy_manager: proxy_manager.clone(),
    };

    // --- Раздел: D-Bus Сервер ---
    let connection = Builder::system()?
        .name("ru.mark.vrxx.daemon")?
        .serve_at("/ru/mark/vrxx/Daemon", daemon)?
        .build()
        .await?;

    tracing::info!("Daemon is now registered on the system bus.");

    let object_server = connection.object_server();
    let iface_ref = object_server.interface::<_, VrxxDaemon>("/ru/mark/vrxx/Daemon").await?;

    // Event processing loop
    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match event {
                DaemonEvent::StatusChanged(_) => {
                    let iface = iface_ref.get().await;
                    let _ = iface.status_changed(iface_ref.signal_emitter()).await;
                }
                DaemonEvent::Log { level, message } => {
                    let _ = iface_ref.log_message(&level, &message).await;
                }
            }
        }
    });
    // ================================

    // Keep the daemon running indefinitely
    pending::<()>().await;

    Ok(())
}

#[cfg(test)]
mod tests;
