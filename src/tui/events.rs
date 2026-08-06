use super::app::{App, ViewMode};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

/// Обработка пользовательского ввода с таймаутом (неблокирующая)
pub async fn handle_events(app: &mut App) -> Result<()> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.view_mode {
                    ViewMode::Main => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.previous_profile();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.next_profile();
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            if let Err(e) = app.toggle_connect().await {
                                app.push_log(format!("[ERROR] Toggle connect failed: {e}"));
                            }
                        }
                        KeyCode::Tab => {
                            if let Err(e) = app.toggle_mode().await {
                                app.push_log(format!("[ERROR] Toggle mode failed: {e}"));
                            }
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            app.toggle_logs_view();
                        }
                        _ => {}
                    },
                    ViewMode::Logs => match key.code {
                        KeyCode::Char('l')
                        | KeyCode::Char('L')
                        | KeyCode::Esc
                        | KeyCode::Char('q')
                        | KeyCode::Char('Q') => {
                            app.toggle_logs_view();
                        }
                        _ => {}
                    },
                }
            }
        }
    }
    Ok(())
}
