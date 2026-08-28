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

    let header_spans = vec![
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

    let down_sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Traffic IN (Download) ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .data(&app.download_history)
        .style(Style::default().fg(Color::Green));

    let up_sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Traffic OUT (Upload) ")
                .border_style(Style::default().fg(Color::Blue)),
        )
        .data(&app.upload_history)
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
