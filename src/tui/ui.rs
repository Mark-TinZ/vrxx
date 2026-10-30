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
    let (status_color, status_text) = if app.is_connected {
        (Color::Green, "CONNECTED")
    } else if app.status.starts_with("Error") {
        (Color::Red, "ERROR")
    } else {
        (Color::Yellow, "DISCONNECTED")
    };

    let mode_text = if app.tun_mode { "TUN" } else { "Proxy" };
    let server_text = app.active_server.as_deref().unwrap_or("None");

    let (down_spd, up_spd) = if app.is_connected {
        (app.current_down_speed, app.current_up_speed)
    } else {
        (0, 0)
    };
    let down_speed_str = format_dynamic_bandwidth(down_spd);
    let up_speed_str = format_dynamic_bandwidth(up_spd);

    let mut header_spans = vec![
        Span::raw(" Status: "),
        Span::styled(
            format!(" {} ", status_text),
            Style::default()
                .bg(status_color)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  Mode: "),
        Span::styled(
            format!(" {} ", mode_text),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  Active Server: "),
        Span::styled(
            server_text,
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    header_spans.push(Span::raw("  |  Speed: "));
    if app.is_connected {
        header_spans.push(Span::styled(
            format!("↓ {down_speed_str}  ↑ {up_speed_str}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        header_spans.push(Span::styled(
            "0 B/s",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let header = Paragraph::new(Line::from(header_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" VRXX TUI Client ")
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
        format!(" Traffic IN: {down_speed_str} (Total: {down_total_str}) ")
    } else {
        format!(" Traffic IN: {down_speed_str} ")
    };

    let up_title = if app.total_up > 0 {
        format!(" Traffic OUT: {up_speed_str} (Total: {up_total_str}) ")
    } else {
        format!(" Traffic OUT: {up_speed_str} ")
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
                .border_style(Style::default().fg(Color::Green)),
        )
        .data(down_data)
        .style(Style::default().fg(Color::Green));

    let up_sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(up_title)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .data(up_data)
        .style(Style::default().fg(Color::Blue));

    f.render_widget(down_sparkline, traffic_chunks[0]);
    f.render_widget(up_sparkline, traffic_chunks[1]);

    // 2. Список профилей (Keys)
    let items: Vec<ListItem> = app
        .settings
        .keys
        .iter()
        .enumerate()
        .map(|(idx, key)| {
            let prefix = if app.active_server.as_deref() == Some(&key.name) {
                "● "
            } else {
                "  "
            };

            let line = format!("{}{} [{}]", prefix, key.name, key.protocol.to_uppercase());
            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list_title = format!(" Profiles ({}) ", app.settings.keys.len());
    let list_widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(list_title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.settings.keys.is_empty() {
        state.select(Some(app.selected_index));
    }

    f.render_stateful_widget(list_widget, main_chunks[1], &mut state);
}

/// Отрисовка нижней панели подсказок горячих клавиш.
fn draw_footer(f: &mut Frame, _app: &App, area: Rect) {
    let footer_text = vec![
        Span::styled(
            " [Space] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Connect/Disconnect  "),
        Span::styled(
            " [Tab] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Toggle TUN/Proxy  "),
        Span::styled(
            " [↑/↓] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Navigate  "),
        Span::styled(
            " [L] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Logs  "),
        Span::styled(
            " [Q] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Quit"),
    ];

    let footer = Paragraph::new(Line::from(footer_text))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" Actions "));

    f.render_widget(footer, area);
}

/// Отрисовка всплывающего окна логов (Modal Overlay).
fn draw_logs_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(80, 70, area);

    // Очищаем область под всплывающим окном
    f.render_widget(Clear, popup_area);

    let logs_text: Vec<Line> = app.logs.iter().map(|l| Line::from(l.as_str())).collect();

    let logs_widget = Paragraph::new(logs_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Daemon Event Logs (Press [L] or [Esc] to Close) ")
                .border_style(Style::default().fg(Color::Magenta)),
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
}
