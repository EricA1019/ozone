use crossterm::event::{KeyCode, KeyEvent};

use super::settings_flow::open_exit_confirm;
use super::{launcher, run_launcher_action, App, LauncherActionId, LauncherActionOutcome};

pub(super) async fn handle_launcher_screen_key(
    app: &mut App,
    key: KeyEvent,
    last_refresh: &mut std::time::Instant,
) -> LauncherActionOutcome {
    match key.code {
        KeyCode::Char('q') => LauncherActionOutcome::Exit,
        KeyCode::Esc => {
            open_exit_confirm(app);
            LauncherActionOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') if app.selected_action > 0 => {
            app.selected_action -= 1;
            LauncherActionOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let count = launcher::visible_launcher_actions(app).len();
            if app.selected_action < count - 1 {
                app.selected_action += 1;
            }
            LauncherActionOutcome::Continue
        }
        KeyCode::Enter => {
            let actions = launcher::visible_launcher_actions(app);
            let action = actions
                .get(app.selected_action)
                .map(|action| action.id)
                .unwrap_or(LauncherActionId::Launch);
            run_launcher_action(app, action, last_refresh).await
        }
        _ => LauncherActionOutcome::Continue,
    }
}
