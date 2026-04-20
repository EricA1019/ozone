use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ozone_core::paths;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::{
    analyze, bench,
    catalog::CatalogRecord,
    db::{self, ProfileRow},
    hardware::HardwareProfile,
    planner::{self, LaunchPlan, RecommendationMode},
    processes::{self, ServiceStatus},
    sweep,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ProfilingBackend {
    #[default]
    KoboldCpp,
    LlamaCpp,
}

impl ProfilingBackend {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProfilingBackend::KoboldCpp => "KoboldCpp",
            ProfilingBackend::LlamaCpp => "llama.cpp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilingAction {
    QuickSweep,
    FullSweep,
    SingleBenchmark,
    GenerateProfiles,
    ExportPresets,
    LaunchRecommended,
    ReviewIssue,
}

impl ProfilingAction {
    pub fn label(&self) -> &'static str {
        match self {
            ProfilingAction::QuickSweep => "Run quick sweep",
            ProfilingAction::FullSweep => "Run full sweep",
            ProfilingAction::SingleBenchmark => "Run single benchmark",
            ProfilingAction::GenerateProfiles => "Generate profiles",
            ProfilingAction::ExportPresets => "Export presets",
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
            ProfilingAction::GenerateProfiles => {
                "Create speed/context profiles from benchmark history."
            }
            ProfilingAction::ExportPresets => "Write the best profile into koboldcpp-presets.conf.",
            ProfilingAction::LaunchRecommended => {
                "Use the best available profile and launch the backend."
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
        )
    }
}

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

#[derive(Debug, Clone)]
pub struct ProfilingWarning {
    pub severity: WarningSeverity,
    pub message: String,
}

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
            FailureClass::LauncherBrokenInstall => "KoboldCpp install is broken",
            FailureClass::BackendTimeout => "KoboldCpp never became ready",
            FailureClass::OomOrOvercommit => "Model likely exceeded memory limits",
            FailureClass::GenerationHttpError => "Generation request failed",
            FailureClass::Unknown => "Profiling failed unexpectedly",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecommendedProfile {
    pub profile_name: String,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_kv: u32,
    pub tokens_per_sec: f64,
    pub vram_mb: u32,
}

#[derive(Debug, Clone)]
pub struct ProfilingAdvisory {
    pub model_name: String,
    pub source_label: String,
    pub benchmark_count: usize,
    pub ok_benchmark_count: usize,
    pub profile_count: usize,
    pub rationale: String,
    pub recommended_action: ProfilingAction,
    pub available_actions: Vec<ProfilingAction>,
    pub warnings: Vec<ProfilingWarning>,
    pub estimated_vram_mb: Option<u32>,
    pub gpu_budget_mb: Option<u32>,
    pub launch_plan: Option<LaunchPlan>,
    pub recommended_profile: Option<RecommendedProfile>,
}

#[derive(Debug, Clone)]
pub struct ProfilingSuccessReport {
    pub model_name: String,
    pub action: ProfilingAction,
    pub summary: String,
    pub benchmark_count: usize,
    pub ok_benchmark_count: usize,
    pub profile_count: usize,
    pub best_tokens_per_sec: Option<f64>,
    pub recommended_profile: Option<RecommendedProfile>,
    pub suggestions: Vec<String>,
    pub export_detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfilingFailureReport {
    pub model_name: String,
    pub action: ProfilingAction,
    pub class: FailureClass,
    pub detail: String,
    pub suggestions: Vec<String>,
    pub retry_action: Option<ProfilingAction>,
    pub log_path: Option<PathBuf>,
}

impl ProfilingSuccessReport {
    pub fn available_actions(&self) -> Vec<ProfilingAction> {
        let mut actions = Vec::new();
        if self.recommended_profile.is_some() {
            actions.push(ProfilingAction::LaunchRecommended);
        }
        if self.ok_benchmark_count >= 2 {
            actions.push(ProfilingAction::GenerateProfiles);
        }
        if self.profile_count > 0 {
            actions.push(ProfilingAction::ExportPresets);
        }
        actions
    }
}

impl ProfilingFailureReport {
    pub fn available_actions(&self) -> Vec<ProfilingAction> {
        self.retry_action.clone().into_iter().collect()
    }
}

#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    Status {
        title: String,
        detail: String,
    },
    Progress {
        title: String,
        detail: String,
        current: u32,
        total: u32,
    },
    Completed(ProfilingSuccessReport),
    Failed(ProfilingFailureReport),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct WorkflowRequest {
    pub record: CatalogRecord,
    pub hardware: HardwareProfile,
    pub action: ProfilingAction,
    pub profiling_backend: ProfilingBackend,
}

#[derive(Debug, Clone, Default)]
struct ModelHistory {
    benchmark_count: usize,
    ok_benchmark_count: usize,
    profile_count: usize,
    best_tokens_per_sec: Option<f64>,
    profiles: Vec<ProfileRow>,
    newest_benchmark_ts: Option<String>,
}

pub fn launcher_path() -> PathBuf {
    processes::resolved_kobold_launcher_path()
}

pub fn presets_path() -> PathBuf {
    models_dir().join("koboldcpp-presets.conf")
}

fn models_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("models")
}

fn kobold_log_path() -> Option<PathBuf> {
    paths::kobold_log_path()
}

fn llamacpp_export_dir() -> PathBuf {
    paths::data_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("ozone")
    })
}

fn export_llamacpp_profiles(profiles: &[ProfileRow]) -> anyhow::Result<PathBuf> {
    let dir = llamacpp_export_dir();
    std::fs::create_dir_all(&dir)?;

    let threads: usize = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(8);

    // --- Shell script ---
    let sh_path = dir.join("llamacpp-profiles.sh");
    let mut sh = String::from(
        "#!/usr/bin/env bash\n\
         # llama.cpp profiles — generated by ozone profiler\n\
         # Usage: source this file, then call the profile function, e.g.: llamacpp_speed \"$MODEL\"\n\n",
    );
    for row in profiles {
        let fn_name = format!("llamacpp_{}", row.profile_name);
        sh.push_str(&format!(
            "{fn_name}() {{\n    local model=\"${{1:?model path required}}\"\n    llama-server -m \"$model\" -ngl {ngl} --ctx-size {ctx} --threads {t}\n}}\n\n",
            fn_name = fn_name,
            ngl = row.gpu_layers,
            ctx = row.context_size,
            t = threads,
        ));
    }
    std::fs::write(&sh_path, sh)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&sh_path)?.permissions();
        perms.set_mode(perms.mode() | 0o111);
        std::fs::set_permissions(&sh_path, perms)?;
    }

    // --- JSON file ---
    let json_path = dir.join("llamacpp-profiles.json");
    let json_entries: Vec<serde_json::Value> = profiles
        .iter()
        .map(|row| {
            serde_json::json!({
                "profile": row.profile_name,
                "ngl": row.gpu_layers,
                "ctx_size": row.context_size,
                "threads": threads,
                "tokens_per_sec": row.tokens_per_sec,
            })
        })
        .collect();
    let json_bytes = serde_json::to_vec_pretty(&json_entries)?;
    std::fs::write(json_path, json_bytes)?;

    Ok(sh_path)
}

fn kobold_log_suggestion() -> String {
    kobold_log_path()
        .map(|path| format!("Inspect the launcher log at {}.", path.display()))
        .unwrap_or_else(|| {
            "Inspect the launcher log once the ozone data directory is available.".into()
        })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

fn is_stale_timestamp(ts: &str) -> bool {
    use chrono::{DateTime, NaiveDateTime, Utc};
    // Try full ISO 8601 with timezone first (matches DB format like
    // "2026-04-12T10:21:38.352962996-04:00"), then fall back to bare
    // NaiveDateTime formats for simpler timestamps.
    let now = Utc::now();
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        let age = now.signed_duration_since(dt);
        return age.num_days() > 7;
    }
    let parsed = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S"));
    match parsed {
        Ok(dt) => {
            let age = now.naive_utc() - dt;
            age.num_days() > 7
        }
        Err(_) => false,
    }
}

fn load_history(model_name: &str) -> Result<ModelHistory> {
    let conn = db::open()?;
    let benchmarks = db::get_benchmarks(&conn, model_name)?;
    let profiles = db::get_profiles(&conn, model_name)?;
    let best_tokens_per_sec = benchmarks
        .iter()
        .filter(|row| row.status == "ok")
        .map(|row| row.tokens_per_sec)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let newest_benchmark_ts = benchmarks
        .iter()
        .map(|row| row.timestamp.as_str())
        .max()
        .map(|s| s.to_string());

    Ok(ModelHistory {
        benchmark_count: benchmarks.len(),
        ok_benchmark_count: benchmarks.iter().filter(|row| row.status == "ok").count(),
        profile_count: profiles.len(),
        best_tokens_per_sec,
        profiles,
        newest_benchmark_ts,
    })
}

fn profile_rank(name: &str) -> u8 {
    match name {
        "speed" => 0,
        "balanced" => 1,
        "context" => 2,
        _ => 3,
    }
}

fn pick_recommended_profile(profiles: &[ProfileRow]) -> Option<RecommendedProfile> {
    let mut sorted: Vec<&ProfileRow> = profiles.iter().collect();
    sorted.sort_by(|a, b| {
        profile_rank(&a.profile_name)
            .cmp(&profile_rank(&b.profile_name))
            .then_with(|| {
                b.tokens_per_sec
                    .partial_cmp(&a.tokens_per_sec)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let profile = sorted.first()?;
    Some(RecommendedProfile {
        profile_name: profile.profile_name.clone(),
        gpu_layers: profile.gpu_layers,
        context_size: profile.context_size,
        quant_kv: profile.quant_kv,
        tokens_per_sec: profile.tokens_per_sec,
        vram_mb: profile.vram_mb,
    })
}

pub fn preferred_launch_plan(
    record: &CatalogRecord,
    hardware: &HardwareProfile,
) -> Result<LaunchPlan> {
    let fallback_layers = planner::estimate_total_layers(record.model_size_gb);
    let topology = crate::gguf::inspect_model_topology(&record.model_path, fallback_layers);
    let history = load_history(&record.model_name)?;
    if let Some(profile) = pick_recommended_profile(&history.profiles) {
        let total_layers = topology.total_layers;
        let gpu_layers = profile.gpu_layers;
        let mode = planner::classify_mode(gpu_layers, total_layers);
        let cpu_layers = planner::estimate_cpu_resident_layers(gpu_layers, total_layers);
        let (threads, blas_threads) = planner::recommend_threads(hardware, &mode);
        return Ok(LaunchPlan {
            model_name: record.model_name.clone(),
            context_size: profile.context_size,
            gpu_layers,
            total_layers,
            cpu_layers,
            quant_kv: profile.quant_kv as u8,
            threads,
            blas_threads,
            mode,
            rationale: format!(
                "Using {} profile from benchmark history.",
                profile.profile_name
            ),
            estimated: false,
            estimated_vram_mb: profile.vram_mb,
            estimated_ram_mb: planner::estimate_ram_mb(
                profile.context_size,
                gpu_layers,
                record.model_size_gb,
                profile.quant_kv as u8,
                total_layers,
            ),
            source: "Profile".into(),
            layer_source_label: topology.source.label().to_string(),
            layer_source_note: topology.note,
        });
    }
    Ok(planner::plan_profiling_launch(record, hardware))
}

pub fn build_advisory(
    record: &CatalogRecord,
    hardware: Option<&HardwareProfile>,
    services: &ServiceStatus,
) -> Result<ProfilingAdvisory> {
    let history = load_history(&record.model_name)?;
    let launcher = launcher_path();
    let model_ok = record.model_path.exists();
    let launcher_ok = launcher.exists() && is_executable(&launcher);
    let launch_plan = hardware
        .map(|hw| preferred_launch_plan(record, hw))
        .transpose()?;
    let recommended_profile = pick_recommended_profile(&history.profiles);
    let (estimated_vram_mb, gpu_budget_mb) =
        if let (Some(hw), Some(plan)) = (hardware, launch_plan.as_ref()) {
            let budget = hw.gpu.as_ref().map(|gpu| (gpu.free_mb as f64 * 0.9) as u32);
            (Some(plan.estimated_vram_mb), budget)
        } else {
            (None, None)
        };

    let recommended_action = if !model_ok || !launcher_ok {
        ProfilingAction::ReviewIssue
    } else if history.profile_count > 0 {
        ProfilingAction::LaunchRecommended
    } else if history.ok_benchmark_count >= 2 {
        ProfilingAction::GenerateProfiles
    } else if hardware.and_then(|hw| hw.gpu.as_ref()).is_some() {
        ProfilingAction::QuickSweep
    } else {
        ProfilingAction::SingleBenchmark
    };

    let mut available_actions = vec![recommended_action.clone()];
    for action in [
        ProfilingAction::QuickSweep,
        ProfilingAction::FullSweep,
        ProfilingAction::SingleBenchmark,
        ProfilingAction::GenerateProfiles,
        ProfilingAction::ExportPresets,
        ProfilingAction::LaunchRecommended,
    ] {
        let allowed = match action {
            ProfilingAction::LaunchRecommended => recommended_profile.is_some(),
            ProfilingAction::GenerateProfiles => history.ok_benchmark_count >= 2,
            ProfilingAction::ExportPresets => history.profile_count > 0,
            ProfilingAction::QuickSweep
            | ProfilingAction::FullSweep
            | ProfilingAction::SingleBenchmark => model_ok && launcher_ok,
            ProfilingAction::ReviewIssue => false,
        };
        if allowed && !available_actions.contains(&action) {
            available_actions.push(action);
        }
    }
    if available_actions.is_empty() {
        available_actions.push(ProfilingAction::ReviewIssue);
    }

    let mut warnings = Vec::new();
    if !model_ok {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Critical,
            message: "The selected model path is missing or its symlink target is broken.".into(),
        });
    }
    if !launcher_ok {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Critical,
            message: format!(
                "Configured KoboldCpp launcher is missing or not executable: {}.",
                launcher.display()
            ),
        });
    }
    if services.kobold_running {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Warning,
            message: "Profiling will interrupt the currently running KoboldCpp backend.".into(),
        });
    } else {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Info,
            message: "Profiling clears KoboldCpp/Ollama runners before it starts.".into(),
        });
    }
    if history.benchmark_count == 0 {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Info,
            message: "No benchmark history exists for this model yet.".into(),
        });
    }
    if history.ok_benchmark_count >= 2 && history.profile_count == 0 {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Info,
            message: "You already have enough successful benchmarks to generate profiles.".into(),
        });
    }
    if let Some(ts) = &history.newest_benchmark_ts {
        if is_stale_timestamp(ts) {
            warnings.push(ProfilingWarning {
                severity: WarningSeverity::Info,
                message:
                    "Newest benchmark is over 7 days old — consider re-profiling for fresh data."
                        .into(),
            });
        }
    }
    if let (Some(hw), Some(plan)) = (hardware, launch_plan.as_ref()) {
        if let Some(gpu) = &hw.gpu {
            let safe_budget = (gpu.free_mb as f64 * 0.9) as u32;
            if plan.estimated_vram_mb > safe_budget {
                warnings.push(ProfilingWarning {
                    severity: WarningSeverity::Warning,
                    message: format!(
                        "Estimated VRAM {} MiB is above the safe budget of {} MiB.",
                        plan.estimated_vram_mb, safe_budget,
                    ),
                });
            }
        }
        if hw.ram_free_mb > 0 && plan.estimated_ram_mb as u64 > hw.ram_free_mb {
            warnings.push(ProfilingWarning {
                severity: WarningSeverity::Warning,
                message: format!(
                    "Estimated RAM {} MiB is above currently free system RAM {} MiB.",
                    plan.estimated_ram_mb, hw.ram_free_mb
                ),
            });
        }
        if plan.layer_source_label != crate::planner::TopologySource::GgufMetadata.label() {
            warnings.push(ProfilingWarning {
                severity: WarningSeverity::Info,
                message: plan
                    .layer_source_note
                    .clone()
                    .unwrap_or_else(|| {
                        "Layer count was estimated from model size because GGUF metadata was unavailable.".into()
                    }),
            });
        }
        if plan.mode == RecommendationMode::CpuOnly {
            warnings.push(ProfilingWarning {
                severity: WarningSeverity::Warning,
                message: format!(
                    "The current profiling start point is CPU-only ({} CPU-resident layers).",
                    plan.total_layers
                ),
            });
        } else if plan.cpu_layers > 0 {
            warnings.push(ProfilingWarning {
                severity: WarningSeverity::Warning,
                message: format!(
                    "The current profiling start point is mixed-memory ({} GPU / {} CPU-resident layers).",
                    plan.gpu_layers_display(),
                    plan.cpu_layers
                ),
            });
        }
    }

    let rationale = match recommended_action {
        ProfilingAction::ReviewIssue => "Fix the blocking issue before profiling so Ozone can produce useful results.".into(),
        ProfilingAction::LaunchRecommended => "Existing benchmark-backed profiles already cover this model, so launching the best one is the fastest next step.".into(),
        ProfilingAction::GenerateProfiles => "You already have enough successful benchmarks to generate speed/context profiles without another sweep.".into(),
        ProfilingAction::QuickSweep => "A quick sweep is the fastest way to discover a safe speed/context pair for this model.".into(),
        ProfilingAction::SingleBenchmark => "A single benchmark is the safest first check when GPU guidance is limited.".into(),
        ProfilingAction::FullSweep => "A full sweep is useful when you want broader context/quant coverage.".into(),
        ProfilingAction::ExportPresets => "Profiles already exist and can be exported directly into the launcher preset file.".into(),
    };

    Ok(ProfilingAdvisory {
        model_name: record.model_name.clone(),
        source_label: record.recommendation.source.label().into(),
        benchmark_count: history.benchmark_count,
        ok_benchmark_count: history.ok_benchmark_count,
        profile_count: history.profile_count,
        rationale,
        recommended_action,
        available_actions,
        warnings,
        estimated_vram_mb,
        gpu_budget_mb,
        launch_plan,
        recommended_profile,
    })
}

pub fn blocking_issue_report(record: &CatalogRecord) -> ProfilingFailureReport {
    build_failure_report(
        record,
        ProfilingAction::ReviewIssue,
        "The selected model or launcher path is not valid enough to start profiling.".into(),
        None,
    )
}

fn build_success_report(
    record: &CatalogRecord,
    action: ProfilingAction,
) -> Result<ProfilingSuccessReport> {
    let history = load_history(&record.model_name)?;
    let recommended_profile = pick_recommended_profile(&history.profiles);
    let summary = match action {
        ProfilingAction::QuickSweep => {
            "Quick sweep completed and stored fresh benchmark coverage.".into()
        }
        ProfilingAction::FullSweep => {
            "Full sweep completed and refreshed the benchmark frontier.".into()
        }
        ProfilingAction::SingleBenchmark => {
            "Single benchmark completed and stored its result.".into()
        }
        ProfilingAction::GenerateProfiles => {
            "Profiles were generated from successful benchmark history.".into()
        }
        ProfilingAction::ExportPresets => {
            format!("Preset export completed: {}", presets_path().display())
        }
        ProfilingAction::LaunchRecommended | ProfilingAction::ReviewIssue => {
            "Workflow finished.".into()
        }
    };

    let mut suggestions = Vec::new();
    if history.profile_count > 0 {
        suggestions
            .push("Launch the recommended profile or export it to koboldcpp-presets.conf.".into());
    } else if history.ok_benchmark_count >= 2 {
        suggestions.push(
            "Generate profiles now so the launcher can reuse the best speed/context pair.".into(),
        );
    } else {
        suggestions.push("Run a fuller sweep if you want broader context coverage.".into());
    }

    Ok(ProfilingSuccessReport {
        model_name: record.model_name.clone(),
        action,
        summary,
        benchmark_count: history.benchmark_count,
        ok_benchmark_count: history.ok_benchmark_count,
        profile_count: history.profile_count,
        best_tokens_per_sec: history.best_tokens_per_sec,
        recommended_profile,
        suggestions,
        export_detail: None,
    })
}

fn build_failure_report(
    record: &CatalogRecord,
    action: ProfilingAction,
    detail: String,
    status: Option<&str>,
) -> ProfilingFailureReport {
    let launcher = launcher_path();
    let history = load_history(&record.model_name).unwrap_or_default();
    let lower = detail.to_lowercase();

    let class = if !record.model_path.exists() {
        FailureClass::InvalidModelPath
    } else if lower.contains("failed to extract")
        || lower.contains("failed to extract entry")
        || lower.contains("decompression resulted in return code")
        || lower.contains("cannot open shared object file")
        || lower.contains("error while loading shared libraries")
        || lower.contains("segmentation fault")
        || lower.contains("core dumped")
    {
        FailureClass::LauncherBrokenInstall
    } else if !(launcher.exists() && is_executable(&launcher)) {
        FailureClass::LauncherMissing
    } else if status == Some("oom") || lower.contains("out of memory") || lower.contains("oom") {
        FailureClass::OomOrOvercommit
    } else if status == Some("timeout")
        || lower.contains("did not start")
        || lower.contains("timeout")
        || lower.contains("not available via api")
    {
        FailureClass::BackendTimeout
    } else if status == Some("error")
        || lower.contains("generation failed")
        || lower.contains("http ")
    {
        FailureClass::GenerationHttpError
    } else {
        FailureClass::Unknown
    };

    let mut suggestions = match class {
        FailureClass::InvalidModelPath => vec![
            "Repair the GGUF symlink or restore the model file in ~/models.".into(),
            "Re-open the model picker after the file resolves correctly.".into(),
        ],
        FailureClass::LauncherMissing => vec![
            format!(
                "Restore the configured launcher and make it executable: {}.",
                launcher.display()
            ),
            format!(
                "Set {}=/path/to/launch-koboldcpp.sh if you want ozone to use a repaired wrapper elsewhere.",
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
        ],
        FailureClass::LauncherBrokenInstall => vec![
            format!(
                "The configured KoboldCpp install behind {} looks broken; repair or replace it before retrying.",
                launcher.display()
            ),
            format!(
                "Set {}=/path/to/launch-koboldcpp.sh to point ozone at a repaired launcher.",
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
            "Run the launcher script manually once to confirm KoboldCpp can start.".into(),
        ],
        FailureClass::BackendTimeout => vec![
            "Retry with a single benchmark or a quick sweep instead of the current action.".into(),
            kobold_log_suggestion(),
        ],
        FailureClass::OomOrOvercommit => vec![
            "Lower context size or GPU layers before retrying.".into(),
            "Prefer a quick sweep so Ozone can search for a safer mixed-memory configuration."
                .into(),
        ],
        FailureClass::GenerationHttpError => vec![
            "Retry a single benchmark to validate the backend before sweeping again.".into(),
            kobold_log_suggestion(),
        ],
        FailureClass::Unknown => vec![
            "Retry the recommended single benchmark first to narrow the failure surface.".into(),
            kobold_log_suggestion(),
        ],
    };

    if history.profile_count > 0 {
        suggestions.push("You already have profiles for this model, so launching the recommended profile may be safer than profiling again.".into());
    }

    let retry_action = match class {
        FailureClass::InvalidModelPath
        | FailureClass::LauncherMissing
        | FailureClass::LauncherBrokenInstall => None,
        FailureClass::OomOrOvercommit => Some(ProfilingAction::QuickSweep),
        FailureClass::BackendTimeout
        | FailureClass::GenerationHttpError
        | FailureClass::Unknown => Some(ProfilingAction::SingleBenchmark),
    };

    ProfilingFailureReport {
        model_name: record.model_name.clone(),
        action,
        class,
        detail,
        suggestions,
        retry_action,
        log_path: kobold_log_path(),
    }
}

pub async fn run_workflow(
    request: WorkflowRequest,
    tx: UnboundedSender<WorkflowEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let action = request.action.clone();
    if action == ProfilingAction::ReviewIssue {
        let report = build_failure_report(
            &request.record,
            action,
            "The selected model or launcher path is not valid enough to start profiling.".into(),
            None,
        );
        let _ = tx.send(WorkflowEvent::Failed(report));
        return Ok(());
    }

    // ExportPresets only reads from DB and writes files — no launcher needed.
    if action == ProfilingAction::ExportPresets {
        let use_llamacpp = processes::resolved_llamacpp_server_path()
            .ok()
            .map(|p| p.exists())
            .unwrap_or(false)
            || !launcher_path().exists();

        let conn = db::open()?;
        let profiles = db::get_profiles(&conn, &request.record.model_name)?;

        if use_llamacpp {
            let sh_path = llamacpp_export_dir().join("llamacpp-profiles.sh");
            let _ = tx.send(WorkflowEvent::Status {
                title: "Export".into(),
                detail: format!("Exporting llama.cpp profiles to {}…", sh_path.display()),
            });
            match export_llamacpp_profiles(&profiles) {
                Ok(out) => {
                    let mut report = build_success_report(&request.record, request.action)?;
                    report.export_detail = Some(format!("llama.cpp: {}", out.display()));
                    let _ = tx.send(WorkflowEvent::Completed(report));
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
            }
        } else {
            let _ = tx.send(WorkflowEvent::Status {
                title: "Export".into(),
                detail: format!(
                    "Exporting KoboldCpp presets to {}…",
                    presets_path().display()
                ),
            });
            match analyze::export_presets_conf_quiet(
                &presets_path(),
                Some(&request.record.model_name),
            ) {
                Ok(_count) => {
                    let mut report = build_success_report(&request.record, request.action)?;
                    if let Ok(content) = std::fs::read_to_string(presets_path()) {
                        let model_lines: Vec<&str> = content
                            .lines()
                            .filter(|line| line.contains(&request.record.model_name))
                            .collect();
                        if !model_lines.is_empty() {
                            report.export_detail = Some(model_lines.join("\n"));
                        }
                    }
                    let _ = tx.send(WorkflowEvent::Completed(report));
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
            }
        }
        return Ok(());
    }

    if !request.record.model_path.exists() {
        let report = build_failure_report(
            &request.record,
            action,
            "Profiling prerequisites are missing.".into(),
            None,
        );
        let _ = tx.send(WorkflowEvent::Failed(report));
        return Ok(());
    }

    if cancel.is_cancelled() {
        let _ = tx.send(WorkflowEvent::Cancelled);
        return Ok(());
    }

    match request.action {
        ProfilingAction::QuickSweep | ProfilingAction::FullSweep => {
            let quick = matches!(request.action, ProfilingAction::QuickSweep);
            let (context_sizes, quant_kv_levels) = if quick {
                (vec![4096, 8192], vec![1u8])
            } else {
                (vec![2048, 4096, 8192, 16384], vec![1u8, 2])
            };
            let gpu_vram_budget_mb = request
                .hardware
                .gpu
                .as_ref()
                .map(|gpu| (gpu.total_mb as f64 * 0.9) as u32)
                .unwrap_or(0);
            let backend = processes::resolved_backend_for_profiling().ok_or_else(|| {
                anyhow!("No profiling backend available (KoboldCpp or llama-server not found)")
            })?;
            let seed_plan = planner::plan_profiling_launch(&request.record, &request.hardware);
            let config = sweep::SweepConfig {
                model_name: request.record.model_name.clone(),
                model_path: request.record.model_path.clone(),
                backend,
                model_size_gb: request.record.model_size_gb,
                total_layers: seed_plan.total_layers,
                context_sizes,
                quant_kv_levels,
                gpu_vram_budget_mb,
                ram_total_mb: request.hardware.ram_total_mb as u32,
            };
            let _ = tx.send(WorkflowEvent::Status {
                title: "Profiling".into(),
                detail: format!("Starting {}…", request.action.label().to_lowercase()),
            });
            let cancel_ref = cancel.clone();
            match sweep::run_sweep_with_progress(config, |progress| {
                if cancel_ref.is_cancelled() {
                    return;
                }
                let _ = tx.send(WorkflowEvent::Progress {
                    title: if quick {
                        "Quick sweep".into()
                    } else {
                        "Full sweep".into()
                    },
                    detail: progress.message,
                    current: progress.current,
                    total: progress.total,
                });
            })
            .await
            {
                Ok(_result) if cancel.is_cancelled() => {
                    let _ = tx.send(WorkflowEvent::Cancelled);
                }
                Ok(result) if result.configs_tested > 0 => {
                    // Auto-chain: generate profiles after sweep success
                    let _ = tx.send(WorkflowEvent::Status {
                        title: "Generating profiles".into(),
                        detail: "Creating speed/context profiles from benchmark data…".into(),
                    });
                    let _ = analyze::generate_profiles_quiet(&request.record.model_name);
                    let report = build_success_report(&request.record, request.action)?;
                    let _ = tx.send(WorkflowEvent::Completed(report));
                }
                Ok(_) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        "Sweep completed without any successful benchmark configurations.".into(),
                        Some("oom"),
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
            }
        }
        ProfilingAction::SingleBenchmark => {
            if cancel.is_cancelled() {
                let _ = tx.send(WorkflowEvent::Cancelled);
                return Ok(());
            }
            let backend = processes::resolved_backend_for_profiling().ok_or_else(|| {
                anyhow!("No profiling backend available (KoboldCpp or llama-server not found)")
            })?;
            let plan = planner::plan_profiling_launch(&request.record, &request.hardware);
            let _ = tx.send(WorkflowEvent::Status {
                title: "Benchmark".into(),
                detail: format!(
                    "Benchmarking ctx={} gpu={}/{} cpu={} qkv={}",
                    plan.context_size,
                    plan.gpu_layers_display(),
                    plan.total_layers,
                    plan.cpu_layers,
                    plan.quant_kv,
                ),
            });
            match bench::run_benchmark_with_progress(
                &request.record.model_name,
                &request.record.model_path,
                &backend,
                plan.gpu_layers,
                plan.context_size,
                plan.quant_kv,
                plan.threads,
                |progress| {
                    let _ = tx.send(WorkflowEvent::Status {
                        title: format!("Benchmark · {}", progress.stage),
                        detail: progress.message,
                    });
                },
            )
            .await
            {
                Ok(_result) if cancel.is_cancelled() => {
                    let _ = tx.send(WorkflowEvent::Cancelled);
                }
                Ok(result) => {
                    let _ = bench::store_result(
                        &request.record.model_name,
                        request.record.model_size_gb,
                        plan.gpu_layers,
                        plan.context_size,
                        plan.quant_kv as u32,
                        plan.threads.unwrap_or(0),
                        &result,
                    );
                    if result.status == "ok" {
                        let report = build_success_report(&request.record, request.action)?;
                        let _ = tx.send(WorkflowEvent::Completed(report));
                    } else {
                        let report = build_failure_report(
                            &request.record,
                            request.action,
                            format!("Benchmark ended with status '{}'.", result.status),
                            Some(&result.status),
                        );
                        let _ = tx.send(WorkflowEvent::Failed(report));
                    }
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
            }
        }
        ProfilingAction::GenerateProfiles => {
            let _ = tx.send(WorkflowEvent::Status {
                title: "Profiles".into(),
                detail: "Generating profiles from benchmark history…".into(),
            });
            match analyze::generate_profiles_quiet(&request.record.model_name) {
                Ok(_) => {
                    let report = build_success_report(&request.record, request.action)?;
                    let _ = tx.send(WorkflowEvent::Completed(report));
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    let _ = tx.send(WorkflowEvent::Failed(report));
                }
            }
        }
        ProfilingAction::LaunchRecommended | ProfilingAction::ReviewIssue => {}
        // ExportPresets is handled before the launcher prerequisite check above.
        ProfilingAction::ExportPresets => unreachable!("ExportPresets handled before match"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{BenchmarkRun, RecSource, Recommendation};

    fn sample_record(path: &str) -> CatalogRecord {
        CatalogRecord {
            model_name: "sample.gguf".into(),
            model_path: PathBuf::from(path),
            model_size_gb: 7.0,
            recommendation: Recommendation {
                context_size: 4096,
                gpu_layers: -1,
                quant_kv: 1,
                note: "sample".into(),
                source: RecSource::Heuristic,
            },
            benchmark: Some(BenchmarkRun {
                context_size: 4096,
                gen_speed: 24.0,
                gpu_layers: -1,
                quant_kv: 1,
                vram_mb: 7200,
                timestamp_ms: 0,
                model_size_gb: 7.0,
            }),
            benchmark_count: 0,
            source_priority: 2,
        }
    }

    #[test]
    fn recommended_profile_prefers_speed() {
        let profiles = vec![
            ProfileRow {
                id: None,
                model_name: "sample.gguf".into(),
                profile_name: "context".into(),
                gpu_layers: 20,
                context_size: 8192,
                quant_kv: 1,
                tokens_per_sec: 10.0,
                vram_mb: 5000,
                source: "auto".into(),
                created_at: "now".into(),
            },
            ProfileRow {
                id: None,
                model_name: "sample.gguf".into(),
                profile_name: "speed".into(),
                gpu_layers: -1,
                context_size: 4096,
                quant_kv: 1,
                tokens_per_sec: 42.0,
                vram_mb: 8000,
                source: "auto".into(),
                created_at: "now".into(),
            },
        ];
        let picked = pick_recommended_profile(&profiles).expect("expected a profile");
        assert_eq!(picked.profile_name, "speed");
        assert_eq!(picked.context_size, 4096);
    }

    #[test]
    fn invalid_model_path_classifies_as_path_issue() {
        let record = sample_record("/definitely/missing/model.gguf");
        let report = build_failure_report(
            &record,
            ProfilingAction::SingleBenchmark,
            "anything".into(),
            None,
        );
        assert_eq!(report.class, FailureClass::InvalidModelPath);
        assert!(report.retry_action.is_none());
    }

    #[test]
    fn oom_failure_suggests_quick_sweep_retry() {
        let record = sample_record(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        let report = build_failure_report(
            &record,
            ProfilingAction::SingleBenchmark,
            "out of memory while generating".into(),
            Some("oom"),
        );
        assert_eq!(report.class, FailureClass::OomOrOvercommit);
        assert_eq!(report.retry_action, Some(ProfilingAction::QuickSweep));
    }

    #[test]
    fn launcher_extract_failure_classifies_as_broken_install() {
        let record = sample_record(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        let report = build_failure_report(
            &record,
            ProfilingAction::SingleBenchmark,
            "[PYI-32814:ERROR] Failed to extract koboldcpp_cublas.so: decompression resulted in return code -3!".into(),
            None,
        );
        assert_eq!(report.class, FailureClass::LauncherBrokenInstall);
        assert!(report.retry_action.is_none());
    }

    #[test]
    fn stale_timestamp_detects_rfc3339() {
        // Real DB format: ISO 8601 with fractional seconds and timezone
        assert!(!is_stale_timestamp("2099-01-01T00:00:00.000000000+00:00"));
        assert!(is_stale_timestamp("2020-01-01T12:00:00.123456789-04:00"));
    }

    #[test]
    fn stale_timestamp_detects_naive_formats() {
        assert!(is_stale_timestamp("2020-01-01 12:00:00"));
        assert!(is_stale_timestamp("2020-01-01T12:00:00"));
        assert!(!is_stale_timestamp("2099-01-01 00:00:00"));
    }

    #[test]
    fn stale_timestamp_returns_false_for_garbage() {
        assert!(!is_stale_timestamp("not-a-date"));
        assert!(!is_stale_timestamp(""));
    }

    #[test]
    fn advisory_warns_when_layer_count_falls_back_to_size_heuristic() {
        let record = sample_record(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        let advisory = build_advisory(
            &record,
            Some(&HardwareProfile {
                gpu: None,
                ram_total_mb: 32000,
                ram_free_mb: 24000,
                ram_used_mb: 8000,
                cpu_logical: 8,
                cpu_physical: 4,
            }),
            &ServiceStatus {
                kobold_running: false,
                kobold_model: None,
                llamacpp_running: false,
                llamacpp_model: None,
                ollama_running: false,
                st_running: false,
            },
        )
        .expect("advisory should build");

        assert!(advisory.warnings.iter().any(|warning| warning
            .message
            .contains("fell back to file-size estimation")));
    }
}
