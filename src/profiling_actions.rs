/// Action types for the profiling workflow.
///
/// Extracted from `profiling.rs` during Wave 2.5 to keep the profiling module
/// focused on orchestration logic while action definitions live separately.
/// Actions the user can take from the profiling UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilingAction {
    QuickSweep,
    FullSweep,
    SingleBenchmark,
    BenchmarkSavedProfile,
    GenerateProfiles,
    ExportPresets,
    LaunchRecommended,
    ReviewIssue,
    /// Capture and save system hardware profile to disk for offline reuse.
    ImportSpecs,
    /// Sweep thread counts to find optimal CPU parallelism for this model.
    ThreadSweep,
}

impl ProfilingAction {
    pub fn label(&self) -> &'static str {
        match self {
            ProfilingAction::QuickSweep => "Run quick sweep",
            ProfilingAction::FullSweep => "Run full sweep",
            ProfilingAction::SingleBenchmark => "Run single benchmark",
            ProfilingAction::BenchmarkSavedProfile => "Benchmark saved profile",
            ProfilingAction::GenerateProfiles => "Generate profiles",
            ProfilingAction::ExportPresets => "Export presets",
            ProfilingAction::ImportSpecs => "Import system specs",
            ProfilingAction::ThreadSweep => "Sweep thread counts",
            ProfilingAction::LaunchRecommended => "Launch recommended profile",
            ProfilingAction::ReviewIssue => "Review issue report",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ProfilingAction::QuickSweep => "Binary-search a safe speed/context pair quickly.",
            ProfilingAction::FullSweep => {
                "Explore a wider context/quant range for deeper coverage."
            }
            ProfilingAction::SingleBenchmark => "Validate one recommended configuration first.",
            ProfilingAction::BenchmarkSavedProfile => {
                "Benchmark the selected saved launch profile and keep its metrics attached."
            }
            ProfilingAction::GenerateProfiles => {
                "Create speed/context profiles from benchmark history."
            }
            ProfilingAction::ExportPresets => {
                "Write the best saved profile export for runtime reuse."
            }
            ProfilingAction::LaunchRecommended => {
                "Use the best available profile and launch the backend."
            }
            ProfilingAction::ImportSpecs => "Capture and save system hardware specs to disk.",
            ProfilingAction::ThreadSweep => {
                "Test thread counts 1-12 to find the sweet spot for this model."
            }
            ProfilingAction::ReviewIssue => "Show the blocking issue and recommended fixes.",
        }
    }

    pub fn clears_backends(&self) -> bool {
        matches!(
            self,
            ProfilingAction::QuickSweep
                | ProfilingAction::FullSweep
                | ProfilingAction::SingleBenchmark
                | ProfilingAction::BenchmarkSavedProfile
        )
    }
}

// ---------------------------------------------------------------------------
// WarningSeverity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningSeverity {
    Info,
    Warning,
    Critical,
}

impl WarningSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            WarningSeverity::Info => "info",
            WarningSeverity::Warning => "warning",
            WarningSeverity::Critical => "critical",
        }
    }
}

// ---------------------------------------------------------------------------
// FailureClass
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureClass {
    InvalidModelPath,
    LauncherMissing,
    LauncherBrokenInstall,
    BackendTimeout,
    OomOrOvercommit,
    GenerationHttpError,
    Unknown,
}

impl FailureClass {
    pub fn title(&self) -> &'static str {
        match self {
            FailureClass::InvalidModelPath => "Model path is invalid",
            FailureClass::LauncherMissing => "Configured launcher is missing",
            FailureClass::LauncherBrokenInstall => "llama.cpp server install is broken",
            FailureClass::BackendTimeout => "llama.cpp server never became ready",
            FailureClass::OomOrOvercommit => "Model likely exceeded memory limits",
            FailureClass::GenerationHttpError => "Generation request failed",
            FailureClass::Unknown => "Profiling failed unexpectedly",
        }
    }
}
