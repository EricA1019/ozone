use ratatui::style::{Color, Modifier, Style};

// ── Palette ────────────────────────────────────────────────────────────────

pub const VIOLET: Color = Color::Rgb(124, 58, 237);
pub const CYAN: Color = Color::Rgb(6, 182, 212);
pub const GREEN: Color = Color::Rgb(34, 197, 94);
pub const AMBER: Color = Color::Rgb(245, 158, 11);
pub const RED: Color = Color::Rgb(239, 68, 68);
pub const GRAY: Color = Color::DarkGray;

pub fn style_violet() -> Style {
    Style::default().fg(VIOLET)
}
pub fn style_cyan() -> Style {
    Style::default().fg(CYAN)
}
pub fn style_green() -> Style {
    Style::default().fg(GREEN)
}
pub fn style_amber() -> Style {
    Style::default().fg(AMBER)
}
pub fn style_red() -> Style {
    Style::default().fg(RED)
}
pub fn style_gray() -> Style {
    Style::default().fg(GRAY)
}
pub fn style_bold_violet() -> Style {
    Style::default().fg(VIOLET).add_modifier(Modifier::BOLD)
}
pub fn style_bold_cyan() -> Style {
    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
}
#[allow(dead_code)]
pub fn style_bold_green() -> Style {
    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
}

// ── OZONE ASCII wordmark (5 rows) ─────────────────────────────────────────
pub const WORDMARK: &[&str] = &[
    " ██████  ███████  ██████  ███    ██ ███████",
    "██    ██    ███  ██    ██ ████   ██ ██     ",
    "██    ██   ███   ██    ██ ██ ██  ██ █████  ",
    "██    ██  ███    ██    ██ ██  ██ ██ ██     ",
    " ██████  ███████  ██████  ██   ████ ███████",
];

pub const HEX_CURSOR: &str = "⬡";
pub const TAGLINE: &str = "local AI stack operator";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
