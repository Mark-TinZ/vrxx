/* events.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # События демона, широковещательная рассылка и Tracing Layer (Events & Logging)
//!
//! Модуль содержит:
//! - Перечисление источников логов [`LogSource`] (`App`, `Core`, `Access`)
//! - Структуру событий [`DaemonEvent`] для информирования UI через SSE
//! - Менеджер событий [`EventManager`] с кольцевым буфером истории фиксированной емкости
//! - Слой `tracing_subscriber::Layer` ([`SseTracingLayer`]) для прозрачной пересылки всех логов приложения в SSE

use crate::protocol::{DaemonStatus, LogLevel};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing_subscriber::Layer;

/// Источник лога для точной фильтрации в UI.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum LogSource {
    #[serde(rename = "app")]
    #[default]
    App,
    #[serde(rename = "core")]
    Core,
    #[serde(rename = "access")]
    Access,
}

/// События, которые демон отправляет подписчикам (клиентам UI).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum DaemonEvent {
    /// Изменение статуса подключения.
    StatusChanged(DaemonStatus),
    /// Лог-сообщение (источник, уровень и текст).
    Log {
        #[serde(default)]
        source: LogSource,
        level: LogLevel,
        message: String,
    },
    /// Данные телеметрии сетевого трафика (скорость в байт/сек, общий объем в байтах).
    Traffic {
        down_speed: u64,
        up_speed: u64,
        down_total: u64,
        up_total: u64,
    },
}

/// Менеджер событий с кольцевым буфером для хранения истории логов.
pub struct EventManager {
    sender: broadcast::Sender<DaemonEvent>,
    history: Arc<Mutex<VecDeque<DaemonEvent>>>,
    max_capacity: usize,
}

impl EventManager {
    /// Создает новый экземпляр EventManager с заданной емкостью истории.
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<DaemonEvent>) {
        let (sender, receiver) = broadcast::channel(1024);
        let history = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));

        (
            Self {
                sender,
                history,
                max_capacity: capacity,
            },
            receiver,
        )
    }

    /// Отправляет событие всем подписчикам и сохраняет логи в кольцевую историю.
    pub fn broadcast(&self, event: DaemonEvent) {
        if let DaemonEvent::Log { .. } = &event {
            let mut history = self.history.lock().unwrap_or_else(|e| e.into_inner());
            while history.len() >= self.max_capacity && !history.is_empty() {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
        let _ = self.sender.send(event);
    }

    /// Возвращает текущий снимок истории логов.
    pub fn get_history(&self) -> Vec<DaemonEvent> {
        let history = self.history.lock().unwrap_or_else(|e| e.into_inner());
        history.iter().cloned().collect()
    }

    /// Создает новый ресивер широковещательного канала для нового подписчика.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.sender.subscribe()
    }
}

/// Кастомный слой tracing для автоматической трансляции логов в SSE.
pub struct SseTracingLayer {
    event_manager: Arc<EventManager>,
}

impl SseTracingLayer {
    pub fn new(event_manager: Arc<EventManager>) -> Self {
        Self { event_manager }
    }
}

impl<S> Layer<S> for SseTracingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        // Игнорируем логи с таргетом sing_box, так как они уже обработаны и отправлены напрямую парсером
        if metadata.target() == "sing_box" {
            return;
        }

        let level_str = metadata.level().to_string();
        let log_level = LogLevel::from(level_str.as_str());

        // Извлекаем сообщение из события
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        if let Some(raw_msg) = visitor.message {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let level_upper = log_level.as_str().to_uppercase();
            let target = metadata.target();
            let formatted_msg = format!("{now} {level_upper} [{target}]: {raw_msg}");

            self.event_manager.broadcast(DaemonEvent::Log {
                source: LogSource::App,
                level: log_level,
                message: formatted_msg,
            });
        }
    }
}

struct MessageVisitor {
    message: Option<String>,
}

impl MessageVisitor {
    fn new() -> Self {
        Self { message: None }
    }
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let dbg = format!("{:?}", value);
            let clean = if dbg.starts_with('"') && dbg.ends_with('"') && dbg.len() >= 2 {
                dbg[1..dbg.len() - 1].to_string()
            } else {
                dbg
            };
            self.message = Some(clean);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_event_traffic_roundtrip() {
        let event = DaemonEvent::Traffic {
            down_speed: 1048576,
            up_speed: 524288,
            down_total: 104857600,
            up_total: 52428800,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_traffic_event_not_in_history() {
        let (manager, mut rx) = EventManager::new(10);
        manager.broadcast(DaemonEvent::Traffic {
            down_speed: 100,
            up_speed: 50,
            down_total: 1000,
            up_total: 500,
        });
        assert!(manager.get_history().is_empty());
        let received = rx.try_recv().unwrap();
        match received {
            DaemonEvent::Traffic { down_speed, .. } => assert_eq!(down_speed, 100),
            _ => panic!("Expected Traffic event"),
        }
    }
}
