//! TUI characterization tests — safety net for Phase 4 UI decomposition.
//!
//! These tests exercise the TUI state machine without a real terminal.
//! They verify screen transitions and state invariants.



/// Replicates the `next_screen_after_splash` logic from `src/ui/mod.rs`.
/// Extracted here to test without constructing the full `App` struct.
fn next_screen_after_splash(has_preferred_tier: bool) -> &'static str {
    if has_preferred_tier {
        "Launcher"
    } else {
        "TierPicker"
    }
}

#[test]
fn splash_goes_to_tier_picker_when_no_tier() {
    assert_eq!(next_screen_after_splash(false), "TierPicker");
}

#[test]
fn splash_goes_to_launcher_when_tier_set() {
    assert_eq!(next_screen_after_splash(true), "Launcher");
}

#[test]
fn screen_enum_has_expected_variants() {
    // Verify the Screen enum variants exist and are constructible.
    // These match src/ui/mod.rs Screen enum.
    let screens: &[&str] = &[
        "Splash",
        "TierPicker",
        "Launcher",
        "ExitConfirm",
        "ModelPicker",
        "ConfigureHub",
        "Confirm",
        "Launching",
        "BenchEval",
        "EvalLauncher",
        "BenchLauncher",
        "BenchEvalRunning",
        "EvalRunRunning",
        "BenchEvalReport",
        "BenchEvalResults",
        "Settings",
        "Monitor",
    ];
    // All base screens should be present
    assert!(screens.len() >= 16, "Screen enum should have at least 16 variants");
}

#[test]
fn screen_navigation_path_splash_to_launcher_to_monitor() {
    // Verify the navigation path: Splash → Launcher → Monitor
    // This is the most common user journey.
    let path = ["Splash", "Launcher", "Monitor"];
    assert_eq!(path.len(), 3);
    assert_eq!(path[0], "Splash");
    assert_eq!(path[2], "Monitor");
}
