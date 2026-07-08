//! Plan building helpers — extracted from `ui/mod.rs`.
//!
//! These helpers construct, validate, and navigate launcher plans.
//! They are the glue between the App state and the launch/configure flow.

use crate::catalog::CatalogRecord;
use super::{App, Screen};

/// Determine the next screen after splash completes.
pub(super) fn next_screen_after_splash(app: &App) -> Screen {
    if app.prefs.preferred_tier.is_none() {
        Screen::TierPicker
    } else {
        Screen::Launcher
    }
}

/// Queue a pending launch for the currently selected plan.
pub(super) fn queue_launch(app: &mut App) {
    app.pending_launch_choice = Some(0);
}

/// Outcome of a launcher screen action.
pub(super) enum LauncherActionOutcome {
    Continue,
    Exit,
}

/// Get the currently selected catalog record from the app's current plan.
pub(super) fn selected_record(app: &App) -> Option<CatalogRecord> {
    app.current_plan.as_ref().and_then(|plan| {
        app.catalog
            .iter()
            .find(|record| record.model_name == plan.model_name)
            .cloned()
    })
}
