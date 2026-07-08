use std::time::Instant;

use super::{backend_args::build_llama_args, App, Screen};

pub(super) enum PendingFrontendLaunchOutcome {
    Continue,
    SkipTick,
    ExitLauncher,
}

pub(super) async fn handle_pending_frontend_launch(
    app: &mut App,
    _choice_idx: usize,
) -> PendingFrontendLaunchOutcome {
    if let Some(plan) = app.current_plan.clone() {
        app.screen = Screen::Launching;
        app.launch_start = Some(Instant::now());

        let server_path = match crate::processes::resolved_llamacpp_server_path() {
            Ok(path) => path,
            Err(error) => {
                app.set_error(format!("Launch failed: {error}"));
                app.screen = Screen::Launcher;
                return PendingFrontendLaunchOutcome::SkipTick;
            }
        };
        let model_path = ozone_core::paths::models_dir().join(&plan.model_name);
        let llama_args = build_llama_args(&plan);
        match crate::processes::start_llamacpp(
            &server_path,
            &model_path.to_string_lossy(),
            &llama_args,
        )
        .await
        {
            Ok(_) => {
                if apply_successful_launch(app, &plan, _choice_idx).await {
                    return PendingFrontendLaunchOutcome::ExitLauncher;
                }
            }
            Err(error) => {
                app.set_error(format!("Launch failed: {error}"));
                app.screen = Screen::Launcher;
            }
        }
    } else {
        app.set_error("No launch plan selected.".into());
        app.screen = Screen::Launcher;
    }

    PendingFrontendLaunchOutcome::Continue
}

pub(super) async fn run_launcher_action(
    app: &mut App,
    action: super::LauncherActionId,
    last_refresh: &mut Instant,
) -> super::LauncherActionOutcome {
    match action {
        super::LauncherActionId::Launch => {
            if !app.catalog.is_empty() {
                #[cfg(feature = "profiling-ui")]
                app.reset_profile_flow();
                app.model_picker_mode = super::ModelPickerMode::Launch;
                app.screen = super::Screen::ModelPicker;
            }
        }
        super::LauncherActionId::ConfigureModel => {
            if !app.catalog.is_empty() {
                #[cfg(feature = "profiling-ui")]
                app.reset_profile_flow();
                app.model_picker_mode = super::ModelPickerMode::Configure;
                app.screen = super::Screen::ModelPicker;
            }
        }
        super::LauncherActionId::BenchLauncher => {
            app.bench_launcher_selected = 0;
            app.screen = super::Screen::BenchLauncher;
        }
        super::LauncherActionId::EvalLauncher => {
            app.eval_launcher_selected = 0;
            app.screen = super::Screen::EvalLauncher;
        }
        super::LauncherActionId::Results => {
            app.discover_result_files();
            app.bench_eval_results_selected = 0;
            app.bench_eval_results_viewing = false;
            app.screen = super::Screen::BenchEvalResults;
        }
        #[cfg(feature = "profiling-ui")]
        super::LauncherActionId::ProfileModel => {
            if !app.catalog.is_empty() {
                app.reset_profile_flow();
                app.model_picker_mode = super::ModelPickerMode::Profile;
                app.screen = super::Screen::ModelPicker;
            }
        }
        super::LauncherActionId::Settings => {
            super::open_settings(app);
        }
        super::LauncherActionId::ClearGpu => {
            let _ = crate::processes::clear_gpu_backends().await;
            app.services = crate::processes::get_service_status().await;
            *last_refresh = Instant::now();
            app.set_status("GPU backends cleared.".into());
        }
        super::LauncherActionId::Monitor => {
            app.screen = super::Screen::Monitor;
            app.launch_start = Some(Instant::now());
        }
        super::LauncherActionId::Exit => super::open_exit_confirm(app),
    }

    super::LauncherActionOutcome::Continue
}

async fn apply_successful_launch(
    app: &mut App,
    plan: &crate::launch_config::LaunchPlan,
    _choice_idx: usize,
) -> bool {
    let mut updated_prefs = app.prefs.clone();
    updated_prefs.last_model_name = plan.model_name.clone();
    updated_prefs.last_context_size = Some(plan.context_size);
    updated_prefs.last_gpu_layers = Some(plan.gpu_layers);
    updated_prefs.last_quant_k = Some(plan.quant_k);
    updated_prefs.last_quant_v = Some(plan.quant_v);
    updated_prefs.preferred_backend = Some(super::BackendMode::LlamaCpp);
    let _ = crate::prefs::save_prefs(&updated_prefs).await;
    app.prefs = updated_prefs;

    app.screen = Screen::Monitor;
    false
}
