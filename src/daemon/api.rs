/* api.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # HTTP REST API и Server-Sent Events (SSE) демона
//!
//! Модуль предоставляет маршруты Axum для управления демоном:
//! - `GET /api/ping`: проверка доступности (health check)
//! - `GET /api/status`: текстовый статус подключения ("Connected", "Disconnected" и др.)
//! - `GET /api/is_running`: булевый статус активности процесса ядра
//! - `POST /api/proxy/start`: запуск ядра sing-box с конфигурацией
//! - `POST /api/proxy/stop`: остановка ядра
//! - `GET /api/history`: история последних сообщений лога
//! - `GET /api/events`: SSE-поток событий в реальном времени

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
    },
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use super::core::ProxyManager;
use super::events::EventManager;
use crate::protocol::{DaemonStatus, StartProxyRequest};

/// Состояние API демона, разделяемое между всеми обработчиками запросов.
pub struct ApiState {
    pub proxy_manager: Arc<ProxyManager>,
    pub event_manager: Arc<EventManager>,
}

/// Проверка работоспособности сервиса (Health Check).
async fn ping() -> &'static str {
    tracing::trace!("REST API GET /api/ping");
    "pong"
}

/// Возвращает текущий статус ядра в формате JSON DaemonStatus.
async fn get_status(State(state): State<Arc<ApiState>>) -> Json<DaemonStatus> {
    let status = state.proxy_manager.get_status().await;
    tracing::debug!("REST API GET /api/status: {:?}", status);
    Json(status)
}

/// Проверяет, запущен ли процесс ядра.
async fn is_running(State(state): State<Arc<ApiState>>) -> Json<bool> {
    let running = state.proxy_manager.is_running().await;
    tracing::debug!("REST API GET /api/is_running: {}", running);
    Json(running)
}

/// Обработчик запуска прокси с 4-уровневой валидацией.
async fn start_proxy_handler(
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<StartProxyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(
        "REST API POST /api/proxy/start: core_type={:?}, proto={}, tun_mode={}",
        payload.core_type,
        payload.profile.protocol,
        payload.options.tun_mode
    );
    match state.proxy_manager.start_proxy(payload).await {
        Ok(_) => {
            tracing::info!("REST API POST /api/proxy/start: proxy successfully started");
            Ok(Json(serde_json::json!({
                "status": "started",
                "message": "Proxy started successfully"
            })))
        }
        Err(e) => {
            let err_msg = format!("Failed to start proxy: {}", e);
            tracing::error!("REST API POST /api/proxy/start failed: {}", err_msg);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": err_msg
                })),
            ))
        }
    }
}

/// Обработчик остановки прокси.
async fn stop_proxy_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!("REST API POST /api/proxy/stop: stopping proxy");
    match state.proxy_manager.stop_proxy().await {
        Ok(_) => {
            tracing::info!("REST API POST /api/proxy/stop: proxy successfully stopped");
            Ok(Json(serde_json::json!({
                "status": "stopped",
                "message": "Proxy stopped successfully"
            })))
        }
        Err(e) => {
            let err_msg = format!("Failed to stop proxy: {}", e);
            tracing::error!("REST API POST /api/proxy/stop failed: {}", err_msg);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": err_msg
                })),
            ))
        }
    }
}

/// Возвращает историю последних логов из кольцевого буфера.
async fn get_history(State(state): State<Arc<ApiState>>) -> Json<Vec<super::events::DaemonEvent>> {
    tracing::debug!("REST API GET /api/history");
    Json(state.event_manager.get_history())
}

/// Обработчик SSE-потока событий в реальном времени.
async fn events_handler(
    State(state): State<Arc<ApiState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, axum::Error>>> {
    tracing::debug!("New SSE subscriber on /api/events");
    let rx = state.event_manager.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(json) => Some(Ok(Event::default().data(json))),
            Err(e) => {
                tracing::error!("SSE event serialization error: {}", e);
                None
            }
        },
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

use axum::response::IntoResponse;

/// Middleware аутентификации вызывающего процесса через SO_PEERCRED и systemd-logind.
async fn auth_middleware(request: Request, next: Next) -> Response {
    let connect_info = request
        .extensions()
        .get::<axum::extract::ConnectInfo<super::auth::CallerIdentity>>();

    let identity = match connect_info {
        Some(ci) if ci.0.is_authenticated => ci.0.clone(),
        _ => {
            tracing::warn!(
                "Rejecting unauthenticated connection: missing or unauthenticated ConnectInfo"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Caller connection is unauthenticated"
                })),
            )
                .into_response();
        }
    };

    if !super::auth::is_caller_authorized(&identity).await {
        tracing::warn!(
            "Rejecting unauthorized request from UID {} (PID {:?}): access denied",
            identity.uid,
            identity.pid
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!("Caller UID {} is not authorized: no active login session", identity.uid)
            })),
        )
            .into_response();
    }

    next.run(request).await
}

/// Создает маршрутизатор Axum с зарегистрированными эндпоинтами демона.
pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/api/ping", get(ping))
        .route("/api/status", get(get_status))
        .route("/api/is_running", get(is_running))
        .route("/api/proxy/start", post(start_proxy_handler))
        .route("/api/proxy/stop", post(stop_proxy_handler))
        .route("/api/history", get(get_history))
        .route("/api/events", get(events_handler))
        .layer(middleware::from_fn(auth_middleware))
        .with_state(state)
}
