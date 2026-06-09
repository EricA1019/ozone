use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::{bench_eval_flow::resolve_bench_eval_model, App, Screen};
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BenchEvalAction {
    ProfileModel,
    EvalGsm8k,
    EvalInstruction,
    EvalMath,
    EvalHumaneval,
    EvalMmlu,
    EvalHellaSwag,
    EvalTruthfulQA,
    EvalBbh,
    EvalCreativeWriting,
    ExportServer,
    ViewResults,
    ViewReport,
    Back,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BenchEvalEntry {
    pub action: BenchEvalAction,
    pub label: &'static str,
    pub description: &'static str,
    pub command: &'static str,
}

pub(super) fn entries() -> Vec<BenchEvalEntry> {
    let mut entries: Vec<BenchEvalEntry> = Vec::new();

    // Generate eval entries from the task registry
    for task in crate::eval::EVAL_TASKS {
        entries.push(BenchEvalEntry {
            action: eval_action_for_cli_name(task.cli_name),
            label: task.report_label,
            description: task.description,
            command: task.cli_name,
        });
    }

    // Add non-eval entries
    entries.push(BenchEvalEntry { action: BenchEvalAction::EvalCreativeWriting, label: "Eval Creative Writing", description: "Diversity & coherence probe", command: "eval-creative" });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ProfileModel, label: "Profile Model", description: "Benchmark/sweep workflow", command: "profile" });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ExportServer, label: "Export Server", description: "Generate standalone launch script", command: "export-server" });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ViewResults, label: "View Results", description: "Browse past eval/sweep/creative results", command: "results" });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ViewReport, label: "View Report", description: "Open latest eval markdown report", command: "report" });
    entries.push(BenchEvalEntry { action: BenchEvalAction::Back, label: "Back", description: "Return to launcher menu", command: "back" });
    entries
}

fn eval_action_for_cli_name(name: &str) -> BenchEvalAction {
    match name {
        "gsm8k" => BenchEvalAction::EvalGsm8k,
        "instruction" => BenchEvalAction::EvalInstruction,
        "math" => BenchEvalAction::EvalMath,
        "humaneval" => BenchEvalAction::EvalHumaneval,
        "mmlu" => BenchEvalAction::EvalMmlu,
        "hellaswag" => BenchEvalAction::EvalHellaSwag,
        "truthfulqa" => BenchEvalAction::EvalTruthfulQA,
        "bbh" => BenchEvalAction::EvalBbh,
        _ => BenchEvalAction::Back,
    }
}

pub(super) fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(15),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(f, chunks[0]);
    render_actions(f, chunks[1], app);
    render_preview(f, chunks[2], app);
    render_hints(f, chunks[3]);
}

pub(super) fn render_running(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Min(20), Constraint::Fill(1)])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(88), Constraint::Fill(1)])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} oz ", HEX_CURSOR), style_bold_lime()),
        Span::styled("Bench + Eval Running", style_bold_cyan()),
        Span::styled("  ·  subprocess output", style_muted()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_lime()),
    );
    f.render_widget(header, chunks[0]);

    let mut summary_lines = vec![Line::from(vec![
        Span::styled("  Stage: ", style_gray()),
        Span::styled(&app.bench_eval_progress_title, style_cyan()),
    ])];
    if let Some(model) = &app.bench_eval_running_model {
        summary_lines.push(Line::from(vec![
            Span::styled("  Model: ", style_gray()),
            Span::styled(model, style_cyan()),
        ]));
    }
    if let Some(preset) = &app.bench_eval_running_preset {
        summary_lines.push(Line::from(vec![
            Span::styled("  Preset: ", style_gray()),
            Span::styled(preset, style_cyan()),
        ]));
    }
    if let Some(limit) = app.bench_eval_running_limit {
        summary_lines.push(Line::from(vec![
            Span::styled("  Samples: ", style_gray()),
            Span::styled(limit.to_string(), style_cyan()),
        ]));
    }
    if let Some(command) = &app.bench_eval_running_command {
        summary_lines.push(Line::from(vec![
            Span::styled("  Command: ", style_gray()),
            Span::styled(command, style_muted()),
        ]));
    }

    let summary = Paragraph::new(summary_lines).block(
        Block::default()
            .title(Span::styled("  Running ", style_bold_cyan()))
            .borders(Borders::ALL)
            .border_style(style_gray()),
    );
    f.render_widget(summary, chunks[1]);

    let lines: Vec<Line> = if app.bench_eval_progress.is_empty() {
        vec![Line::from(Span::styled(
            "  Waiting for the first output line…",
            style_gray(),
        ))]
    } else {
        app.bench_eval_progress
            .iter()
            .map(|line| Line::from(Span::styled(format!("  {line}"), style_gray())))
            .collect()
    };

    let visible_height = chunks[2].height as usize;
    let scroll_offset = if lines.len() > visible_height {
        (lines.len() - visible_height) as u16
    } else {
        0
    };
    let log_block = Block::default()
        .title(Span::styled("  Eval Log ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  Esc/q return to menu",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(
        Paragraph::new(lines).scroll((scroll_offset, 0)).block(log_block),
        chunks[2],
    );

    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc", style_hint_key()),
        Span::styled("/q", style_hint_key()),
        Span::styled(" back to menu", style_muted()),
    ]));
    f.render_widget(hints, chunks[3]);
}

pub(super) fn render_report(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Min(20), Constraint::Fill(1)])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(100), Constraint::Fill(1)])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} oz ", HEX_CURSOR), style_bold_lime()),
        Span::styled("Bench + Eval Report", style_bold_cyan()),
        Span::styled("  ·  markdown view", style_muted()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_lime()),
    );
    f.render_widget(header, chunks[0]);

    let mut summary_lines = vec![Line::from(vec![
        Span::styled("  Report: ", style_gray()),
        Span::styled(&app.bench_eval_report_title, style_cyan()),
    ])];
    if let Some(source) = &app.bench_eval_report_source {
        summary_lines.push(Line::from(vec![
            Span::styled("  Source: ", style_gray()),
            Span::styled(source.display().to_string(), style_muted()),
        ]));
    }
    if let Some(markdown_path) = &app.bench_eval_report_markdown_path {
        summary_lines.push(Line::from(vec![
            Span::styled("  Markdown: ", style_gray()),
            Span::styled(markdown_path.display().to_string(), style_muted()),
        ]));
    }

    let summary = Paragraph::new(summary_lines).block(
        Block::default()
            .title(Span::styled("  Report Info ", style_bold_cyan()))
            .borders(Borders::ALL)
            .border_style(style_gray()),
    );
    f.render_widget(summary, chunks[1]);

    let report_text = if app.bench_eval_report_markdown.is_empty() {
        "No markdown report is available yet.".to_string()
    } else {
        app.bench_eval_report_markdown.clone()
    };
    let report_line_count = report_text.lines().count().max(1);
    let visible_height = chunks[2].height.saturating_sub(2) as usize;
    let max_scroll = report_line_count.saturating_sub(visible_height) as u16;
    let scroll = app.bench_eval_report_scroll.min(max_scroll);

    let report_block = Block::default()
        .title(Span::styled("  Markdown ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  Up/Down/PgUp/PgDn scroll · Esc/q back",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(
        Paragraph::new(report_text).scroll((scroll, 0)).block(report_block),
        chunks[2],
    );

    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc", style_hint_key()),
        Span::styled("/q back  ", style_muted()),
        Span::styled("PgUp/PgDn", style_hint_key()),
        Span::styled(" page  ", style_muted()),
        Span::styled("Home/End", style_hint_key()),
        Span::styled(" jump", style_muted()),
    ]));
    f.render_widget(hints, chunks[3]);
}

fn render_header(f: &mut Frame, area: Rect) {
    let title = Line::from(vec![
        Span::styled(format!(" {} oz ", HEX_CURSOR), style_bold_lime()),
        Span::styled("Bench + Eval", style_bold_cyan()),
        Span::styled("  ·  dedicated tuning menu", style_muted()),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(Paragraph::new(title).block(block), area);
}

fn render_actions(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Actions ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == app.bench_eval_selected;
            let marker = if selected {
                if (app.ticker / 6).is_multiple_of(2) {
                    HEX_CURSOR
                } else {
                    HEX_FILLED
                }
            } else {
                " "
            };
            let label_style = if selected {
                Style::default().fg(LIME).add_modifier(Modifier::BOLD)
            } else {
                style_gray()
            };
            let cmd_style = if selected { style_hint_key() } else { style_muted() };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    if selected { style_lime() } else { style_muted() },
                ),
                Span::styled(format!("{}", index + 1), style_gray()),
                Span::raw("  "),
                Span::styled(entry.label, label_style),
                Span::styled(format!("  {}", entry.description), style_muted()),
                Span::styled(format!("  /{}", entry.command), cmd_style),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_preview(f: &mut Frame, area: Rect, app: &App) {
    let selected = entries()
        .get(app.bench_eval_selected)
        .copied()
        .unwrap_or(entries()[0]);

        let resolved_model = resolve_bench_eval_model(app);
        let model_hint = resolved_model.as_deref().unwrap_or("<MODEL>");

    let preview = match selected.action {
        BenchEvalAction::ProfileModel => {
            "Enter opens model picker with profiling workflow (bench/sweep/analyze).".to_string()
        }
        BenchEvalAction::EvalGsm8k => {
            format!("oz eval {model_hint} --preset gsm8k --limit 1")
        }
        BenchEvalAction::EvalInstruction => {
            format!("oz eval {model_hint} --preset instruction --limit 1")
        }
        BenchEvalAction::EvalMath => {
            format!("oz eval {model_hint} --preset math --limit 1")
        }
        BenchEvalAction::EvalHumaneval => {
            format!("oz eval {model_hint} --preset humaneval --limit 1")
        }
        BenchEvalAction::EvalMmlu => {
            format!("oz eval {model_hint} --preset mmlu --limit 1")
        }
        BenchEvalAction::EvalHellaSwag => {
            format!("oz eval {model_hint} --preset hellaswag --limit 1")
        }
        BenchEvalAction::EvalTruthfulQA => {
            format!("oz eval {model_hint} --preset truthfulqa --limit 1")
        }
        BenchEvalAction::EvalBbh => {
            format!("oz eval {model_hint} --preset bbh --limit 1")
        }
        BenchEvalAction::EvalCreativeWriting => {
            format!("oz eval {model_hint} --preset creative-writing --limit 3 --temperature 0.7")
        }
        BenchEvalAction::ExportServer => {
            format!("oz export-server {model_hint}")
        }
        BenchEvalAction::ViewResults => {
            let count = app.bench_eval_results_files.len();
            if count > 0 {
                format!("Browse {count} result files from past runs")
            } else {
                "Scan disk for eval/sweep/creative results and view them.".to_string()
            }
        }
        BenchEvalAction::ViewReport => {
            if let Some(path) = &app.bench_eval_report_markdown_path {
                format!("Open markdown report at {}", path.display())
            } else {
                "No markdown report has been generated yet.".to_string()
            }
        }
        BenchEvalAction::Back => "Return to launcher menu.".to_string(),
    };

    let status_line = if app.bench_eval_event_rx.is_some() {
        "Evaluation running in the background…".to_string()
    } else if app.screen == Screen::BenchEval {
        app.status_msg
            .as_deref()
            .or(app.error_msg.as_deref())
            .unwrap_or("Ready")
            .to_string()
    } else {
        "Ready".to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("  Preview: ", style_gray()),
            Span::styled(preview, style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Status: ", style_gray()),
            Span::styled(status_line, style_muted()),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled("  Command Preview ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_hints(f: &mut Frame, area: Rect) {
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("m", style_hint_key()),
        Span::styled(" model  ", style_muted()),
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" choose  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" run/open  ", style_muted()),
        Span::styled("Esc", style_hint_key()),
        Span::styled(" back  ", style_muted()),
        Span::styled("q", style_hint_key()),
        Span::styled(" exit", style_muted()),
    ]));
    f.render_widget(hints, area);
}

// ── Results Viewer ──

pub(super) fn render_results(f: &mut Frame, app: &App) {
    let area = f.area();

    if app.bench_eval_results_viewing {
        render_results_content(f, area, app);
    } else {
        render_results_list(f, area, app);
    }
}

fn render_results_list(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} oz ", HEX_CURSOR), style_bold_lime()),
        Span::styled("Results", style_bold_cyan()),
        Span::styled("  ·  past eval / sweep / creative runs", style_muted()),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(style_lime()));
    f.render_widget(header, chunks[0]);

    // File list
    let block = Block::default()
        .title(Span::styled("  Result Files ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(chunks[1]);
    f.render_widget(block, chunks[1]);

    if app.bench_eval_results_files.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  No result files found. Run an eval, sweep, or creative-write first.",
            style_gray(),
        )));
        f.render_widget(empty, inner);
    } else {
        let items: Vec<ListItem> = app.bench_eval_results_files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let selected = i == app.bench_eval_results_selected;
                let marker = if selected {
                    if (app.ticker / 6).is_multiple_of(2) { HEX_CURSOR } else { HEX_FILLED }
                } else { " " };
                let label_style = if selected { Style::default().fg(LIME).add_modifier(Modifier::BOLD) } else { style_gray() };
                let kind_color = match file.kind {
                    crate::ui::ResultFileKind::Sweep => style_cyan(),
                    crate::ui::ResultFileKind::Eval => style_lime(),
                    crate::ui::ResultFileKind::CreativeWriting => style_violet(),
                    crate::ui::ResultFileKind::Report => style_lime(),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{marker} "), if selected { style_lime() } else { style_muted() }),
                    Span::styled(format!("{:>2} ", i + 1), style_gray()),
                    Span::styled(format!("[{}] ", file.kind.label()), kind_color),
                    Span::styled(&file.model, label_style),
                    Span::styled("  ", style_muted()),
                    Span::styled(&file.summary, style_muted()),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), inner);
    }

    // Preview of selected file
    let preview_text = if let Some(file) = app.bench_eval_results_files.get(app.bench_eval_results_selected) {
        format!("  {}  |  {}  |  {}", file.kind.label(), file.model, file.path.display())
    } else {
        "  Select a file to preview".into()
    };
    let preview = Paragraph::new(Line::from(vec![
        Span::styled("  File: ", style_gray()),
        Span::styled(preview_text, style_muted()),
    ]))
    .block(Block::default().title(Span::styled("  Preview ", style_bold_cyan())).borders(Borders::ALL).border_style(style_gray()));
    f.render_widget(preview, chunks[2]);

    // Hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" choose  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" view file  ", style_muted()),
        Span::styled("r", style_hint_key()),
        Span::styled(" refresh  ", style_muted()),
        Span::styled("Esc/q", style_hint_key()),
        Span::styled(" back", style_muted()),
    ]));
    f.render_widget(hints, chunks[3]);
}

fn render_results_content(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(area);

    // Header
    let header_title = if let Some(file) = app.bench_eval_results_files.get(app.bench_eval_results_selected) {
        format!("  {}  |  {}", file.kind.label(), file.path.file_name().and_then(|n| n.to_str()).unwrap_or("?"))
    } else {
        "  Result File".into()
    };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} oz ", HEX_CURSOR), style_bold_lime()),
        Span::styled("Result Viewer", style_bold_cyan()),
        Span::styled("  ·  ", style_muted()),
        Span::styled(&header_title, style_cyan()),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(style_lime()));
    f.render_widget(header, chunks[0]);

    // Sub-header with file path
    let sub = Paragraph::new(Line::from(vec![
        Span::styled("  Path: ", style_gray()),
        Span::styled(
            app.bench_eval_results_files.get(app.bench_eval_results_selected)
                .map(|f| f.path.display().to_string())
                .unwrap_or_default(),
            style_muted(),
        ),
    ]));
    f.render_widget(sub, chunks[1]);

    // Content
    let content = &app.bench_eval_results_content;
    let line_count = content.lines().count().max(1);
    let visible = chunks[2].height.saturating_sub(2) as usize;
    let max_scroll = line_count.saturating_sub(visible) as u16;
    let scroll = app.bench_eval_results_scroll.min(max_scroll);

    let content_block = Block::default()
        .title(Span::styled("  Contents ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled("  ↑↓/PgUp/PgDn scroll · Esc/q back to list", style_gray())))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(
        Paragraph::new(content.as_str()).scroll((scroll, 0)).block(content_block),
        chunks[2],
    );

    // Hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc/q", style_hint_key()),
        Span::styled(" back  ", style_muted()),
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" scroll  ", style_muted()),
        Span::styled("PgUp/PgDn", style_hint_key()),
        Span::styled(" page", style_muted()),
    ]));
    f.render_widget(hints, chunks[3]);
}
