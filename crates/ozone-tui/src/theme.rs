//! Brand-aligned color palette and style helpers for the ozone+ TUI shell.
//!
//! These mirror the base ozone palette from `src/theme.rs` so the product
//! family feels cohesive while ozone+ retains its own accent (VIOLET).

use ratatui::style::{Color, Modifier, Style};

// ── Brand palette ────────────────────────────────────────────────────────

/// Primary teal accent shared across the ozone family.
pub const TEAL: Color = Color::Rgb(118, 183, 178); // #76b7b2

/// Lighter teal for highlights and secondary accents.
pub const CYAN: Color = Color::Rgb(141, 214, 209); // #8dd6d1

/// ozone+ accent — violet for differentiation within the family.
pub const VIOLET: Color = Color::Rgb(124, 58, 237); // #7c3aed

/// Brighter violet for selected/high-contrast states.
pub const VIOLET_BRIGHT: Color = Color::Rgb(196, 181, 253); // #c4b5fd

/// Success / positive states.
pub const GREEN: Color = Color::Rgb(34, 197, 94); // #22c55e

/// Warning / caution states.
pub const AMBER: Color = Color::Rgb(245, 158, 11); // #f59e0b

/// Error / critical states.
pub const RED: Color = Color::Rgb(239, 68, 68); // #ef4444

// ── Neutral palette ──────────────────────────────────────────────────────

/// Readable off-white for primary text on dark backgrounds.
pub const TEXT: Color = Color::Rgb(220, 220, 220); // #dcdcdc

/// Subdued text for labels, hints, and secondary copy.
pub const TEXT_DIM: Color = Color::Rgb(140, 140, 140); // #8c8c8c

/// Muted text for decorative borders and inactive elements.
pub const BORDER: Color = Color::Rgb(70, 70, 70); // #464646

/// Slightly brighter border for focused panes.
pub const BORDER_FOCUS: Color = Color::Rgb(118, 183, 178); // teal accent

/// Subtle background tint for pane titles (foreground only).
pub const TITLE_FG: Color = Color::Rgb(180, 180, 180); // #b4b4b4

// ── Brand symbols ────────────────────────────────────────────────────────

pub const HEX: &str = "⬡";
pub const HEX_FILLED: &str = "⬢";

// ── Semantic styles ──────────────────────────────────────────────────────

/// Normal body text.
pub fn text_style() -> Style {
    Style::default().fg(TEXT)
}

/// Dim / hint / secondary text.
pub fn dim_style() -> Style {
    Style::default().fg(TEXT_DIM)
}

/// Muted border / decorative text.
pub fn muted_style() -> Style {
    Style::default().fg(BORDER)
}

/// Focused pane border.
pub fn focus_border_style() -> Style {
    Style::default().fg(BORDER_FOCUS)
}

/// Unfocused pane border.
pub fn border_style() -> Style {
    Style::default().fg(BORDER)
}

/// Title in a focused pane.
pub fn title_focused_style() -> Style {
    Style::default()
        .fg(TEAL)
        .add_modifier(Modifier::BOLD)
}

/// Title in an unfocused pane.
pub fn title_style() -> Style {
    Style::default().fg(TITLE_FG)
}

/// Primary highlight — teal with bold.
pub fn highlight_style() -> Style {
    Style::default()
        .fg(TEAL)
        .add_modifier(Modifier::BOLD)
}

/// Selected conversation entry author — violet accent.
pub fn author_selected_style() -> Style {
    Style::default()
        .fg(VIOLET_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

/// Normal author label.
pub fn author_style() -> Style {
    Style::default().fg(CYAN)
}

/// User author label — slightly different shade for self-identification.
pub fn author_user_style() -> Style {
    Style::default().fg(TEAL)
}

/// Bookmark star.
pub fn bookmark_style() -> Style {
    Style::default().fg(AMBER)
}

/// Warning / caution text.
pub fn warning_style() -> Style {
    Style::default().fg(AMBER)
}

/// Error text.
pub fn error_style() -> Style {
    Style::default().fg(RED)
}

/// Success text.
pub fn success_style() -> Style {
    Style::default().fg(GREEN)
}

/// Mode badge (INSERT, COMMAND, etc.).
pub fn mode_badge_style() -> Style {
    Style::default()
        .fg(VIOLET_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

/// Accent style for hint keys and breadcrumb text.
pub fn accent_style() -> Style {
    Style::default()
        .fg(TEAL)
        .add_modifier(Modifier::BOLD)
}

/// Streaming cursor / generation-in-progress indicator.
pub fn streaming_style() -> Style {
    Style::default()
        .fg(TEAL)
        .add_modifier(Modifier::BOLD)
}

/// The hex prefix for the ozone+ title.
pub fn brand_hex_style() -> Style {
    Style::default().fg(TEAL)
}
