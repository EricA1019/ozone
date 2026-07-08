use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::unbounded_channel;

use super::{
    bench_eval::{entries, BenchEvalAction},
    App, ModelPickerMode, Screen,
};
use crate::eval::EvalPreset;

pub(super) enum BenchEvalRunningOutcome {
    Continue,
}

pub(super) enum BenchEvalOutcome {
    Continue,
    ExitLauncher,
}

pub(super) async fn handle_bench_eval_key(app: &mut App, key: KeyEvent) -> BenchEvalOutcome {
    match key.code {
        KeyCode::Char('q') => return BenchEvalOutcome::ExitLauncher,
        KeyCode::Esc => {
            app.screen = Screen::Launcher;
        }
        KeyCode::Up => {
            if app.bench_eval.selected > 0 {
                app.bench_eval.selected -= 1;
            }
        }
        KeyCode::Down => {
            let max_index = entries().len().saturating_sub(1);
            if app.bench_eval.selected < max_index {
                app.bench_eval.selected += 1;
            }
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            if let Some(index) = ch.to_digit(10).map(|value| value as usize) {
                if index > 0 && index <= entries().len() {
                    app.bench_eval.selected = index - 1;
                    activate_selected(app).await;
                }
            }
        }
        KeyCode::Char('m') => {
            if !app.catalog.is_empty() {
                app.model_picker_mode = ModelPickerMode::BenchEval;
                app.screen = Screen::ModelPicker;
            } else {
                app.set_error("No models available. Add models first.".into());
            }
        }
        KeyCode::Enter => {
            activate_selected(app).await;
        }
        _ => {}
    }

    BenchEvalOutcome::Continue
}

async fn activate_selected(app: &mut App) {
    let selected = entries()
        .get(app.bench_eval.selected)
        .copied()
        .unwrap_or(entries()[0]);

    match selected.action {
        BenchEvalAction::ProfileModel => {
            #[cfg(feature = "profiling-ui")]
            {
                if app.catalog.is_empty() {
                    app.set_error("No models available. Add or sync models first.".into());
                    return;
                }
                app.reset_profile_flow();
                app.model_picker_mode = super::ModelPickerMode::Profile;
                app.screen = Screen::ModelPicker;
                app.set_status("Opened profiling workflow.".into());
            }

            #[cfg(not(feature = "profiling-ui"))]
            {
                app.set_error(
                    "Profiling workflow requires full build with profiling-ui feature.".into(),
                );
            }
        }
        BenchEvalAction::EvalGsm8k => start_eval_with_cli_name(app, "gsm8k").await,
        BenchEvalAction::EvalInstruction => start_eval_with_cli_name(app, "instruction").await,
        BenchEvalAction::EvalMath => start_eval_with_cli_name(app, "math").await,
        BenchEvalAction::EvalHumaneval => start_eval_with_cli_name(app, "humaneval").await,
        BenchEvalAction::EvalMmlu => start_eval_with_cli_name(app, "mmlu").await,
        BenchEvalAction::EvalHellaSwag => start_eval_with_cli_name(app, "hellaswag").await,
        BenchEvalAction::EvalTruthfulQA => start_eval_with_cli_name(app, "truthfulqa").await,
        BenchEvalAction::EvalBbh => start_eval_with_cli_name(app, "bbh").await,
        BenchEvalAction::EvalMmluPro => start_eval_with_cli_name(app, "mmlu_pro").await,
        BenchEvalAction::EvalArcChallenge => start_eval_with_cli_name(app, "arc_challenge").await,
        BenchEvalAction::EvalMmluPhilosophy => start_eval_with_cli_name(app, "mmlu_philosophy").await,
        BenchEvalAction::EvalHendrycksEthics => start_eval_with_cli_name(app, "hendrycks_ethics").await,
        BenchEvalAction::EvalBbhFormalFallacies => start_eval_with_cli_name(app, "bbh_formal_fallacies").await,
        BenchEvalAction::EvalBbhCausalJudgement => start_eval_with_cli_name(app, "bbh_causal_judgement").await,
        BenchEvalAction::EvalMbpp => start_eval_with_cli_name(app, "mbpp").await,
        BenchEvalAction::EvalDrop => start_eval_with_cli_name(app, "drop").await,
        BenchEvalAction::EvalGpqa => start_eval_with_cli_name(app, "gpqa").await,
        BenchEvalAction::EvalRun => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Select or launch a model first.".into());
                return;
            };
            if app.bench_eval.eval_run_event_rx.is_some() {
                app.set_error("An eval run is already in progress.".into());
                return;
            }
            let (tx, rx) = unbounded_channel();
            let model_path = ozone_core::paths::models_dir().join(&model_name);
            let base_url = ozone_core::paths::llamacpp_base_url();
            let config = crate::runner::EvalRunConfig {
                model_name: model_name.clone(),
                model_path: model_path.to_string_lossy().to_string(),
                backend: "llama.cpp".into(),
                base_url,
                context_length: 4096,
                skip_warmup: false,
                skip_health_gate: false,
                ..Default::default()
            };
            app.bench_eval.eval_run_event_rx = Some(rx);
            app.bench_eval.eval_run_stage = "Starting...".into();
            app.bench_eval.eval_run_running = true;
            app.bench_eval.eval_run_tasks_run = 0;
            app.bench_eval.eval_run_tasks_passed = 0;
            app.bench_eval.eval_run_model = Some(model_name);
            app.bench_eval.eval_run_progress.clear();
            app.screen = Screen::EvalRunRunning;
            app.set_status("Eval run started...".into());
            super::eval_run_workflow::spawn_eval_run(config, tx);
        }
        BenchEvalAction::EvalCreativeWriting => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Select or launch a model first.".into());
                return;
            };
            let model = model_name.to_string();
            app.set_status("Running creative writing eval…".into());
            tokio::spawn(async move {
                let root = match crate::eval::resolve_project_root() {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("creative writing: failed to resolve project root: {e}");
                        return;
                    }
                };
                let prompts = match crate::creative_writing::load_prompt_bank(&root) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("creative writing: failed to load prompt bank: {e}");
                        return;
                    }
                };
                let artifacts_dir = root.join("results").join("creative_writing");
                let base_url = ozone_core::paths::llamacpp_base_url();
                tracing::info!("Creative writing eval starting for {model}");
                match crate::creative_writing::run_creative_writing_eval(
                    &model,
                    &prompts,
                    &base_url,
                    &artifacts_dir,
                )
                .await
                {
                    Ok(csv_path) => {
                        tracing::info!("Creative writing eval complete: {}", csv_path.display());
                    }
                    Err(e) => {
                        tracing::error!("Creative writing eval failed: {e}");
                    }
                }
            });
        }
        BenchEvalAction::ExportServer => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Launch or select a model first.".into());
                return;
            };
            let model = model_name.to_string();
            app.set_status("Generating server script…".into());
            tokio::spawn(async move {
                let model_dir = ozone_core::paths::models_dir();
                let model_path = model_dir.join(&model);
                let server_path = match crate::processes::resolved_llamacpp_server_path() {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("export server: failed to resolve server path: {e}");
                        return;
                    }
                };
                let report = match crate::catalog::load_catalog_report(
                    &model_dir,
                    &ozone_core::paths::catalog_preset_path(),
                    &model_dir.join("bench-results.txt"),
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("export server: failed to load catalog: {e}");
                        return;
                    }
                };
                let Some(record) = report.records.iter().find(|r| r.model_name == model) else {
                    tracing::error!("export server: model '{}' not found in catalog", model);
                    return;
                };
                let plan = crate::launch_config::plan_launch(record, &Default::default());
                let output = model_dir.join(format!("serve-{model}.sh"));
                match crate::export_server::generate_serve_script(
                    &plan,
                    &model_path,
                    &server_path,
                    ozone_core::paths::DEFAULT_LLAMACPP_PORT,
                    &output,
                ) {
                    Ok(path) => tracing::info!("Server script written to {}", path.display()),
                    Err(e) => tracing::error!("export server: failed to generate script: {e}"),
                }
            });
        }
        BenchEvalAction::ViewResults => {
            app.discover_result_files();
            app.bench_eval.results_viewing = false;
            app.bench_eval.results_selected = 0;
            if app.bench_eval.results_files.is_empty() {
                app.set_error(
                    "No result files found. Run an eval, sweep, or creative-write first.".into(),
                );
            } else {
                app.screen = Screen::BenchEvalResults;
            }
        }
        BenchEvalAction::ViewReport => {
            if app.bench_eval.report_markdown.is_empty() {
                app.set_error("Run an eval first so there is a markdown report to view.".into());
            } else {
                app.screen = Screen::BenchEvalReport;
            }
        }
        BenchEvalAction::Back => {
            app.screen = Screen::Launcher;
        }
    }
}

pub(crate) fn resolve_bench_eval_model(app: &App) -> Option<String> {
    app.filtered_catalog_get(app.selected_model)
        .map(|record| record.model_name)
        .or_else(|| {
            app.current_plan
                .as_ref()
                .map(|plan| plan.model_name.clone())
        })
        .or_else(|| {
            (!app.prefs.last_model_name.is_empty()).then(|| app.prefs.last_model_name.clone())
        })
}

/// Start an eval using a CLI task name string (works with the task registry).
pub(crate) async fn start_eval_with_cli_name(app: &mut App, cli_name: &str) {
    let Some(model_name) = resolve_bench_eval_model(app) else {
        app.set_error("Launch a model first so Bench + Eval knows which model to evaluate.".into());
        return;
    };

    if app.bench_eval.event_rx.is_some() {
        app.set_error("An evaluation is already running.".into());
        return;
    }

    let limit = 1;
    let base_url = ozone_core::paths::llamacpp_base_url();
    let command_preview =
        format!("oz eval {model_name} --preset {cli_name} --limit {limit} --base-url {base_url}");
    let (tx, rx) = unbounded_channel();
    let error_tx = tx.clone();

    // Convert CLI name to EvalPreset for workflow backward compat
    let preset = match cli_name {
        "gsm8k" => EvalPreset::Gsm8k,
        "instruction" => EvalPreset::Instruction,
        "math" => EvalPreset::Math,
        "humaneval" => EvalPreset::Humaneval,
        "mmlu" => EvalPreset::Mmlu,
        "hellaswag" => EvalPreset::HellaSwag,
        "truthfulqa" => EvalPreset::TruthfulQA,
        "bbh" => EvalPreset::Bbh,
        "mmlu_pro" => EvalPreset::MmluPro,
        "arc_challenge" => EvalPreset::ArcChallenge,
        "mmlu_philosophy" => EvalPreset::MmluPhilosophy,
        "hendrycks_ethics" => EvalPreset::HendrycksEthics,
        "bbh_formal_fallacies" => EvalPreset::BbhFormalFallacies,
        "bbh_causal_judgement" => EvalPreset::BbhCausalJudgement,
        "mbpp" => EvalPreset::Mbpp,
        "drop" => EvalPreset::Drop,
        "gpqa" => EvalPreset::Gpqa,
        _ => {
            app.set_error(format!("Unknown eval preset: {cli_name}"));
            return;
        }
    };

    app.start_bench_eval_workflow(rx, model_name.clone(), preset, limit, command_preview);
    let cli_name_owned = cli_name.to_string();

    // Resolve server and model paths so the spawned task can auto-launch
    // the model if llama.cpp isn't already serving it.
    let model_path = ozone_core::paths::models_dir().join(&model_name);
    let server_path = crate::processes::resolved_llamacpp_server_path().ok();

    tokio::spawn(async move {
        // Auto-launch the model if llama.cpp isn't running
        if let Some(sp) = &server_path {
            let mp = &model_path;
            let ready_url = ozone_core::paths::llamacpp_ready_url();
            if !crate::processes::is_url_ready(&ready_url).await {
                let _ = error_tx.send(super::bench_eval_workflow::BenchEvalWorkflowEvent::Status {
                    title: "Launching model…".into(),
                    detail: format!("Starting {} for eval", model_name),
                });
                // Use minimal default args for the eval context
                let args: Vec<String> = vec![
                    "--host".into(),
                    ozone_core::paths::DEFAULT_LOCALHOST.into(),
                    "--port".into(),
                    ozone_core::paths::DEFAULT_LLAMACPP_PORT.to_string(),
                    "--ctx-size".into(),
                    "4096".into(),
                    "--no-webui".into(),
                ];
                match crate::processes::start_llamacpp(sp, &mp.to_string_lossy(), &args).await {
                    Ok(_) => {
                        // Give the server a moment to become ready
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    }
                    Err(e) => {
                        let _ = error_tx.send(
                            super::bench_eval_workflow::BenchEvalWorkflowEvent::Output {
                                is_stderr: true,
                                line: format!("Model launch warning: {e}"),
                            },
                        );
                    }
                }
            }
        }

        if let Err(error) = super::bench_eval_workflow::run_bench_eval_workflow_with_cli_name(
            model_name,
            preset,
            cli_name_owned,
            limit,
            base_url,
            tx,
        )
        .await
        {
            let _ = error_tx.send(super::bench_eval_workflow::BenchEvalWorkflowEvent::Failed {
                message: format!("Failed to run eval: {error}"),
            });
        }
    });
}

pub(super) fn handle_bench_eval_running_key(
    app: &mut App,
    key: KeyEvent,
) -> BenchEvalRunningOutcome {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            if app.bench_eval.event_rx.is_some() {
                app.set_status("Evaluation continues in the background.".into());
            }
            app.screen = Screen::BenchEval;
            BenchEvalRunningOutcome::Continue
        }
        _ => BenchEvalRunningOutcome::Continue,
    }
}

pub(super) fn handle_bench_eval_report_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = Screen::BenchEval;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.bench_eval.report_scroll = app.bench_eval.report_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.bench_eval.report_scroll = app.bench_eval.report_scroll.saturating_add(1);
        }
        KeyCode::PageUp => {
            app.bench_eval.report_scroll = app.bench_eval.report_scroll.saturating_sub(8);
        }
        KeyCode::PageDown => {
            app.bench_eval.report_scroll = app.bench_eval.report_scroll.saturating_add(8);
        }
        KeyCode::Home => {
            app.bench_eval.report_scroll = 0;
        }
        KeyCode::End => {
            app.bench_eval.report_scroll = u16::MAX;
        }
        _ => {}
    }
}

pub(super) fn handle_bench_eval_results_key(app: &mut App, key: KeyEvent) {
    if app.bench_eval.results_viewing {
        // Viewing file contents
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.bench_eval.results_viewing = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.bench_eval.results_scroll = app.bench_eval.results_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.bench_eval.results_scroll = app.bench_eval.results_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                app.bench_eval.results_scroll = app.bench_eval.results_scroll.saturating_sub(12);
            }
            KeyCode::PageDown => {
                app.bench_eval.results_scroll = app.bench_eval.results_scroll.saturating_add(12);
            }
            KeyCode::Home => {
                app.bench_eval.results_scroll = 0;
            }
            KeyCode::End => {
                app.bench_eval.results_scroll = u16::MAX;
            }
            _ => {}
        }
    } else {
        // Browsing result file list
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.screen = Screen::BenchEval;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if app.bench_eval.results_selected > 0 {
                    app.bench_eval.results_selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.bench_eval.results_files.len().saturating_sub(1);
                if app.bench_eval.results_selected < max {
                    app.bench_eval.results_selected += 1;
                }
            }
            KeyCode::Enter => {
                app.load_result_file_content(app.bench_eval.results_selected);
            }
            KeyCode::Char('r') => {
                app.discover_result_files();
                app.bench_eval.results_selected = 0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_bench_eval_model;
    use crate::catalog::{CatalogRecord, RecSource, Recommendation};
    use crate::prefs::Preferences;
    use crate::ui::App;
    use std::path::PathBuf;

    fn test_catalog_record(name: &str) -> CatalogRecord {
        CatalogRecord {
            model_name: name.to_string(),
            model_path: PathBuf::from(format!("/models/{name}")),
            model_size_gb: 4.2,
            recommendation: Recommendation {
                context_size: 4096,
                gpu_layers: -1,
                quant_k: 1,
                quant_v: 1,
                note: "test".into(),
                source: RecSource::Heuristic,
            },
            benchmark: None,
            benchmark_count: 0,
            source_priority: RecSource::Heuristic.priority(),
        }
    }

    #[test]
    fn resolve_bench_eval_model_prefers_saved_last_model() {
        let mut app = App::new(Preferences::default());
        app.prefs.last_model_name = "gemma-4.gguf".into();

        assert_eq!(resolve_bench_eval_model(&app), Some("gemma-4.gguf".into()));
    }

    #[test]
    fn resolve_bench_eval_model_falls_back_to_selected_saved_catalog_model() {
        let mut app = App::new(Preferences::default());
        app.catalog = vec![test_catalog_record("saved-model-1.gguf")];

        assert_eq!(
            resolve_bench_eval_model(&app),
            Some("saved-model-1.gguf".into())
        );
    }

    #[test]
    fn resolve_bench_eval_model_prefers_selected_catalog_model_over_last_model_name() {
        let mut app = App::new(Preferences::default());
        app.catalog = vec![test_catalog_record("saved-model-1.gguf")];
        app.prefs.last_model_name = "stale-last-model.gguf".into();

        assert_eq!(
            resolve_bench_eval_model(&app),
            Some("saved-model-1.gguf".into())
        );
    }
}
