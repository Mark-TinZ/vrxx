/* logger.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Отраслевая система логирования VRXX (Enterprise Tracing System)
//!
//! Обеспечивает:
//! - Структурированное логирование (Structured Key-Value logging) через `tracing`
//! - Динамическую горячую смену уровня логирования на лету без перезапуска (`tracing_subscriber::reload::Handle`)
//! - Неблокирующую потокобезопасную запись в файл (`tracing-appender`)
//! - Суточную ротацию логов с лимитом размера файла (макс. 10 МБ) и ротацией устаревших файлов (7 дней / 50 МБ)
//! - Санитизацию конфиденциальных данных (маскирование ключей, токенов, паролей)
//! - Перехват консольных логов через локальный Tracing хук (`ConsoleUiTracingLayer`) и буферизацию в памяти (`UiLogBroadcaster`)
//! - Опциональный вывод в консоль для разработки и SSE-слой для трансляции в UI

use crate::daemon::events::LogSource;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter, Layer};

/// Глобальный дескриптор для динамического обновления фильтра логирования на лету
pub type LogFilterReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;
static RELOAD_HANDLE: OnceLock<LogFilterReloadHandle> = OnceLock::new();

/// Элемент лога для UI виртуальной консоли и межпоточного вещания
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LogEntryItem {
    pub source: LogSource,
    pub level: String,
    pub message: String,
}

/// Максимальное количество записей в кольцевом буфере памяти UI логов (минимизация потребления RAM)
pub const MAX_UI_LOG_HISTORY: usize = 1000;

/// Потокобезопасный кольцевой буфер и диспетчер широковещательной рассылки логов для UI
pub struct UiLogBroadcaster {
    sender: broadcast::Sender<LogEntryItem>,
    history: Mutex<VecDeque<LogEntryItem>>,
    capacity: usize,
}

impl UiLogBroadcaster {
    /// Создает новый экземпляр UiLogBroadcaster с заданной максимальной емкостью
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            sender,
            history: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    /// Добавляет запись в кольцевую историю и отправляет всем активным подписчикам
    pub fn broadcast(&self, entry: LogEntryItem) {
        if let Ok(mut hist) = self.history.lock() {
            while hist.len() >= self.capacity && !hist.is_empty() {
                hist.pop_front();
            }
            hist.push_back(entry.clone());
        }
        let _ = self.sender.send(entry);
    }

    /// Возвращает моментальный снимок последних записей логов из оперативной памяти
    pub fn get_history(&self) -> Vec<LogEntryItem> {
        self.history
            .lock()
            .map(|hist| hist.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Очищает кольцевой буфер истории логов в оперативной памяти
    pub fn clear_history(&self) {
        if let Ok(mut hist) = self.history.lock() {
            hist.clear();
        }
    }

    /// Создает новый ресивер широковещательного канала для окна логов
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntryItem> {
        self.sender.subscribe()
    }
}

static UI_BROADCASTER: OnceLock<UiLogBroadcaster> = OnceLock::new();

/// Возвращает ссылку на глобальный синглтон UiLogBroadcaster
pub fn get_ui_broadcaster() -> &'static UiLogBroadcaster {
    UI_BROADCASTER.get_or_init(|| UiLogBroadcaster::new(MAX_UI_LOG_HISTORY))
}

/// Рассылает запись лога в глобальный UI канал и добавляет в историю
pub fn broadcast_ui_log(entry: LogEntryItem) {
    get_ui_broadcaster().broadcast(entry);
}

/// Возвращает накопленную историю UI логов из памяти
pub fn get_ui_log_history() -> Vec<LogEntryItem> {
    get_ui_broadcaster().get_history()
}

/// Очищает накопленную историю UI логов в памяти
pub fn clear_ui_log_history() {
    get_ui_broadcaster().clear_history();
}

/// Подписывается на поток новых UI логов
pub fn subscribe_ui_logs() -> broadcast::Receiver<LogEntryItem> {
    get_ui_broadcaster().subscribe()
}

/// Кастомный слой Tracing для автоматического перехвата всех событий консоли в виртуальную консоль UI
pub struct ConsoleUiTracingLayer;

impl<S> Layer<S> for ConsoleUiTracingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        // Игнорируем внутренние логи sing_box, так как они парсятся отдельно через stdout/stderr ядра
        if metadata.target() == "sing_box" {
            return;
        }

        let level = metadata.level().to_string().to_lowercase();
        let mut visitor = UiMessageVisitor::new();
        event.record(&mut visitor);

        let (raw_msg, extra) = match visitor.message {
            Some(m) => (m, visitor.fields.join(" ")),
            None => (visitor.fields.join(" "), String::new()),
        };
        if raw_msg.is_empty() && extra.is_empty() {
            return;
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let level_upper = level.to_uppercase();
        let target = metadata.target();

        let formatted_msg = if extra.is_empty() {
            format!("{now} {level_upper} [{target}]: {raw_msg}")
        } else {
            format!("{now} {level_upper} [{target}]: {raw_msg} {extra}")
        };

        broadcast_ui_log(LogEntryItem {
            source: LogSource::App,
            level,
            message: formatted_msg,
        });
    }
}

struct UiMessageVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl UiMessageVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: Vec::new(),
        }
    }
}

impl tracing::field::Visit for UiMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let dbg = format!("{:?}", value);
            let clean = if dbg.starts_with('"') && dbg.ends_with('"') && dbg.len() >= 2 {
                dbg[1..dbg.len() - 1].to_string()
            } else {
                dbg
            };
            self.message = Some(clean);
        } else {
            self.fields.push(format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }
}

/// Максимальный возраст файлов логов (7 дней)
pub const MAX_LOG_AGE_SECS: u64 = 7 * 24 * 3600;
/// Максимальный размер отдельного файла лога (10 МБ)
pub const MAX_LOG_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
/// Максимальный суммарный объем всех логов (50 МБ)
pub const MAX_TOTAL_LOGS_SIZE_BYTES: u64 = 50 * 1024 * 1024;

/// Возвращает канонический путь к каталогу логов приложения (`~/.local/share/vrxx/logs/`)
pub fn get_log_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("vrxx")
        .join("logs")
}

/// Выполняет очистку каталога логов:
/// 1. Удаляет файлы старше `MAX_LOG_AGE_SECS` (7 дней).
/// 2. Если суммарный объем каталога превышает `MAX_TOTAL_LOGS_SIZE_BYTES` (50 МБ),
///    удаляет самые старые файлы до нормализации размера.
pub fn cleanup_old_logs(log_dir: &Path) {
    if !log_dir.exists() {
        return;
    }

    let entries = match fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(err) => {
            tracing::error!("[vrxx-logger] Failed to read log directory: {err}");
            return;
        }
    };

    let now = SystemTime::now();
    let mut log_files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    let mut total_size: u64 = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                let modified = meta.modified().unwrap_or(now);

                // Проверка возраста (7 дней)
                if let Ok(age) = now.duration_since(modified) {
                    if age > Duration::from_secs(MAX_LOG_AGE_SECS) {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                }

                total_size += size;
                log_files.push((path, modified, size));
            }
        }
    }

    // Если общий размер логов превышает лимит (50 МБ), удаляем самые старые файлы
    if total_size > MAX_TOTAL_LOGS_SIZE_BYTES {
        log_files.sort_by_key(|item| item.1); // Сортировка по времени модификации (старые первыми)
        for (path, _, size) in log_files {
            if total_size <= MAX_TOTAL_LOGS_SIZE_BYTES {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total_size = total_size.saturating_sub(size);
            }
        }
    }
}

/// Формирует строку директив EnvFilter на основе заданного уровня или переменной окружения `RUST_LOG`
pub fn build_filter(level_str: &str) -> EnvFilter {
    // Если пользователь явно задал RUST_LOG в терминале, уважаем его выбор
    if let Ok(env_val) = std::env::var("RUST_LOG") {
        if let Ok(filter) = EnvFilter::try_new(&env_val) {
            return filter;
        }
    }

    let normalized_level = match level_str.to_lowercase().as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "warning" | "warn" => "warn",
        "error" => "error",
        _ => "info",
    };

    // Промышленный стандарт: подробный уровень для модулей vrxx, умеренный для шумных внешних библиотек
    let directive = format!(
        "vrxx={},singbox={},{}=warn",
        normalized_level,
        normalized_level,
        if normalized_level == "trace" {
            "zbus=info"
        } else {
            "zbus"
        }
    );

    EnvFilter::try_new(&directive).unwrap_or_else(|_| EnvFilter::new(normalized_level))
}

/// Динамически обновляет фильтр логирования на лету без перезапуска программы.
/// Вызывается при переключении настройки `log_level` в графическом интерфейсе.
pub fn update_log_level(new_level: &str) {
    if let Some(handle) = RELOAD_HANDLE.get() {
        let filter = build_filter(new_level);
        match handle.reload(filter) {
            Ok(_) => {
                tracing::info!(
                    target: "vrxx::logger",
                    level = %new_level,
                    "Logging filter dynamically reloaded"
                );
            }
            Err(e) => {
                tracing::error!(
                    target: "vrxx::logger",
                    error = %e,
                    "Failed to dynamically reload logging filter"
                );
            }
        }
    }
}

/// Маскирует чувствительные данные (ключи, пароли, приватные UUID) перед выводом в лог.
/// Оставляет видимыми только первые 3 и последние 3 символа для верификации оператором.
pub fn mask_secret(secret: &str) -> String {
    let len = secret.len();
    if len == 0 {
        return String::new();
    }
    if len <= 8 {
        return "[REDACTED]".to_string();
    }
    let prefix = &secret[..3];
    let suffix = &secret[len - 3..];
    format!("{prefix}...{suffix}")
}

/// Параметры конфигурации логирования
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    pub is_tui: bool,
    pub is_daemon: bool,
    pub initial_level: String,
}

/// Инициализация логирования для GUI и TUI клиентов
pub fn init_gui_logging(config: LoggerConfig) -> WorkerGuard {
    let log_dir = get_log_directory();
    let _ = fs::create_dir_all(&log_dir);
    cleanup_old_logs(&log_dir);

    let log_prefix = if config.is_tui { "tui.log" } else { "app.log" };

    let rolling_appender = tracing_appender::rolling::daily(&log_dir, log_prefix);
    let (non_blocking, guard) = tracing_appender::non_blocking(rolling_appender);

    let initial_filter = build_filter(&config.initial_level);
    let (reload_layer, reload_handle) = reload::Layer::new(initial_filter);
    let _ = RELOAD_HANDLE.set(reload_handle);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(true);

    let ui_layer = ConsoleUiTracingLayer;

    if config.is_tui {
        let _ = tracing_subscriber::registry()
            .with(reload_layer)
            .with(file_layer)
            .with(ui_layer)
            .try_init();
    } else {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false);

        let _ = tracing_subscriber::registry()
            .with(reload_layer)
            .with(file_layer)
            .with(console_layer)
            .with(ui_layer)
            .try_init();
    }

    guard
}

/// Инициализация логирования для системного демона (с добавлением SSE-слоя и UI-слоя)
pub fn init_daemon_logging(
    config: LoggerConfig,
    sse_layer: crate::daemon::events::SseTracingLayer,
) -> WorkerGuard {
    let log_dir = get_log_directory();
    let _ = fs::create_dir_all(&log_dir);
    cleanup_old_logs(&log_dir);

    let rolling_appender = tracing_appender::rolling::daily(&log_dir, "daemon.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(rolling_appender);

    let initial_filter = build_filter(&config.initial_level);
    let (reload_layer, reload_handle) = reload::Layer::new(initial_filter);
    let _ = RELOAD_HANDLE.set(reload_handle);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(true);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(false);

    let ui_layer = ConsoleUiTracingLayer;

    let _ = tracing_subscriber::registry()
        .with(reload_layer)
        .with(file_layer)
        .with(console_layer)
        .with(sse_layer)
        .with(ui_layer)
        .try_init();

    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_log_broadcaster_and_history() {
        let broadcaster = UiLogBroadcaster::new(5);

        // Проверка пустой истории
        assert_eq!(broadcaster.get_history().len(), 0);

        // Добавление 3 записей
        for i in 1..=3 {
            broadcaster.broadcast(LogEntryItem {
                source: LogSource::App,
                level: "info".to_string(),
                message: format!("Log entry {i}"),
            });
        }

        let history = broadcaster.get_history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].message, "Log entry 1");
        assert_eq!(history[2].message, "Log entry 3");

        // Проверка кольцевого буфера (переполнение лимита емкости 5)
        for i in 4..=7 {
            broadcaster.broadcast(LogEntryItem {
                source: LogSource::App,
                level: "warn".to_string(),
                message: format!("Log entry {i}"),
            });
        }

        let history = broadcaster.get_history();
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].message, "Log entry 3");
        assert_eq!(history[4].message, "Log entry 7");

        // Очистка истории
        broadcaster.clear_history();
        assert_eq!(broadcaster.get_history().len(), 0);
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("12345"), "[REDACTED]");
        assert_eq!(mask_secret("12345678"), "[REDACTED]");
        assert_eq!(mask_secret("abcdefghij"), "abc...hij");
        assert_eq!(
            mask_secret("vless://e07d206f-1234-4567-89ab-cdef01234567@example.com"),
            "vle...com"
        );
    }

    #[test]
    fn test_build_filter() {
        let f_info = build_filter("info");
        assert!(f_info.to_string().contains("vrxx=info"));

        let f_debug = build_filter("debug");
        assert!(f_debug.to_string().contains("vrxx=debug"));

        let f_err = build_filter("error");
        assert!(f_err.to_string().contains("vrxx=error"));
    }

    #[tokio::test]
    async fn test_ui_log_broadcaster_subscribe() {
        let broadcaster = UiLogBroadcaster::new(10);
        let mut rx = broadcaster.subscribe();

        broadcaster.broadcast(LogEntryItem {
            source: LogSource::App,
            level: "info".to_string(),
            message: "Broadcaster async test".to_string(),
        });

        let received = rx.recv().await.expect("Failed to receive log entry");
        assert_eq!(received.message, "Broadcaster async test");
        assert_eq!(received.level, "info");
        assert_eq!(received.source, LogSource::App);
    }
}
