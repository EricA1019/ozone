use crossterm::event::{KeyCode, KeyEvent};

use super::configure_profile_flow::{build_effective_plan, refresh_configure_profiles};
use super::{App, ModelPickerMode, Screen};
#[cfg(feature = "profiling-ui")]
use crate::profiling;

pub(super) fn handle_model_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            if !app.model_filter.is_empty() {
                app.model_filter.clear();
            } else {
                app.current_plan = None;
                app.configure_recommended_plan = None;
                app.screen = Screen::Launcher;
            }
        }
        KeyCode::Up => {
            if app.selected_model > 0 {
                app.selected_model -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.filtered_catalog_len();
            if app.selected_model + 1 < count {
                app.selected_model += 1;
            }
        }
        KeyCode::Backspace => {
            app.model_filter.pop();
            app.selected_model = 0;
        }
        KeyCode::Enter => {
            if let Some(record) = app.filtered_catalog_get(app.selected_model) {
                match app.model_picker_mode {
                    ModelPickerMode::BenchEval => {
                        // Return directly to Bench+Eval — the selected model
                        // is already reflected in app.selected_model and will
                        // be picked up by resolve_bench_eval_model on the next action.
                        app.current_plan = None;
                        app.configure_recommended_plan = None;
                        app.screen = Screen::BenchEval;
                    }
                    ModelPickerMode::Launch | ModelPickerMode::Configure => {
                        if let Some(hw) = &app.hardware {
                            let recommended = crate::planner::plan_launch(&record, hw);
                            app.current_plan = build_effective_plan(app, &record, &recommended)
                                .or_else(|| Some(recommended.clone()));
                            app.configure_recommended_plan = Some(recommended);
                            app.configure_field_index = 0;
                            refresh_configure_profiles(app, &record.model_name);
                            app.screen = Screen::ConfigureHub;
                        }
                    }
                    #[cfg(feature = "profiling-ui")]
                    ModelPickerMode::Profile => match profiling::build_advisory(
                        &record,
                        app.hardware.as_ref(),
                        &app.services,
                    ) {
                        Ok(advisory) => {
                            app.profiling_advisory = Some(advisory);
                            app.profiling_choice_index = 0;
                            app.profiling_success = None;
                            app.profiling_failure = None;
                            app.screen = Screen::ProfileAdvisory;
                        }
                        Err(error) => {
                            app.set_error(format!("Could not prepare profiling advice: {error}"));
                            app.screen = Screen::Launcher;
                        }
                    },
                }
            }
        }
        KeyCode::Char(c) if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' => {
            app.model_filter.push(c);
            app.selected_model = 0;
        }
        _ => {}
    }
}
