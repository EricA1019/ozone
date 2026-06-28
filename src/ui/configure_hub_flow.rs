use crossterm::event::{KeyCode, KeyEvent};

use super::configure_plan_flow::{adjust_configure_plan, reset_configure_plan};
#[cfg(feature = "profiling-ui")]
use super::configure_profile_flow::selected_saved_profile;
use super::configure_profile_flow::{
    apply_selected_saved_profile, build_override_from_plans, cycle_saved_profile,
    delete_selected_saved_profile, save_current_plan_as_profile, set_selected_profile_default,
    update_selected_profile_from_current_plan,
};
use super::{App, Screen};
#[cfg(feature = "profiling-ui")]
use crate::profiling::ProfilingAction;

pub(super) async fn handle_configure_hub_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.current_plan = None;
            app.configure_recommended_plan = None;
            app.configure_saved_profiles.clear();
            app.configure_profile_index = 0;
            #[cfg(feature = "profiling-ui")]
            app.configure_profile_reports.clear();
            app.screen = Screen::ModelPicker;
        }
        KeyCode::Up | KeyCode::Char('k') if app.configure_field_index > 0 => {
            app.configure_field_index -= 1;
        }
        KeyCode::Down | KeyCode::Char('j') if app.configure_field_index < 5 => {
            app.configure_field_index += 1;
        }
        KeyCode::Left => adjust_configure_plan(app, -1),
        KeyCode::Right => adjust_configure_plan(app, 1),
        KeyCode::Char('p') | KeyCode::Char('P') => cycle_saved_profile(app, -1),
        KeyCode::Char('n') | KeyCode::Char('N') => cycle_saved_profile(app, 1),
        KeyCode::Char('l') | KeyCode::Char('L') => {
            if let Some(profile_name) = apply_selected_saved_profile(app) {
                app.set_status(format!(
                    "Loaded saved profile '{profile_name}' into Configure Hub."
                ));
            } else {
                app.set_error("No saved profile is selected.".into());
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            if let Some(profile_name) = save_current_plan_as_profile(app) {
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.set_status(format!("Saved profile '{profile_name}'."));
            } else {
                app.set_error("No launch plan is available to save.".into());
            }
        }
        KeyCode::Char('u') | KeyCode::Char('U') => {
            if let Some(profile_name) = update_selected_profile_from_current_plan(app) {
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.set_status(format!("Updated saved profile '{profile_name}'."));
            } else {
                app.set_error("Select a saved profile before updating it.".into());
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if let Some(profile_name) = delete_selected_saved_profile(app) {
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.set_status(format!("Deleted saved profile '{profile_name}'."));
            } else {
                app.set_error("Select a saved profile before deleting it.".into());
            }
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            if let Some(profile_name) = set_selected_profile_default(app) {
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.set_status(format!("Default launch profile set to '{profile_name}'."));
            } else {
                app.set_error("Select a saved profile before marking it default.".into());
            }
        }
        #[cfg(feature = "profiling-ui")]
        KeyCode::Char('b') | KeyCode::Char('B') => {
            if selected_saved_profile(app).is_some() {
                app.profiling_pending_action = Some(ProfilingAction::BenchmarkSavedProfile);
                app.screen = Screen::ProfileConfirm;
            } else {
                // No saved profile? Start a quick sweep against the current model immediately.
                start_quick_sweep(app);
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => reset_configure_plan(app),
        KeyCode::Char(ch) if ch.is_ascii_digit() && ch != '0' => {
            // Jump directly to a saved profile by number (1-9)
            let idx = (ch as u8 - b'1') as usize;
            if idx < app.configure_saved_profiles.len() {
                app.configure_profile_index = idx;
                if let Some(profile_name) = apply_selected_saved_profile(app) {
                    app.set_status(format!("Loaded saved profile '{profile_name}'."));
                }
            } else {
                app.set_error(format!(
                    "Profile #{ch} does not exist (have {}).",
                    app.configure_saved_profiles.len()
                ));
            }
        }
        KeyCode::Enter => {
            if let (Some(recommended), Some(effective)) = (
                app.configure_recommended_plan.clone(),
                app.current_plan.clone(),
            ) {
                let override_state = build_override_from_plans(&recommended, &effective);
                app.prefs
                    .set_model_launch_override(effective.model_name.clone(), override_state);
                let _ = crate::prefs::save_prefs(&app.prefs).await;
                app.screen = Screen::Confirm;
            }
        }
        _ => {}
    }
}

#[cfg(feature = "profiling-ui")]
pub(super) fn start_quick_sweep(app: &mut App) {
    // Use the same pattern as handle_profile_confirm_key: get model from catalog
    let Some(record) = app
        .filtered_catalog_get(app.selected_model)
        .map(|r| r.clone())
    else {
        app.set_error("No model selected. Open the launcher and pick a model first.".into());
        return;
    };

    let model_name = record.model_name.clone();
    let request = crate::profiling::WorkflowRequest {
        record,
        hardware: app.hardware.clone().unwrap_or_default(),
        action: crate::profiling::ProfilingAction::QuickSweep,
        profiling_backend: crate::profiling::ProfilingBackend::LlamaCpp,
        launch_plan_override: app.current_plan.clone(),
        launch_profile_name: None,
    };

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();
    app.start_profile_workflow(rx, cancel);
    let _handle = tokio::spawn(async move {
        crate::profiling::run_workflow(request, tx, cancel_clone).await;
    });
    app.set_status(format!("Quick sweep started for '{}'...", model_name));
}
