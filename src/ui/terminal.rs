//! Terminal lifecycle management for the TUI.
//!
//! Provides safe initialization and teardown of raw-mode terminal and
//! alternate screen buffer via `TerminalRestoreGuard`.

use crossterm::{
    cursor::Show,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io;
pub(crate) struct TerminalRestoreGuard {
    raw_mode_enabled: bool,
    alt_screen_entered: bool,
}

impl TerminalRestoreGuard {
    pub(crate) fn new() -> Self {
        Self {
            raw_mode_enabled: false,
            alt_screen_entered: false,
        }
    }

    pub(crate) fn mark_raw_mode_enabled(&mut self) {
        self.raw_mode_enabled = true;
    }

    pub(crate) fn mark_alt_screen_entered(&mut self) {
        self.alt_screen_entered = true;
    }

    pub(crate) fn restore(&mut self) -> io::Result<()> {
        let raw_mode_enabled = self.raw_mode_enabled;
        let alt_screen_entered = self.alt_screen_entered;
        self.raw_mode_enabled = false;
        self.alt_screen_entered = false;

        let mut first_error = None;
        if raw_mode_enabled {
            if let Err(error) = disable_raw_mode() {
                first_error = Some(error);
            }
        }
        if alt_screen_entered {
            let mut stdout = io::stdout();
            if let Err(error) = execute!(stdout, Show, LeaveAlternateScreen) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    /// Returns whether raw mode is currently enabled (for assertions).
    #[cfg(test)]
    pub(crate) fn is_raw_mode_enabled(&self) -> bool {
        self.raw_mode_enabled
    }

    /// Returns whether alternate screen is currently entered (for assertions).
    #[cfg(test)]
    pub(crate) fn is_alt_screen_entered(&self) -> bool {
        self.alt_screen_entered
    }
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if self.raw_mode_enabled {
            let _ = disable_raw_mode();
        }
        if self.alt_screen_entered {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, Show, LeaveAlternateScreen);
        }
    }
}