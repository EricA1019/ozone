use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
};
use crate::theme::*;
use super::App;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // header
            Constraint::Length(4),   // resources
            Constraint::Length(4),   // services
            Constraint::Fill(1),     // actions
            Constraint::Length(2),   // status bar
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
    let block = Block::default().borders(Borders::ALL).border_style(style_violet());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let title = Line::from(vec![
        Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_violet()),
        Span::styled(format!("v{} ", VERSION), style_gray()),
        Span::styled("— ", style_gray()),
        Span::styled(format!("{model_count} models"), style_cyan()),
    ]);
    let subtitle = Line::from(Span::styled("  Local AI stack operator", style_gray()));
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
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    if let Some(hw) = &app.hardware {
        if let Some(gpu) = &hw.gpu {
            let ratio = (gpu.used_mb as f64 / gpu.total_mb as f64).clamp(0.0, 1.0);
            let color = if ratio > 0.9 { RED } else if ratio > 0.75 { AMBER } else { VIOLET };
            let gauge = Gauge::default()
                .label(format!("GPU VRAM  {}/{} MB  ({:.0}%)", gpu.used_mb, gpu.total_mb, ratio * 100.0))
                .ratio(ratio)
                .gauge_style(Style::default().fg(color));
            f.render_widget(gauge, rows[0]);
        }
        let ram_ratio = (hw.ram_used_mb as f64 / hw.ram_total_mb as f64).clamp(0.0, 1.0);
        let ram_gauge = Gauge::default()
            .label(format!(" System RAM  {}/{} MB  ({:.0}%)", hw.ram_used_mb, hw.ram_total_mb, ram_ratio * 100.0))
            .ratio(ram_ratio)
            .gauge_style(style_cyan());
        f.render_widget(ram_gauge, rows[1]);
    } else {
        f.render_widget(Paragraph::new(Span::styled("  Loading hardware…", style_gray())), rows[0]);
    }
}

fn render_services(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Services ", style_bold_cyan()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (kc_icon, kc_style) = if app.services.kobold_running { ("●", style_green()) } else { ("○", style_gray()) };
    let (st_icon, st_style) = if app.services.st_running { ("●", style_green()) } else { ("○", style_gray()) };

    let model_label = app.services.kobold_model.as_deref().unwrap_or("—");
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("  {kc_icon} KoboldCpp  "), kc_style),
            Span::styled(model_label, style_cyan()),
            Span::styled("  :5001", style_gray()),
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
        .title_bottom(Line::from(Span::styled("  ↑↓ navigate · Enter select · q quit", style_gray())))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let actions = [
        "Launch backend + SillyTavern",
        "Start SillyTavern only",
        "Clear GPU backends",
        "Monitor services",
        "Browse model catalog",
        "Exit",
    ];

    let items: Vec<ListItem> = actions.iter().enumerate().map(|(i, label)| {
        if i == app.selected_action {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", HEX_CURSOR), style_cyan()),
                Span::styled(format!("{}", i + 1), style_gray()),
                Span::raw("  "),
                Span::styled(*label, Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            ]))
        } else {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}", i + 1), style_gray()),
                Span::raw("  "),
                Span::styled(*label, style_gray()),
            ]))
        }
    }).collect();
    f.render_widget(List::new(items), inner);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let msg = app.status_msg.as_deref()
        .or(app.error_msg.as_deref())
        .unwrap_or("");
    let style = if app.error_msg.is_some() { style_red() } else { style_gray() };
    let bar = Paragraph::new(Line::from(Span::styled(format!("  {msg}"), style)));
    f.render_widget(bar, area);
}

pub fn render_model_picker(f: &mut Frame, app: &App) {
    let area = f.area();
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(format!(" {} Ozone ", HEX_CURSOR), style_bold_violet()),
            Span::styled("Model Picker", style_bold_cyan()),
        ]))
        .title_bottom(Line::from(Span::styled(" ↑↓ scroll · Enter select · Esc back", style_gray())))
        .borders(Borders::ALL)
        .border_style(style_violet());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.catalog.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("  No models found in ~/models/", style_amber())), inner);
        return;
    }

    let hw = app.hardware.as_ref();
    let items: Vec<ListItem> = app.catalog.iter().enumerate().map(|(i, rec)| {
        let selected = i == app.selected_model;
        let prefix = if selected { format!("{} ", HEX_CURSOR) } else { "  ".to_string() };

        let plan_vram = hw.map(|_h| crate::planner::estimate_vram_mb(
            rec.recommendation.context_size,
            rec.recommendation.gpu_layers,
            rec.model_size_gb,
            rec.recommendation.quant_kv,
            crate::planner::estimate_total_layers(rec.model_size_gb),
        ));

        let (fit_icon, fit_style) = if let (Some(vram_est), Some(hw)) = (plan_vram, hw) {
            if let Some(gpu) = &hw.gpu {
                let budget = (gpu.free_mb as f64 * 0.9) as u32;
                if vram_est <= budget { ("✓", style_green()) }
                else if vram_est <= gpu.total_mb as u32 { ("~", style_amber()) }
                else { ("✗", style_red()) }
            } else { ("?", style_gray()) }
        } else { ("?", style_gray()) };

        let source_label = rec.recommendation.source.label();
        let speed_label = rec.benchmark.as_ref()
            .map(|b| format!("{:.1} t/s", b.gen_speed))
            .unwrap_or_else(|| "— t/s".into());

        let name = if rec.model_name.len() > 40 {
            format!("{}…", &rec.model_name[..38])
        } else { rec.model_name.clone() };

        let base_style = if selected {
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
        } else { style_gray() };

        ListItem::new(Line::from(vec![
            Span::styled(prefix, if selected { style_cyan() } else { style_gray() }),
            Span::styled(format!("[{:02}] ", i + 1), style_gray()),
            Span::styled(format!("{:<42}", name), base_style),
            Span::styled(format!(" {:5}  ", source_label), style_gray()),
            Span::styled(fit_icon, fit_style),
            Span::styled(format!("  {:>10}", speed_label), style_gray()),
        ]))
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_model));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);
}

pub fn render_launching(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(8), Constraint::Fill(1)])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(50), Constraint::Fill(1)])
        .split(center)[1];

    let model = app.current_plan.as_ref().map(|p| p.model_name.as_str()).unwrap_or("…");
    let dots = match app.ticker % 4 { 0 => "·  ", 1 => "·· ", 2 => "···", _ => "   " };

    let lines = vec![
        Line::from(Span::styled("  Launching KoboldCpp…", style_bold_violet())),
        Line::from(Span::styled(format!("  {model}"), style_cyan())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(format!("  Loading {dots}"), style_amber())),
    ];
    let block = Block::default().borders(Borders::ALL).border_style(style_violet());
    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, center_h);
}

pub fn render_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(12), Constraint::Fill(1)])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(60), Constraint::Fill(1)])
        .split(center)[1];

    if let Some(plan) = &app.current_plan {
        let mode_label = plan.mode.label();
        let lines = vec![
            Line::from(Span::styled("  Confirm Launch", style_bold_violet())),
            Line::from(Span::raw("")),
            Line::from(vec![Span::styled("  Model:    ", style_gray()), Span::styled(&plan.model_name, style_cyan())]),
            Line::from(vec![Span::styled("  Layers:   ", style_gray()), Span::styled(plan.gpu_layers.to_string(), style_cyan())]),
            Line::from(vec![Span::styled("  Context:  ", style_gray()), Span::styled(plan.context_size.to_string(), style_cyan())]),
            Line::from(vec![Span::styled("  QuantKV:  ", style_gray()), Span::styled(plan.quant_kv.to_string(), style_cyan())]),
            Line::from(vec![Span::styled("  Mode:     ", style_gray()), Span::styled(mode_label, style_amber())]),
            Line::from(Span::styled(format!("  {}", plan.rationale), style_gray())),
            Line::from(Span::raw("")),
            Line::from(Span::styled("  Press Enter to launch · Esc to cancel", style_gray())),
        ];
        let block = Block::default().borders(Borders::ALL).border_style(style_violet());
        f.render_widget(Paragraph::new(lines).block(block), center_h);
    }
}
