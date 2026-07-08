//! Secondary screen rendering for the launcher — model picker, launch, confirm,
//! configure hub, plan summaries, and saved profiles.
//!
//! Extracted from `launcher.rs`.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use super::{App, ModelPickerMode};
use crate::launch_config;
use crate::launch_config::ConfigureWarningSeverity;
use crate::theme::*;

// Re-use helpers from the parent launcher module.
use super::launcher::{chrome_block_with_hint, launcher_hint, launcher_title};
pub fn render_model_picker(f: &mut Frame, app: &App) {
    let area = f.area();
    let filtered = app.filtered_catalog();
    let total = filtered.len();

    let (mode_label, hint_label) = match app.model_picker_mode {
        ModelPickerMode::Launch => (
            "Model Picker · Launch",
            "↑↓ scroll · Enter configure hub · Esc back · type to filter",
        ),
        ModelPickerMode::Configure => (
            "Model Picker · Configure",
            "↑↓ scroll · Enter configure hub · Esc back · type to filter",
        ),
        ModelPickerMode::BenchEval => (
            "Model Picker · Bench+Eval",
            "↑↓ scroll · Enter select model · Esc back · type to filter",
        ),
        ModelPickerMode::EvalLauncher => (
            "Model Picker · Evals",
            "↑↓ scroll · Enter select model · Esc back · type to filter",
        ),
        #[cfg(feature = "profiling-ui")]
        ModelPickerMode::Profile => (
            "Model Picker · Profile",
            "↑↓ scroll · Enter advisory · Esc back · type to filter",
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

    let block = chrome_block_with_hint(Line::from(title_spans), hint_label, style_lime());
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
    let visible_count = inner.height as usize;
    let scroll_offset = model_picker_scroll_offset(total, visible_count, app.selected_model);

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_count)
        .map(|(i, rec)| {
            let selected = i == app.selected_model;
            let prefix = if selected {
                format!("{} ", HEX_CURSOR)
            } else {
                "  ".to_string()
            };

            let path_ok = rec.model_path.exists();

            let plan_vram = hw.map(|_| {
                crate::launch_config::estimate_vram_mb(
                    rec.recommendation.context_size,
                    rec.recommendation.gpu_layers,
                    rec.model_size_gb,
                    rec.recommendation.quant_k,
                    rec.recommendation.quant_v,
                    crate::launch_config::estimate_total_layers(rec.model_size_gb),
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
                if (app.ticker / 6).is_multiple_of(2) {
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(VIOLET).add_modifier(Modifier::BOLD)
                }
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
    list_state.select(Some(app.selected_model.saturating_sub(scroll_offset)));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);

    if total > visible_count && visible_count > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(total).position(scroll_offset);
        f.render_stateful_widget(
            scrollbar,
            inner.inner(Margin {
                vertical: 0,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}


fn model_picker_scroll_offset(total: usize, visible_count: usize, selected: usize) -> usize {
    if visible_count == 0 || total <= visible_count {
        return 0;
    }
    selected
        .saturating_sub(visible_count.saturating_sub(1))
        .min(total.saturating_sub(visible_count))
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
        Line::from(Span::styled("  Launching llama.cpp…", style_bold_violet())),
        Line::from(Span::styled(format!("  {model}"), style_cyan())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(format!("  Loading {dots}"), style_amber())),
    ];
    let block = Block::default()
        .title(launcher_title("Launching"))
        .borders(Borders::ALL)
        .border_style(style_lime())
        .title_bottom(launcher_hint("loading…"));
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
                Span::styled(plan.quant_k.to_string(), style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  Mode:     ", style_gray()),
                Span::styled(mode_label, style_amber()),
            ]),
            Line::from(Span::styled(format!("  {}", plan.rationale), style_gray())),
        ];
        let block = chrome_block_with_hint(
            launcher_title("Confirm Launch"),
            "Enter launch · Esc back",
            style_lime(),
        );
        f.render_widget(Paragraph::new(lines).block(block), center_h);
    }
}

pub fn render_configure_hub(f: &mut Frame, app: &App) {
    let Some(recommended) = app.configure_recommended_plan.as_ref() else {
        return;
    };
    let Some(effective) = app.current_plan.as_ref() else {
        return;
    };

    // Build control lines first so we can size the panel dynamically
    let context_selected = app.configure_field_index == 0;
    let layers_selected = app.configure_field_index == 1;
    let quant_k_selected = app.configure_field_index == 2;
    let quant_v_selected = app.configure_field_index == 3;
    let threads_selected = app.configure_field_index == 4;
    let batch_selected = app.configure_field_index == 5;
    let quant_k_label = match effective.quant_k {
        1 => "f16 (default)",
        2 => "q8_0",
        3 => "q4_0",
        _ => "unknown",
    };
    let quant_v_label = match effective.quant_v {
        1 => "f16 (default)",
        2 => "q8_0",
        3 => "q4_0",
        _ => "unknown",
    };

    let control_lines = vec![
        Line::from(vec![
            Span::styled(
                if context_selected {
                    format!("{HEX_CURSOR} Context")
                } else {
                    "  Context".into()
                },
                if context_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(context_step_label(effective.context_size), style_amber()),
        ]),
        Line::from(vec![
            Span::styled(
                if layers_selected {
                    format!("{HEX_CURSOR} GPU layers")
                } else {
                    "  GPU layers".into()
                },
                if layers_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(
                format!(
                    "{} GPU / {} CPU / {} total",
                    effective.gpu_layers_display(),
                    effective.cpu_layers,
                    effective.total_layers
                ),
                style_amber(),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                if quant_k_selected {
                    format!("{HEX_CURSOR} K cache")
                } else {
                    "  K cache".into()
                },
                if quant_k_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(quant_k_label, style_amber()),
        ]),
        Line::from(vec![
            Span::styled(
                if quant_v_selected {
                    format!("{HEX_CURSOR} V cache")
                } else {
                    "  V cache".into()
                },
                if quant_v_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(quant_v_label, style_amber()),
        ]),
        Line::from(vec![
            Span::styled(
                if threads_selected {
                    format!("{HEX_CURSOR} Threads")
                } else {
                    "  Threads".into()
                },
                if threads_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(format!("{}", effective.threads.unwrap_or(crate::launch_config::DEFAULT_THREADS)), style_amber()),
        ]),
        Line::from(vec![
            Span::styled(
                if batch_selected {
                    format!("{HEX_CURSOR} Batch threads")
                } else {
                    "  Batch threads".into()
                },
                if batch_selected {
                    style_bold_cyan()
                } else {
                    style_gray()
                },
            ),
            Span::styled("  ", style_gray()),
            Span::styled(
                format!("{}", effective.blas_threads.unwrap_or(crate::launch_config::DEFAULT_THREADS)),
                style_amber(),
            ),
        ]),
    ];
    // Dynamically size the controls panel: lines + 2 for border, but never
    // smaller than 4 to keep the panel visible on tiny terminals.
    let controls_height = (control_lines.len() as u16 + 2).max(4);

    let area = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(controls_height),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .split(area);

    let header_lines = vec![
        Line::from(Span::styled("  Configure Hub", style_bold_violet())),
        Line::from(vec![
            Span::styled("  Model: ", style_gray()),
            Span::styled(&effective.model_name, style_cyan()),
        ]),
        Line::from(Span::styled(
            "  Review the recommended profile, then tune context and GPU/CPU split.",
            style_gray(),
        )),
    ];
    let header_block = chrome_block_with_hint(
        launcher_title("Configure Hub"),
        "↑↓ field · ←→ adjust · 1-9 profile · p/n cycle · l load · s save · u update · d delete · f default · b benchmark · Enter confirm",
        style_lime(),
    );
    f.render_widget(Paragraph::new(header_lines).block(header_block), outer[0]);

    let plan_panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(outer[1]);
    render_plan_summary(f, plan_panels[0], "Recommended", recommended);
    render_plan_summary(f, plan_panels[1], "Customized", effective);

    let controls_block = Block::default()
        .title(Span::styled("  Controls ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    // Scroll so the selected field stays visible when there are more lines
    // than the panel can show (e.g. on small terminals)
    let visible_rows = outer[2].height.saturating_sub(2) as usize;
    let selected = app.configure_field_index;
    let scroll_offset = if control_lines.len() > visible_rows && visible_rows > 0 {
        // Keep the selected line in the visible window
        let max_offset = control_lines.len() - visible_rows;
        if selected > max_offset {
            max_offset
        } else if selected > visible_rows / 2 {
            selected.saturating_sub(visible_rows / 2)
        } else {
            0
        }
    } else {
        0
    };
    f.render_widget(
        Paragraph::new(control_lines)
            .block(controls_block)
            .scroll((scroll_offset as u16, 0)),
        outer[2],
    );

    let profile_panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(outer[3]);
    render_saved_profiles_panel(f, profile_panels[0], app);
    render_saved_profile_report_panel(f, profile_panels[1], app);

    let estimate_lines = vec![
        Line::from(vec![
            Span::styled("  Estimated VRAM: ", style_gray()),
            Span::styled(format!("{} MiB", effective.estimated_vram_mb), style_cyan()),
            Span::styled("  Estimated RAM: ", style_gray()),
            Span::styled(format!("{} MiB", effective.estimated_ram_mb), style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  Mode: ", style_gray()),
            Span::styled(effective.mode.label(), style_amber()),
            Span::styled("  Layer source: ", style_gray()),
            Span::styled(&effective.layer_source_label, style_gray()),
        ]),
        Line::from(Span::styled(
            format!("  {}", effective.rationale),
            style_gray(),
        )),
    ];
    let estimate_block = Block::default()
        .title(Span::styled("  Effective Plan ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(
        Paragraph::new(estimate_lines).block(estimate_block),
        outer[4],
    );

    let warnings = app
        .hardware
        .as_ref()
        .map(|hw| launch_config::build_configure_warnings(effective, hw))
        .unwrap_or_default();
    let warning_lines = if warnings.is_empty() {
        vec![Line::from(Span::styled(
            "  No active warnings. This profile fits the current heuristic budget.",
            style_green(),
        ))]
    } else {
        warnings
            .into_iter()
            .map(|warning| {
                let label = match warning.severity {
                    ConfigureWarningSeverity::Info => "INFO",
                    ConfigureWarningSeverity::Warning => "WARN",
                    ConfigureWarningSeverity::Critical => "RISK",
                };
                Line::from(vec![
                    Span::styled(
                        format!("  [{label}] "),
                        configure_warning_style(warning.severity),
                    ),
                    Span::styled(warning.message, style_gray()),
                ])
            })
            .collect()
    };
    let warning_block = Block::default()
        .title(Span::styled("  Warnings ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(warning_lines).block(warning_block), outer[5]);
}

fn render_plan_summary(f: &mut Frame, area: Rect, title: &str, plan: &launch_config::LaunchPlan) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  Context: ", style_gray()),
            Span::styled(plan.context_size.to_string(), style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  GPU layers: ", style_gray()),
            Span::styled(plan.gpu_layers_display().to_string(), style_cyan()),
        ]),
        Line::from(vec![
            Span::styled("  CPU layers: ", style_gray()),
            Span::styled(plan.cpu_layers.to_string(), style_cyan()),
        ]),
    ];
    let block = Block::default()
        .title(Span::styled(format!("  {title} "), style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_saved_profiles_panel(f: &mut Frame, area: Rect, app: &App) {
    let default_name = app.current_plan.as_ref().and_then(|plan| {
        app.prefs
            .default_saved_launch_profile_name_for(&plan.model_name)
    });
    let lines = if app.configure_saved_profiles.is_empty() {
        vec![Line::from(Span::styled(
            "  No saved launch profiles yet. Press S to save the current config.",
            style_gray(),
        ))]
    } else {
        app.configure_saved_profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let selected = index == app.configure_profile_index;
                let marker = if selected {
                    format!("{HEX_CURSOR} ")
                } else {
                    "  ".to_string()
                };
                let default_badge = if default_name == Some(profile.profile_name.as_str()) {
                    " ★"
                } else {
                    ""
                };
                Line::from(vec![
                    Span::styled(marker, if selected { style_cyan() } else { style_gray() }),
                    Span::styled(
                        format!(
                            "{}{}  {}k  gpu {}",
                            profile.profile_name,
                            default_badge,
                            profile.context_size / 1024,
                            profile.gpu_layers
                        ),
                        if selected {
                            style_bold_cyan()
                        } else {
                            style_gray()
                        },
                    ),
                ])
            })
            .collect()
    };
    let block = Block::default()
        .title(Span::styled("  Saved Profiles ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_saved_profile_report_panel(f: &mut Frame, area: Rect, app: &App) {
    let lines = if let Some(profile) = app
        .configure_saved_profiles
        .get(app.configure_profile_index)
    {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("  Selected: ", style_gray()),
                Span::styled(&profile.profile_name, style_cyan()),
            ]),
            Line::from(vec![
                Span::styled("  Config: ", style_gray()),
                Span::styled(
                    format!(
                        "ctx {} · gpu {} · K=q{} V=q{}",
                        profile.context_size, profile.gpu_layers, profile.quant_k, profile.quant_v
                    ),
                    style_cyan(),
                ),
            ]),
        ];
        #[cfg(feature = "profiling-ui")]
        {
            if let Some(report) = app.configure_profile_reports.get(&profile.profile_name) {
                lines.push(Line::from(vec![
                    Span::styled("  Benchmarks: ", style_gray()),
                    Span::styled(report.benchmark_count.to_string(), style_cyan()),
                    Span::styled("   OK: ", style_gray()),
                    Span::styled(report.ok_benchmark_count.to_string(), style_cyan()),
                ]));
                if let Some(latest) = report.latest_tokens_per_sec {
                    lines.push(Line::from(vec![
                        Span::styled("  Latest tok/s: ", style_gray()),
                        Span::styled(format!("{latest:.2}"), style_cyan()),
                        Span::styled("   Best: ", style_gray()),
                        Span::styled(
                            report
                                .best_tokens_per_sec
                                .map(|value| format!("{value:.2}"))
                                .unwrap_or_else(|| "—".into()),
                            style_cyan(),
                        ),
                    ]));
                }
                if let Some(ttft) = report.latest_time_to_first_token_ms {
                    lines.push(Line::from(vec![
                        Span::styled("  Latest TTFT: ", style_gray()),
                        Span::styled(format!("{ttft} ms"), style_cyan()),
                    ]));
                }
                if let Some(vram) = report.latest_vram_peak_mb {
                    lines.push(Line::from(vec![
                        Span::styled("  Latest VRAM/RAM: ", style_gray()),
                        Span::styled(
                            format!(
                                "{} / {} MiB",
                                vram,
                                report.latest_ram_peak_mb.unwrap_or_default()
                            ),
                            style_cyan(),
                        ),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "  No benchmark report yet. Press B to profile this saved config.",
                    style_gray(),
                )));
            }
        }
        #[cfg(not(feature = "profiling-ui"))]
        lines.push(Line::from(Span::styled(
            "  Profiling UI is not enabled in this build.",
            style_gray(),
        )));
        lines
    } else {
        vec![Line::from(Span::styled(
            "  Select or save a profile to inspect benchmark results.",
            style_gray(),
        ))]
    };
    let block = Block::default()
        .title(Span::styled("  Profile Report ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn context_step_label(current: u32) -> String {
    launch_config::CONFIGURE_CONTEXT_STEPS
        .iter()
        .map(|step| {
            if *step == current {
                format!("[{}]", format_context_step(*step))
            } else {
                format_context_step(*step)
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn format_context_step(step: u32) -> String {
    format!("{}k", step / 1024)
}

fn configure_warning_style(severity: ConfigureWarningSeverity) -> Style {
    match severity {
        ConfigureWarningSeverity::Info => style_gray(),
        ConfigureWarningSeverity::Warning => style_amber(),
        ConfigureWarningSeverity::Critical => style_red(),
    }
}
