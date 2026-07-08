use crate::prefs::ModelLaunchOverride;

use super::configure_profile_flow::build_override_from_plans;
use super::{selected_record, App};

const CONFIGURE_FIELD_CONTEXT_SIZE: usize = 0;
const CONFIGURE_FIELD_QUANT_K: usize = 2;
const CONFIGURE_FIELD_QUANT_V: usize = 3;
const CONFIGURE_FIELD_THREADS: usize = 4;
const CONFIGURE_FIELD_BATCH_THREADS: usize = 5;

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
            override_state.context_size =
                Some(crate::launch_config::step_context_size(current, direction));
        }
        CONFIGURE_FIELD_THREADS => {
            let current = app
                .current_plan
                .as_ref()
                .and_then(|p| p.threads)
                .unwrap_or(crate::launch_config::DEFAULT_THREADS);
            let next = (current as i32 + direction).clamp(1, 32) as u32;
            if next != current {
                override_state.threads = Some(next);
            }
        }
        CONFIGURE_FIELD_BATCH_THREADS => {
            let current = app
                .current_plan
                .as_ref()
                .and_then(|p| p.blas_threads)
                .unwrap_or(crate::launch_config::DEFAULT_THREADS);
            let next = (current as i32 + direction).clamp(1, 32) as u32;
            if next != current {
                override_state.blas_threads = Some(next);
            }
        }
        CONFIGURE_FIELD_QUANT_K => {
            let current = app
                .current_plan
                .as_ref()
                .map(|plan| plan.quant_k)
                .unwrap_or(recommended.quant_k);
            let next = (current as i32 + direction).clamp(1, 3) as u8;
            if next != current {
                override_state.quant_k = Some(next);
            }
        }
        CONFIGURE_FIELD_QUANT_V => {
            let current = app
                .current_plan
                .as_ref()
                .map(|plan| plan.quant_v)
                .unwrap_or(recommended.quant_v);
            let next = (current as i32 + direction).clamp(1, 3) as u8;
            if next != current {
                override_state.quant_v = Some(next);
            }
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

    app.current_plan = Some(crate::launch_config::apply_launch_override(
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
        app.current_plan = Some(crate::launch_config::apply_launch_override(
            &recommended,
            &record,
            hw,
            &ModelLaunchOverride::default(),
        ));
    }
}
