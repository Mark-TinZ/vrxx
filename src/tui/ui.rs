/* ui.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Отрисовка компонентов терминального интерфейса (TUI Renderer)
//!
//! Модуль отвечает за:
//! - Разметку терминального экрана (Header, Графики трафика, Список профилей, Панель действий)
//! - Отрисовку цветовых индикаторов статуса подключения (CONNECTED, DISCONNECTED, ERROR)
//! - Графическое отображение входящего/исходящего трафика через Sparkline
//! - Модальное всплывающее окно просмотра системных логов демона

use super::app::{App, ViewMode};
use gettextrs::gettext;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Sparkline, Wrap},
    Frame,
};

/// Динамическое форматирование скорости передачи данных (B/s, KB/s, MB/s, GB/s).
pub fn format_dynamic_bandwidth(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_073_741_824 {
        format!("{:.1} GB/s", bytes_per_sec as f64 / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_048_576.0)
    } else if bytes_per_sec >= 1_024 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1_024.0)
    } else {
        format!("{bytes_per_sec} B/s")
    }
}

/// Динамическое форматирование объема данных (B, KB, MB, GB, TB).
pub fn format_dynamic_bytes(bytes: u64) -> String {
    if bytes >= 1_099_511_627_776 {
        format!("{:.2} TB", bytes as f64 / 1_099_511_627_776.0)
    } else if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Главная функция рендеринга TUI интерфейса.
pub fn draw_ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Верхняя панель (Header)
            Constraint::Min(8),    // Центр (График трафика + Список профилей)
            Constraint::Length(3), // Нижняя панель (Footer / Hotkeys)
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_center(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    if app.view_mode == ViewMode::Logs {
        draw_logs_modal(f, app, f.area());
    }
}

/// Отрисовка верхней панели статуса.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let (status_color, status_text) = if app.status == "Connecting" {
        (Color::Yellow, gettext("Connecting..."))
    } else if app.status == "Disconnecting" {
        (Color::Yellow, gettext("Disconnecting..."))
    } else if app.status == "Error (Daemon Offline)" {
        (Color::Red, gettext("Error: Daemon Offline"))
    } else if app.status == "Error (Invalid Key)" {
        (Color::Red, gettext("Error: Invalid Key"))
    } else if app.status == "Error (Server/Auth Failed)" {
        (Color::Red, gettext("Error: Server/Auth Failed"))
    } else if app.status.starts_with("Error") {
        (Color::Red, gettext("Connection error"))
    } else if app.is_connected {
        (Color::Green, gettext("Connected"))
    } else {
        (Color::DarkGray, gettext("Disconnected"))
    };

    let mode_text = if app.tun_mode {
        "TUN".to_string()
    } else {
        gettext("Proxy")
    };
    let server_text = app
        .active_server
        .as_deref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| gettext("None"));

    let (down_spd, up_spd) = if app.is_connected {
        (app.current_down_speed, app.current_up_speed)
    } else {
        (0, 0)
    };
    let down_speed_str = format_dynamic_bandwidth(down_spd);
    let up_speed_str = format_dynamic_bandwidth(up_spd);

    let mut header_spans = vec![
        Span::styled(
            format!(" {}: ", gettext("Status")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!("● {status_text}"),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}: ", gettext("Mode")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            mode_text,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}: ", gettext("Active Server")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            server_text,
            Style::default()
                .fg(if app.active_server.is_some() {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}: ", gettext("Speed")),
            Style::default().fg(Color::Gray),
        ),
    ];

    if app.is_connected {
        header_spans.push(Span::styled(
            format!("↓ {down_speed_str}  ↑ {up_speed_str}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        header_spans.push(Span::styled("0 B/s", Style::default().fg(Color::DarkGray)));
    }

    let header = Paragraph::new(Line::from(header_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" {} ", gettext("VRXX TUI Client")))
            .title_alignment(Alignment::Left),
    );

    f.render_widget(header, area);
}

/// Отрисовка центральной части (спектрограммы скорости и списка профилей).
fn draw_center(f: &mut Frame, app: &mut App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Графики трафика
            Constraint::Min(4),    // Список профилей
        ])
        .split(area);

    // 1. График трафика (Download / Upload)
    let traffic_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    let (down_spd, up_spd) = if app.is_connected {
        (app.current_down_speed, app.current_up_speed)
    } else {
        (0, 0)
    };

    let down_speed_str = format_dynamic_bandwidth(down_spd);
    let up_speed_str = format_dynamic_bandwidth(up_spd);
    let down_total_str = format_dynamic_bytes(app.total_down);
    let up_total_str = format_dynamic_bytes(app.total_up);

    let down_title = if app.total_down > 0 {
        format!(
            " {}: {} ({}: {}) ",
            gettext("Download"),
            down_speed_str,
            gettext("Total"),
            down_total_str
        )
    } else {
        format!(" {}: {} ", gettext("Download"), down_speed_str)
    };

    let up_title = if app.total_up > 0 {
        format!(
            " {}: {} ({}: {}) ",
            gettext("Upload"),
            up_speed_str,
            gettext("Total"),
            up_total_str
        )
    } else {
        format!(" {}: {} ", gettext("Upload"), up_speed_str)
    };

    let inner_width = traffic_chunks[0].width.saturating_sub(2) as usize;
    let down_start = app.download_history.len().saturating_sub(inner_width);
    let down_data = &app.download_history[down_start..];

    let up_start = app.upload_history.len().saturating_sub(inner_width);
    let up_data = &app.upload_history[up_start..];

    let down_sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(down_title)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(down_data)
        .style(Style::default().fg(Color::Green));

    let up_sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(up_title)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(up_data)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(down_sparkline, traffic_chunks[0]);
    f.render_widget(up_sparkline, traffic_chunks[1]);

    // 2. Список профилей (Keys)
    let items: Vec<ListItem> = if app.settings.keys.is_empty() {
        vec![ListItem::new(format!(
            "  {}",
            gettext("No profiles available. Add a profile in GUI or configuration.")
        ))
        .style(Style::default().fg(Color::DarkGray))]
    } else {
        app.settings
            .keys
            .iter()
            .enumerate()
            .map(|(idx, key)| {
                let is_active = app.active_server.as_deref() == Some(&key.name);
                let is_selected = idx == app.selected_index;

                let mut spans = Vec::new();
                if is_active {
                    spans.push(Span::styled(
                        "● ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::raw("  "));
                }

                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                spans.push(Span::styled(key.name.clone(), name_style));
                spans.push(Span::styled(
                    format!(" [{}]", key.protocol.to_uppercase()),
                    Style::default().fg(Color::DarkGray),
                ));

                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    let list_title = format!(" {} ({}) ", gettext("Profiles"), app.settings.keys.len());
    let list_widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(list_title)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.settings.keys.is_empty() {
        state.select(Some(app.selected_index));
    }

    f.render_stateful_widget(list_widget, main_chunks[1], &mut state);
}

/// Отрисовка нижней панели подсказок горячих клавиш.
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let connect_action = if app.is_connected {
        gettext("Disconnect")
    } else {
        gettext("Connect")
    };

    let footer_text = vec![
        Span::styled(
            " [Space] ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{connect_action}  "),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            " [Tab] ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", gettext("Toggle Mode")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            " [↑/↓] ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", gettext("Navigate")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            " [L] ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", gettext("Logs")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            " [Q/Esc] ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(gettext("Quit"), Style::default().fg(Color::Gray)),
    ];

    let footer = Paragraph::new(Line::from(footer_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(format!(" {} ", gettext("Actions"))),
        );

    f.render_widget(footer, area);
}

/// Отрисовка всплывающего окна логов (Modal Overlay).
fn draw_logs_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(80, 70, area);

    // Очищаем область под всплывающим окном
    f.render_widget(Clear, popup_area);

    let logs_text: Vec<Line> = if app.logs.is_empty() {
        vec![Line::from(Span::styled(
            gettext("No log entries yet."),
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.logs.iter().map(|l| Line::from(l.as_str())).collect()
    };

    let logs_widget = Paragraph::new(logs_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " {} ",
                    gettext("Daemon Event Logs (Press [L] or [Esc] to Close)")
                ))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(logs_widget, popup_area);
}

/// Вспомогательная функция для центрирования прямоугольника.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_dynamic_bandwidth_zero() {
        assert_eq!(format_dynamic_bandwidth(0), "0 B/s");
    }

    #[test]
    fn test_format_dynamic_bandwidth_bytes() {
        assert_eq!(format_dynamic_bandwidth(500), "500 B/s");
        assert_eq!(format_dynamic_bandwidth(1023), "1023 B/s");
    }

    #[test]
    fn test_format_dynamic_bandwidth_kilobytes() {
        assert_eq!(format_dynamic_bandwidth(1024), "1.0 KB/s");
        assert_eq!(format_dynamic_bandwidth(153_600), "150.0 KB/s");
    }

    #[test]
    fn test_format_dynamic_bandwidth_megabytes() {
        assert_eq!(format_dynamic_bandwidth(1_048_576), "1.0 MB/s");
        assert_eq!(format_dynamic_bandwidth(10_485_760), "10.0 MB/s");
    }

    #[test]
    fn test_format_dynamic_bandwidth_gigabytes() {
        assert_eq!(format_dynamic_bandwidth(1_073_741_824), "1.0 GB/s");
        assert_eq!(format_dynamic_bandwidth(2_684_354_560), "2.5 GB/s");
    }

    #[test]
    fn test_format_dynamic_bandwidth_overflow_resilience() {
        let extreme = u64::MAX;
        let formatted = format_dynamic_bandwidth(extreme);
        assert!(formatted.ends_with("GB/s"));
        assert!(!formatted.contains("NaN"));
    }

    #[test]
    fn test_format_dynamic_bytes() {
        assert_eq!(format_dynamic_bytes(0), "0 B");
        assert_eq!(format_dynamic_bytes(512), "512 B");
        assert_eq!(format_dynamic_bytes(1024), "1.0 KB");
        assert_eq!(format_dynamic_bytes(52_428_800), "50.0 MB");
        assert_eq!(format_dynamic_bytes(1_073_741_824), "1.00 GB");
        assert_eq!(format_dynamic_bytes(1_099_511_627_776), "1.00 TB");
    }

    #[test]
    fn test_tui_localization_roundtrip() {
        // 1. Проверяем русский язык
        crate::config::init_i18n("ru");

        assert_eq!(gettext("VRXX TUI Client"), "VRXX TUI Клиент");
        assert_eq!(gettext("Status"), "Статус");
        assert_eq!(gettext("Connected"), "Подключено");
        assert_eq!(gettext("Connecting..."), "Подключение...");
        assert_eq!(gettext("Disconnecting..."), "Отключение...");
        assert_eq!(gettext("Disconnected"), "Отключено");
        assert_eq!(gettext("Error: Daemon Offline"), "Ошибка: демон оффлайн");
        assert_eq!(
            gettext("Error: Invalid Key"),
            "Ошибка: недействительный ключ"
        );
        assert_eq!(
            gettext("Error: Server/Auth Failed"),
            "Ошибка: сбой сервера/аутентификации"
        );
        assert_eq!(gettext("Connection error"), "Ошибка соединения");
        assert_eq!(gettext("Mode"), "Режим");
        assert_eq!(gettext("Proxy"), "Прокси");
        assert_eq!(gettext("Active Server"), "Активный сервер");
        assert_eq!(gettext("None"), "Нет");
        assert_eq!(gettext("Speed"), "Скорость");
        assert_eq!(gettext("Download"), "Входящий");
        assert_eq!(gettext("Upload"), "Исходящий");
        assert_eq!(gettext("Total"), "Всего");
        assert_eq!(gettext("Profiles"), "Профили");
        assert_eq!(gettext("Connect"), "Подключить");
        assert_eq!(gettext("Disconnect"), "Отключить");
        assert_eq!(gettext("Toggle Mode"), "Сменить режим");
        assert_eq!(gettext("Navigate"), "Навигация");
        assert_eq!(gettext("Logs"), "Логи");
        assert_eq!(gettext("Quit"), "Выход");
        assert_eq!(
            gettext("Daemon Event Logs (Press [L] or [Esc] to Close)"),
            "Логи событий демона (Нажмите [L] или [Esc] для закрытия)"
        );
        assert_eq!(gettext("No log entries yet."), "Записей в логе пока нет.");
        assert_eq!(
            gettext("Proxy stopped via TUI"),
            "Прокси остановлен через TUI"
        );
        assert_eq!(
            gettext("Failed to toggle connection"),
            "Ошибка переключения подключения"
        );
        assert_eq!(
            gettext("Failed to toggle mode"),
            "Ошибка переключения режима"
        );

        // 2. Проверяем возврат к английскому языку
        crate::config::init_i18n("en");

        assert_eq!(gettext("VRXX TUI Client"), "VRXX TUI Client");
        assert_eq!(gettext("Status"), "Status");
        assert_eq!(gettext("Connected"), "Connected");
        assert_eq!(gettext("Connecting..."), "Connecting...");
        assert_eq!(gettext("Disconnecting..."), "Disconnecting...");
        assert_eq!(gettext("Disconnected"), "Disconnected");
        assert_eq!(gettext("Error: Daemon Offline"), "Error: Daemon Offline");
        assert_eq!(gettext("Error: Invalid Key"), "Error: Invalid Key");
        assert_eq!(
            gettext("Error: Server/Auth Failed"),
            "Error: Server/Auth Failed"
        );
        assert_eq!(gettext("Connection error"), "Connection error");
        assert_eq!(gettext("Proxy"), "Proxy");
        assert_eq!(gettext("Download"), "Download");
        assert_eq!(gettext("Upload"), "Upload");
        assert_eq!(gettext("Quit"), "Quit");
    }

    #[test]
    fn test_tui_draw_ui_renders_without_panic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = App::new();

        // Проверяем отрисовку при различных размерах терминала
        for (width, height) in [(120, 40), (80, 24), (40, 15), (20, 8), (10, 5)] {
            let backend = TestBackend::new(width, height);
            let mut terminal =
                Terminal::new(backend).expect("Failed to create TestBackend terminal");

            terminal
                .draw(|f| draw_ui(f, &mut app))
                .expect("draw_ui must not panic in Main mode");

            // Проверяем режим логов
            app.view_mode = ViewMode::Logs;
            app.logs.push("[INFO] Test log line".to_string());
            terminal
                .draw(|f| draw_ui(f, &mut app))
                .expect("draw_ui must not panic in Logs mode");
            app.view_mode = ViewMode::Main;
        }

        // Проверяем отрисовку с активными ключами и подключениями
        app.settings.keys.push(crate::settings::VpnKeyData {
            name: "TestServer".to_string(),
            protocol: "vless".to_string(),
            is_active: true,
            traffic_down: "0 MB".to_string(),
            traffic_up: "0 MB".to_string(),
            time_connected: "00:00:00".to_string(),
            ping: "25 ms".to_string(),
            location: "DE".to_string(),
            country_code: "DE".to_string(),
            city: "Frankfurt".to_string(),
            isp: "Test ISP".to_string(),
            timezone: "UTC".to_string(),
            url: "vless://test".to_string(),
            group: "Default".to_string(),
        });
        app.is_connected = true;
        app.status = "Connected".to_string();
        app.active_server = Some("TestServer".to_string());
        app.apply_traffic(1024, 2048, 5000, 10000);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("Failed to create TestBackend terminal");
        terminal
            .draw(|f| draw_ui(f, &mut app))
            .expect("draw_ui must not panic with populated state");
    }
}
