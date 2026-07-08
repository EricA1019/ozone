use crossterm::event::{KeyCode, KeyEvent};

use crate::profiling::{self, ProfilingAction, WorkflowEvent};

use super::{configure_profile_flow::refresh_configure_profiles, App, Screen};

pub(super) enum ProfilingResultOutcome {
    Continue,
    RestartLoop,
}

pub(super) fn handle_profile_running_key(app: &mut App, key: KeyEvent) {
    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
        if let Some(token) = &app.profiling.cancel {
            token.cancel();
            app.push_profile_progress("⏳ Cancelling…".into());
        }
    }
}

pub(super) fn handle_profile_success_key(app: &mut App, key: KeyEvent) -> ProfilingResultOutcome {
    match key.code {
        KeyCode::Esc => {
            if matches!(
                app.profiling.success.as_ref().map(|report| &report.action),
                Some(ProfilingAction::BenchmarkSavedProfile)
            ) && app.configure_recommended_plan.is_some()
            {
                if let Some(plan) = app.current_plan.as_ref() {
                    let model_name = plan.model_name.clone();
                    refresh_configure_profiles(app, &model_name);
                }
                app.clear_profile_success_and_open_configure_hub();
                return ProfilingResultOutcome::RestartLoop;
            }
            return return_to_profile_advisory_or_launcher(app);
        }
        KeyCode::Char('q') => {
            app.reset_profile_and_open_launcher();
        }
        KeyCode::Up if app.profiling.choice_index > 0 => {
            app.profiling.choice_index -= 1;
        }
        KeyCode::Down => {
            let count = app
                .profiling.success
                .as_ref()
                .map(|report| report.available_actions().len())
                .unwrap_or(0);
            if app.profiling.choice_index + 1 < count {
                app.profiling.choice_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(report) = &app.profiling.success {
                let actions = report.available_actions();
                if let Some(action) = actions.get(app.profiling.choice_index).cloned() {
                    match action {
                        ProfilingAction::LaunchRecommended => {
                            if let (Some(record), Some(hw)) = (
                                app.filtered_catalog_get(app.selected_model),
                                app.hardware.as_ref(),
                            ) {
                                match profiling::preferred_launch_plan(&record, hw) {
                                    Ok(plan) => {
                                        app.open_confirm_with_plan(plan);
                                    }
                                    Err(error) => {
                                        app.set_error(format!(
                                            "Could not build launch plan: {error}"
                                        ));
                                        app.screen = Screen::Launcher;
                                    }
                                }
                            }
                        }
                        action => {
                            app.profiling.pending_action = Some(action);
                            app.screen = Screen::ProfileConfirm;
                        }
                    }
                } else {
                    app.reset_profile_and_open_launcher();
                }
            }
        }
        _ => {}
    }

    ProfilingResultOutcome::Continue
}

pub(super) fn handle_profile_failure_key(app: &mut App, key: KeyEvent) -> ProfilingResultOutcome {
    match key.code {
        KeyCode::Esc => {
            if matches!(
                app.profiling.pending_action,
                Some(ProfilingAction::BenchmarkSavedProfile)
            ) && app.configure_recommended_plan.is_some()
            {
                if let Some(plan) = app.current_plan.as_ref() {
                    let model_name = plan.model_name.clone();
                    refresh_configure_profiles(app, &model_name);
                }
                app.clear_profile_failure_and_open_configure_hub();
                return ProfilingResultOutcome::RestartLoop;
            }
            return return_to_profile_advisory_or_launcher(app);
        }
        KeyCode::Char('q') => {
            app.reset_profile_and_open_launcher();
        }
        KeyCode::Up if app.profiling.choice_index > 0 => {
            app.profiling.choice_index -= 1;
        }
        KeyCode::Down => {
            let count = app
                .profiling.failure
                .as_ref()
                .map(|report| report.available_actions().len())
                .unwrap_or(0);
            if app.profiling.choice_index + 1 < count {
                app.profiling.choice_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(report) = &app.profiling.failure {
                let actions = report.available_actions();
                if let Some(action) = actions.get(app.profiling.choice_index).cloned() {
                    app.profiling.pending_action = Some(action);
                    app.screen = Screen::ProfileConfirm;
                }
            }
        }
        _ => {}
    }

    ProfilingResultOutcome::Continue
}

fn return_to_profile_advisory_or_launcher(app: &mut App) -> ProfilingResultOutcome {
    if let Some(record) = app.filtered_catalog_get(app.selected_model) {
        match profiling::build_advisory(&record, app.hardware.as_ref(), &app.services) {
            Ok(advisory) => {
                app.open_profile_advisory(advisory);
            }
            Err(_) => {
                app.reset_profile_and_open_launcher();
            }
        }
    } else {
        app.reset_profile_and_open_launcher();
    }

    ProfilingResultOutcome::Continue
}

#[cfg(feature = "profiling-ui")]
pub(super) fn apply_workflow_event(app: &mut App, event: WorkflowEvent) {
    match event {
        WorkflowEvent::Status { title, detail } => {
            app.profiling.progress_title = title;
            app.push_profile_progress(detail);
        }
        WorkflowEvent::Progress {
            title,
            detail,
            current,
            total,
        } => {
            app.profiling.progress_title = title;
            app.profiling.progress_current = current;
            app.profiling.progress_total = total;
            app.push_profile_progress(detail);
        }
        WorkflowEvent::Completed(report) => {
            let report = *report;
            app.profiling.event_rx = None;
            app.profiling.cancel = None;
            if let Some(ref profile) = report.recommended_profile {
                app.prefs.llamacpp_gpu_layers = Some(profile.gpu_layers);
                app.prefs.llamacpp_context_size = Some(profile.context_size);
            }
            // Apply auto-saved profile from the profiling task (avoids block_on panic)
            if let Some(ref saved) = report.auto_saved_profile {
                let model = report.model_name.clone();
                app.prefs.upsert_saved_launch_profile(&model, saved.clone());
                app.prefs
                    .set_default_saved_launch_profile(&model, &saved.profile_name);
            }
            // Refresh configure profiles if we have a model name
            if let Some(plan) = app.current_plan.as_ref() {
                let model_name = plan.model_name.clone();
                refresh_configure_profiles(app, &model_name);
            }
            let prefs_clone = app.prefs.clone();
            tokio::spawn(async move {
                let _ = crate::prefs::save_prefs(&prefs_clone).await;
            });
            app.profiling.success = Some(report);
            app.profiling.failure = None;
            app.profiling.choice_index = 0;
            app.screen = Screen::ProfileSuccess;
        }
        WorkflowEvent::Failed(report) => {
            let report = *report;
            app.profiling.event_rx = None;
            app.profiling.cancel = None;
            app.profiling.failure = Some(report);
            app.profiling.success = None;
            app.profiling.choice_index = 0;
            app.screen = Screen::ProfileFailure;
        }
        WorkflowEvent::Cancelled => {
            app.profiling.event_rx = None;
            app.profiling.cancel = None;
            app.set_status("Profiling cancelled.".into());
            app.screen = Screen::Launcher;
        }
    }
}
