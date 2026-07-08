use crossterm::event::{KeyCode, KeyEvent};
use tokio_util::sync::CancellationToken;

use crate::profiling::{self, ProfilingAction, WorkflowRequest};

use super::{configure_profile_flow::selected_saved_profile, App, Screen};

pub(super) fn handle_profile_advisory_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.screen = Screen::ModelPicker,
        KeyCode::Up if app.profiling_choice_index > 0 => {
            app.profiling_choice_index -= 1;
        }
        KeyCode::Down => {
            let count = app
                .profiling_advisory
                .as_ref()
                .map(|advisory| advisory.available_actions.len())
                .unwrap_or(0);
            if app.profiling_choice_index + 1 < count {
                app.profiling_choice_index += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(advisory) = &app.profiling_advisory {
                if let Some(action) = advisory
                    .available_actions
                    .get(app.profiling_choice_index)
                    .cloned()
                {
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
                        ProfilingAction::ReviewIssue => {
                            if let Some(record) = app.filtered_catalog_get(app.selected_model) {
                                app.open_profile_failure(profiling::blocking_issue_report(&record));
                            }
                        }
                        action => {
                            app.profiling_pending_action = Some(action);
                            app.screen = Screen::ProfileConfirm;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub(super) fn handle_profile_confirm_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            if matches!(
                app.profiling_pending_action,
                Some(ProfilingAction::BenchmarkSavedProfile)
            ) && app.configure_recommended_plan.is_some()
            {
                app.screen = Screen::ConfigureHub;
            } else {
                app.screen = Screen::ProfileAdvisory;
            }
        }
        KeyCode::Enter => {
            if let (Some(record), Some(action)) = (
                app.filtered_catalog_get(app.selected_model),
                app.profiling_pending_action,
            ) {
                let launch_plan_override = matches!(action, ProfilingAction::BenchmarkSavedProfile)
                    .then(|| app.current_plan.clone())
                    .flatten();
                let launch_profile_name = matches!(action, ProfilingAction::BenchmarkSavedProfile)
                    .then(|| selected_saved_profile(app).map(|profile| profile.profile_name))
                    .flatten();
                let request = WorkflowRequest {
                    record,
                    hardware: app.hardware.clone().unwrap_or_default(),
                    action,
                    profiling_backend: profiling::ProfilingBackend::LlamaCpp,
                    launch_plan_override,
                    launch_profile_name,
                };
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let cancel = CancellationToken::new();
                let cancel_clone = cancel.clone();
                app.start_profile_workflow(rx, cancel);
                let _handle = tokio::spawn(async move {
                    profiling::run_workflow(request, tx, cancel_clone).await
                });
            }
        }
        _ => {}
    }
}
