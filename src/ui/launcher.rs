use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use super::{App, LauncherAction, LauncherActionId, ModelPickerMode};
use crate::planner::{self, ConfigureWarningSeverity};
#[cfg(feature = "profiling-ui")]
use crate::profiling::{ProfilingAction, WarningSeverity};
use crate::theme::*;
use ratatui_braille_bar::BrailleBar;

pub(super) fn visible_launcher_actions(app: &App) -> Vec<LauncherAction> {
    launcher_actions(app)
}

pub(super) fn filtered_launcher_actions(app: &App) -> Vec<LauncherAction> {
    let query = app.command_overlay_query();
    let query = query.to_ascii_lowercase();
    visible_launcher_actions(app)
        .into_iter()
        .filter(|action| {
            query.is_empty()
                || action.command.to_ascii_lowercase().contains(&query)
                || action.label.to_ascii_lowercase().contains(&query)
                || action.description.to_ascii_lowercase().contains(&query)
        })
        .collect()
}

fn launcher_actions(_app: &App) -> Vec<LauncherAction> {
    let mut actions = vec![
        LauncherAction {
            id: LauncherActionId::Launch,
            label: "Launch".into(),
            description: "Pick a model, review config, and launch".into(),
            command: "launch",
        },
        LauncherAction {
            id: LauncherActionId::ConfigureModel,
            label: "Configure Model".into(),
            description: "Open Configure Hub before launch".into(),
            command: "configure",
        },
    ];
    #[cfg(feature = "profiling-ui")]
    actions.push(LauncherAction {
        id: LauncherActionId::ProfileModel,
        label: "Profile".into(),
        description: "Auto-tune GPU layers for a model".into(),
        command: "profile",
    });
    actions.extend([
        LauncherAction {
            id: LauncherActionId::Settings,
            label: "Settings".into(),
            description: "Configure backend defaults".into(),
            command: "settings",
        },
        LauncherAction {
            id: LauncherActionId::ClearGpu,
            label: "Clear GPU".into(),
            description: "Kill running backends".into(),
            command: "clear-gpu",
        },
        LauncherAction {
            id: LauncherActionId::Monitor,
            label: "Monitor".into(),
            description: "View system resources".into(),
            command: "monitor",
        },
        LauncherAction {
            id: LauncherActionId::Exit,
            label: "Exit".into(),
            description: "Quit launcher".into(),
            command: "exit",
        },
    ]);
    actions
}

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

pub(super) fn render_command_overlay(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let popup = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(78),
            Constraint::Fill(1),
        ])
        .split(popup)[1];

    let block = chrome_block_with_hint(
        launcher_title("Quick Command"),
        "Type to filter · ↑↓ choose · Enter run · Esc close",
        style_lime(),
    );
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Fill(1)])
        .split(inner);

    let input_block = chrome_block(
        Line::from(Span::styled(" Command ", style_bold_cyan())),
        style_gray(),
    );
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);

    let input_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(2), Constraint::Fill(1)])
        .split(input_inner);
    f.render_widget(
        Paragraph::new(Span::styled("/", style_hint_key())),
        input_layout[0],
    );
    f.render_widget(&app.command_overlay, input_layout[1]);

    let commands = filtered_launcher_actions(app);
    if commands.is_empty() {
        let empty = Paragraph::new(Line::from(vec![
            Span::styled("No launcher commands match ", style_muted()),
            Span::styled(
                format!("/{}", app.command_overlay_query()),
                style_hint_key(),
            ),
            Span::styled(".", style_muted()),
        ]))
        .block(chrome_block(
            Line::from(Span::styled(" Matches ", style_bold_cyan())),
            style_gray(),
        ));
        f.render_widget(empty, rows[1]);
        return;
    }

    let items: Vec<ListItem> = commands
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let selected = index == app.command_overlay_selected;
            let command_style = if selected {
                style_hint_key()
            } else {
                style_muted()
            };
            let label_style = if selected {
                Style::default().fg(LIME).add_modifier(Modifier::BOLD)
            } else {
                style_gray()
            };
            let marker = if selected { HEX_CURSOR } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    if selected {
                        style_lime()
                    } else {
                        style_muted()
                    },
                ),
                Span::styled(action.label.clone(), label_style),
                Span::styled(format!("  {}", action.description), style_muted()),
                Span::styled(format!("  /{}", action.command), command_style),
            ]))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(app.command_overlay_selected));
    let list = List::new(items).block(chrome_block(
        Line::from(Span::styled(" Matches ", style_bold_cyan())),
        style_gray(),
    ));
    f.render_stateful_widget(list, rows[1], &mut state);
}

fn launcher_title(section: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_lime()),
        Span::styled(section.to_string(), style_bold_cyan()),
    ])
}

fn launcher_hint(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(format!(" {}", text.into()), style_gray()))
}

fn chrome_block(title: Line<'static>, border_style: Style) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
}

fn chrome_block_with_hint(
    title: Line<'static>,
    hint: impl Into<String>,
    border_style: Style,
) -> Block<'static> {
    chrome_block(title, border_style).title_bottom(launcher_hint(hint))
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

    let tier_name = match app.prefs.preferred_tier {
        Some(crate::prefs::Tier::Lite) => "ozonelite",
        _ => "Ozone",
    };

    let title = Line::from(vec![
        Span::styled(format!(" {} {} ", HEX_CURSOR, tier_name), style_bold_lime()),
        Span::styled(format!("v{} ", VERSION), style_gray()),
        Span::styled("— ", style_gray()),
        Span::styled(format!("{model_count} models"), style_cyan()),
    ]);

    // Backend badge line
    let subtitle = Line::from(vec![
        Span::styled("  Backend: ", style_gray()),
        Span::styled("llama.cpp", style_violet()),
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

    let (llama_icon, llama_style) = if app.services.llamacpp_running {
        ("●", style_green())
    } else {
        ("○", style_gray())
    };

    let llama_model_label = app.services.llamacpp_model.as_deref().unwrap_or("—");
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("  {llama_icon} llama.cpp  "), llama_style),
            Span::styled(llama_model_label, style_violet()),
            Span::styled("  :8989", style_gray()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_actions(f: &mut Frame, area: Rect, app: &App) {
    let block = chrome_block_with_hint(
        launcher_title("Launcher"),
        "↑↓ navigate · Enter select · Esc exit · q quit",
        style_gray(),
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = launcher_actions(app)
        .iter()
        .enumerate()
        .map(|(i, action)| {
            if i == app.selected_action {
                let marker = if (app.ticker / 6).is_multiple_of(2) {
                    HEX_CURSOR
                } else {
                    HEX_FILLED
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{marker} "), style_lime()),
                    Span::styled(format!("{}", i + 1), style_gray()),
                    Span::raw("  "),
                    Span::styled(
                        action.label.clone(),
                        Style::default().fg(LIME).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {}", action.description), style_gray()),
                    Span::styled(format!("  /{}", action.command), style_hint_key()),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{}", i + 1), style_gray()),
                    Span::raw("  "),
                    Span::styled(action.label.clone(), style_gray()),
                    Span::styled(
                        format!("  {}", action.description),
                        Style::default().fg(GRAY),
                    ),
                    Span::styled(format!("  /{}", action.command), style_muted()),
                ]))
            }
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let selected_action = visible_launcher_actions(app)
        .get(app.selected_action)
        .cloned();
    let msg = app
        .status_msg
        .clone()
        .or_else(|| app.error_msg.clone())
        .or_else(|| {
            selected_action
                .as_ref()
                .map(|action| format!("{} · /{}", action.description, action.command))
        })
        .unwrap_or_default();
    let style = if app.error_msg.is_some() {
        style_red()
    } else if app.status_msg.is_some() {
        style_gray()
    } else {
        style_muted()
    };
    let tier_badge = match app.prefs.preferred_tier {
        Some(crate::prefs::Tier::Lite) => Span::styled(" [lite] ", style_cyan()),
        _ => Span::raw(" "),
    };
    let pulse = if (app.ticker / 8).is_multiple_of(2) {
        HEX_CURSOR
    } else {
        HEX_FILLED
    };
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {pulse}"), style_hint_key()),
        tier_badge,
        Span::styled(format!(" {msg}"), style),
    ]));
    f.render_widget(bar, area);
}

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
                if (app.ticker / 6).is_multiple_of(2) {
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(VIOLET)
                        .add_modifier(Modifier::BOLD)
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
        Line::from(Span::styled(
            "  Launching llama.cpp…",
            style_bold_violet(),
        )),
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
                Span::styled(plan.quant_kv.to_string(), style_cyan()),
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

    let area = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Min(5),
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
        "↑↓ field · ←→ adjust · p/n profile · l load · s save · u update · d delete · f default · b benchmark · Enter confirm",
        style_lime(),
    );
    f.render_widget(Paragraph::new(header_lines).block(header_block), outer[0]);

    let plan_panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(outer[1]);
    render_plan_summary(f, plan_panels[0], "Recommended", recommended);
    render_plan_summary(f, plan_panels[1], "Customized", effective);

    let context_selected = app.configure_field_index == 0;
    let layers_selected = app.configure_field_index == 1;
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
    ];
    let controls_block = Block::default()
        .title(Span::styled("  Controls ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    f.render_widget(
        Paragraph::new(control_lines).block(controls_block),
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
        .map(|hw| planner::build_configure_warnings(effective, hw))
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

fn render_plan_summary(f: &mut Frame, area: Rect, title: &str, plan: &planner::LaunchPlan) {
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
                        "ctx {} · gpu {} · qkv {}",
                        profile.context_size, profile.gpu_layers, profile.quant_kv
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
    planner::CONFIGURE_CONTEXT_STEPS
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
            "  Warning: this will clear the managed llama.cpp runtime before it starts.",
            style_amber(),
        )));
    }
    if matches!(action, ProfilingAction::BenchmarkSavedProfile) {
        if let Some(profile) = app
            .configure_saved_profiles
            .get(app.configure_profile_index)
        {
            lines.push(Line::from(vec![
                Span::styled("  Saved profile: ", style_gray()),
                Span::styled(&profile.profile_name, style_cyan()),
            ]));
        }
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
    if let Some(saved_profile_report) = &report.saved_profile_report {
        report_lines.push(Line::from(vec![
            Span::styled("  Saved profile: ", style_gray()),
            Span::styled(&saved_profile_report.profile_name, style_cyan()),
        ]));
        report_lines.push(Line::from(vec![
            Span::styled("  Latest tok/s: ", style_gray()),
            Span::styled(
                saved_profile_report
                    .latest_tokens_per_sec
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "—".into()),
                style_cyan(),
            ),
            Span::styled("   Best: ", style_gray()),
            Span::styled(
                saved_profile_report
                    .best_tokens_per_sec
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "—".into()),
                style_cyan(),
            ),
        ]));
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
            Constraint::Length(21),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(64),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // summary
            Constraint::Length(5), // backend block
            Constraint::Length(3), // hint
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(Span::styled(
        " tune launcher defaults before model selection",
        style_muted(),
    )))
    .block(chrome_block_with_hint(
        launcher_title("Settings"),
        "↑↓ choose · Enter save · Esc back",
        style_lime(),
    ));
    f.render_widget(header, chunks[0]);

    let summary = Paragraph::new(Line::from(vec![
        Span::styled(" backend ", style_muted()),
        Span::styled("llama.cpp", style_cyan()),
    ]))
    .block(chrome_block(
        Line::from(Span::styled(" Active Defaults ", style_bold_cyan())),
        style_gray(),
    ));
    f.render_widget(summary, chunks[1]);

    // Backend block
    let backend_block = chrome_block(
        Line::from(Span::styled(
            " Backend ",
            style_panel_title(app.settings_section == 0),
        )),
        style_panel_border(app.settings_section == 0),
    );
    let backend_inner = backend_block.inner(chunks[2]);
    f.render_widget(backend_block, chunks[2]);

    let backend_options = ["llama.cpp"];
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
                Span::styled(
                    if selected { "  selected" } else { "" },
                    if focused {
                        style_hint_key()
                    } else {
                        style_muted()
                    },
                ),
            ]))
        })
        .collect();
    f.render_widget(List::new(backend_items), backend_inner);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" choose  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" save  ", style_muted()),
        Span::styled("Esc", style_hint_key()),
        Span::styled(" back", style_muted()),
    ]))
    .block(chrome_block(
        Line::from(Span::styled(" Navigation ", style_bold_cyan())),
        style_gray(),
    ));
    f.render_widget(hint, chunks[3]);
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use super::*;
    use crate::catalog::{CatalogRecord, RecSource, Recommendation};
    use crate::prefs::{Preferences, Tier};

    fn base_app() -> App {
        App::new(Preferences {
            preferred_tier: Some(Tier::Base),
            ..Preferences::default()
        })
    }

    fn test_catalog_record(name: &str) -> CatalogRecord {
        CatalogRecord {
            model_name: name.to_owned(),
            model_path: ozone_core::paths::models_dir().join(name),
            model_size_gb: 8.0,
            recommendation: Recommendation {
                context_size: 8192,
                gpu_layers: -1,
                quant_kv: 1,
                note: "test".into(),
                source: RecSource::Heuristic,
            },
            benchmark: None,
            benchmark_count: 0,
            source_priority: RecSource::Heuristic.priority(),
        }
    }

    fn render_to_string(
        width: u16,
        height: u16,
        draw: impl FnOnce(&mut Frame, &App),
        app: &App,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        buffer_to_string(terminal.backend().buffer(), width, height)
    }

    fn buffer_to_string(buffer: &Buffer, width: u16, height: u16) -> String {
        (0..height)
            .map(|y| {
                let mut line = String::new();
                for x in 0..width {
                    line.push_str(buffer[(x, y)].symbol());
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn launcher_actions_expose_command_metadata() {
        let app = base_app();
        let actions = visible_launcher_actions(&app);

        assert!(actions.iter().any(|action| {
            action.id == LauncherActionId::ConfigureModel && action.command == "configure"
        }));
        assert!(actions
            .iter()
            .any(|action| action.id == LauncherActionId::Settings && action.command == "settings"));
    }

    #[test]
    fn launcher_actions_do_not_expose_legacy_plus_commands() {
        let app = App::new(Preferences {
            preferred_tier: Some(Tier::Lite),
            ..Preferences::default()
        });
        let actions = visible_launcher_actions(&app);

        assert!(!actions
            .iter()
            .any(|action| action.command.contains("ozone-plus")));
    }

    #[test]
    fn launcher_status_bar_uses_selected_action_metadata_when_idle() {
        let mut app = base_app();
        app.selected_action = visible_launcher_actions(&app)
            .iter()
            .position(|action| action.id == LauncherActionId::Settings)
            .unwrap();

        let rendered = render_to_string(100, 24, render, &app);

        assert!(rendered.contains("Configure backend defaults · /settings"));
    }

    #[test]
    fn settings_render_shows_normalized_summary_and_navigation() {
        let mut app = base_app();
        app.settings_backend_index = 2;

        let rendered = render_to_string(100, 24, render_settings, &app);

        assert!(rendered.contains("Active Defaults"));
        assert!(rendered.contains("Navigation"));
        assert!(rendered.contains("llama.cpp"));
    }

    #[test]
    fn filtered_launcher_actions_match_command_query() {
        let mut app = base_app();
        app.command_overlay.insert_str("sett");

        let actions = filtered_launcher_actions(&app);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, LauncherActionId::Settings);
    }

    #[test]
    fn quick_command_overlay_renders_filtered_matches() {
        let mut app = base_app();
        app.command_overlay_open = true;
        app.command_overlay.insert_str("sett");

        let rendered = render_to_string(100, 24, render_command_overlay, &app);

        assert!(rendered.contains("Quick Command"));
        assert!(rendered.contains("/settings"));
        assert!(rendered.contains("Configure backend defaults"));
    }

    #[test]
    fn quick_command_overlay_renders_empty_state() {
        let mut app = base_app();
        app.command_overlay_open = true;
        app.command_overlay.insert_str("zzz");

        let rendered = render_to_string(100, 24, render_command_overlay, &app);

        assert!(rendered.contains("No launcher commands match"));
    }

    #[test]
    fn model_picker_scrolls_selected_model_into_view() {
        let mut app = base_app();
        app.catalog = (0..30)
            .map(|index| test_catalog_record(&format!("model-{index:02}.gguf")))
            .collect();
        app.selected_model = 29;

        let rendered = render_to_string(100, 12, render_model_picker, &app);

        assert!(rendered.contains("model-29.gguf"));
        assert!(!rendered.contains("model-00.gguf"));
        assert!(rendered.contains("↑") || rendered.contains("↓"));
    }
}
