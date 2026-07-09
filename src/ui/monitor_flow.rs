//! Monitor screen event loop — extracted from `ui/mod.rs`.
//!
//! Manages the live monitor TUI that shows VRAM/RAM/CPU/service status
//! with configurable refresh rates.

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::Clear, Terminal};

use super::{
    monitor, App, Preferences, Screen, TerminalRestoreGuard,
};

pub(super) enum MonitorOutcome {
    Continue,
    ExitLauncher,
}

pub(super) async fn handle_monitor_key(app: &mut App, key: KeyEvent) -> MonitorOutcome {
    match key.code {
        KeyCode::Char('q') => return MonitorOutcome::ExitLauncher,
        KeyCode::Esc | KeyCode::Char('r') => {
            app.screen = Screen::Launcher;
        }
        KeyCode::Char('s') => {
            let _ = crate::llamacpp::clear_gpu_backends().await;
            app.services = crate::llamacpp::get_service_status().await;
            app.set_status("GPU backends cleared.".into());
            app.screen = Screen::Launcher;
        }
        _ => {}
    }

    MonitorOutcome::Continue
}

/// Run the monitor dashboard as a standalone TUI.
///
/// This is a top-level entry point (called from `lib.rs`) that owns the
/// entire terminal for the duration of the monitor session. It sets up
/// raw mode, alternate screen, runs its own event loop with three refresh
/// tiers (tick 100ms, stats 500ms, disk 2s), and restores the terminal
/// on exit.
pub async fn run_monitor() -> Result<()> {
    let (prefs, startup_error) = match crate::prefs::load_prefs().await {
        Ok(prefs) => (prefs, None),
        Err(error) => (
            Preferences::default(),
            Some(format!("Failed to load preferences: {error}")),
        ),
    };
    let mut app = App::new(prefs);
    if let Some(error) = startup_error {
        app.set_error(error);
    }
    app.screen = Screen::Monitor;
    app.hardware = Some(
        tokio::task::spawn_blocking(crate::hardware::load_hardware)
            .await
            .unwrap_or_default(),
    );
    app.services = crate::llamacpp::get_service_status().await;

    enable_raw_mode()?;
    let mut terminal_restore = TerminalRestoreGuard::new();
    terminal_restore.mark_raw_mode_enabled();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    terminal_restore.mark_alt_screen_entered();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    let mut last_fast_refresh = Instant::now();

    loop {
        terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            monitor::render(f, &app);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('s') => {
                        let _ = crate::llamacpp::clear_gpu_backends().await;
                        app.services = crate::llamacpp::get_service_status().await;
                        terminal.draw(|f| monitor::render(f, &app))?;
                        break;
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= Duration::from_millis(100) {
            last_tick = Instant::now();
            app.tick();
        }

        // Fast path (500ms): service status, GPU/RAM stats
        if last_fast_refresh.elapsed() >= Duration::from_millis(500) {
            last_fast_refresh = Instant::now();
            app.services = crate::llamacpp::get_service_status().await;
            app.tokens_per_sec = None;
            if let Some(ref mut hw) = app.hardware {
                *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware_live)
                    .await
                    .unwrap_or_default();
            }
        }

        // Slow path (2s): disk usage
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            app.update_disk();
        }
    }

    terminal_restore.restore()?;
    Ok(())
}
