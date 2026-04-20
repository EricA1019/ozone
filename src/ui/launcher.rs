use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use ratatui_braille_bar::BrailleBar;

use super::{App, BackendMode, FrontendMode, ModelPickerMode};
#[cfg(feature = "profiling-ui")]
use crate::profiling::{ProfilingAction, WarningSeverity};
use crate::theme::*;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // header (increased for badge line)
            Constraint::Length(6), // resources
            Constraint::Length(6), // services
            Constraint::Fill(1),   // actions
            Constraint::Length(2), // status bar
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_resources(f, chunks[1], app);
    render_services(f, chunks[2], app);
    render_actions(f, chunks[3], app);
    render_status_bar(f, chunks[4], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let model_count = app.catalog.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let title = Line::from(vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_lime()),
        Span::styled(format!("v{} ", VERSION), style_gray()),
        Span::styled("— ", style_gray()),
        Span::styled(format!("{model_count} models"), style_cyan()),
    ]);

    // Backend/frontend badge line
    let (backend_label, backend_style) = match app.prefs.preferred_backend {
        Some(BackendMode::KoboldCpp) => ("KoboldCpp", style_cyan()),
        Some(BackendMode::LlamaCpp) => ("LlamaCpp", style_violet()),
        Some(BackendMode::Ollama) => ("Ollama", style_green()),
        None => ("—", style_gray()),
    };
    let (frontend_label, frontend_style) = match app.prefs.preferred_frontend {
        Some(FrontendMode::SillyTavern) => ("SillyTavern", style_cyan()),
        Some(FrontendMode::OzonePlus) => ("ozone+", style_violet()),
        None => ("—", style_gray()),
    };
    let subtitle = Line::from(vec![
        Span::styled("  Backend: ", style_gray()),
        Span::styled(backend_label, backend_style),
        Span::styled("  Frontend: ", style_gray()),
        Span::styled(frontend_label, frontend_style),
    ]);

    f.render_widget(Paragraph::new(title), text_chunks[0]);
    f.render_widget(Paragraph::new(subtitle), text_chunks[1]);
}

fn render_resources(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Resources ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // GPU label
            Constraint::Length(1), // GPU braille bar
            Constraint::Length(1), // RAM label
            Constraint::Length(1), // RAM braille bar
        ])
        .split(inner);

    if let Some(hw) = &app.hardware {
        if let Some(gpu) = &hw.gpu {
            let ratio = (gpu.used_mb as f64 / gpu.total_mb as f64).clamp(0.0, 1.0);
            let color = if ratio > 0.9 {
                RED
            } else if ratio > 0.75 {
                AMBER
            } else {
                LIME
            };
            let label = Line::from(vec![Span::styled(
                format!(
                    "  GPU VRAM  {}/{} MB  ({:.0}%)",
                    gpu.used_mb,
                    gpu.total_mb,
                    ratio * 100.0
                ),
                Style::default().fg(color),
            )]);
            f.render_widget(Paragraph::new(label), rows[0]);

            let bar = BrailleBar::new(gpu.used_mb as f64, gpu.total_mb as f64).fill_color(color);
            f.render_widget(bar, rows[1]);
        }
        let ram_ratio = (hw.ram_used_mb as f64 / hw.ram_total_mb as f64).clamp(0.0, 1.0);
        let ram_label = Line::from(vec![Span::styled(
            format!(
                "  System RAM  {}/{} MB  ({:.0}%)",
                hw.ram_used_mb,
                hw.ram_total_mb,
                ram_ratio * 100.0
            ),
            style_cyan(),
        )]);
        f.render_widget(Paragraph::new(ram_label), rows[2]);

        let ram_bar =
            BrailleBar::new(hw.ram_used_mb as f64, hw.ram_total_mb as f64).fill_color(CYAN);
        f.render_widget(ram_bar, rows[3]);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  Loading hardware…", style_gray())),
            rows[0],
        );
    }
}

fn render_services(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Services ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (kc_icon, kc_style) = if app.services.kobold_running {
        ("●", style_green())
    } else {
        ("○", style_gray())
    };
    let (st_icon, st_style) = if app.services.st_running {
        ("●", style_green())
    } else {
        ("○", style_gray())
    };
    let (ollama_icon, ollama_style) = if app.services.ollama_running {
        ("●", style_green())
    } else {
        ("○", style_gray())
    };
    let (llama_icon, llama_style) = if app.services.llamacpp_running {
        ("●", style_green())
    } else {
        ("○", style_gray())
    };

    let model_label = app.services.kobold_model.as_deref().unwrap_or("—");
    let llama_model_label = app.services.llamacpp_model.as_deref().unwrap_or("—");
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("  {kc_icon} KoboldCpp  "), kc_style),
            Span::styled(model_label, style_cyan()),
            Span::styled("  :5001", style_gray()),
        ]),
        Line::from(vec![
            Span::styled(format!("  {llama_icon} LlamaCpp   "), llama_style),
            Span::styled(llama_model_label, style_violet()),
            Span::styled("  :8080", style_gray()),
        ]),
        Line::from(vec![
            Span::styled(format!("  {ollama_icon} Ollama     "), ollama_style),
            Span::styled(":11434", style_gray()),
        ]),
        Line::from(vec![
            Span::styled(format!("  {st_icon} SillyTavern  "), st_style),
            Span::styled(":8000", style_gray()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_actions(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Actions ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  ↑↓ navigate · Enter select · q quit",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Action 2 label/desc adapts to the side_by_side_monitor preference.
    let (open_ozone_label, open_ozone_desc) = if app.prefs.side_by_side_monitor {
        (
            "Open ozone+ [new window]",
            "New terminal (side-by-side on)",
        )
    } else {
        ("Open ozone+", "Direct shell (no model needed)")
    };

    let mut actions: Vec<(&str, &str)> = vec![
        ("Launch", "Start configured backend & frontend"),
    ];
    #[cfg(feature = "profiling-ui")]
    actions.push(("Profile", "Auto-tune GPU layers for a model"));
    actions.push((open_ozone_label, open_ozone_desc));
    actions.extend([
        (
            "Launch ozone+ (side-by-side)",
            "Spawn new window, save preference",
        ),
        ("Settings", "Configure backend & frontend"),
        ("Clear GPU", "Kill running backends"),
        ("Monitor", "View system resources"),
        ("Exit", "Quit launcher"),
    ]);

    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, (label, desc))| {
            if i == app.selected_action {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", HEX_CURSOR), style_lime()),
                    Span::styled(format!("{}", i + 1), style_gray()),
                    Span::raw("  "),
                    Span::styled(
                        *label,
                        Style::default().fg(LIME).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {}", desc), style_gray()),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{}", i + 1), style_gray()),
                    Span::raw("  "),
                    Span::styled(*label, style_gray()),
                    Span::styled(format!("  {}", desc), Style::default().fg(GRAY)),
                ]))
            }
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let msg = app
        .status_msg
        .as_deref()
        .or(app.error_msg.as_deref())
        .unwrap_or("");
    let style = if app.error_msg.is_some() {
        style_red()
    } else {
        style_gray()
    };
    let bar = Paragraph::new(Line::from(Span::styled(format!("  {msg}"), style)));
    f.render_widget(bar, area);
}

pub fn render_model_picker(f: &mut Frame, app: &App) {
    let area = f.area();
    let filtered = app.filtered_catalog();
    let total = filtered.len();

    let (mode_label, hint_label) = match app.model_picker_mode {
        ModelPickerMode::Launch => (
            "Model Picker · Launch",
            " ↑↓ scroll · Enter launch plan · Esc back · type to filter",
        ),
        #[cfg(feature = "profiling-ui")]
        ModelPickerMode::Profile => (
            "Model Picker · Profile",
            " ↑↓ scroll · Enter advisory · Esc back · type to filter",
        ),
    };

    let mut title_spans = vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_lime()),
        Span::styled(mode_label, style_bold_cyan()),
    ];
    // Show active filter
    if !app.model_filter.is_empty() {
        title_spans.push(Span::styled(
            format!("  Filter: {}▏", app.model_filter),
            style_amber(),
        ));
    }
    // Scroll position [N/M]
    if total > 0 {
        title_spans.push(Span::styled(
            format!("  [{}/{}]", app.selected_model + 1, total),
            style_gray(),
        ));
    }

    let block = Block::default()
        .title(Line::from(title_spans))
        .title_bottom(Line::from(Span::styled(hint_label, style_gray())))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if filtered.is_empty() {
        let msg = if app.model_filter.is_empty() {
            &format!(
                "  No models found in {}",
                ozone_core::paths::models_dir().display()
            )
        } else {
            "  No models match filter"
        };
        f.render_widget(Paragraph::new(Span::styled(msg, style_amber())), inner);
        return;
    }

    let hw = app.hardware.as_ref();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let selected = i == app.selected_model;
            let prefix = if selected {
                format!("{} ", HEX_CURSOR)
            } else {
                "  ".to_string()
            };

            let path_ok = rec.model_path.exists();

            let plan_vram = hw.map(|_| {
                crate::planner::estimate_vram_mb(
                    rec.recommendation.context_size,
                    rec.recommendation.gpu_layers,
                    rec.model_size_gb,
                    rec.recommendation.quant_kv,
                    crate::planner::estimate_total_layers(rec.model_size_gb),
                )
            });

            let (fit_icon, fit_style) = if !path_ok {
                ("⚠", style_amber())
            } else if let (Some(vram_est), Some(hw)) = (plan_vram, hw) {
                if let Some(gpu) = &hw.gpu {
                    let budget = (gpu.free_mb as f64 * 0.9) as u32;
                    if vram_est <= budget {
                        ("✓", style_green())
                    } else if vram_est <= gpu.total_mb as u32 {
                        ("~", style_amber())
                    } else {
                        ("✗", style_red())
                    }
                } else {
                    ("?", style_gray())
                }
            } else {
                ("?", style_gray())
            };

            let source_label = rec.recommendation.source.label();
            let speed_label = rec
                .benchmark
                .as_ref()
                .map(|b| format!("{:.1} t/s", b.gen_speed))
                .unwrap_or_else(|| "— t/s".into());

            let size_label = format!("{:>5.1}G", rec.model_size_gb);

            let name = if rec.model_name.len() > 40 {
                format!("{}…", &rec.model_name[..38])
            } else {
                rec.model_name.clone()
            };

            let base_style = if selected {
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
            } else {
                style_gray()
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, if selected { style_cyan() } else { style_gray() }),
                Span::styled(format!("[{:02}] ", i + 1), style_gray()),
                Span::styled(format!("{:<42}", name), base_style),
                Span::styled(format!(" {} ", size_label), style_gray()),
                Span::styled(format!(" {:5}  ", source_label), style_gray()),
                Span::styled(fit_icon, fit_style),
                Span::styled(format!("  {:>10}", speed_label), style_gray()),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_model));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);
}

pub fn render_launching(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let model = app
        .current_plan
        .as_ref()
        .map(|p| p.model_name.as_str())
        .unwrap_or("…");
    let dots = match app.ticker % 4 {
        0 => "·  ",
        1 => "·· ",
        2 => "···",
        _ => "   ",
    };

    let lines = vec![
        Line::from(Span::styled(
            match app.prefs.preferred_backend {
                Some(BackendMode::KoboldCpp) => "  Launching KoboldCpp…",
                Some(BackendMode::LlamaCpp) => "  Launching llama.cpp…",
                Some(BackendMode::Ollama) => "  Launching Ollama…",
                None => "  Launching backend…",
            },
            style_bold_violet(),
        )),
        Line::from(Span::styled(format!("  {model}"), style_cyan())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(format!("  Loading {dots}"), style_amber())),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime())
        .title_bottom(Line::from(Span::styled("  loading…", style_gray())));
    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, center_h);
}

pub fn render_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    if let Some(plan) = &app.current_plan {
        let mode_label = plan.mode.label();
        let lines = vec![
            Line::from(Span::styled("  Confirm Launch", style_bold_violet())),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::styled("  Model:    ", style_gray()),
                Span::styled(&plan.model_name, style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  Layers:   ", style_gray()),
                Span::styled(plan.gpu_layers.to_string(), style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  Context:  ", style_gray()),
                Span::styled(plan.context_size.to_string(), style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  QuantKV:  ", style_gray()),
                Span::styled(plan.quant_kv.to_string(), style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  Mode:     ", style_gray()),
                Span::styled(mode_label, style_amber()),
            ]),
            Line::from(Span::styled(format!("  {}", plan.rationale), style_gray())),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style_lime())
            .title_bottom(Line::from(Span::styled(
                "  Enter launch · Esc cancel",
                style_gray(),
            )));
        f.render_widget(Paragraph::new(lines).block(block), center_h);
    }
}

pub fn render_frontend_choice(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(58),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let choices: &[(&str, &str)] = &[
        ("SillyTavern", "open browser to SillyTavern web UI"),
        ("ozone+", "launch ozone+ conversation shell"),
    ];
    let items: Vec<ListItem> = choices
        .iter()
        .enumerate()
        .map(|(i, (label, desc))| {
            if i == app.frontend_choice_index {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", HEX_CURSOR), style_cyan()),
                    Span::styled(
                        *label,
                        Style::default()
                            .fg(crate::theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  — {desc}"), style_gray()),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(*label, style_gray()),
                    Span::styled(format!("  — {desc}"), style_gray()),
                ]))
            }
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(
            format!(" {} Choose Frontend ", HEX_CURSOR),
            style_bold_violet(),
        ))
        .title_bottom(Line::from(Span::styled(
            "  ↑↓ choose · Enter launch · Esc back",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(center_h);
    f.render_widget(block, center_h);

    let mut list_state = ListState::default();
    list_state.select(Some(app.frontend_choice_index));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);
}

pub fn render_exit_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(9),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(54),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let yes_style = if app.exit_confirm_index == 0 {
        style_bold_lime()
    } else {
        style_gray()
    };
    let no_style = if app.exit_confirm_index == 1 {
        style_bold_cyan()
    } else {
        style_gray()
    };
    let yes_marker = if app.exit_confirm_index == 0 {
        format!("{} Quit", HEX_CURSOR)
    } else {
        "  Quit".to_string()
    };
    let no_marker = if app.exit_confirm_index == 1 {
        format!("{} Stay", HEX_CURSOR)
    } else {
        "  Stay".to_string()
    };

    let lines = vec![
        Line::from(Span::styled("  Leave Ozone?", style_bold_lime())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "  Esc from the launcher now asks before quitting.",
            style_gray(),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("  ", style_gray()),
            Span::styled(yes_marker, yes_style),
            Span::styled("     ", style_gray()),
            Span::styled(no_marker, no_style),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(
            format!(" {} Confirm Exit ", HEX_CURSOR),
            style_bold_lime(),
        ))
        .title_bottom(Line::from(Span::styled(
            "  ←→ choose · Enter confirm · Esc back",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(Paragraph::new(lines).block(block), center_h);
}

#[cfg(feature = "profiling-ui")]
fn warning_style(severity: &WarningSeverity) -> Style {
    match severity {
        WarningSeverity::Info => style_gray(),
        WarningSeverity::Warning => style_amber(),
        WarningSeverity::Critical => style_red(),
    }
}

#[cfg(feature = "profiling-ui")]
fn action_items(actions: &[ProfilingAction], selected: usize) -> (Vec<ListItem<'_>>, ListState) {
    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            if i == selected {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", HEX_CURSOR), style_cyan()),
                    Span::styled(
                        action.label(),
                        Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                    ),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(action.label(), style_gray()),
                ]))
            }
        })
        .collect();
    let mut state = ListState::default();
    if !actions.is_empty() {
        state.select(Some(selected.min(actions.len().saturating_sub(1))));
    }
    (items, state)
}

#[cfg(feature = "profiling-ui")]
pub fn render_profile_advisory(f: &mut Frame, app: &App) {
    let Some(advisory) = app.profiling_advisory.as_ref() else {
        return;
    };
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Min(4),
            Constraint::Fill(1),
            Constraint::Min(6),
        ])
        .split(area);

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("  Model: ", style_gray()),
            Span::styled(&advisory.model_name, style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Source: ", style_gray()),
            Span::styled(&advisory.source_label, style_cyan()),
            Span::styled("   Benchmarks: ", style_gray()),
            Span::styled(advisory.benchmark_count.to_string(), style_cyan()),
            Span::styled("   OK: ", style_gray()),
            Span::styled(advisory.ok_benchmark_count.to_string(), style_cyan()),
            Span::styled("   Profiles: ", style_gray()),
            Span::styled(advisory.profile_count.to_string(), style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Recommendation: ", style_gray()),
            Span::styled(advisory.recommended_action.label(), style_amber()),
        ]),
        Line::from(Span::styled(
            format!("  {}", advisory.rationale),
            style_gray(),
        )),
    ];
    let summary_block = Block::default()
        .title(Span::styled("  Profiling Advisor ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(
        Paragraph::new(summary_lines).block(summary_block),
        chunks[0],
    );

    let mut snapshot_lines = Vec::new();
    if let Some(plan) = &advisory.launch_plan {
        snapshot_lines.push(Line::from(vec![
            Span::styled("  Layers: ", style_gray()),
            Span::styled(
                format!(
                    "GPU {}/{}   CPU {}",
                    plan.gpu_layers_display(),
                    plan.total_layers,
                    plan.cpu_layers
                ),
                style_cyan(),
            ),
            Span::styled("   Source: ", style_gray()),
            Span::styled(&plan.layer_source_label, style_cyan()),
        ]));
        if let Some(vram) = advisory.estimated_vram_mb {
            if let Some(budget) = advisory.gpu_budget_mb {
                snapshot_lines.push(Line::from(vec![
                    Span::styled("  Est. VRAM: ", style_gray()),
                    Span::styled(format!("{vram} MiB"), style_cyan()),
                    Span::styled("   Safe budget: ", style_gray()),
                    Span::styled(format!("{budget} MiB"), style_cyan()),
                    Span::styled("   Est. RAM: ", style_gray()),
                    Span::styled(format!("{} MiB", plan.estimated_ram_mb), style_cyan()),
                ]));
            } else {
                snapshot_lines.push(Line::from(vec![
                    Span::styled("  Est. RAM: ", style_gray()),
                    Span::styled(format!("{} MiB", plan.estimated_ram_mb), style_cyan()),
                ]));
            }
        }
        snapshot_lines.push(Line::from(vec![
            Span::styled("  Launch plan: ", style_gray()),
            Span::styled(
                format!(
                    "{} · ctx {} · gpu {} · cpu {} · qkv {}",
                    plan.mode.label(),
                    plan.context_size,
                    plan.gpu_layers_display(),
                    plan.cpu_layers,
                    plan.quant_kv
                ),
                style_cyan(),
            ),
        ]));
    }
    if let Some(profile) = &advisory.recommended_profile {
        snapshot_lines.push(Line::from(vec![
            Span::styled("  Best profile: ", style_gray()),
            Span::styled(
                format!(
                    "{} · {:.1} t/s · ctx {}",
                    profile.profile_name, profile.tokens_per_sec, profile.context_size
                ),
                style_cyan(),
            ),
        ]));
    }
    if snapshot_lines.is_empty() {
        snapshot_lines.push(Line::from(Span::styled(
            "  No benchmark-backed launch profile is available yet.",
            style_gray(),
        )));
    }
    let snapshot_block = Block::default()
        .title(Span::styled("  Snapshot ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(
        Paragraph::new(snapshot_lines).block(snapshot_block),
        chunks[1],
    );

    let mut warning_lines: Vec<Line> = advisory
        .warnings
        .iter()
        .map(|warning| {
            Line::from(vec![
                Span::styled(
                    format!("  [{}] ", warning.severity.label()),
                    warning_style(&warning.severity),
                ),
                Span::styled(&warning.message, warning_style(&warning.severity)),
            ])
        })
        .collect();
    if warning_lines.is_empty() {
        warning_lines.push(Line::from(Span::styled("  No warnings.", style_gray())));
    }
    let warnings_block = Block::default()
        .title(Span::styled("  Warnings ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(
        Paragraph::new(warning_lines).block(warnings_block),
        chunks[2],
    );

    let actions = advisory.available_actions.clone();
    let (items, mut state) = action_items(&actions, app.profiling_choice_index);
    let actions_block = Block::default()
        .title(Span::styled("  Next Actions ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  ↑↓ choose · Enter continue · Esc back",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = actions_block.inner(chunks[3]);
    f.render_widget(actions_block, chunks[3]);
    f.render_stateful_widget(List::new(items), inner, &mut state);
}

#[cfg(feature = "profiling-ui")]
pub fn render_profile_confirm(f: &mut Frame, app: &App) {
    let Some(action) = app.profiling_pending_action.as_ref() else {
        return;
    };
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(76),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let mut lines = vec![
        Line::from(Span::styled(
            "  Confirm Profiling Step",
            style_bold_violet(),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("  Action: ", style_gray()),
            Span::styled(action.label(), style_cyan()),
        ]),
        Line::from(Span::styled(
            format!("  {}", action.description()),
            style_gray(),
        )),
    ];
    if action.clears_backends() {
        lines.push(Line::from(Span::styled(
            "  Warning: this will clear KoboldCpp/Ollama runners before it starts.",
            style_amber(),
        )));
    }
    if let Some(advisory) = &app.profiling_advisory {
        if let Some(plan) = &advisory.launch_plan {
            lines.push(Line::from(vec![
                Span::styled("  Start point: ", style_gray()),
                Span::styled(
                    format!(
                        "{} · GPU {}/{} · CPU {} · ctx {} · qkv {}",
                        plan.mode.label(),
                        plan.gpu_layers_display(),
                        plan.total_layers,
                        plan.cpu_layers,
                        plan.context_size,
                        plan.quant_kv
                    ),
                    style_cyan(),
                ),
            ]));
        }
        if let Some(warning) = advisory
            .warnings
            .iter()
            .find(|warning| warning.severity != WarningSeverity::Info)
        {
            lines.push(Line::from(Span::styled(
                format!("  Heads up: {}", warning.message),
                warning_style(&warning.severity),
            )));
        }
    }
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "  Press Enter to start · Esc to review again",
        style_gray(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(Paragraph::new(lines).block(block), center_h);
}

#[cfg(feature = "profiling-ui")]
pub fn render_profile_running(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Min(14),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(84),
            Constraint::Fill(1),
        ])
        .split(center)[1];
    let block = Block::default()
        .title(Span::styled(
            "  Profiling In Progress ",
            style_bold_violet(),
        ))
        .title_bottom(Line::from(Span::styled(
            "  Esc cancel · please wait…",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(center_h);
    f.render_widget(block, center_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Fill(1),
        ])
        .split(inner);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("  Stage: ", style_gray()),
        Span::styled(&app.profiling_progress_title, style_cyan()),
    ]));
    f.render_widget(title, chunks[0]);

    if app.profiling_progress_total > 0 {
        let current = app.profiling_progress_current;
        let total = app.profiling_progress_total;

        let bar_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(chunks[1]);

        let label = Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}/{}", current, total),
            style_cyan(),
        )]));
        f.render_widget(label, bar_rows[0]);

        let bar = BrailleBar::new(current as f64, total as f64).fill_color(CYAN);
        f.render_widget(bar, bar_rows[1]);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  Preparing…", style_gray())),
            chunks[1],
        );
    }

    let lines: Vec<Line> = if app.profiling_progress.is_empty() {
        vec![Line::from(Span::styled(
            "  Waiting for the first progress update…",
            style_gray(),
        ))]
    } else {
        app.profiling_progress
            .iter()
            .map(|line| Line::from(Span::styled(format!("  {line}"), style_gray())))
            .collect()
    };
    // Scroll so the latest line is visible
    let visible_height = chunks[2].height as usize;
    let scroll_offset = if lines.len() > visible_height {
        (lines.len() - visible_height) as u16
    } else {
        0
    };
    f.render_widget(Paragraph::new(lines).scroll((scroll_offset, 0)), chunks[2]);
}

#[cfg(feature = "profiling-ui")]
pub fn render_profile_success(f: &mut Frame, app: &App) {
    let Some(report) = app.profiling_success.as_ref() else {
        return;
    };
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Min(4),
            Constraint::Fill(1),
            Constraint::Min(6),
        ])
        .split(area);

    let mut header_lines = vec![
        Line::from(Span::styled("  Profiling Complete", style_bold_violet())),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("  Model: ", style_gray()),
            Span::styled(&report.model_name, style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Completed action: ", style_gray()),
            Span::styled(report.action.label(), style_cyan()),
        ]),
        Line::from(Span::styled(format!("  {}", report.summary), style_gray())),
    ];
    if let Some(best) = report.best_tokens_per_sec {
        header_lines.push(Line::from(vec![
            Span::styled("  Best tok/s: ", style_gray()),
            Span::styled(format!("{best:.2}"), style_cyan()),
            Span::styled("   Benchmarks: ", style_gray()),
            Span::styled(report.benchmark_count.to_string(), style_cyan()),
            Span::styled("   Profiles: ", style_gray()),
            Span::styled(report.profile_count.to_string(), style_cyan()),
        ]));
    }
    let header_block = Block::default()
        .title(Span::styled("  Success ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(Paragraph::new(header_lines).block(header_block), chunks[0]);

    let mut report_lines = Vec::new();
    if let Some(profile) = &report.recommended_profile {
        report_lines.push(Line::from(vec![
            Span::styled("  Recommended profile: ", style_gray()),
            Span::styled(
                format!(
                    "{} · ctx {} · layers {} · {:.1} t/s",
                    profile.profile_name,
                    profile.context_size,
                    profile.gpu_layers,
                    profile.tokens_per_sec
                ),
                style_cyan(),
            ),
        ]));
    } else {
        report_lines.push(Line::from(Span::styled(
            "  No launch profile exists yet for this model.",
            style_gray(),
        )));
    }
    // Export detail (b2)
    if let Some(detail) = &report.export_detail {
        report_lines.push(Line::from(vec![
            Span::styled("  Exported: ", style_gray()),
            Span::styled(detail, style_cyan()),
        ]));
    }
    for suggestion in &report.suggestions {
        report_lines.push(Line::from(vec![
            Span::styled("  → ", style_amber()),
            Span::styled(suggestion, style_gray()),
        ]));
    }
    let report_block = Block::default()
        .title(Span::styled("  Report ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(report_lines).block(report_block), chunks[1]);

    let info_block = Block::default()
        .title(Span::styled("  Review First ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  Profiles are not applied automatically.",
                style_gray(),
            )),
            Line::from(Span::styled(
                "  Choose the next step below to generate, export, or launch.",
                style_gray(),
            )),
        ])
        .block(info_block),
        chunks[2],
    );

    let actions = report.available_actions();
    let (items, mut state) = action_items(&actions, app.profiling_choice_index);
    let actions_block = Block::default()
        .title(Span::styled("  Next Actions ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  ↑↓ choose · Enter continue · Esc advisor · q launcher",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = actions_block.inner(chunks[3]);
    f.render_widget(actions_block, chunks[3]);
    if actions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No follow-up actions available. Press Esc to return.",
                style_gray(),
            )),
            inner,
        );
    } else {
        f.render_stateful_widget(List::new(items), inner, &mut state);
    }
}

#[cfg(feature = "profiling-ui")]
pub fn render_profile_failure(f: &mut Frame, app: &App) {
    let Some(report) = app.profiling_failure.as_ref() else {
        return;
    };
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Fill(1), Constraint::Min(6)])
        .split(area);

    let header_block = Block::default()
        .title(Span::styled("  Profiling Failed ", style_bold_violet()))
        .borders(Borders::ALL)
        .border_style(style_red());
    let header_lines = vec![
        Line::from(vec![
            Span::styled("  Model: ", style_gray()),
            Span::styled(&report.model_name, style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Category: ", style_gray()),
            Span::styled(report.class.title(), style_red()),
        ]),
        Line::from(vec![
            Span::styled("  Action: ", style_gray()),
            Span::styled(report.action.label(), style_cyan()),
        ]),
        Line::from(Span::styled(format!("  {}", report.detail), style_gray())),
    ];
    f.render_widget(Paragraph::new(header_lines).block(header_block), chunks[0]);

    let mut detail_lines: Vec<Line> = report
        .suggestions
        .iter()
        .map(|suggestion| {
            Line::from(vec![
                Span::styled("  → ", style_amber()),
                Span::styled(suggestion, style_gray()),
            ])
        })
        .collect();
    if let Some(path) = &report.log_path {
        detail_lines.push(Line::from(vec![
            Span::styled("  Log: ", style_gray()),
            Span::styled(path.display().to_string(), style_cyan()),
        ]));
    }
    let detail_block = Block::default()
        .title(Span::styled("  Suggestions ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(detail_lines).block(detail_block), chunks[1]);

    let actions = report.available_actions();
    let (items, mut state) = action_items(&actions, app.profiling_choice_index);
    let actions_block = Block::default()
        .title(Span::styled("  Recovery Actions ", style_bold_cyan()))
        .title_bottom(Line::from(Span::styled(
            "  ↑↓ choose · Enter retry · Esc advisor · q launcher",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = actions_block.inner(chunks[2]);
    f.render_widget(actions_block, chunks[2]);
    if actions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No automatic retry is recommended. Press Esc to return.",
                style_gray(),
            )),
            inner,
        );
    } else {
        f.render_stateful_widget(List::new(items), inner, &mut state);
    }
}

pub fn render_settings(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Min(16),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(5), // backend block
            Constraint::Length(5), // frontend block
            Constraint::Length(3), // hint
        ])
        .split(center_h);

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} ", HEX_CURSOR), style_lime()),
        Span::styled("Settings", style_bold_lime()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_lime()),
    );
    f.render_widget(header, chunks[0]);

    // Backend block
    let backend_border = if app.settings_section == 0 {
        style_lime()
    } else {
        style_gray()
    };
    let backend_title = if app.settings_section == 0 {
        style_bold_lime()
    } else {
        style_bold_cyan()
    };
    let backend_block = Block::default()
        .title(Span::styled("  Backend ", backend_title))
        .borders(Borders::ALL)
        .border_style(backend_border);
    let backend_inner = backend_block.inner(chunks[1]);
    f.render_widget(backend_block, chunks[1]);

    let backend_options = ["KoboldCpp", "LlamaCpp", "Ollama"];
    let backend_items: Vec<ListItem> = backend_options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let selected = i == app.settings_backend_index;
            let focused = app.settings_section == 0;
            let marker = if selected && focused {
                HEX_CURSOR
            } else if selected {
                "●"
            } else {
                "○"
            };
            let style = if selected && focused {
                style_bold_lime()
            } else if selected {
                style_bold_cyan()
            } else {
                style_gray()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {marker} "), style),
                Span::styled(*label, style),
            ]))
        })
        .collect();
    f.render_widget(List::new(backend_items), backend_inner);

    // Frontend block
    let frontend_border = if app.settings_section == 1 {
        style_lime()
    } else {
        style_gray()
    };
    let frontend_title = if app.settings_section == 1 {
        style_bold_lime()
    } else {
        style_bold_cyan()
    };
    let frontend_block = Block::default()
        .title(Span::styled("  Frontend ", frontend_title))
        .borders(Borders::ALL)
        .border_style(frontend_border);
    let frontend_inner = frontend_block.inner(chunks[2]);
    f.render_widget(frontend_block, chunks[2]);

    let frontend_options = ["SillyTavern", "ozone+"];
    let frontend_items: Vec<ListItem> = frontend_options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let selected = i == app.settings_frontend_index;
            let focused = app.settings_section == 1;
            let marker = if selected && focused {
                HEX_CURSOR
            } else if selected {
                "●"
            } else {
                "○"
            };
            let style = if selected {
                match (*label, focused) {
                    ("ozone+", true) => style_bold_bright_violet(),
                    ("ozone+", false) => style_violet(),
                    (_, true) => style_bold_lime(),
                    _ => style_bold_cyan(),
                }
            } else {
                style_gray()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {marker} "), style),
                Span::styled(*label, style),
            ]))
        })
        .collect();
    f.render_widget(List::new(frontend_items), frontend_inner);

    // Hint
    let hint = Paragraph::new(Line::from(Span::styled(
        "  Tab/←→ switch · ↑↓ select · Enter save · Esc back",
        style_gray(),
    )))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_gray()),
    );
    f.render_widget(hint, chunks[3]);
}
