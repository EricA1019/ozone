//! Eval Launcher screen — dedicated screen for evaluation actions.
//!
//! Sweep levels: Quick (health+canary), Standard (+code_micro), Full (all 5 suites).
//! Also provides individual eval access and result/report viewing.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::{App, Screen};
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalLauncherAction {
    QuickSweep,
    StandardSweep,
    FullSweep,
    CreativeWriting,
    RegisteredEval { index: usize },
    ViewResults,
    ViewReport,
    Back,
}

#[derive(Debug, Clone)]
pub(super) struct EvalLauncherEntry {
    pub action: EvalLauncherAction,
    pub label: String,
    pub description: String,
    pub command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalLauncherOutcome {
    Continue,
    ExitLauncher,
}

pub(super) fn entries(_app: &App) -> Vec<EvalLauncherEntry> {
    let mut entries: Vec<EvalLauncherEntry> = vec![
        EvalLauncherEntry {
            action: EvalLauncherAction::QuickSweep,
            label: "Quick Eval Sweep".into(),
            description: "Health checks + canary gates (~17 tasks)".into(),
            command: "eval-quick",
        },
        EvalLauncherEntry {
            action: EvalLauncherAction::StandardSweep,
            label: "Standard Eval Sweep".into(),
            description: "Quick + code micro (~21 tasks)".into(),
            command: "eval-standard",
        },
        EvalLauncherEntry {
            action: EvalLauncherAction::FullSweep,
            label: "Full Eval Sweep".into(),
            description: "All 5 suites: health, canary, code, format, math (~36 tasks)".into(),
            command: "eval-full",
        },
        EvalLauncherEntry {
            action: EvalLauncherAction::CreativeWriting,
            label: "Creative Writing Probe".into(),
            description: "Diversity & coherence check".into(),
            command: "eval-creative",
        },
    ];

    for (i, task) in crate::eval::EVAL_TASKS.iter().enumerate() {
        entries.push(EvalLauncherEntry {
            action: EvalLauncherAction::RegisteredEval { index: i },
            label: task.report_label.to_string(),
            description: task.description.to_string(),
            command: task.cli_name,
        });
    }

    entries.push(EvalLauncherEntry {
        action: EvalLauncherAction::ViewResults,
        label: "View Eval Results".into(),
        description: "Browse past eval/sweep results".into(),
        command: "results",
    });
    entries.push(EvalLauncherEntry {
        action: EvalLauncherAction::ViewReport,
        label: "View Eval Report".into(),
        description: "Open latest eval markdown report".into(),
        command: "report",
    });
    entries.push(EvalLauncherEntry {
        action: EvalLauncherAction::Back,
        label: "Back".into(),
        description: "Return to main launcher".into(),
        command: "back",
    });

    entries
}

pub(super) fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Min(24),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(80),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Eval Launcher ", style_bold_cyan()),
        Span::styled("  ·  pick a sweep level or individual eval", style_muted()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_cyan()),
    );
    f.render_widget(header, chunks[0]);

    let eval_entries = entries(app);
    let items: Vec<ListItem> = eval_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_sel = idx == app.eval_launcher_selected;
            let marker = if is_sel { "▶ " } else { "  " };
            let marker_span =
                Span::styled(marker, if is_sel { style_lime() } else { style_muted() });
            let label_style = if is_sel {
                style_bold_lime()
            } else {
                style_bold_cyan()
            };
            let desc_style = if is_sel { style_lime() } else { style_gray() };
            ListItem::new(Line::from(vec![
                marker_span,
                Span::styled(format!("{} ", entry.label), label_style),
                Span::styled(entry.description.as_str(), desc_style),
                Span::styled(format!("  /{}", entry.command), style_muted()),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(" Sweeps & Evals ", style_bold_cyan()))
            .borders(Borders::ALL)
            .border_style(style_gray()),
    );

    f.render_stateful_widget(
        list,
        chunks[1],
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.eval_launcher_selected)),
    );

    let hints = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" navigate  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" select  ", style_muted()),
        Span::styled("Esc/q", style_hint_key()),
        Span::styled(" back", style_muted()),
    ]))
    .block(Block::default().borders(Borders::NONE));
    f.render_widget(hints, chunks[2]);
}

pub(super) async fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
) -> EvalLauncherOutcome {
    let eval_entries = entries(app);

    match key.code {
        crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') => {
            app.screen = Screen::Launcher;
            return EvalLauncherOutcome::ExitLauncher;
        }
        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
            if eval_entries.is_empty() {
                return EvalLauncherOutcome::Continue;
            }
            app.eval_launcher_selected = if app.eval_launcher_selected == 0 {
                eval_entries.len() - 1
            } else {
                app.eval_launcher_selected - 1
            };
        }
        crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
            if eval_entries.is_empty() {
                return EvalLauncherOutcome::Continue;
            }
            app.eval_launcher_selected = if app.eval_launcher_selected + 1 >= eval_entries.len() {
                0
            } else {
                app.eval_launcher_selected + 1
            };
        }
        crossterm::event::KeyCode::Enter => {
            let idx = app.eval_launcher_selected;
            if let Some(entry) = eval_entries.get(idx) {
                dispatch_action(app, entry.action).await;
            }
        }
        _ => {}
    }

    EvalLauncherOutcome::Continue
}

async fn dispatch_action(app: &mut App, action: EvalLauncherAction) {
    match action {
        EvalLauncherAction::QuickSweep => {
            start_eval_sweep(app, crate::runner::SweepLevel::Quick);
        }
        EvalLauncherAction::StandardSweep => {
            start_eval_sweep(app, crate::runner::SweepLevel::Standard);
        }
        EvalLauncherAction::FullSweep => {
            start_eval_sweep(app, crate::runner::SweepLevel::Full);
        }
        EvalLauncherAction::CreativeWriting => {
            let Some(model_name) = super::bench_eval_flow::resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Select or launch a model first.".into());
                return;
            };
            let model = model_name.clone();
            app.screen = Screen::Launcher;
            app.set_status("Running creative writing eval…".into());
            tokio::spawn(async move {
                let root = match crate::eval::resolve_project_root() {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("creative writing: {e}");
                        return;
                    }
                };
                let prompts = match crate::creative_writing::load_prompt_bank(&root) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("creative writing: {e}");
                        return;
                    }
                };
                let artifacts_dir = root
                    .join("contrib/evals/artifacts")
                    .join("creative_writing");
                let base_url = ozone_core::paths::llamacpp_base_url();
                let _ = crate::creative_writing::run_creative_writing_eval(
                    &model,
                    &prompts,
                    &base_url,
                    &artifacts_dir,
                )
                .await;
            });
        }
        EvalLauncherAction::RegisteredEval { index } => {
            if let Some(task) = crate::eval::EVAL_TASKS.get(index) {
                super::bench_eval_flow::start_eval_with_cli_name(app, task.cli_name).await;
            }
        }
        EvalLauncherAction::ViewResults => {
            app.screen = Screen::BenchEvalResults;
        }
        EvalLauncherAction::ViewReport => {
            app.screen = Screen::BenchEvalReport;
        }
        EvalLauncherAction::Back => {
            app.screen = Screen::Launcher;
        }
    }
}

fn start_eval_sweep(app: &mut App, level: crate::runner::SweepLevel) {
    let Some(model_name) = super::bench_eval_flow::resolve_bench_eval_model(app) else {
        app.set_error("No model selected. Select or launch a model first.".into());
        return;
    };
    if app.eval_run_event_rx.is_some() {
        app.set_error("An eval run is already in progress.".into());
        return;
    }
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
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
        sweep_level: level,
        ..Default::default()
    };
    app.eval_run_event_rx = Some(rx);
    app.eval_run_stage = format!("{} starting...", level.label());
    app.eval_run_running = true;
    app.eval_run_tasks_run = 0;
    app.eval_run_tasks_passed = 0;
    app.eval_run_model = Some(model_name);
    app.eval_run_progress.clear();
    app.screen = Screen::EvalRunRunning;
    app.set_status(format!("{} started...", level.label()));
    super::eval_run_workflow::spawn_eval_run(config, tx);
}
