use super::events::{DaemonEvent, EventManager};
use super::{dns, network};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

use serde::{Deserialize, Serialize};

/// Структура запроса на запуск прокси-сервера.
#[derive(Deserialize, Serialize)]
pub struct StartProxyRequest {
    /// Тип ядра (сейчас поддерживается только sing-box).
    pub core_type: String,
    /// Полный JSON конфигурации ядра.
    pub config_json: String,
    /// Флаг включения режима TUN (прозрачное проксирование всего трафика).
    pub tun_mode: bool,
}

/// Основной менеджер жизненного цикла прокси-процесса.
pub struct ProxyManager {
    child: Arc<Mutex<Option<Child>>>,
    status: Arc<Mutex<String>>,
    event_manager: Arc<EventManager>,
    tun_manager: Arc<Mutex<Option<network::TunManager>>>,
    dns_manager: Arc<Mutex<Option<dns::DnsManager>>>,
}

impl ProxyManager {
    pub fn new(event_manager: Arc<EventManager>) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Disconnected".to_string())),
            event_manager,
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
        self.event_manager
            .broadcast(DaemonEvent::StatusChanged(new_status.to_string()));
    }

    pub async fn start_proxy(
        &self,
        core_type: &str,
        config_json: &str,
        tun_mode: bool,
    ) -> anyhow::Result<()> {
        self.stop_proxy().await?;
        self.set_status("Connecting").await;

        if tun_mode && core_type != "sing-box" {
            tracing::info!("Setting up TUN interface and DNS for proxy (manual mode)");
            let setup_res = async {
                let mut tun_mgr = network::TunManager::new().await?;
                tun_mgr.setup().await?;
                let if_index = tun_mgr.if_index;
                let mut tun_guard = self.tun_manager.lock().await;
                *tun_guard = Some(tun_mgr);

                let dns_mgr = dns::DnsManager::new().await?;
                dns_mgr
                    .set_dns(if_index as i32, vec!["172.19.0.1".to_string()])
                    .await?;
                let mut dns_guard = self.dns_manager.lock().await;
                *dns_guard = Some(dns_mgr);
                Ok::<(), anyhow::Error>(())
            }
            .await;

            if let Err(e) = setup_res {
                tracing::error!("Network setup failed: {}", e);
                self.set_status("Error").await;
                return Err(e);
            }
        } else {
            tracing::debug!("Пропускаем ручную настройку TUN: sing-box настраивает TUN автоматически (auto_route: true).");
        }

        let bin_name = match super::updater::resolve_singbox_binary().await {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Core not found. Please install it via UI: {}", e);
                self.set_status("Error").await;
                return Err(e);
            }
        };
        tracing::info!("Daemon starting core {} via tokio...", bin_name);

        let mut cmd = Command::new(&bin_name);
        cmd.arg("run").arg("-c").arg("stdin");

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                cmd.current_dir(parent);
            }
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                tracing::error!("Failed to start {bin_name}: {e}");
                self.event_manager
                    .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
                anyhow::anyhow!(e)
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(config_json.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // Stdout reader task (пишем напрямую в tracing, Layer сам отправит в SSE)
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::info!("{}", line);
            }
        });

        // Stderr reader task
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::error!("{}", line);
            }
        });

        let mut guard = self.child.lock().await;
        *guard = Some(child);

        // Monitor task
        let child_arc = self.child.clone();
        let status_arc = self.status.clone();
        let event_manager = self.event_manager.clone();
        tokio::spawn(async move {
            let mut guard = child_arc.lock().await;
            if let Some(child) = guard.as_mut() {
                match child.wait().await {
                    Ok(exit_status) => {
                        let mut status = status_arc.lock().await;
                        if *status != "Disconnecting" && *status != "Disconnected" {
                            tracing::warn!(
                                "Proxy exited unexpectedly with status: {}",
                                exit_status
                            );
                            *status = "Error".to_string();
                            event_manager
                                .broadcast(DaemonEvent::StatusChanged("Error".to_string()));
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
            matches!(child.try_wait(), Ok(None))
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

                let _ = signal::kill(pid, Signal::SIGTERM);

                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(_) => tracing::info!("Proxy stopped gracefully."),
                    Err(_) => {
                        tracing::warn!("Proxy stop timeout, killing (SIGKILL)...");
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                }
            }
            self.set_status("Disconnected").await;
        }

        if let Some(dns_mgr) = self.dns_manager.lock().await.take() {
            if let Some(tun_mgr) = self.tun_manager.lock().await.as_mut() {
                let _ = dns_mgr.reset_dns(tun_mgr.if_index as i32).await;
            }
        }

        if let Some(mut tun_mgr) = self.tun_manager.lock().await.take() {
            let _ = tun_mgr.teardown().await;
        }

        Ok(())
    }
}
