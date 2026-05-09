use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use super::core::{ProxyManager, StartProxyRequest};
use super::events::EventManager;

/// Состояние API демона.
pub struct ApiState {
    pub proxy_manager: Arc<ProxyManager>,
    pub event_manager: Arc<EventManager>,
}

async fn ping() -> &'static str {
    "pong"
}

async fn get_status(State(state): State<Arc<ApiState>>) -> String {
    state.proxy_manager.get_status().await
}

async fn is_running(State(state): State<Arc<ApiState>>) -> Json<bool> {
    Json(state.proxy_manager.is_running().await)
}

async fn start_proxy_handler(
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<StartProxyRequest>,
) -> Result<&'static str, axum::http::StatusCode> {
    match state
        .proxy_manager
        .start_proxy(&payload.core_type, &payload.config_json, payload.tun_mode)
        .await
    {
        Ok(_) => Ok("Proxy started successfully"),
        Err(e) => {
            tracing::error!("Failed to start proxy: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn stop_proxy_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<&'static str, axum::http::StatusCode> {
    match state.proxy_manager.stop_proxy().await {
        Ok(_) => Ok("Proxy stopped successfully"),
        Err(e) => {
            tracing::error!("Failed to stop proxy: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Возвращает историю последних логов.
async fn get_history(State(state): State<Arc<ApiState>>) -> Json<Vec<super::events::DaemonEvent>> {
    Json(state.event_manager.get_history())
}

async fn sse_handler(
    State(state): State<Arc<ApiState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.event_manager.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(json) => Some(Ok(Event::default().data(json))),
            Err(_) => None,
        },
        Err(_) => None,
    });

    // Добавляем KeepAlive для стабильности соединения
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/api/ping", get(ping))
        .route("/api/status", get(get_status))
        .route("/api/is_running", get(is_running))
        .route("/api/proxy/start", post(start_proxy_handler))
        .route("/api/proxy/stop", post(stop_proxy_handler))
        .route("/api/events", get(sse_handler))
        .route("/api/history", get(get_history))
        .with_state(state)
}
