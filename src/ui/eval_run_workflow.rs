//! Eval runner workflow: event types, event applier, and async spawn.
//!
//! Follows the same pattern as bench_eval_workflow but for the native
//! eval runner pipeline (warmup, calibration, gates, suites, scoring).

use crate::runner::{self, EvalRunConfig};
use tokio::sync::mpsc::UnboundedSender;

/// Events emitted by the eval runner pipeline for TUI progress display.
#[derive(Debug, Clone)]
pub enum EvalRunEvent {
    /// Pipeline stage change.
    Stage { name: String, detail: String },
    /// A single task completed.
    TaskResult {
        task_key: String,
        passed: bool,
        score: f64,
        detail: String,
        latency_ms: u64,
    },
    /// Task was skipped (e.g. context too small).
    TaskSkipped { task_key: String, reason: String },
    /// Pipeline completed successfully.
    Completed {
        tasks_run: usize,
        tasks_passed: usize,
        duration_ms: u64,
    },
    /// Pipeline failed with an error.
    Failed { message: String },
}

/// Apply an eval run event to the App state.
pub(super) fn apply_eval_run_event(app: &mut super::App, event: EvalRunEvent) {
    match event {
        EvalRunEvent::Stage { name, detail } => {
            app.bench_eval.eval_run_stage = name;
            app.bench_eval.eval_run_progress.push(format!("  {}", detail));
        }
        EvalRunEvent::TaskResult {
            task_key,
            passed,
            score,
            detail,
            latency_ms,
        } => {
            let mark = if passed { "[PASS]" } else { "[FAIL]" };
            let line = format!(
                "  {mark} {task_key} ({:.1}s) score={score:.2} {detail}",
                latency_ms as f64 / 1000.0
            );
            app.bench_eval.eval_run_progress.push(line);
            app.bench_eval.eval_run_tasks_run += 1;
            if passed {
                app.bench_eval.eval_run_tasks_passed += 1;
            }
        }
        EvalRunEvent::TaskSkipped { task_key, reason } => {
            app.bench_eval.eval_run_progress
                .push(format!("  [SKIP] {task_key} {reason}"));
        }
        EvalRunEvent::Completed {
            tasks_run,
            tasks_passed,
            duration_ms,
        } => {
            app.bench_eval.eval_run_event_rx = None;
            app.bench_eval.eval_run_running = false;
            app.bench_eval.eval_run_progress.push(format!(
                "  Done: {tasks_passed}/{tasks_run} passed in {:.1}s",
                duration_ms as f64 / 1000.0
            ));
            app.set_status(format!(
                "Eval run complete: {tasks_passed}/{tasks_run} passed"
            ));
            if matches!(app.screen, super::Screen::EvalRunRunning) {
                app.screen = super::Screen::BenchEval;
            }
        }
        EvalRunEvent::Failed { message } => {
            app.bench_eval.eval_run_event_rx = None;
            app.bench_eval.eval_run_running = false;
            let error_msg = format!("Eval run failed: {message}");
            app.set_error(error_msg.clone());
            app.bench_eval.eval_run_progress.push(format!("  ERROR: {message}"));
            // Log to file
            let model = app.bench_eval.eval_run_model.as_deref().unwrap_or("unknown");
            if let Ok(root) = crate::eval::resolve_project_root() {
                let log_dir = root.join("results").join("logs");
                let _ = std::fs::create_dir_all(&log_dir);
                let log_path = log_dir.join("eval-errors.log");
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    use std::io::Write;
                    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                    let _ = writeln!(f, "[{ts}] model={model} {error_msg}");
                }
            }
            if matches!(app.screen, super::Screen::EvalRunRunning) {
                app.screen = super::Screen::BenchEval;
            }
        }
    }
}

/// Spawn the eval runner as a background task with a TUI event channel.
pub(super) fn spawn_eval_run(config: EvalRunConfig, tx: UnboundedSender<EvalRunEvent>) {
    tokio::spawn(async move {
        let _ = tx.send(EvalRunEvent::Stage {
            name: "Starting".into(),
            detail: format!("Warming up {}...", config.model_name),
        });

        if let Err(e) = runner::run_eval_with_events(&config, tx).await {
            tracing::error!("eval_run_workflow: runner failed: {e}");
        }
    });
}
