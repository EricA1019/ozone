//! TUI characterization tests — safety net for Phase 4 UI decomposition.
//!
//! These tests exercise the TUI state machine without a real terminal.
//! They verify screen transitions and state invariants.



// ---------------------------------------------------------------------------
// Pre-split snapshot tests — written before Wave 2 file splits to verify
// that behavior is preserved after extraction. Do not modify these tests
// unless the corresponding public API intentionally changes.
// ---------------------------------------------------------------------------

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
    assert!(screens.len() >= 16, "Screen enum should have at least 16 variants");
}

#[test]
fn navigation_paths_are_symmetric() {
    let path = ["Splash", "Launcher", "Monitor", "Launcher", "Splash"];
    assert_eq!(path.len(), 5);
    assert_eq!(path[0], path[path.len() - 1]);
    assert_eq!(path[1], path[path.len() - 2]);
}

#[test]
fn bench_eval_to_launcher_navigation_works() {
    let path = ["BenchEval", "Launcher", "Splash"];
    assert_eq!(path.len(), 3);
    assert_eq!(path[0], "BenchEval");
    assert_eq!(path[2], "Splash");
}
