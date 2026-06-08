use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc::UnboundedSender,
};

use crate::eval::EvalPreset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BenchEvalWorkflowEvent {
    Status { title: String, detail: String },
    Output { is_stderr: bool, line: String },
    Completed {
        exit_code: Option<i32>,
        report: Option<crate::eval_report::EvalMarkdownReport>,
    },
    Failed { message: String },
}

pub(super) fn apply_bench_eval_event(
    app: &mut super::App,
    event: BenchEvalWorkflowEvent,
) {
    match event {
        BenchEvalWorkflowEvent::Status { title, detail } => {
            app.bench_eval_progress_title = title;
            app.push_bench_eval_progress(detail);
        }
        BenchEvalWorkflowEvent::Output { is_stderr, line } => {
            let prefix = if is_stderr { "stderr" } else { "stdout" };
            app.push_bench_eval_progress(format!("[{prefix}] {line}"));
        }
        BenchEvalWorkflowEvent::Completed { exit_code, report } => {
            app.bench_eval_event_rx = None;
            app.bench_eval_running_model = None;
            app.bench_eval_running_preset = None;
            app.bench_eval_running_command = None;
            match exit_code {
                Some(0) | None => {
                    app.set_status("Evaluation completed successfully.".into());
                }
                Some(code) => {
                    app.set_error(format!("Evaluation exited with code {code}."));
                }
            }
            if let Some(report) = report {
                if matches!(app.screen, super::Screen::BenchEvalRunning) {
                    app.open_bench_eval_report(report);
                } else {
                    app.store_bench_eval_report(report);
                    app.set_status("Markdown report ready. Open View Report to inspect it.".into());
                }
            }
            if matches!(app.screen, super::Screen::BenchEvalRunning) {
                app.screen = super::Screen::BenchEval;
            }
        }
        BenchEvalWorkflowEvent::Failed { message } => {
            app.bench_eval_event_rx = None;
            app.bench_eval_running_model = None;
            app.bench_eval_running_preset = None;
            app.bench_eval_running_command = None;
            app.set_error(message);
            if matches!(app.screen, super::Screen::BenchEvalRunning) {
                app.screen = super::Screen::BenchEval;
            }
        }
    }
}

#[allow(dead_code)]
pub(super) async fn run_bench_eval_workflow(
    model_name: String,
    preset: EvalPreset,
    limit: u32,
    base_url: String,
    tx: UnboundedSender<BenchEvalWorkflowEvent>,
) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let command_preview = format!(
        "{} eval {} --preset {} --limit {} --base-url {}",
        current_exe.display(),
        model_name,
        preset.cli_name(),
        limit,
        base_url
    );

    let _ = tx.send(BenchEvalWorkflowEvent::Status {
        title: "Launching eval".into(),
        detail: command_preview.clone(),
    });

    let mut command = Command::new(current_exe);
    command
        .arg("eval")
        .arg(&model_name)
        .arg("--preset")
        .arg(preset.cli_name())
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--base-url")
        .arg(&base_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = command.spawn().context("failed to spawn eval command")?;

    let mut stdout_reader = child
        .stdout
        .take()
        .map(|stdout| BufReader::new(stdout).lines());
    let mut stderr_reader = child
        .stderr
        .take()
        .map(|stderr| BufReader::new(stderr).lines());

    let stdout_tx = tx.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(ref mut lines) = stdout_reader {
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stdout_tx.send(BenchEvalWorkflowEvent::Output {
                    is_stderr: false,
                    line,
                });
            }
        }
    });

    let stderr_tx = tx.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(ref mut lines) = stderr_reader {
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stderr_tx.send(BenchEvalWorkflowEvent::Output {
                    is_stderr: true,
                    line,
                });
            }
        }
    });

    let status = child.wait().await.context("failed to wait for eval command")?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let report = crate::eval_report::build_eval_report_for_preset(&model_name, preset).ok();

    let _ = tx.send(BenchEvalWorkflowEvent::Completed {
        exit_code: status.code(),
        report,
    });
    Ok(())
}

pub(super) async fn run_bench_eval_workflow_with_cli_name(
    model_name: String,
    _preset: EvalPreset,
    cli_name: String,
    limit: u32,
    base_url: String,
    tx: UnboundedSender<BenchEvalWorkflowEvent>,
) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let command_preview = format!(
        "{} eval {} --preset {} --limit {} --base-url {}",
        current_exe.display(),
        model_name,
        cli_name,
        limit,
        base_url
    );

    let _ = tx.send(BenchEvalWorkflowEvent::Status {
        title: "Launching eval".into(),
        detail: command_preview.clone(),
    });

    let mut command = Command::new(current_exe);
    command
        .arg("eval")
        .arg(&model_name)
        .arg("--preset")
        .arg(&cli_name)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--base-url")
        .arg(&base_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = command.spawn().context("failed to spawn eval command")?;

    let mut stdout_reader = child
        .stdout
        .take()
        .map(|stdout| BufReader::new(stdout).lines());
    let mut stderr_reader = child
        .stderr
        .take()
        .map(|stderr| BufReader::new(stderr).lines());

    let stdout_tx = tx.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(ref mut lines) = stdout_reader {
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stdout_tx.send(BenchEvalWorkflowEvent::Output {
                    is_stderr: false,
                    line,
                });
            }
        }
    });

    let stderr_tx = tx.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(ref mut lines) = stderr_reader {
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stderr_tx.send(BenchEvalWorkflowEvent::Output {
                    is_stderr: true,
                    line,
                });
            }
        }
    });

    let status = child.wait().await.context("failed to wait for eval command")?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let report = crate::eval_report::build_eval_report_for_preset(&model_name, _preset).ok();

    let _ = tx.send(BenchEvalWorkflowEvent::Completed {
        exit_code: status.code(),
        report,
    });
    Ok(())
}
