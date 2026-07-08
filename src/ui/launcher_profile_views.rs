//! Profile view rendering for the launcher — extracted from `launcher.rs`.
//!
//! All functions here are behind `#[cfg(feature = "profiling-ui")]` and
//! are re-exported into `launcher.rs` via `use super::launcher_profile_views::*`.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use super::App;
use crate::profiling::{ProfilingAction, WarningSeverity};
use crate::theme::*;

fn launcher_title(section: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_lime()),
        Span::styled(section.to_string(), style_bold_cyan()),
    ])
}

fn warning_style(severity: &WarningSeverity) -> Style {
    match severity {
        WarningSeverity::Info => style_gray(),
        WarningSeverity::Warning => style_amber(),
        WarningSeverity::Critical => style_red(),
    }
}

fn action_items(actions: &[ProfilingAction], selected: usize) -> (Vec<ListItem<'_>>, ListState) {
    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            if i == selected {
                ListItem::new(format!("  ◉ {}", action.label()))
                    .style(Style::default().fg(style_cyan().fg.unwrap_or(ratatui::style::Color::Reset)))
            } else {
                ListItem::new(format!("  ○ {}", action.label()))
            }
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected));
    (items, state)
}

pub fn render_profile_advisory(f: &mut Frame, app: &App) {
    let Some(ref advisory) = app.profiling.advisory else { return };
    let area = f.area();
    f.render_widget(Clear, area);

    let header = launcher_title(&format!("Profile — {}", advisory.model_name));
    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(style_amber());
    let inner = block.inner(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3 + advisory.warnings.len() as u16 * 2),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(inner);
    f.render_widget(block, area);

    // -- Source & model info --
    let benchmark_count = advisory.benchmark_count.to_string();
    let profile_count = advisory.profile_count.to_string();
    let info_line = Line::from(vec![
        Span::styled("Source: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(&advisory.source_label),
        Span::raw("  |  "),
        Span::styled("Benchmarks: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(benchmark_count),
        Span::raw(" passed: "),
        Span::styled(advisory.ok_benchmark_count.to_string(), style_green()),
        Span::raw("  |  "),
        Span::styled("Profiles: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(profile_count),
    ]);
    f.render_widget(Paragraph::new(info_line), chunks[0]);

    // -- Warnings --
    let warning_lines: Vec<Line> = advisory
        .warnings
        .iter()
        .map(|w| {
            let icon = match w.severity {
                WarningSeverity::Info => "ℹ",
                WarningSeverity::Warning => "⚠",
                WarningSeverity::Critical => "✖",
            };
            Line::from(vec![
                Span::styled(format!(" {} ", icon), warning_style(&w.severity)),
                Span::styled(&w.message, warning_style(&w.severity)),
            ])
        })
        .collect();
    let warning_block = Block::default()
        .title(" Warnings ")
        .borders(Borders::TOP)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(warning_lines).block(warning_block), chunks[1]);

    // -- Actions --
    let (items, state) = action_items(&advisory.available_actions, app.profiling.choice_index);
    let action_list = List::new(items)
        .block(
            Block::default()
                .title(" Actions ")
                .borders(Borders::TOP)
                .border_style(style_cyan()),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(action_list, chunks[2], &mut state.clone());

    // -- VRAM / budget hint --
    let hint = if let (Some(estimated), Some(budget)) =
        (advisory.estimated_vram_mb, advisory.gpu_budget_mb)
    {
        let pct = if budget > 0 {
            estimated as f64 / budget as f64 * 100.0
        } else {
            0.0
        };
        let style = if pct > 95.0 {
            style_red()
        } else if pct > 80.0 {
            style_amber()
        } else {
            style_green()
        };
        Line::from(vec![
            Span::styled("VRAM: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} MiB / {} MiB ({:.0}%)", estimated, budget, pct), style),
        ])
    } else if let Some(estimated) = advisory.estimated_vram_mb {
        Line::from(vec![
            Span::styled("VRAM: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} MiB (estimated)", estimated)),
        ])
    } else {
        Line::from(Span::raw(""))
    };
    f.render_widget(Paragraph::new(hint), chunks[3]);
}

pub fn render_profile_confirm(f: &mut Frame, app: &App) {
    let Some(ref report) = app.profiling.success else { return };
    let area = f.area();
    f.render_widget(Clear, area);

    let header = launcher_title(&format!("Profile — {}", report.model_name));
    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(style_green());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(6),
            Constraint::Length(8),
        ])
        .split(inner);

    // -- Summary --
    let summary_lines = vec![
        Line::from(Span::styled(&report.summary, style_cyan())),
        Line::from(""),
        Line::from(vec![
            Span::styled("Benchmarks: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} ok / {} total", report.ok_benchmark_count, report.benchmark_count)),
        ]),
    ];
    f.render_widget(Paragraph::new(summary_lines), chunks[0]);

    // -- Recommended profile --
    let available_actions = report.available_actions();
    let (items, state) = action_items(&available_actions, app.profiling.choice_index);
    let action_list = List::new(items)
        .block(
            Block::default()
                .title(" Recommended Actions ")
                .borders(Borders::TOP)
                .border_style(style_cyan()),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(action_list, chunks[2], &mut state.clone());
}

pub fn render_profile_running(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let header = launcher_title("Profiling in progress…");
    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(style_amber());
    f.render_widget(block, area);

    let inner = area;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(inner);

    let title = &app.profiling.progress_title;
    f.render_widget(Paragraph::new(Line::from(Span::styled(title, style_cyan()))), chunks[0]);

    if app.profiling.progress_total > 0 {
        let pct = app.profiling.progress_current as f64 / app.profiling.progress_total as f64;
        let bar_width = (chunks[1].width as f64 * pct) as u16;
        let bar = "█".repeat(bar_width as usize);
        let label = format!("{}/{}", app.profiling.progress_current, app.profiling.progress_total);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(bar),
                Span::raw(format!(" {label}")),
            ])),
            chunks[1],
        );
    }

    let progress_lines: Vec<Line> = app
        .profiling.progress
        .iter()
        .map(|s| Line::from(Span::raw(s)))
        .collect();
    f.render_widget(Paragraph::new(progress_lines), chunks[2]);
}

pub fn render_profile_success(f: &mut Frame, app: &App) {
    let Some(ref report) = app.profiling.success else { return };
    let area = f.area();
    f.render_widget(Clear, area);

    let header = launcher_title(&format!("Profile — {} ✓", report.model_name));
    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(style_green());
    f.render_widget(block, area);

    let inner = area;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(inner);

    let summary = format!(
        "{} — {} benchmarks, {} profiles",
        report.summary, report.benchmark_count, report.profile_count
    );
    f.render_widget(Paragraph::new(Line::from(Span::styled(summary, style_cyan()))), chunks[0]);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(ref best) = report.best_tokens_per_sec {
        lines.push(Line::from(vec![
            Span::styled("Best speed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{best:.1} tok/s")),
        ]));
    }
    if let Some(ref rec) = report.recommended_profile {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Recommended:", Style::default().add_modifier(Modifier::BOLD))));
        lines.push(Line::from(format!(
            "  {}  ctx={}  gpu={}  K=q{}  V=q{}",
            rec.profile_name, rec.context_size, rec.gpu_layers, rec.quant_k, rec.quant_v
        )));
    }
    for suggestion in &report.suggestions {
        lines.push(Line::from(Span::styled(suggestion, style_gray())));
    }
    f.render_widget(Paragraph::new(lines).block(
        Block::default().borders(Borders::TOP).border_style(style_gray())
    ), chunks[1]);

    let hint = Line::from(Span::styled(
        "Press Tab to cycle actions, Enter to select, Esc to go back",
        style_gray(),
    ));
    f.render_widget(Paragraph::new(hint), chunks[2]);
}

pub fn render_profile_failure(f: &mut Frame, app: &App) {
    let Some(ref report) = app.profiling.failure else { return };
    let area = f.area();
    f.render_widget(Clear, area);

    let header = launcher_title(&format!("Profile — {} ✗", report.model_name));
    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(style_red());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&report.detail, style_red()))),
        chunks[0],
    );

    let suggestion_lines: Vec<Line> = report
        .suggestions
        .iter()
        .map(|s| Line::from(Span::styled(s, style_amber())))
        .collect();
    f.render_widget(Paragraph::new(suggestion_lines), chunks[1]);

    let available_actions = report.available_actions();
    let (items, state) = action_items(&available_actions, app.profiling.choice_index);
    let action_list = List::new(items)
        .block(
            Block::default()
                .title(" Actions ")
                .borders(Borders::TOP)
                .border_style(style_cyan()),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(action_list, chunks[2], &mut state.clone());
}
