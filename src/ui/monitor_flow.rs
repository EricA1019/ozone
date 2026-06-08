use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Screen};

pub(super) enum MonitorOutcome {
    Continue,
    ExitLauncher,
}

pub(super) async fn handle_monitor_key(app: &mut App, key: KeyEvent) -> MonitorOutcome {
    match key.code {
        KeyCode::Char('q') => return MonitorOutcome::ExitLauncher,
        KeyCode::Esc | KeyCode::Char('r') => {
            app.screen = Screen::Launcher;
        }
        KeyCode::Char('s') => {
            let _ = crate::processes::clear_gpu_backends().await;
            app.services = crate::processes::get_service_status().await;
            app.set_status("GPU backends cleared.".into());
            app.screen = Screen::Launcher;
        }
        _ => {}
    }

    MonitorOutcome::Continue
}