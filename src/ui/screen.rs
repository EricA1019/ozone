/// Top-level screen identifiers for the TUI state machine.
///
/// Each variant represents a distinct user-visible screen. The `App` struct
/// holds the current screen and transitions between them via event handlers.
///
/// Extracted from `mod.rs` during Wave 2.2 to keep the module index focused
/// on declarations and re-exports.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Splash,
    TierPicker,
    Launcher,
    ExitConfirm,
    ModelPicker,
    ConfigureHub,
    Confirm,
    Launching,
    #[cfg(feature = "profiling-ui")]
    ProfileAdvisory,
    #[cfg(feature = "profiling-ui")]
    ProfileConfirm,
    #[cfg(feature = "profiling-ui")]
    ProfileRunning,
    #[cfg(feature = "profiling-ui")]
    ProfileSuccess,
    #[cfg(feature = "profiling-ui")]
    ProfileFailure,
    BenchEval,
    EvalLauncher,
    BenchLauncher,
    BenchEvalRunning,
    EvalRunRunning,
    BenchEvalReport,
    BenchEvalResults,
    Settings,
    Monitor,
}
