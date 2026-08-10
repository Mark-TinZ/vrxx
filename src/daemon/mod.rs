pub mod api;
pub mod core;
pub mod dns;
pub mod events;
pub mod network;
pub mod updater;

pub use core::StartProxyRequest;
pub use events::DaemonEvent;
use std::sync::Arc;

/// Точка входа для запуска демона с предустановленным менеджером событий.
pub async fn run_with_manager(event_manager: Arc<events::EventManager>) -> anyhow::Result<()> {
    tracing::info!("Starting vrxx daemon REST API on 127.0.0.1:13337...");

    // 0. Запуск процедур самовосстановления сети (Self-Healing)
    if let Err(e) = network::self_heal().await {
        tracing::warn!("Network self-healing warning: {}", e);
    }

    // 1. Инициализируем менеджер прокси
    let proxy_manager = Arc::new(core::ProxyManager::new(event_manager.clone()));

    // 2. Создаем состояние API
    let state = Arc::new(api::ApiState {
        proxy_manager,
        event_manager,
    });

    // 3. Запускаем сервер
    let app = api::create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:13337").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Точка входа для запуска демона (создает свой менеджер).
pub async fn run() -> anyhow::Result<()> {
    let (event_manager, _) = events::EventManager::new(100);
    run_with_manager(Arc::new(event_manager)).await
}

#[cfg(test)]
mod tests;
