use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
use ozone_core::paths;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    catalog::CatalogRecord,
    hardware::HardwareProfile,
    launch_config::{self, LaunchPlan, RecommendationMode},
    prefs::SavedLaunchProfile,
    llamacpp::{self, ServiceStatus},
};
#[cfg(any(feature = "analyze", feature = "bench", feature = "profiling-ui"))]
use crate::bench;
#[cfg(any(feature = "analyze", feature = "bench", feature = "eval", feature = "profiling-ui"))]
use crate::db::{self, BenchmarkRow, ProfileRow};

// Re-export profiling action types so existing `crate::profiling::*` imports
// continue to work after they were moved to profiling_actions.rs.
pub use crate::profiling_actions::{FailureClass, ProfilingAction, WarningSeverity};
pub mod workflow;
pub use self::workflow::run_workflow;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ProfilingBackend {
    #[default]
    LlamaCpp,
}

impl ProfilingBackend {
    fn resolve_backend(&self) -> Option<bench::BenchBackend> {
        match self {
            ProfilingBackend::LlamaCpp => llamacpp::resolved_llamacpp_server_path()
                .ok()
                .map(|server_path| bench::BenchBackend::LlamaCpp { server_path }),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProfilingBackend::LlamaCpp => "llama.cpp",
        }
    }
}





#[derive(Debug, Clone)]
pub struct ProfilingWarning {
    pub severity: WarningSeverity,
    pub message: String,
}



#[derive(Debug, Clone)]
pub struct RecommendedProfile {
    pub profile_name: String,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u32,
    pub quant_v: u32,
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
    pub saved_profile_report: Option<SavedProfileReport>,
    pub auto_saved_profile: Option<crate::prefs::SavedLaunchProfile>,
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
        self.retry_action.into_iter().collect()
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
    Completed(Box<ProfilingSuccessReport>),
    Failed(Box<ProfilingFailureReport>),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct WorkflowRequest {
    pub record: CatalogRecord,
    pub hardware: HardwareProfile,
    pub action: ProfilingAction,
    pub profiling_backend: ProfilingBackend,
    pub launch_plan_override: Option<LaunchPlan>,
    pub launch_profile_name: Option<String>,
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

#[derive(Debug, Clone)]
pub struct SavedProfileReport {
    pub profile_name: String,
    pub benchmark_count: usize,
    pub ok_benchmark_count: usize,
    pub latest_tokens_per_sec: Option<f64>,
    pub best_tokens_per_sec: Option<f64>,
    pub latest_time_to_first_token_ms: Option<u32>,
    pub latest_vram_peak_mb: Option<u32>,
    pub latest_ram_peak_mb: Option<u32>,
}

pub(super) fn send_completed(tx: &UnboundedSender<WorkflowEvent>, report: ProfilingSuccessReport) {
    let _ = tx.send(WorkflowEvent::Completed(Box::new(report)));
}

pub(super) fn send_failed(tx: &UnboundedSender<WorkflowEvent>, report: ProfilingFailureReport) {
    let _ = tx.send(WorkflowEvent::Failed(Box::new(report)));
}

/// Resolve the path to the llama-server binary.
///
/// Falls back to `"llama-server"` (hoping it is on $PATH) when the
/// configured path cannot be resolved.
pub fn launcher_path() -> PathBuf {
    llamacpp::resolved_llamacpp_server_path().unwrap_or_else(|_| PathBuf::from("llama-server"))
}

/// Resolve the directory path for exported launch profiles.
pub fn presets_path() -> PathBuf {
    ozone_core::paths::runtime_profiles_path()
}

fn backend_log_path() -> Option<PathBuf> {
    paths::llamacpp_log_path()
}

pub(super) fn llamacpp_export_dir() -> PathBuf {
    paths::data_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("ozone")
    })
}

pub(super) fn export_llamacpp_profiles(profiles: &[ProfileRow]) -> anyhow::Result<PathBuf> {
    let dir = llamacpp_export_dir();
    std::fs::create_dir_all(&dir)?;

    let threads: usize = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(crate::launch_config::DEFAULT_THREADS as usize);

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

fn backend_log_suggestion() -> String {
    backend_log_path()
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

fn build_saved_profile_report(
    profile_name: &str,
    benchmarks: &[BenchmarkRow],
) -> Option<SavedProfileReport> {
    let rows: Vec<&BenchmarkRow> = benchmarks
        .iter()
        .filter(|row| row.launch_profile_name.as_deref() == Some(profile_name))
        .collect();
    if rows.is_empty() {
        return None;
    }

    let latest = rows
        .iter()
        .max_by(|left, right| left.timestamp.cmp(&right.timestamp))?;
    let best_tokens_per_sec = rows
        .iter()
        .filter(|row| row.status == "ok")
        .map(|row| row.tokens_per_sec)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Some(SavedProfileReport {
        profile_name: profile_name.to_string(),
        benchmark_count: rows.len(),
        ok_benchmark_count: rows.iter().filter(|row| row.status == "ok").count(),
        latest_tokens_per_sec: (latest.status == "ok").then_some(latest.tokens_per_sec),
        best_tokens_per_sec,
        latest_time_to_first_token_ms: (latest.status == "ok")
            .then_some(latest.time_to_first_token_ms),
        latest_vram_peak_mb: Some(latest.vram_peak_mb),
        latest_ram_peak_mb: Some(latest.ram_peak_mb),
    })
}

/// Collect saved profile reports for all models that have benchmarks and profiles.
pub fn saved_profile_reports(
    model_name: &str,
    profiles: &[SavedLaunchProfile],
) -> Result<BTreeMap<String, SavedProfileReport>> {
    let conn = db::open()?;
    let benchmarks = db::get_benchmarks(&conn, model_name)?;
    let mut reports = BTreeMap::new();
    for profile in profiles {
        if let Some(report) = build_saved_profile_report(&profile.profile_name, &benchmarks) {
            reports.insert(profile.profile_name.clone(), report);
        }
    }
    Ok(reports)
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
        quant_k: profile.quant_k,
        quant_v: profile.quant_v,
        tokens_per_sec: profile.tokens_per_sec,
        vram_mb: profile.vram_mb,
    })
}

/// Given a catalog record and hardware, recommend the best launch plan.
///
/// Uses benchmark-backed recommendations when available, falls back to
/// heuristic estimates. Wraps `planner::plan_launch` with profiling-specific
/// adjustments.
pub fn preferred_launch_plan(
    record: &CatalogRecord,
    hardware: &HardwareProfile,
) -> Result<LaunchPlan> {
    let fallback_layers = launch_config::estimate_total_layers(record.model_size_gb);
    let topology = crate::gguf::inspect_model_topology(&record.model_path, fallback_layers);
    let history = load_history(&record.model_name).unwrap_or_default();
    if let Some(profile) = pick_recommended_profile(&history.profiles) {
        let total_layers = topology.total_layers;
        let gpu_layers = profile.gpu_layers;
        let mode = launch_config::classify_mode(gpu_layers, total_layers);
        let cpu_layers = launch_config::estimate_cpu_resident_layers(gpu_layers, total_layers);
        let (threads, blas_threads) = launch_config::recommend_threads(hardware, &mode);
        return Ok(LaunchPlan {
            model_name: record.model_name.clone(),
            context_size: profile.context_size,
            gpu_layers,
            total_layers,
            cpu_layers,
            quant_k: profile.quant_k as u8,
            quant_v: profile.quant_v as u8,
            n_parallel: 1,
            threads,
            blas_threads,
            mode,
            rationale: format!(
                "Using {} profile from benchmark history.",
                profile.profile_name
            ),
            estimated: false,
            estimated_vram_mb: profile.vram_mb,
            estimated_ram_mb: launch_config::estimate_ram_mb(
                profile.context_size,
                gpu_layers,
                record.model_size_gb,
                profile.quant_k as u8,
                profile.quant_v as u8,
                total_layers,
            ),
            source: "Profile".into(),
            layer_source_label: topology.source.label().to_string(),
            layer_source_note: topology.note,
        });
    }
    Ok(launch_config::plan_profiling_launch(record, hardware))
}

/// Build a profiling advisory for a model given its catalog record and hardware.
///
/// The advisory contains warnings, estimated VRAM, recommended actions, and
/// the computed launch plan. Used by the profiling UI to guide the user.
pub fn build_advisory(
    record: &CatalogRecord,
    hardware: Option<&HardwareProfile>,
    services: &ServiceStatus,
) -> Result<ProfilingAdvisory> {
    let history = load_history(&record.model_name).unwrap_or_default();
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

    let mut available_actions = vec![recommended_action];
    for action in [
        ProfilingAction::QuickSweep,
        ProfilingAction::FullSweep,
        ProfilingAction::SingleBenchmark,
        ProfilingAction::GenerateProfiles,
        ProfilingAction::ExportPresets,
        ProfilingAction::LaunchRecommended,
        ProfilingAction::ImportSpecs,
        ProfilingAction::ThreadSweep,
    ] {
        let allowed = match action {
            ProfilingAction::LaunchRecommended => recommended_profile.is_some(),
            ProfilingAction::GenerateProfiles => history.ok_benchmark_count >= 2,
            ProfilingAction::ExportPresets => history.profile_count > 0,
            ProfilingAction::ImportSpecs => true, // always available
            ProfilingAction::ThreadSweep => model_ok && launcher_ok,
            ProfilingAction::QuickSweep
            | ProfilingAction::FullSweep
            | ProfilingAction::SingleBenchmark => model_ok && launcher_ok,
            ProfilingAction::BenchmarkSavedProfile => false,
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
                "Configured llama.cpp server is missing or not executable: {}.",
                launcher.display()
            ),
        });
    }
    if services.llamacpp_running {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Warning,
            message: "Profiling will interrupt the currently running managed llama.cpp runtime."
                .into(),
        });
    } else {
        warnings.push(ProfilingWarning {
            severity: WarningSeverity::Info,
            message: "Profiling clears the managed llama.cpp runtime before it starts.".into(),
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
        if plan.layer_source_label != crate::gguf::TopologySource::GgufMetadata.label() {
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
        ProfilingAction::BenchmarkSavedProfile => {
            "Benchmarking saved profiles starts from Configure Hub after you pick a specific config.".into()
        }
        ProfilingAction::FullSweep => "A full sweep is useful when you want broader context/quant coverage.".into(),
        ProfilingAction::ExportPresets => "Profiles already exist and can be exported directly into the launcher preset file.".into(),
        ProfilingAction::ImportSpecs => "Capturing system specs lets Ozone skip hardware polling for 24 hours.".into(),
        ProfilingAction::ThreadSweep => "Thread sweep finds the optimal CPU thread count for your model.".into(),
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

pub(super) fn build_success_report(
    record: &CatalogRecord,
    action: ProfilingAction,
    launch_profile_name: Option<&str>,
) -> Result<ProfilingSuccessReport> {
    let history = load_history(&record.model_name)?;
    let saved_profile_report = match launch_profile_name {
        Some(profile_name) => {
            let conn = db::open()?;
            let benchmarks = db::get_benchmarks(&conn, &record.model_name)?;
            build_saved_profile_report(profile_name, &benchmarks)
        }
        None => None,
    };
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
        ProfilingAction::BenchmarkSavedProfile => {
            match launch_profile_name {
                Some(profile_name) => {
                    format!("Saved profile '{profile_name}' was benchmarked and its report was refreshed.")
                }
                None => "Saved profile benchmark completed and its report was refreshed.".into(),
            }
        }
        ProfilingAction::GenerateProfiles => {
            "Profiles were generated from successful benchmark history.".into()
        }
        ProfilingAction::ExportPresets => {
            format!("Profile export completed: {}", presets_path().display())
        }
        ProfilingAction::LaunchRecommended
        | ProfilingAction::ReviewIssue
        | ProfilingAction::ImportSpecs
        | ProfilingAction::ThreadSweep => "Workflow finished.".into(),
    };

    let mut suggestions = Vec::new();
    if let Some(profile_name) = launch_profile_name {
        suggestions.push(format!(
            "Review the saved profile report for '{profile_name}' in Configure Hub before launching."
        ));
    }
    if history.profile_count > 0 {
        suggestions.push("Launch the recommended profile or export it for reuse.".into());
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
        saved_profile_report,
        suggestions,
        export_detail: None,
        auto_saved_profile: None,
    })
}

pub(super) fn build_failure_report(
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
    } else if status == Some("oom") || lower.contains("out of memory") || lower.contains("oom") {
        FailureClass::OomOrOvercommit
    } else if !(launcher.exists() && is_executable(&launcher)) {
        FailureClass::LauncherMissing
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
                "Restore the configured llama.cpp server binary and make it executable: {}.",
                launcher.display()
            ),
            "Re-run launcher discovery after fixing the llama.cpp install.".into(),
        ],
        FailureClass::LauncherBrokenInstall => vec![
            format!(
                "The configured llama.cpp server behind {} looks broken; repair or replace it before retrying.",
                launcher.display()
            ),
            "Run llama-server manually once to confirm it can start cleanly.".into(),
        ],
        FailureClass::BackendTimeout => vec![
            "Retry with a single benchmark or a quick sweep instead of the current action.".into(),
            backend_log_suggestion(),
        ],
        FailureClass::OomOrOvercommit => vec![
            "Lower context size or GPU layers before retrying.".into(),
            "Prefer a quick sweep so Ozone can search for a safer mixed-memory configuration."
                .into(),
        ],
        FailureClass::GenerationHttpError => vec![
            "Retry a single benchmark to validate the backend before sweeping again.".into(),
            backend_log_suggestion(),
        ],
        FailureClass::Unknown => vec![
            "Retry the recommended single benchmark first to narrow the failure surface.".into(),
            backend_log_suggestion(),
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
        log_path: backend_log_path(),
    }
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
                quant_k: 1,
                quant_v: 1,
                note: "sample".into(),
                source: RecSource::Heuristic,
            },
            benchmark: Some(BenchmarkRun {
                context_size: 4096,
                gen_speed: 24.0,
                gpu_layers: -1,
                quant_k: 1,
                quant_v: 1,
                vram_mb: 7200,
            }),
            benchmark_count: 0,
            source_priority: 2,
        }
    }

    #[test]
    fn recommended_profile_prefers_speed() {
        let profiles = vec![
            ProfileRow {
                model_name: "sample.gguf".into(),
                profile_name: "context".into(),
                gpu_layers: 20,
                context_size: 8192,
                quant_k: 1,
                quant_v: 1,
                tokens_per_sec: 10.0,
                vram_mb: 5000,
                source: "auto".into(),
                created_at: "now".into(),
            },
            ProfileRow {
                model_name: "sample.gguf".into(),
                profile_name: "speed".into(),
                gpu_layers: -1,
                context_size: 4096,
                quant_k: 1,
                quant_v: 1,
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
    fn saved_profile_report_aggregates_latest_and_best_metrics() {
        let benchmarks = vec![
            BenchmarkRow {
                model_name: "sample.gguf".into(),
                model_size_gb: 7.0,
                gpu_layers: 20,
                context_size: 8192,
                quant_k: 1,
                quant_v: 1,
                threads: 8,
                tokens_per_sec: 11.0,
                time_to_first_token_ms: 500,
                vram_peak_mb: 7600,
                ram_peak_mb: 6200,
                total_tokens: 100,
                total_time_ms: 9000,
                status: "ok".into(),
                gpu_name: "GPU".into(),
                gpu_vram_mb: 12000,
                ram_total_mb: 32000,
                timestamp: "2026-04-21T00:00:00+00:00".into(),
                notes: String::new(),
                launch_profile_name: Some("custom-1".into()),
            },
            BenchmarkRow {
                model_name: "sample.gguf".into(),
                model_size_gb: 7.0,
                gpu_layers: 20,
                context_size: 8192,
                quant_k: 1,
                quant_v: 1,
                threads: 8,
                tokens_per_sec: 13.5,
                time_to_first_token_ms: 480,
                vram_peak_mb: 7700,
                ram_peak_mb: 6300,
                total_tokens: 100,
                total_time_ms: 8000,
                status: "ok".into(),
                gpu_name: "GPU".into(),
                gpu_vram_mb: 12000,
                ram_total_mb: 32000,
                timestamp: "2026-04-22T00:00:00+00:00".into(),
                notes: String::new(),
                launch_profile_name: Some("custom-1".into()),
            },
        ];

        let report =
            build_saved_profile_report("custom-1", &benchmarks).expect("saved profile report");

        assert_eq!(report.profile_name, "custom-1");
        assert_eq!(report.benchmark_count, 2);
        assert_eq!(report.latest_tokens_per_sec, Some(13.5));
        assert_eq!(report.best_tokens_per_sec, Some(13.5));
        assert_eq!(report.latest_time_to_first_token_ms, Some(480));
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
                ..Default::default()
            }),
            &ServiceStatus {
                llamacpp_running: false,
                llamacpp_model: None,
            },
        )
        .expect("advisory should build");

        assert!(advisory.warnings.iter().any(|warning| warning
            .message
            .contains("fell back to file-size estimation")));
    }
}
