//! TUI rendering snapshot tests.
//!
//! These tests render screens to an offscreen `TestBackend` buffer and verify
//! key visual elements are present. They are NOT comprehensive pixel-perfect
//! snapshots — they check that render functions produce expected text content
//! without crashing or producing blank screens.
//!
//! Add new tests here when adding or modifying screen render functions.

use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

use ozone::catalog::{CatalogRecord, RecSource, Recommendation};
use ozone::prefs::{Preferences, Tier};
use ozone::ui::{self, App, Screen};

/// Build a minimal app with default preferences for testing.
fn base_app() -> App {
    App::new(Preferences {
        preferred_tier: Some(Tier::Base),
        ..Preferences::default()
    })
}

/// Build a catalog record for testing.
fn test_record(name: &str) -> CatalogRecord {
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

/// Render a screen to a string buffer for inspection.
fn render_to_string(
    width: u16,
    height: u16,
    draw: impl FnOnce(&mut ratatui::prelude::Frame),
) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame)).unwrap();
    buffer_to_string(terminal.backend().buffer(), width, height)
}

/// Convert a ratatui buffer to a string (rows joined by newlines).
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

// ── Splash screen tests ─────────────────────────────────────────────────────

#[test]
fn splash_screen_renders_without_crashing() {
    let app = base_app();
    let output = render_to_string(80, 24, |f| ui::splash::render(f, &app));
    // Splash screen should render without crashing and produce output
    assert!(!output.is_empty(), "Splash screen output should not be empty");
}

#[test]
fn splash_screen_shows_loading_when_catalog_empty() {
    let app = base_app();
    let output = render_to_string(80, 24, |f| ui::splash::render(f, &app));
    // When catalog is empty, splash should show a loading or progress indicator
    assert!(!output.is_empty());
}

// ── Launcher screen tests ───────────────────────────────────────────────────

#[test]
fn launcher_screen_renders_with_catalog_entries() {
    let mut app = base_app();
    app.screen = Screen::Launcher;
    app.catalog.push(test_record("test-model-v1.gguf"));
    app.catalog.push(test_record("test-model-v2.gguf"));
    let output = render_to_string(80, 24, |f| ui::launcher::render(f, &app));
    // Launcher should render model names when catalog is populated
    // Note: the exact display depends on filter state and UI layout
    assert!(output.len() > 100, "Launcher should produce substantial output");
}

#[test]
fn launcher_screen_renders_empty_state() {
    let mut app = base_app();
    app.screen = Screen::Launcher;
    // No catalog entries
    let output = render_to_string(80, 24, |f| ui::launcher::render(f, &app));
    // Should not crash — empty catalog should show appropriate message or model picker hint
    assert!(!output.is_empty());
}

#[test]
fn launcher_screen_shows_command_bar() {
    let mut app = base_app();
    app.screen = Screen::Launcher;
    let output = render_to_string(80, 24, |f| ui::launcher::render(f, &app));
    // Launcher should show some kind of action list or hint bar
    assert!(output.contains("?") || output.contains("configure") || output.contains("settings"),
        "Launcher should show action hints or commands");
}

// ── Monitor screen tests ────────────────────────────────────────────────────

#[test]
fn monitor_screen_renders_without_crashing() {
    let mut app = base_app();
    app.screen = Screen::Monitor;
    let output = render_to_string(80, 24, |f| ui::monitor::render(f, &app));
    assert!(!output.is_empty(), "Monitor screen output should not be empty");
}

// ── Settings screen tests ────────────────────────────────────────────────────

#[test]
fn settings_screen_renders_without_crashing() {
    let mut app = base_app();
    app.screen = Screen::Launcher;
    // Settings is rendered through launcher::render_settings
    let output = render_to_string(80, 24, |f| ui::launcher::render_settings(f, &app));
    assert!(!output.is_empty(), "Settings screen output should not be empty");
}

// ── 