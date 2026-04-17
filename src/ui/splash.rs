use super::App;
use crate::theme::*;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Gauge, Paragraph},
    Frame,
};

/// The selected tier for display in the splash screen tier preview
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplashTier {
    Lite,
    #[default]
    Base,
    Plus,
}

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let bg = Block::default().style(Style::default());
    f.render_widget(bg, area);

    // Vertical centering with extra room for tier preview
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(22), // Increased for tier preview
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
            Constraint::Length(1), // hex flourish top
            Constraint::Length(5), // wordmark
            Constraint::Length(1), // spacer
            Constraint::Length(1), // tagline
            Constraint::Length(1), // hex divider
            Constraint::Length(3), // tier preview
            Constraint::Length(1), // hex divider
            Constraint::Length(1), // version line
            Constraint::Length(1), // spacer
            Constraint::Length(1), // VRAM bar
            Constraint::Length(1), // RAM bar
            Constraint::Length(1), // spacer
            Constraint::Length(1), // press any key
            Constraint::Length(1), // hex footer
        ])
        .split(content);

    // Hex flourish at top
    let hex_top = Paragraph::new(Line::from(vec![Span::styled(
        format!("{HEX} {HEX} {HEX}"),
        style_lime(),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(hex_top, sections[0]);

    // OZONE wordmark in LIME
    let wordmark_lines: Vec<Line> = WORDMARK
        .iter()
        .map(|line| Line::from(Span::styled(*line, style_bold_lime())))
        .collect();
    let wordmark = Paragraph::new(wordmark_lines).alignment(Alignment::Center);
    f.render_widget(wordmark, sections[1]);

    // Tagline with hex prefix
    let tagline = Paragraph::new(Line::from(Span::styled(TAGLINE, style_gray())))
        .alignment(Alignment::Center);
    f.render_widget(tagline, sections[3]);

    // Hex divider
    render_hex_divider(f, sections[4]);

    // Tier preview - show all three tiers with the current one highlighted
    render_tier_preview(f, sections[5], SplashTier::Base);

    // Hex divider
    render_hex_divider(f, sections[6]);

    // Version line: v0.4.1-alpha · MIT · local-first AI tooling
    let version_line = Paragraph::new(Line::from(vec![
        Span::styled(format!("v{VERSION}"), style_gray()),
        Span::styled("  ·  ", style_gray()),
        Span::styled(LICENSE, style_gray()),
        Span::styled("  ·  ", style_gray()),
        Span::styled(TAGLINE_SHORT, style_gray()),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(version_line, sections[7]);

    // Hardware bars or loading dots
    if let Some(hw) = &app.hardware {
        if let Some(gpu) = &hw.gpu {
            let ratio = if gpu.total_mb > 0 {
                gpu.used_mb as f64 / gpu.total_mb as f64
            } else {
                0.0
            };
            let gauge = Gauge::default()
                .label(format!("VRAM  {}/{} MB", gpu.used_mb, gpu.total_mb))
                .ratio(ratio.clamp(0.0, 1.0))
                .gauge_style(style_lime());
            f.render_widget(gauge, sections[9]);
        }
        let ram_ratio = if hw.ram_total_mb > 0 {
            hw.ram_used_mb as f64 / hw.ram_total_mb as f64
        } else {
            0.0
        };
        let ram_gauge = Gauge::default()
            .label(format!(" RAM  {}/{} MB", hw.ram_used_mb, hw.ram_total_mb))
            .ratio(ram_ratio.clamp(0.0, 1.0))
            .gauge_style(style_cyan());
        f.render_widget(ram_gauge, sections[10]);
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
        )))
        .alignment(Alignment::Center);
        f.render_widget(loading, sections[9]);
    }

    // Press any key / scanning
    let pulse_style = if app.splash_pulse {
        style_gray()
    } else {
        style_lime()
    };
    let prompt = if app.splash_ready {
        Paragraph::new(Line::from(vec![
            Span::styled("[Enter]", pulse_style),
            Span::styled(" Continue   ", style_gray()),
            Span::styled("[Q]", style_gray()),
            Span::styled(" Quit   ", style_gray()),
            Span::styled("[?]", style_gray()),
            Span::styled(" About", style_gray()),
        ]))
        .alignment(Alignment::Center)
    } else {
        Paragraph::new(Line::from(Span::styled("Scanning models…", style_gray())))
            .alignment(Alignment::Center)
    };
    f.render_widget(prompt, sections[12]);

    // Hex footer
    let hex_footer =
        Paragraph::new(Line::from(Span::styled(HEX, style_lime()))).alignment(Alignment::Center);
    f.render_widget(hex_footer, sections[13]);
}

/// Render a hex-accented divider line
fn render_hex_divider(f: &mut Frame, area: Rect) {
    let width = area.width as usize;
    let dash_count = width.saturating_sub(6) / 2;
    let dashes: String = "─".repeat(dash_count);

    let divider = Paragraph::new(Line::from(vec![
        Span::styled(HEX, style_lime()),
        Span::styled(" ", Style::default()),
        Span::styled(&dashes, style_gray()),
        Span::styled(" ", Style::default()),
        Span::styled(HEX, style_lime()),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(divider, area);
}

/// Render the tier preview widget showing all three tiers
fn render_tier_preview(f: &mut Frame, area: Rect, current: SplashTier) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let tiers = [
        (TIER_LITE, TIER_LITE_DESC, SplashTier::Lite),
        (TIER_BASE, TIER_BASE_DESC, SplashTier::Base),
        (TIER_PLUS, TIER_PLUS_DESC, SplashTier::Plus),
    ];

    for (i, (name, desc, tier)) in tiers.iter().enumerate() {
        let is_current = *tier == current;
        let hex = if is_current { HEX_FILLED } else { HEX };
        let hex_style = if is_current {
            style_lime()
        } else {
            style_gray()
        };
        let name_style = if is_current {
            style_lime()
        } else {
            style_gray()
        };
        let arrow = if is_current { " ◄" } else { "  " };

        let line = Paragraph::new(Line::from(vec![
            Span::styled(hex, hex_style),
            Span::styled(" ", Style::default()),
            Span::styled(format!("{:12}", name), name_style),
            Span::styled(format!("{:24}", desc), style_gray()),
            Span::styled(arrow, style_lime()),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(line, rows[i]);
    }
}
