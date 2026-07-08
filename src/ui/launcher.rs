use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph,
    },
    Frame,
};

use super::{App, LauncherAction, LauncherActionId};
use crate::theme::*;

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
        LauncherAction {
            id: LauncherActionId::BenchLauncher,
            label: "Benchmarks".into(),
            description: "Profile models, sweep configs & export".into(),
            command: "benchmarks",
        },
        LauncherAction {
            id: LauncherActionId::EvalLauncher,
            label: "Evals".into(),
            description: "Run eval sweeps, creative probes & view results".into(),
            command: "evals",
        },
        LauncherAction {
            id: LauncherActionId::Results,
            label: "Results".into(),
            description: "Browse past eval and benchmark results".into(),
            command: "results",
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
            Constraint::Length(4), // resources
            Constraint::Length(4), // services
            Constraint::Fill(1),   // actions
            Constraint::Length(2), // status bar
            Constraint::Length(1), // hint bar
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_resources(f, chunks[1], app);
    render_services(f, chunks[2], app);
    render_actions(f, chunks[3], app);
    render_status_bar(f, chunks[4], app);
    render_hint_bar(f, chunks[5]);
}

fn render_hint_bar(f: &mut Frame, area: Rect) {

    let spans = vec![
        Span::styled("↑↓/jk", style_hint_key()),
        Span::styled(" navigate  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" select  ", style_muted()),
        Span::styled("Esc", style_hint_key()),
        Span::styled(" exit  ", style_muted()),
        Span::styled("q", style_hint_key()),
        Span::styled(" quit  ", style_muted()),
        Span::styled("/", style_hint_key()),
        Span::styled(" command", style_muted()),
    ];
    let bar = Paragraph::new(Line::from(spans));
    f.render_widget(bar, area);
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

pub(super) fn launcher_title(section: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_lime()),
        Span::styled(section.to_string(), style_bold_cyan()),
    ])
}

pub(super) fn launcher_hint(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(format!(" {}", text.into()), style_gray()))
}

pub(super) fn chrome_block(title: Line<'static>, border_style: Style) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
}

pub(super) fn chrome_block_with_hint(
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

    let tier_name = "oz";

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
            Constraint::Length(1), // GPU
            Constraint::Length(1), // RAM
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
            let gpu_name = hw.gpu_name.as_deref().unwrap_or("GPU");
            let pct = ratio * 100.0;
            let bar_len = 12usize;
            let filled = (ratio * bar_len as f64).round() as usize;
            let bar: String = std::iter::repeat_n("\u{2588}", filled)
                .chain(std::iter::repeat_n(
                    "\u{2591}",
                    bar_len.saturating_sub(filled),
                ))
                .collect();
            let cuda_flag = if hw.cuda_available {
                let ver = hw.cuda_version.as_deref().unwrap_or("?");
                format!("  CUDA v{ver} \u{2713}")
            } else {
                "  CUDA \u{2717}".into()
            };
            let label = Line::from(vec![
                Span::styled(format!("  {gpu_name} "), style_bold_lime()),
                Span::styled(&bar, Style::default().fg(color)),
                Span::styled(
                    format!(" {}/{} MB ({:.0}%)", gpu.used_mb, gpu.total_mb, pct),
                    Style::default().fg(color),
                ),
                Span::styled(&cuda_flag, Style::default().fg(LIME)),
            ]);
            f.render_widget(Paragraph::new(label), rows[0]);
        }
        let ram_ratio = (hw.ram_used_mb as f64 / hw.ram_total_mb as f64).clamp(0.0, 1.0);
        let ram_pct = ram_ratio * 100.0;
        let bar_len = 12usize;
        let filled = (ram_ratio * bar_len as f64).round() as usize;
        let ram_bar: String = std::iter::repeat_n("\u{2588}", filled)
            .chain(std::iter::repeat_n(
                "\u{2591}",
                bar_len.saturating_sub(filled),
            ))
            .collect();
        let ram_label = Line::from(vec![
            Span::styled("  RAM ", style_bold_cyan()),
            Span::styled(&ram_bar, style_cyan()),
            Span::styled(
                format!(
                    " {}/{} MB ({:.0}%)",
                    hw.ram_used_mb, hw.ram_total_mb, ram_pct
                ),
                style_cyan(),
            ),
        ]);
        f.render_widget(Paragraph::new(ram_label), rows[1]);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  Loading hardware\u{2026}", style_gray())),
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
    let lines = vec![Line::from(vec![
        Span::styled(format!("  {llama_icon} llama.cpp  "), llama_style),
        Span::styled(llama_model_label, style_violet()),
        Span::styled(
            format!("  :{}", ozone_core::paths::DEFAULT_LLAMACPP_PORT),
            style_gray(),
        ),
    ])];
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

    let items: Vec<ListItem> = {
        // Optional model info banner at the top
        let mut all: Vec<ListItem> = Vec::new();
        if let Some(record) = super::selected_record(app) {
            all.push(ListItem::new(Line::from(vec![
                Span::styled("  Model: ", crate::theme::style_gray()),
                Span::styled(record.model_name.clone(), crate::theme::style_bold_lime()),
                Span::styled(
                    format!(
                        "  {:.1}GB  ctx:{}",
                        record.model_size_gb, record.recommendation.context_size
                    ),
                    crate::theme::style_cyan(),
                ),
            ])));
        }
        let action_items: Vec<ListItem> = launcher_actions(app)
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
        all.extend(action_items);
        all
    };
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
    let tier_badge = Span::raw(" ");
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


#[cfg(feature = "profiling-ui")]
pub use super::launcher_profile_views::*;

pub use super::launcher_settings::*;
pub use super::launcher_screens::*;

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
            .any(|action| action.id == LauncherActionId::BenchLauncher
                && action.command == "benchmarks"));
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
            .any(|action| action.command.contains("--help"))); // ozone+ removed
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
