use std::time::Instant;

use super::{
    backend_args::{build_kc_args, build_llama_args},
    App, BackendMode, Screen,
};

pub(super) enum PendingFrontendLaunchOutcome {
    Continue,
    SkipTick,
    ExitLauncher,
}

pub(super) async fn handle_pending_frontend_launch(
    app: &mut App,
    choice_idx: usize,
) -> PendingFrontendLaunchOutcome {
    match app.prefs.preferred_backend {
        Some(BackendMode::KoboldCpp) => {
            if let Some(plan) = app.current_plan.clone() {
                app.screen = Screen::Launching;
                app.launch_start = Some(Instant::now());

                let home = std::env::var("HOME").unwrap_or_default();
                let launcher_path = crate::processes::resolved_kobold_launcher_path();
                let model_path = std::path::PathBuf::from(&home)
                    .join("models")
                    .join(&plan.model_name);
                let kc_args = build_kc_args(&plan);
                match crate::processes::start_kobold(
                    &launcher_path,
                    &model_path.to_string_lossy(),
                    &kc_args,
                )
                .await
                {
                    Ok(_) => {
                        if apply_successful_launch(app, &plan, choice_idx).await {
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
        }
        Some(BackendMode::LlamaCpp) => {
            if let Some(plan) = app.current_plan.clone() {
                app.screen = Screen::Launching;
                app.launch_start = Some(Instant::now());

                let home = std::env::var("HOME").unwrap_or_default();
                let server_path = match crate::processes::resolved_llamacpp_server_path() {
                    Ok(path) => path,
                    Err(error) => {
                        app.set_error(format!("Launch failed: {error}"));
                        app.screen = Screen::Launcher;
                        return PendingFrontendLaunchOutcome::SkipTick;
                    }
                };
                let model_path = std::path::PathBuf::from(&home)
                    .join("models")
                    .join(&plan.model_name);
                let llama_args = build_llama_args(&plan);
                match crate::processes::start_llamacpp(
                    &server_path,
                    &model_path.to_string_lossy(),
                    &llama_args,
                )
                .await
                {
                    Ok(_) => {
                        if apply_successful_launch(app, &plan, choice_idx).await {
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
        }
        Some(BackendMode::Ollama) => {
            if choice_idx == 0 {
                if !app.prefs.no_browser {
                    crate::processes::open_browser_app("http://localhost:8000");
                }
                app.screen = Screen::Monitor;
            } else {
                app.set_error(
                    "ozone+ handoff currently requires KoboldCpp. Use SillyTavern for Ollama-backed launches.".into(),
                );
                app.screen = Screen::Launcher;
            }
        }
        None => {
            app.set_error("Configure backend in Settings first".into());
            app.screen = Screen::Launcher;
        }
    }

    PendingFrontendLaunchOutcome::Continue
}

async fn apply_successful_launch(
    app: &mut App,
    plan: &crate::planner::LaunchPlan,
    choice_idx: usize,
) -> bool {
    let mut updated_prefs = app.prefs.clone();
    updated_prefs.last_model_name = plan.model_name.clone();
    updated_prefs.last_context_size = Some(plan.context_size);
    updated_prefs.last_gpu_layers = Some(plan.gpu_layers);
    updated_prefs.last_quant_kv = Some(plan.quant_kv);
    let _ = crate::prefs::save_prefs(&updated_prefs).await;
    app.prefs = updated_prefs;

    if choice_idx == 0 {
        if !app.prefs.no_browser {
            crate::processes::open_browser_app("http://localhost:8000");
        }
        app.screen = Screen::Monitor;
        false
    } else {
        app.ozone_plus_handoff = true;
        true
    }
}
