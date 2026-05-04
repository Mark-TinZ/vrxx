use crate::daemon::ProxyManager;
use std::sync::Arc;
use tokio::sync::OnceCell;
use zbus::Connection;

use zbus::{interface, proxy};

static SYSTEM_CONNECTION: OnceCell<Connection> = OnceCell::const_new();
static DAEMON_PROXY: OnceCell<DaemonProxy<'static>> = OnceCell::const_new();

pub async fn get_daemon_proxy() -> zbus::Result<DaemonProxy<'static>> {
    DAEMON_PROXY
        .get_or_try_init(|| async {
            let conn = get_system_connection().await?;
            DaemonProxy::new(&conn).await
        })
        .await
        .cloned()
}

pub async fn get_system_connection() -> zbus::Result<Connection> {
    SYSTEM_CONNECTION
        .get_or_try_init(|| async { Connection::system().await })
        .await
        .cloned()
}

pub struct VrxxDaemon {
    pub proxy_manager: Arc<ProxyManager>,
}

#[interface(name = "ru.mark.vrxx.Daemon")]
impl VrxxDaemon {
    async fn ping(&self) -> zbus::fdo::Result<String> {
        Ok("pong".to_string())
    }

    async fn start_proxy(
        &self,
        core_type: String,
        config_json: String,
        tun_mode: bool,
    ) -> zbus::fdo::Result<String> {
        match self
            .proxy_manager
            .start_proxy(&core_type, &config_json, tun_mode)
            .await
        {
            Ok(_) => Ok("Proxy started successfully".to_string()),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    async fn stop_proxy(&self) -> zbus::fdo::Result<String> {
        match self.proxy_manager.stop_proxy().await {
            Ok(_) => Ok("Proxy stopped successfully".to_string()),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    async fn is_running(&self) -> zbus::fdo::Result<bool> {
        Ok(self.proxy_manager.is_running().await)
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.proxy_manager.get_status().await
    }

    #[zbus(signal)]
    async fn log_message(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        level: &str,
        message: &str,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "ru.mark.vrxx.Daemon",
    default_service = "ru.mark.vrxx.Daemon",
    default_path = "/ru/mark/vrxx/Daemon"
)]
pub trait Daemon {
    async fn ping(&self) -> zbus::Result<String>;
    async fn start_proxy(
        &self,
        core_type: String,
        config_json: String,
        tun_mode: bool,
    ) -> zbus::Result<String>;
    async fn stop_proxy(&self) -> zbus::Result<String>;
    async fn is_running(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn log_message(&self, level: &str, message: &str) -> zbus::Result<()>;
}
