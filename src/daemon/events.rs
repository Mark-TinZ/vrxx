use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing_subscriber::Layer;

/// События, которые демон отправляет в UI.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DaemonEvent {
    /// Изменение статуса подключения.
    StatusChanged(String),
    /// Лог-сообщение (уровень и текст).
    Log { level: String, message: String },
}

/// Менеджер событий с кольцевым буфером для хранения истории логов.
pub struct EventManager {
    sender: broadcast::Sender<DaemonEvent>,
    history: Arc<Mutex<VecDeque<DaemonEvent>>>,
}

impl EventManager {
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<DaemonEvent>) {
        let (sender, receiver) = broadcast::channel(1024);
        let history = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));

        (Self { sender, history }, receiver)
    }

    /// Отправляет событие всем подписчикам и сохраняет логи в историю.
    pub fn broadcast(&self, event: DaemonEvent) {
        if let DaemonEvent::Log { .. } = &event {
            let mut history = self.history.lock().unwrap();
            if history.len() >= history.capacity() {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
        let _ = self.sender.send(event);
    }

    /// Возвращает текущую историю логов.
    pub fn get_history(&self) -> Vec<DaemonEvent> {
        let history = self.history.lock().unwrap();
        history.iter().cloned().collect()
    }

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
        let level = metadata.level().to_string().to_lowercase();

        // Извлекаем сообщение из события
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        if let Some(message) = visitor.message {
            self.event_manager
                .broadcast(DaemonEvent::Log { level, message });
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
            self.message = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}
