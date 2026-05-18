use crossterm::event::{KeyCode, KeyEvent};

use super::settings_flow::sync_settings_from_prefs;
use super::{App, BackendMode, FrontendMode, Screen};

const SETTINGS_SECTION_COUNT: usize = 2;
const SETTINGS_BACKEND_MAX_INDEX: usize = 2;
const SETTINGS_FRONTEND_MAX_INDEX: usize = 1;

pub(super) async fn handle_settings_key(
    app: &mut App,
    key: KeyEvent,
    preferred_frontend: Option<FrontendMode>,
) {
    match key.code {
        KeyCode::Tab | KeyCode::Right => {
            app.settings_section = (app.settings_section + 1) % SETTINGS_SECTION_COUNT;
        }
        KeyCode::BackTab | KeyCode::Left => {
            app.settings_section = if app.settings_section == 0 { 1 } else { 0 };
        }
        KeyCode::Up => match app.settings_section {
            0 => {
                if app.settings_backend_index > 0 {
                    app.settings_backend_index -= 1;
                }
            }
            _ => {
                if app.settings_frontend_index > 0 {
                    app.settings_frontend_index -= 1;
                }
            }
        },
        KeyCode::Down => match app.settings_section {
            0 => {
                if app.settings_backend_index < SETTINGS_BACKEND_MAX_INDEX {
                    app.settings_backend_index += 1;
                }
            }
            _ => {
                if app.settings_frontend_index < SETTINGS_FRONTEND_MAX_INDEX {
                    app.settings_frontend_index += 1;
                }
            }
        },
        KeyCode::Enter => {
            app.prefs.preferred_backend = match app.settings_backend_index {
                0 => Some(BackendMode::KoboldCpp),
                1 => Some(BackendMode::LlamaCpp),
                2 => Some(BackendMode::Ollama),
                _ => None,
            };
            app.prefs.preferred_frontend = match app.settings_frontend_index {
                0 => Some(FrontendMode::SillyTavern),
                1 => Some(FrontendMode::OzonePlus),
                _ => None,
            };
            let _ = crate::prefs::save_prefs(&app.prefs).await;
            app.preferred_frontend = preferred_frontend.or(app.prefs.preferred_frontend);
            app.set_status("Launcher defaults saved.".into());
            app.screen = Screen::Launcher;
        }
        KeyCode::Esc => {
            sync_settings_from_prefs(app);
            app.set_status("Settings changes discarded.".into());
            app.screen = Screen::Launcher;
        }
        _ => {}
    }
}
