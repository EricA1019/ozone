use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Screen};

const FRONTEND_CHOICE_MIN_INDEX: usize = 0;
const FRONTEND_CHOICE_MAX_INDEX: usize = 1;

pub(super) fn handle_frontend_choice_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Confirm,
        KeyCode::Up if app.frontend_choice_index > FRONTEND_CHOICE_MIN_INDEX => {
            app.frontend_choice_index -= 1;
        }
        KeyCode::Down if app.frontend_choice_index < FRONTEND_CHOICE_MAX_INDEX => {
            app.frontend_choice_index += 1;
        }
        KeyCode::Enter if app.current_plan.is_some() => {
            app.pending_launch_choice = Some(app.frontend_choice_index);
        }
        _ => {}
    }
}
