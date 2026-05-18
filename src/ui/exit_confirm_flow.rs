use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Screen};

pub(super) enum ExitConfirmOutcome {
    Continue,
    ExitLauncher,
}

pub(super) fn handle_exit_confirm_key(app: &mut App, key: KeyEvent) -> ExitConfirmOutcome {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => app.screen = Screen::Launcher,
        KeyCode::Left | KeyCode::Up if app.exit_confirm_index > 0 => {
            app.exit_confirm_index -= 1;
        }
        KeyCode::Right | KeyCode::Down if app.exit_confirm_index < 1 => {
            app.exit_confirm_index += 1;
        }
        KeyCode::Enter | KeyCode::Char('y') => {
            if app.exit_confirm_index == 0 {
                return ExitConfirmOutcome::ExitLauncher;
            }
            app.screen = Screen::Launcher;
        }
        _ => {}
    }

    ExitConfirmOutcome::Continue
}
