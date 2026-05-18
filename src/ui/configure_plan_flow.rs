use crate::prefs::ModelLaunchOverride;

use super::configure_profile_flow::build_override_from_plans;
use super::{selected_record, App};

const CONFIGURE_FIELD_CONTEXT_SIZE: usize = 0;

pub(super) fn adjust_configure_plan(app: &mut App, direction: i32) {
    let Some(record) = selected_record(app) else {
        return;
    };
    let Some(recommended) = app.configure_recommended_plan.clone() else {
        return;
    };
    let Some(hw) = app.hardware.as_ref() else {
        return;
    };
    let mut override_state = build_override_from_plans(
        &recommended,
        app.current_plan.as_ref().unwrap_or(&recommended),
    );

    match app.configure_field_index {
        CONFIGURE_FIELD_CONTEXT_SIZE => {
            let current = app
                .current_plan
                .as_ref()
                .map(|plan| plan.context_size)
                .unwrap_or(recommended.context_size);
            override_state.context_size = Some(crate::planner::step_context_size(current, direction));
        }
        _ => {
            let current = app
                .current_plan
                .as_ref()
                .map(|plan| plan.gpu_layers)
                .unwrap_or_else(|| {
                    if recommended.gpu_layers < 0 {
                        recommended.total_layers as i32
                    } else {
                        recommended.gpu_layers
                    }
                });
            override_state.gpu_layers =
                Some((current + direction).clamp(0, recommended.total_layers as i32));
        }
    }

    app.current_plan = Some(crate::planner::apply_launch_override(
        &recommended,
        &record,
        hw,
        &override_state,
    ));
}

pub(super) fn reset_configure_plan(app: &mut App) {
    if let (Some(record), Some(recommended), Some(hw)) = (
        selected_record(app),
        app.configure_recommended_plan.clone(),
        app.hardware.as_ref(),
    ) {
        app.current_plan = Some(crate::planner::apply_launch_override(
            &recommended,
            &record,
            hw,
            &ModelLaunchOverride::default(),
        ));
    }
}
