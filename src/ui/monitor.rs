use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
};
use chrono::Local;
use crate::theme::*;
use super::App;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // header
            Constraint::Length(4),   // VRAM + RAM bars
            Constraint::Length(3),   // CPU + disk info
            Constraint::Length(3),   // disk sparkline
            Constraint::Length(5),   // services
            Constraint::Fill(1),     // spacer
            Constraint::Length(2),   // key hints
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_resources(f, chunks[1], app);
    render_cpu_disk(f, chunks[2], app);
    render_sparkline(f, chunks[3], app);
    render_services(f, chunks[4], app);
    render_hints(f, chunks[6]);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let now = Local::now().format("%H:%M:%S");
    let title = Line::from(vec![
        Span::styled(format!(" {} Ozone Monitor ", crate::theme::HEX_CURSOR), style_bold_violet()),
        Span::styled("—", style_gray()),
        Span::styled(" live ", style_green()),
        Span::styled(format!("  {now}"), style_gray()),
    ]);
    let block = Block::default().borders(Borders::ALL).border_style(style_violet());
    f.render_widget(Paragraph::new(title).block(block), area);
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
                .label(format!("GPU  {}/{} MB  ({:.0}%)", gpu.used_mb, gpu.total_mb, ratio * 100.0))
                .ratio(ratio)
                .gauge_style(Style::default().fg(color));
            f.render_widget(gauge, rows[0]);
        }
        let ram_ratio = (hw.ram_used_mb as f64 / hw.ram_total_mb as f64).clamp(0.0, 1.0);
        let ram_gauge = Gauge::default()
            .label(format!(" RAM  {}/{} MB  ({:.0}%)", hw.ram_used_mb, hw.ram_total_mb, ram_ratio * 100.0))
            .ratio(ram_ratio)
            .gauge_style(style_cyan());
        f.render_widget(ram_gauge, rows[1]);
    }
}

fn render_cpu_disk(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let hw_text = if let Some(hw) = &app.hardware {
        format!("  CPU  {} logical / {} physical cores", hw.cpu_logical, hw.cpu_physical)
    } else {
        "  CPU  —".into()
    };

    let is_loading = app.disk_read_mbs > 40.0;
    let disk_style = if is_loading { style_amber() } else { style_gray() };
    let disk_hint = if is_loading { "  ⟳ loading model…" } else { "" };

    let lines = vec![
        Line::from(Span::styled(hw_text, style_gray())),
        Line::from(vec![
            Span::styled(format!(
                "  Disk  ↑{:.0} MB/s  ↓{:.0} MB/s",
                app.disk_read_mbs, app.disk_write_mbs
            ), disk_style),
            Span::styled(disk_hint, style_amber()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_sparkline(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled("  Disk Read (30s) ", style_gray()))
        .borders(Borders::ALL)
        .border_style(style_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let data: Vec<u64> = app.disk_read_history.clone();
    let sparkline = Sparkline::default()
        .data(data.iter().copied())
        .style(style_cyan());
    f.render_widget(sparkline, inner);
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

    let model_str = app.services.kobold_model.as_deref().unwrap_or("—");
    let tps_str = app.tokens_per_sec.map(|t| format!("  {t:.1} t/s")).unwrap_or_default();

    let lines = vec![
        Line::from(vec![
            Span::styled(format!("  {kc_icon} KoboldCpp  "), kc_style),
            Span::styled(model_str, style_cyan()),
            Span::styled(tps_str, style_green()),
        ]),
        Line::from(vec![
            Span::styled(format!("  {st_icon} SillyTavern  "), st_style),
            Span::styled(":8000", style_gray()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_hints(f: &mut Frame, area: Rect) {
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("  q", style_cyan()),
        Span::styled(" exit  ", style_gray()),
        Span::styled("s", style_cyan()),
        Span::styled(" stop all  ", style_gray()),
        Span::styled("r", style_cyan()),
        Span::styled(" back to launcher", style_gray()),
    ]));
    f.render_widget(hints, area);
}
