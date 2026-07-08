//! Brand-aligned color palette and style helpers for the ozone TUI.
//!
//! Three preset themes: DarkMint (default), OzoneDark (original), HighContrast.
//! All style functions are preset-aware via the active preset singleton.
//! Use `set_preset()` at startup or at runtime to switch themes instantly.

// Infrastructure for later phases — not dead.


use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

// ── Theme preset ─────────────────────────────────────────────────────────────

/// Selectable palette preset for the ozone TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    /// Green-forward mint palette — default.
    #[default]
    DarkMint,
    /// Original blue-green palette shipped before 0.4.4.
    OzoneDark,
    /// Maximum contrast for accessibility or bright ambient light.
    HighContrast,
}

impl ThemePreset {
    /// Parse the kebab-case string stored in prefs (e.g. `"dark-mint"`).
    #[allow(dead_code)]
    pub fn from_pref_str(s: &str) -> Self {
        match s {
            "dark-mint" => Self::DarkMint,
            "ozone-dark" => Self::OzoneDark,
            "high-contrast" => Self::HighContrast,
            _ => Self::default(),
        }
    }
}

// ── Active-preset singleton ──────────────────────────────────────────────────

/// Stores the active preset discriminant as a `u8`.
/// 0 = DarkMint (default), 1 = OzoneDark, 2 = HighContrast.
static ACTIVE_PRESET: AtomicU8 = AtomicU8::new(0);

/// Set the active preset. Can be called multiple times to change the
/// theme at runtime — each call takes effect immediately.
#[allow(dead_code)]
pub fn set_preset(preset: ThemePreset) {
    let disc = match preset {
        ThemePreset::DarkMint => 0,
        ThemePreset::OzoneDark => 1,
        ThemePreset::HighContrast => 2,
    };
    ACTIVE_PRESET.store(disc, Ordering::Relaxed);
}

/// Return the active preset, defaulting to `DarkMint` for unknown values.
pub fn active_preset() -> ThemePreset {
    match ACTIVE_PRESET.load(Ordering::Relaxed) {
        0 => ThemePreset::DarkMint,
        1 => ThemePreset::OzoneDark,
        2 => ThemePreset::HighContrast,
        _ => ThemePreset::DarkMint,
    }
}

// ── Preset-aware color getters ───────────────────────────────────────────────

/// Primary accent color for the given preset (maps to `style_lime()`).
pub fn accent(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(45, 175, 130),
        ThemePreset::OzoneDark => LIME,
        ThemePreset::HighContrast => Color::Rgb(0, 255, 180),
    }
}

/// Lighter highlight accent (maps to `style_cyan()`).
pub fn highlight(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(78, 210, 165),
        ThemePreset::OzoneDark => CYAN,
        ThemePreset::HighContrast => Color::Rgb(100, 255, 200),
    }
}

/// Violet/purple accent (maps to `style_violet()`).
pub fn violet(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(100, 58, 200),
        ThemePreset::OzoneDark => VIOLET,
        ThemePreset::HighContrast => Color::Rgb(180, 100, 255),
    }
}

/// Muted secondary text color (maps to `style_gray()`).
pub fn muted_color(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(100, 140, 130),
        ThemePreset::OzoneDark => GRAY,
        ThemePreset::HighContrast => Color::Rgb(180, 180, 180),
    }
}

// ── OzoneDark reference palette (kept as consts for backward compat) ─────────

/// Primary teal accent — OzoneDark reference value.
pub const LIME: Color = Color::Rgb(118, 183, 178); // #76b7b2

/// Lighter teal highlight — OzoneDark reference value.
pub const CYAN: Color = Color::Rgb(141, 214, 209); // #8dd6d1

/// Violet/purple accent — OzoneDark reference value.
pub const VIOLET: Color = Color::Rgb(124, 58, 237); // #7c3aed

/// Success / positive states (same across all presets).
pub const GREEN: Color = Color::Rgb(34, 197, 94); // #22c55e

/// Warning / caution states (same across all presets).
pub const AMBER: Color = Color::Rgb(245, 158, 11); // #f59e0b

/// Error / critical states (same across all presets).
pub const RED: Color = Color::Rgb(239, 68, 68); // #ef4444

/// Muted secondary text — OzoneDark reference value.
pub const GRAY: Color = Color::Rgb(141, 214, 209); // #8dd6d1 (same as CYAN)

// ── Semantic style functions (preset-aware) ──────────────────────────────────

/// Primary teal accent style — follows active preset.
pub fn style_lime() -> Style {
    Style::default().fg(accent(active_preset()))
}
/// Violet accent style — follows active preset.
pub fn style_violet() -> Style {
    Style::default().fg(violet(active_preset()))
}
/// Lighter teal highlight style — follows active preset.
pub fn style_cyan() -> Style {
    Style::default().fg(highlight(active_preset()))
}
/// Success / green style (same across all presets).
pub fn style_green() -> Style {
    Style::default().fg(GREEN)
}
/// Warning / amber style (same across all presets).
pub fn style_amber() -> Style {
    Style::default().fg(AMBER)
}
/// Error / red style (same across all presets).
pub fn style_red() -> Style {
    Style::default().fg(RED)
}
/// Muted / secondary text — follows active preset.
pub fn style_gray() -> Style {
    Style::default().fg(muted_color(active_preset()))
}
/// Dimmed secondary text — follows active preset.
pub fn style_muted() -> Style {
    Style::default()
        .fg(muted_color(active_preset()))
        .add_modifier(Modifier::DIM)
}
/// Bold primary accent — follows active preset.
pub fn style_bold_lime() -> Style {
    Style::default()
        .fg(accent(active_preset()))
        .add_modifier(Modifier::BOLD)
}
/// Bold violet — follows active preset.
pub fn style_bold_violet() -> Style {
    Style::default()
        .fg(violet(active_preset()))
        .add_modifier(Modifier::BOLD)
}
/// Bold highlight — follows active preset.
pub fn style_bold_cyan() -> Style {
    Style::default()
        .fg(highlight(active_preset()))
        .add_modifier(Modifier::BOLD)
}
/// Keyboard hint style — bold highlight.
pub fn style_hint_key() -> Style {
    Style::default()
        .fg(highlight(active_preset()))
        .add_modifier(Modifier::BOLD)
}
/// Panel border style — active state uses accent, inactive uses muted.
pub fn style_panel_border(active: bool) -> Style {
    if active {
        style_lime()
    } else {
        Style::default().fg(muted_color(active_preset()))
    }
}
/// Panel title style — active uses bold accent, inactive uses bold highlight.
pub fn style_panel_title(active: bool) -> Style {
    if active {
        style_bold_lime()
    } else {
        style_bold_cyan()
    }
}

// ── OZONE ASCII wordmark (5 rows) ────────────────────────────────────────────

pub const WORDMARK: &[&str] = &[
    " ██████  ███████  ██████  ███    ██ ███████",
    "██    ██    ███  ██    ██ ████   ██ ██     ",
    "██    ██   ███   ██    ██ ██ ██  ██ █████  ",
    "██    ██  ███    ██    ██ ██  ██ ██ ██     ",
    " ██████  ███████  ██████  ██   ████ ███████",
];

// ── Brand ────────────────────────────────────────────────────────────────────

pub const HEX: &str = "⬡";
pub const HEX_FILLED: &str = "⬢";
pub const HEX_CURSOR: &str = "⬡";
pub const TAGLINE: &str = "⬡ Use AI smarter. Not bigger.";
pub const TAGLINE_SHORT: &str = "local-first AI tooling";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LICENSE: &str = "MIT";

/// Tier names for display
pub const TIER_LITE: &str = "oz";
pub const TIER_BASE: &str = "oz";

/// Tier descriptions
pub const TIER_LITE_DESC: &str = "lean backend control";
pub const TIER_BASE_DESC: &str = "tuning + profiles";
