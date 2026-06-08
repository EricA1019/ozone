use super::{App, BackendMode, Screen};

pub(super) fn sync_settings_from_prefs(app: &mut App) {
    app.settings_section = 0;
    app.settings_backend_index = 0;
    app.prefs.preferred_backend = Some(BackendMode::LlamaCpp);
}

pub(super) fn open_settings(app: &mut App) {
    sync_settings_from_prefs(app);
    app.screen = Screen::Settings;
}

pub(super) fn open_exit_confirm(app: &mut App) {
    app.exit_confirm_index = 1;
    app.screen = Screen::ExitConfirm;
}

pub(super) fn back_from_confirm(app: &App) -> Screen {
    if app.configure_recommended_plan.is_some() {
        return Screen::ConfigureHub;
    }
    #[cfg(feature = "profiling-ui")]
    {
        if app.profiling_success.is_some() {
            return Screen::ProfileSuccess;
        } else if app.profiling_failure.is_some() {
            return Screen::ProfileFailure;
        } else if app.profiling_advisory.is_some() {
            return Screen::ProfileAdvisory;
        }
    }
    Screen::ModelPicker
}
