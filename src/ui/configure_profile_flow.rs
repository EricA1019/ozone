use crate::catalog::CatalogRecord;
use crate::planner::LaunchPlan;
use crate::prefs::{ModelLaunchOverride, SavedLaunchProfile};

use super::{selected_record, App};

pub(super) fn build_effective_plan(
    app: &App,
    record: &CatalogRecord,
    recommended: &LaunchPlan,
) -> Option<LaunchPlan> {
    app.hardware.as_ref().map(|hw| {
        if let Some(profile_name) = app
            .prefs
            .default_saved_launch_profile_name_for(&record.model_name)
        {
            if let Some(saved_profile) = app
                .prefs
                .saved_launch_profile(&record.model_name, profile_name)
            {
                return crate::planner::apply_saved_profile(
                    recommended,
                    record,
                    hw,
                    saved_profile.context_size,
                    saved_profile.gpu_layers,
                    saved_profile.quant_kv,
                    saved_profile.threads,
                );
            }
        }
        let override_state = app
            .prefs
            .launch_override_for(&record.model_name)
            .unwrap_or_default();
        crate::planner::apply_launch_override(recommended, record, hw, &override_state)
    })
}

pub(super) fn selected_saved_profile(app: &App) -> Option<SavedLaunchProfile> {
    app.configure_saved_profiles
        .get(app.configure_profile_index)
        .cloned()
}

#[cfg(feature = "profiling-ui")]
fn refresh_configure_profile_reports(app: &mut App, model_name: &str) {
    let reports = crate::profiling::saved_profile_reports(model_name, &app.configure_saved_profiles)
        .unwrap_or_default();
    app.configure_profile_reports = reports;
}

#[cfg(not(feature = "profiling-ui"))]
fn refresh_configure_profile_reports(_app: &mut App, _model_name: &str) {}

pub(super) fn refresh_configure_profiles(app: &mut App, model_name: &str) {
    let profiles = app.prefs.saved_launch_profiles_for(model_name);
    app.configure_saved_profiles = profiles;
    if app.configure_saved_profiles.is_empty() {
        app.configure_profile_index = 0;
    } else if let Some(default_name) = app.prefs.default_saved_launch_profile_name_for(model_name) {
        app.configure_profile_index = app
            .configure_saved_profiles
            .iter()
            .position(|profile| profile.profile_name == default_name)
            .unwrap_or(0);
    } else {
        app.configure_profile_index = app
            .configure_profile_index
            .min(app.configure_saved_profiles.len().saturating_sub(1));
    }
    refresh_configure_profile_reports(app, model_name);
}

fn next_saved_profile_name(profiles: &[SavedLaunchProfile]) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("custom-{index}");
        if profiles
            .iter()
            .all(|profile| profile.profile_name != candidate)
        {
            return candidate;
        }
        index += 1;
    }
}

pub(super) fn save_current_plan_as_profile(app: &mut App) -> Option<String> {
    let plan = app.current_plan.clone()?;
    let profile_name = next_saved_profile_name(&app.configure_saved_profiles);
    app.prefs.upsert_saved_launch_profile(
        plan.model_name.clone(),
        SavedLaunchProfile {
            profile_name: profile_name.clone(),
            context_size: plan.context_size,
            gpu_layers: plan.gpu_layers,
            quant_kv: plan.quant_kv,
            threads: plan.threads,
        },
    );
    if app
        .prefs
        .default_saved_launch_profile_name_for(&plan.model_name)
        .is_none()
    {
        app.prefs
            .set_default_saved_launch_profile(plan.model_name.clone(), profile_name.clone());
    }
    refresh_configure_profiles(app, &plan.model_name);
    app.configure_profile_index = app
        .configure_saved_profiles
        .iter()
        .position(|profile| profile.profile_name == profile_name)
        .unwrap_or(0);
    Some(profile_name)
}

pub(super) fn update_selected_profile_from_current_plan(app: &mut App) -> Option<String> {
    let plan = app.current_plan.clone()?;
    let selected = selected_saved_profile(app)?;
    app.prefs.upsert_saved_launch_profile(
        plan.model_name.clone(),
        SavedLaunchProfile {
            profile_name: selected.profile_name.clone(),
            context_size: plan.context_size,
            gpu_layers: plan.gpu_layers,
            quant_kv: plan.quant_kv,
            threads: plan.threads,
        },
    );
    refresh_configure_profiles(app, &plan.model_name);
    app.configure_profile_index = app
        .configure_saved_profiles
        .iter()
        .position(|profile| profile.profile_name == selected.profile_name)
        .unwrap_or(0);
    Some(selected.profile_name)
}

pub(super) fn apply_selected_saved_profile(app: &mut App) -> Option<String> {
    let record = selected_record(app)?;
    let recommended = app.configure_recommended_plan.clone()?;
    let hw = app.hardware.as_ref()?;
    let profile = selected_saved_profile(app)?;
    app.current_plan = Some(crate::planner::apply_saved_profile(
        &recommended,
        &record,
        hw,
        profile.context_size,
        profile.gpu_layers,
        profile.quant_kv,
        profile.threads,
    ));
    Some(profile.profile_name)
}

pub(super) fn cycle_saved_profile(app: &mut App, direction: i32) {
    if app.configure_saved_profiles.is_empty() {
        return;
    }
    let len = app.configure_saved_profiles.len() as i32;
    let current = app.configure_profile_index as i32;
    app.configure_profile_index = (current + direction).rem_euclid(len) as usize;
}

pub(super) fn set_selected_profile_default(app: &mut App) -> Option<String> {
    let model_name = app.current_plan.as_ref()?.model_name.clone();
    let profile = selected_saved_profile(app)?;
    app.prefs
        .set_default_saved_launch_profile(model_name.clone(), profile.profile_name.clone());
    refresh_configure_profiles(app, &model_name);
    Some(profile.profile_name)
}

pub(super) fn delete_selected_saved_profile(app: &mut App) -> Option<String> {
    let model_name = app.current_plan.as_ref()?.model_name.clone();
    let profile = selected_saved_profile(app)?;
    if !app
        .prefs
        .remove_saved_launch_profile(&model_name, &profile.profile_name)
    {
        return None;
    }
    refresh_configure_profiles(app, &model_name);
    Some(profile.profile_name)
}

pub(super) fn build_override_from_plans(
    recommended: &LaunchPlan,
    effective: &LaunchPlan,
) -> ModelLaunchOverride {
    let recommended_gpu_layers = if recommended.gpu_layers < 0 {
        recommended.total_layers as i32
    } else {
        recommended.gpu_layers
    };
    ModelLaunchOverride {
        context_size: (effective.context_size != recommended.context_size)
            .then_some(effective.context_size),
        gpu_layers: (effective.gpu_layers != recommended_gpu_layers).then_some(effective.gpu_layers),
        threads: (effective.threads != recommended.threads)
            .then_some(effective.threads)
            .flatten(),
        blas_threads: (effective.blas_threads != recommended.blas_threads)
            .then_some(effective.blas_threads)
            .flatten(),
        quant_kv: (effective.quant_kv != recommended.quant_kv)
            .then_some(effective.quant_kv),
    }
}
