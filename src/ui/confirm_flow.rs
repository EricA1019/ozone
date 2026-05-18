use crossterm::event::{KeyCode, KeyEvent};

use super::settings_flow::back_from_confirm;
use super::{queue_frontend_launch, App};

pub(super) fn handle_confirm_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => app.screen = back_from_confirm(app),
        KeyCode::Enter | KeyCode::Char('y') if app.current_plan.is_some() => {
            queue_frontend_launch(app);
        }
        _ => {}
    }
}
