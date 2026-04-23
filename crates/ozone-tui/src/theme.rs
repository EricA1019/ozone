//! Brand-aligned color palette and style helpers for the ozone+ TUI shell.
//!
//! These mirror the base ozone palette from `src/theme.rs` so the product
//! family feels cohesive while ozone+ retains its own accent (VIOLET).

use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

// ── Theme preset ─────────────────────────────────────────────────────────

/// Selectable palette preset for the ozone+ TUI.
///
/// Serializes as a kebab-case string (e.g. `"dark-mint"`) so it can be
/// round-tripped through a plain-text prefs file without depending on
/// non-TUI crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    /// Green-forward mint palette — default since 0.4.4.
    #[default]
    DarkMint,
    /// Original blue-green palette shipped before 0.4.4.
    OzoneDark,
    /// Maximum contrast for accessibility or bright ambient light.
    HighContrast,
}

impl ThemePreset {
    /// Parse the kebab-case string stored in prefs (e.g. `"dark-mint"`).
    /// Returns `DarkMint` for any unrecognised value.
    pub fn from_pref_str(s: &str) -> Self {
        match s {
            "dark-mint" => Self::DarkMint,
            "ozone-dark" => Self::OzoneDark,
            "high-contrast" => Self::HighContrast,
            _ => Self::default(),
        }
    }
}

// ── Active-preset singleton ───────────────────────────────────────────────

/// Stores the active preset discriminant as a `u8`.
/// 0 = DarkMint (default), 1 = OzoneDark, 2 = HighContrast.
static ACTIVE_PRESET: AtomicU8 = AtomicU8::new(0);

/// Set the active preset.  Can be called multiple times to change the
/// theme at runtime — each call takes effect immediately for all
/// subsequent `active_preset()` reads.
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

// ── Preset-aware color getters ────────────────────────────────────────────

/// Primary teal accent for the given preset.
pub fn teal(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(45, 175, 130),
        ThemePreset::OzoneDark => TEAL,
        ThemePreset::HighContrast => Color::Rgb(0, 255, 180),
    }
}

/// Lighter teal / highlight accent for the given preset.
pub fn cyan(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(78, 210, 165),
        ThemePreset::OzoneDark => CYAN,
        ThemePreset::HighContrast => Color::Rgb(100, 255, 200),
    }
}

/// Violet accent for the given preset.
pub fn violet(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(100, 58, 200),
        ThemePreset::OzoneDark => VIOLET,
        ThemePreset::HighContrast => Color::Rgb(180, 100, 255),
    }
}

/// Brighter violet for selected / high-contrast states.
pub fn violet_bright(preset: ThemePreset) -> Color {
    match preset {
        ThemePreset::DarkMint => Color::Rgb(180, 165, 240),
        ThemePreset::OzoneDark => VIOLET_BRIGHT,
        ThemePreset::HighContrast => Color::Rgb(220, 200, 255),
    }
}

/// Convenience alias — the primary accent colour for the active preset.
pub fn accent_color(preset: ThemePreset) -> Color {
    teal(preset)
}

// ── Brand palette (OzoneDark values — kept as consts) ────────────────────

/// Primary teal accent — OzoneDark preset reference value.
pub const TEAL: Color = Color::Rgb(118, 183, 178); // #76b7b2

/// Lighter teal for highlights — OzoneDark preset reference value.
pub const CYAN: Color = Color::Rgb(141, 214, 209); // #8dd6d1

/// ozone+ accent violet — OzoneDark preset reference value.
pub const VIOLET: Color = Color::Rgb(124, 58, 237); // #7c3aed

/// Brighter violet for selected states — OzoneDark preset reference value.
pub const VIOLET_BRIGHT: Color = Color::Rgb(196, 181, 253); // #c4b5fd

/// Success / positive states.
pub const GREEN: Color = Color::Rgb(34, 197, 94); // #22c55e

/// Warning / caution states.
pub const AMBER: Color = Color::Rgb(245, 158, 11); // #f59e0b

/// Error / critical states.
pub const RED: Color = Color::Rgb(239, 68, 68); // #ef4444

// ── Mode badge backgrounds ────────────────────────────────────────────────

/// Background for INSERT mode badge.
pub const MODE_INSERT_BG: Color = Color::Rgb(44, 18, 93); // dark violet

/// Background for COMMAND mode badge.
pub const MODE_CMD_BG: Color = Color::Rgb(45, 30, 0); // dark amber

/// Background for NORMAL mode badge.
pub const MODE_NORMAL_BG: Color = Color::Rgb(20, 20, 20); // near-black

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
    Style::default().fg(teal(active_preset()))
}

/// Unfocused pane border.
pub fn border_style() -> Style {
    Style::default().fg(BORDER)
}

/// Title in a focused pane.
pub fn title_focused_style() -> Style {
    Style::default()
        .fg(teal(active_preset()))
        .add_modifier(Modifier::BOLD)
}

/// Title in an unfocused pane.
pub fn title_style() -> Style {
    Style::default().fg(TITLE_FG)
}

/// Primary highlight — teal with bold.
pub fn highlight_style() -> Style {
    Style::default()
        .fg(teal(active_preset()))
        .add_modifier(Modifier::BOLD)
}

/// Selected conversation entry author — violet accent.
pub fn author_selected_style() -> Style {
    Style::default()
        .fg(violet_bright(active_preset()))
        .add_modifier(Modifier::BOLD)
}

/// Normal author label.
pub fn author_style() -> Style {
    Style::default().fg(cyan(active_preset()))
}

/// User author label — slightly different shade for self-identification.
pub fn author_user_style() -> Style {
    Style::default().fg(teal(active_preset()))
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
        .fg(violet_bright(active_preset()))
        .add_modifier(Modifier::BOLD)
}

/// Accent style for hint keys and breadcrumb text.
pub fn accent_style() -> Style {
    Style::default()
        .fg(teal(active_preset()))
        .add_modifier(Modifier::BOLD)
}

pub fn textarea_selection_style() -> Style {
    Style::default()
        .bg(violet(active_preset()))
        .fg(Color::White)
}

pub fn textarea_placeholder_style() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn textarea_line_number_style() -> Style {
    Style::default().fg(BORDER)
}

pub fn textarea_cursor_line_style() -> Style {
    Style::default().bg(Color::Rgb(20, 28, 28))
}

/// Streaming cursor / generation-in-progress indicator.
pub fn streaming_style() -> Style {
    Style::default()
        .fg(teal(active_preset()))
        .add_modifier(Modifier::BOLD)
}

/// The hex prefix for the ozone+ title.
pub fn brand_hex_style() -> Style {
    Style::default().fg(teal(active_preset()))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_preset_default_is_dark_mint() {
        assert_eq!(ThemePreset::default(), ThemePreset::DarkMint);
    }

    #[test]
    fn theme_preset_from_pref_str() {
        assert_eq!(
            ThemePreset::from_pref_str("dark-mint"),
            ThemePreset::DarkMint
        );
        assert_eq!(
            ThemePreset::from_pref_str("ozone-dark"),
            ThemePreset::OzoneDark
        );
        assert_eq!(
            ThemePreset::from_pref_str("high-contrast"),
            ThemePreset::HighContrast
        );
        assert_eq!(ThemePreset::from_pref_str("unknown"), ThemePreset::DarkMint);
        assert_eq!(ThemePreset::from_pref_str(""), ThemePreset::DarkMint);
    }

    #[test]
    fn theme_preset_serde_roundtrip() {
        for preset in [
            ThemePreset::DarkMint,
            ThemePreset::OzoneDark,
            ThemePreset::HighContrast,
        ] {
            let json = serde_json::to_string(&preset).unwrap();
            let back: ThemePreset = serde_json::from_str(&json).unwrap();
            assert_eq!(back, preset);
        }
    }

    #[test]
    fn teal_dark_mint_is_green_leaning() {
        // DarkMint teal should have more green than blue.
        match teal(ThemePreset::DarkMint) {
            Color::Rgb(_r, g, b) => assert!(g > b, "DarkMint teal: g={g} should exceed b={b}"),
            _ => panic!("expected Rgb"),
        }
    }

    #[test]
    fn ozone_dark_values_match_consts() {
        assert_eq!(teal(ThemePreset::OzoneDark), TEAL);
        assert_eq!(cyan(ThemePreset::OzoneDark), CYAN);
        assert_eq!(violet(ThemePreset::OzoneDark), VIOLET);
        assert_eq!(violet_bright(ThemePreset::OzoneDark), VIOLET_BRIGHT);
    }

    #[test]
    fn accent_color_matches_teal() {
        for preset in [
            ThemePreset::DarkMint,
            ThemePreset::OzoneDark,
            ThemePreset::HighContrast,
        ] {
            assert_eq!(accent_color(preset), teal(preset));
        }
    }

    #[test]
    fn set_preset_can_change_at_runtime() {
        // The old OnceLock implementation silently ignored subsequent calls.
        // With AtomicU8, set_preset must update immediately.
        set_preset(ThemePreset::OzoneDark);
        assert_eq!(active_preset(), ThemePreset::OzoneDark);

        set_preset(ThemePreset::HighContrast);
        assert_eq!(active_preset(), ThemePreset::HighContrast);

        set_preset(ThemePreset::DarkMint);
        assert_eq!(active_preset(), ThemePreset::DarkMint);
    }
}
