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
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use super::core::{ProxyManager, StartProxyRequest};
use super::events::EventManager;

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

/// Возвращает текущий статус ядра.
async fn get_status(State(state): State<Arc<ApiState>>) -> String {
    let status = state.proxy_manager.get_status().await;
    tracing::debug!("REST API GET /api/status: {}", status);
    status
}

/// Проверяет, запущен ли процесс ядра.
async fn is_running(State(state): State<Arc<ApiState>>) -> Json<bool> {
    let running = state.proxy_manager.is_running().await;
    tracing::debug!("REST API GET /api/is_running: {}", running);
    Json(running)
}

/// Обработчик запуска прокси.
async fn start_proxy_handler(
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<StartProxyRequest>,
) -> Result<&'static str, (StatusCode, String)> {
    tracing::info!(
        "REST API POST /api/proxy/start: core_type={}, tun_mode={}",
        payload.core_type,
        payload.tun_mode
    );
    match state
        .proxy_manager
        .start_proxy(&payload.core_type, &payload.config_json, payload.tun_mode)
        .await
    {
        Ok(_) => {
            tracing::info!("REST API POST /api/proxy/start: proxy successfully started");
            Ok("Proxy started successfully")
        }
        Err(e) => {
            tracing::error!("REST API POST /api/proxy/start failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start proxy: {}", e),
            ))
        }
    }
}

/// Обработчик остановки прокси.
async fn stop_proxy_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<&'static str, (StatusCode, String)> {
    tracing::info!("REST API POST /api/proxy/stop: stopping proxy");
    match state.proxy_manager.stop_proxy().await {
        Ok(_) => {
            tracing::info!("REST API POST /api/proxy/stop: proxy successfully stopped");
            Ok("Proxy stopped successfully")
        }
        Err(e) => {
            tracing::error!("REST API POST /api/proxy/stop failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to stop proxy: {}", e),
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
        .with_state(state)
}
