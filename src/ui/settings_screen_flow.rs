use crossterm::event::{KeyCode, KeyEvent};

use super::settings_flow::sync_settings_from_prefs;
use super::{App, BackendMode, Screen};

pub(super) async fn handle_settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::BackTab | KeyCode::Left => {
            app.settings_section = 0;
        }
        KeyCode::Up | KeyCode::Down => {}
        KeyCode::Enter => {
            app.settings_backend_index = 0;
            app.prefs.preferred_backend = Some(BackendMode::LlamaCpp);
            let _ = crate::prefs::save_prefs(&app.prefs).await;
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
