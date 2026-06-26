use crossterm::event::{KeyCode, KeyEvent};

use super::settings_flow::sync_settings_from_prefs;
use super::{App, BackendMode, Screen};

pub(super) async fn handle_settings_key(app: &mut App, key: KeyEvent) {
    // If editing a text field, handle input directly
    if app.settings_editing {
        match key.code {
            KeyCode::Enter => {
                app.settings_editing = false;
                let input = app.settings_input_buffer.trim().to_string();
                app.prefs.models_dir = if input.is_empty() { None } else { Some(input) };
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                if let Some(ref dir) = app.prefs.models_dir {
                    ozone_core::paths::set_models_dir_override(std::path::Path::new(dir));
                }
                app.set_status("Model directory saved.".into());
                app.screen = Screen::Launcher;
            }
            KeyCode::Esc => {
                app.settings_editing = false;
                app.settings_input_buffer.clear();
                sync_settings_from_prefs(app);
                app.set_status("Model directory edit discarded.".into());
                app.screen = Screen::Launcher;
            }
            KeyCode::Backspace => {
                app.settings_input_buffer.pop();
            }
            KeyCode::Char(c) => {
                if c.is_ascii() {
                    app.settings_input_buffer.push(c);
                }
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::BackTab | KeyCode::Left => {
            app.settings_section = 0;
        }
        KeyCode::Up => {
            if app.settings_section > 0 {
                app.settings_section -= 1;
            }
        }
        KeyCode::Down => {
            if app.settings_section < 1 {
                app.settings_section += 1;
            }
        }
        KeyCode::Enter => {
            if app.settings_section == 0 {
                // Save backend selection
                app.settings_backend_index = 0;
                app.prefs.preferred_backend = Some(BackendMode::LlamaCpp);
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.set_status("Launcher defaults saved.".into());
                app.screen = Screen::Launcher;
            } else {
                // Enter model directory editing
                app.settings_editing = true;
                app.settings_input_buffer = app.prefs.models_dir.clone().unwrap_or_else(|| {
                    ozone_core::paths::models_dir()
                        .to_string_lossy()
                        .to_string()
                });
            }
        }
        KeyCode::Esc => {
            sync_settings_from_prefs(app);
            app.set_status("Settings changes discarded.".into());
            app.screen = Screen::Launcher;
        }
        _ => {}
    }
}
