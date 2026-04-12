use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Gauge, Paragraph},
};
use crate::theme::*;
use super::App;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let bg = Block::default().style(Style::default());
    f.render_widget(bg, area);

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Fill(1),
        ])
        .split(area);

    let center = vert[1];
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center);
    let content = horiz[1];

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // wordmark
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // tagline
            Constraint::Length(1),  // version
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // VRAM bar
            Constraint::Length(1),  // RAM bar
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // press any key
        ])
        .split(content);

    // OZONE wordmark
    let wordmark_lines: Vec<Line> = WORDMARK.iter()
        .map(|line| Line::from(Span::styled(*line, style_bold_violet())))
        .collect();
    let wordmark = Paragraph::new(wordmark_lines).alignment(Alignment::Center);
    f.render_widget(wordmark, sections[0]);

    // Tagline
    let tagline = Paragraph::new(Line::from(Span::styled(TAGLINE, style_gray())))
        .alignment(Alignment::Center);
    f.render_widget(tagline, sections[2]);

    // Version
    let version_text = Paragraph::new(Line::from(Span::styled(format!("v{VERSION}"), style_gray())))
        .alignment(Alignment::Center);
    f.render_widget(version_text, sections[3]);

    // Hardware bars or loading dots
    if let Some(hw) = &app.hardware {
        if let Some(gpu) = &hw.gpu {
            let ratio = if gpu.total_mb > 0 { gpu.used_mb as f64 / gpu.total_mb as f64 } else { 0.0 };
            let gauge = Gauge::default()
                .label(format!("VRAM  {}/{} MB", gpu.used_mb, gpu.total_mb))
                .ratio(ratio.clamp(0.0, 1.0))
                .gauge_style(style_violet());
            f.render_widget(gauge, sections[5]);
        }
        let ram_ratio = if hw.ram_total_mb > 0 { hw.ram_used_mb as f64 / hw.ram_total_mb as f64 } else { 0.0 };
        let ram_gauge = Gauge::default()
            .label(format!(" RAM  {}/{} MB", hw.ram_used_mb, hw.ram_total_mb))
            .ratio(ram_ratio.clamp(0.0, 1.0))
            .gauge_style(style_cyan());
        f.render_widget(ram_gauge, sections[6]);
    } else {
        let dots = match app.ticker % 4 {
            0 => "·  ",
            1 => "·· ",
            2 => "···",
            _ => "   ",
        };
        let loading = Paragraph::new(Line::from(Span::styled(
            format!("Loading hardware profile {dots}"),
            style_gray(),
        ))).alignment(Alignment::Center);
        f.render_widget(loading, sections[5]);
    }

    // Press any key / scanning
    let pulse_style = if app.splash_pulse { style_gray() } else { Style::default().fg(crate::theme::VIOLET) };
    let prompt = if app.splash_ready {
        Paragraph::new(Line::from(Span::styled("Press any key to continue", pulse_style)))
            .alignment(Alignment::Center)
    } else {
        Paragraph::new(Line::from(Span::styled("Scanning models…", style_gray())))
            .alignment(Alignment::Center)
    };
    f.render_widget(prompt, sections[8]);
}
