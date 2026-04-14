use ratatui::style::{Color, Modifier, Style};

// ── Palette ────────────────────────────────────────────────────────────────

pub const LIME: Color = Color::Rgb(168, 230, 0);   // #a8e600 — primary accent
pub const VIOLET: Color = Color::Rgb(124, 58, 237); // #7c3aed — ozone+ accent
pub const CYAN: Color = Color::Rgb(6, 182, 212);
pub const GREEN: Color = Color::Rgb(34, 197, 94);
pub const AMBER: Color = Color::Rgb(245, 158, 11);
pub const RED: Color = Color::Rgb(239, 68, 68);
pub const GRAY: Color = Color::DarkGray;

pub fn style_lime() -> Style {
    Style::default().fg(LIME)
}
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
pub fn style_bold_lime() -> Style {
    Style::default().fg(LIME).add_modifier(Modifier::BOLD)
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

// ── Brand ─────────────────────────────────────────────────────────────────
pub const HEX: &str = "⬡";
pub const HEX_FILLED: &str = "⬢";
pub const HEX_CURSOR: &str = "⬡";
pub const TAGLINE: &str = "⬡ Use AI smarter. Not bigger.";
pub const TAGLINE_SHORT: &str = "local-first AI tooling";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MATURITY: &str = "alpha";
pub const LICENSE: &str = "MIT";

/// Tier names for display
pub const TIER_LITE: &str = "ozonelite";
pub const TIER_BASE: &str = "ozone";
pub const TIER_PLUS: &str = "ozone+";

/// Tier descriptions
pub const TIER_LITE_DESC: &str = "lean backend control";
pub const TIER_BASE_DESC: &str = "tuning + profiles";
pub const TIER_PLUS_DESC: &str = "chat shell + memory";
