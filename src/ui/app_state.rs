//! Application state types — screen enum, action types, App struct,
//! and navigation helpers. Extracted from `mod.rs` for SRP compliance.
//!
//! This module depends on many sub-modules of `crate::ui` — imports use
//! `crate::ui::` paths to avoid circular dependency with `mod.rs`.

#[cfg(feature = "profiling-ui")]
use std::collections::BTreeMap;
use std::{
    path::PathBuf,
    time::Instant,
};

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::planner::LaunchPlan;
use crate::prefs::{Preferences, SavedLaunchProfile};
use crate::processes::{DiskSnapshot, ServiceStatus};
use crate::ui::command_overlay_flow::new_command_overlay;
use crate::ui::results::{ResultFile, ResultFileKind};
use crate::ui::tier_picker;

use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;

#[cfg(feature = "profiling-ui")]
use crate::profiling::{
    ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent,
};
use crate::ui::bench_eval_workflow::BenchEvalWorkflowEvent;
use crate::ui::eval_run_workflow::EvalRunEvent;
use crate::ui::results::{first_csv_summary, format_result_text, scan_result_dir};

/// Maximum number of results to show in the bench/eval results list.
const MAX_RESULTS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPickerMode {
    Launch,
    Configure,
    #[cfg(feature = "profiling-ui")]
    Profile,
    /// Opened from Bench+Eval — returns to BenchEval after selection.
    BenchEval,
    /// Opened from EvalLauncher — returns to EvalLauncher after selection.
    EvalLauncher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherActionId {
    Launch,
    ConfigureModel,
    #[cfg(feature = "profiling-ui")]
    ProfileModel,
    BenchLauncher,
    EvalLauncher,
    Results,
    Settings,
    ClearGpu,
    Monitor,
    Exit,
}

#[derive(Debug, Clone)]
pub struct LauncherAction {
    pub id: LauncherActionId,
    pub label: String,
    pub description: String,
    pub command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendMode {
    LlamaCpp,
}

impl<'de> Deserialize<'de> for BackendMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        const LLAMACPP_BACKEND_NAME: &str = "llama-cpp";
        const LEGACY_KOBOLD_BACKEND_NAME: &str = "kobold-cpp";
        const LEGACY_OLLAMA_BACKEND_NAME: &str = "ollama";

        let raw = String::deserialize(deserializer)?;
        match raw.as_str() {
            LLAMACPP_BACKEND_NAME | LEGACY_KOBOLD_BACKEND_NAME | LEGACY_OLLAMA_BACKEND_NAME => {
                Ok(Self::LlamaCpp)
            }
            _ => Err(de::Error::unknown_variant(
                &raw,
                &[
                    LLAMACPP_BACKEND_NAME,
                    LEGACY_KOBOLD_BACKEND_NAME,
                    LEGACY_OLLAMA_BACKEND_NAME,
                ],
            )),
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub hardware: Option<HardwareProfile>,
    pub catalog: Vec<CatalogRecord>,
    pub selected_model: usize,
    pub selected_action: usize,
    pub model_picker_mode: ModelPickerMode,
    pub current_plan: Option<LaunchPlan>,
    pub configure_recommended_plan: Option<LaunchPlan>,
    pub configure_field_index: usize,
    pub configure_profile_index: usize,
    pub configure_saved_profiles: Vec<SavedLaunchProfile>,
    #[cfg(feature = "profiling-ui")]
    pub configure_profile_reports: BTreeMap<String, crate::profiling::SavedProfileReport>,
    pub prefs: Preferences,
    pub services: ServiceStatus,
    pub splash_pulse: bool,
    pub splash_ready: bool,
    pub disk_name: Option<String>,
    pub disk_prev: Option<DiskSnapshot>,
    pub disk_prev_time: Instant,
    pub disk_read_history: Vec<u64>,
    pub disk_write_history: Vec<u64>,
    pub disk_read_mbs: f64,
    pub disk_write_mbs: f64,
    pub tokens_per_sec: Option<f64>,
    pub launch_start: Option<Instant>,
    pub ticker: u64,
    pub error_msg: Option<String>,
    pub status_msg: Option<String>,
    pub status_set_at: Option<Instant>,
    pub model_filter: String,
    pub pending_launch_choice: Option<usize>,
    pub exit_confirm_index: usize,
    pub settings_section: usize,
    pub settings_backend_index: usize,
    pub settings_input_buffer: String,
    pub settings_editing: bool,
    pub bench_eval_selected: usize,
    pub eval_launcher_selected: usize,
    pub bench_launcher_selected: usize,
    pub bench_eval_progress_title: String,
    pub bench_eval_progress: Vec<String>,
    pub bench_eval_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>>,
    pub eval_run_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<EvalRunEvent>>,
    pub eval_run_stage: String,
    pub eval_run_progress: Vec<String>,
    pub eval_run_running: bool,
    pub eval_run_tasks_run: usize,
    pub eval_run_tasks_passed: usize,
    pub eval_run_model: Option<String>,
    pub bench_eval_running_model: Option<String>,
    pub bench_eval_running_preset: Option<String>,
    pub bench_eval_running_limit: Option<u32>,
    pub bench_eval_running_command: Option<String>,
    pub bench_eval_report_title: String,
    pub bench_eval_report_markdown: String,
    pub bench_eval_report_source: Option<PathBuf>,
    pub bench_eval_report_markdown_path: Option<PathBuf>,
    pub bench_eval_report_scroll: u16,
    pub bench_eval_results_files: Vec<ResultFile>,
    pub bench_eval_results_selected: usize,
    pub bench_eval_results_content: String,
    pub bench_eval_results_scroll: u16,
    pub bench_eval_results_viewing: bool,
    pub command_overlay_open: bool,
    pub command_overlay: TextArea<'static>,
    pub command_overlay_selected: usize,
    #[cfg(feature = "profiling-ui")]
    pub profiling_advisory: Option<ProfilingAdvisory>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_pending_action: Option<ProfilingAction>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_progress_title: String,
    #[cfg(feature = "profiling-ui")]
    pub profiling_progress_current: u32,
    #[cfg(feature = "profiling-ui")]
    pub profiling_progress_total: u32,
    #[cfg(feature = "profiling-ui")]
    pub profiling_progress: Vec<String>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_choice_index: usize,
    #[cfg(feature = "profiling-ui")]
    pub profiling_success: Option<ProfilingSuccessReport>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_failure: Option<ProfilingFailureReport>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_event_rx: Option<UnboundedReceiver<WorkflowEvent>>,
    #[cfg(feature = "profiling-ui")]
    pub profiling_cancel: Option<CancellationToken>,
    pub tier_picker: tier_picker::TierPickerState,
}

impl App {
    pub fn new(prefs: Preferences) -> Self {
        let disk_name = crate::processes::get_root_disk_name();
        // In lite mode (no profiling-ui feature), return without profiling fields.
        #[cfg(not(feature = "profiling-ui"))]
        return App {
            screen: Screen::Splash,
            hardware: None,
            catalog: Vec::new(),
            selected_model: 0,
            selected_action: 0,
            model_picker_mode: ModelPickerMode::Launch,
            current_plan: None,
            configure_recommended_plan: None,
            configure_field_index: 0,
            configure_profile_index: 0,
            configure_saved_profiles: Vec::new(),
            prefs,
            services: ServiceStatus {
                llamacpp_running: false,
                llamacpp_model: None,
            },
            splash_pulse: false,
            splash_ready: false,
            disk_name,
            disk_prev: None,
            disk_prev_time: Instant::now(),
            disk_read_history: vec![0u64; 30],
            disk_write_history: vec![0u64; 30],
            disk_read_mbs: 0.0,
            disk_write_mbs: 0.0,
            tokens_per_sec: None,
            launch_start: None,
            ticker: 0,
            error_msg: None,
            status_msg: None,
            status_set_at: None,
            model_filter: String::new(),
            pending_launch_choice: None,
            exit_confirm_index: 1,
            settings_section: 0,
            settings_backend_index: 0,
            settings_input_buffer: String::new(),
            settings_editing: false,
            bench_eval_selected: 0,
            eval_launcher_selected: 0,
            bench_launcher_selected: 0,
            bench_eval_progress_title: "Ready".into(),
            bench_eval_progress: Vec::new(),
            bench_eval_event_rx: None,
            eval_run_event_rx: None,
            eval_run_stage: String::new(),
            eval_run_progress: Vec::new(),
            eval_run_running: false,
            eval_run_tasks_run: 0,
            eval_run_tasks_passed: 0,
            eval_run_model: None,
            bench_eval_running_model: None,
            bench_eval_running_preset: None,
            bench_eval_running_limit: None,
            bench_eval_running_command: None,
            bench_eval_report_title: String::new(),
            bench_eval_report_markdown: String::new(),
            bench_eval_report_source: None,
            bench_eval_report_markdown_path: None,
            bench_eval_report_scroll: 0,
            bench_eval_results_files: Vec::new(),
            bench_eval_results_selected: 0,
            bench_eval_results_content: String::new(),
            bench_eval_results_scroll: 0,
            bench_eval_results_viewing: false,
            command_overlay_open: false,
            command_overlay: new_command_overlay(),
            command_overlay_selected: 0,
            tier_picker: tier_picker::TierPickerState::default(),
        };
        // Full mode (profiling-ui feature enabled) — includes profiling fields.
        #[cfg(feature = "profiling-ui")]
        App {
            screen: Screen::Splash,
            hardware: None,
            catalog: Vec::new(),
            selected_model: 0,
            selected_action: 0,
            model_picker_mode: ModelPickerMode::Launch,
            current_plan: None,
            configure_recommended_plan: None,
            configure_field_index: 0,
            configure_profile_index: 0,
            configure_saved_profiles: Vec::new(),
            configure_profile_reports: BTreeMap::new(),
            prefs,
            services: ServiceStatus {
                llamacpp_running: false,
                llamacpp_model: None,
            },
            splash_pulse: false,
            splash_ready: false,
            disk_name,
            disk_prev: None,
            disk_prev_time: Instant::now(),
            disk_read_history: vec![0u64; 30],
            disk_write_history: vec![0u64; 30],
            disk_read_mbs: 0.0,
            disk_write_mbs: 0.0,
            tokens_per_sec: None,
            launch_start: None,
            ticker: 0,
            error_msg: None,
            status_msg: None,
            status_set_at: None,
            model_filter: String::new(),
            pending_launch_choice: None,
            exit_confirm_index: 1,
            settings_section: 0,
            settings_backend_index: 0,
            settings_input_buffer: String::new(),
            settings_editing: false,
            bench_eval_selected: 0,
            eval_launcher_selected: 0,
            bench_launcher_selected: 0,
            bench_eval_progress_title: "Ready".into(),
            bench_eval_progress: Vec::new(),
            bench_eval_event_rx: None,
            eval_run_event_rx: None,
            eval_run_stage: String::new(),
            eval_run_progress: Vec::new(),
            eval_run_running: false,
            eval_run_tasks_run: 0,
            eval_run_tasks_passed: 0,
            eval_run_model: None,
            bench_eval_running_model: None,
            bench_eval_running_preset: None,
            bench_eval_running_limit: None,
            bench_eval_running_command: None,
            bench_eval_report_title: String::new(),
            bench_eval_report_markdown: String::new(),
            bench_eval_report_source: None,
            bench_eval_report_markdown_path: None,
            bench_eval_report_scroll: 0,
            bench_eval_results_files: Vec::new(),
            bench_eval_results_selected: 0,
            bench_eval_results_content: String::new(),
            bench_eval_results_scroll: 0,
            bench_eval_results_viewing: false,
            command_overlay_open: false,
            command_overlay: new_command_overlay(),
            command_overlay_selected: 0,
            profiling_advisory: None,
            profiling_pending_action: None,
            profiling_progress_title: "Preparing".into(),
            profiling_progress_current: 0,
            profiling_progress_total: 0,
            profiling_progress: Vec::new(),
            profiling_choice_index: 0,
            profiling_success: None,
            profiling_failure: None,
            profiling_event_rx: None,
            profiling_cancel: None,
            tier_picker: tier_picker::TierPickerState::default(),
        }
    }

    pub fn tick(&mut self) {
        self.ticker += 1;
        self.splash_pulse = (self.ticker / 5).is_multiple_of(2);
        // Auto-clear status/error messages after 5 seconds
        if let Some(set_at) = self.status_set_at {
            if set_at.elapsed() >= Duration::from_secs(5) {
                self.error_msg = None;
                self.status_msg = None;
                self.status_set_at = None;
            }
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_msg = Some(msg);
        self.error_msg = None;
        self.status_set_at = Some(Instant::now());
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_msg = Some(msg);
        self.status_msg = None;
        self.status_set_at = Some(Instant::now());
    }

    pub fn command_overlay_query(&self) -> String {
        self.command_overlay
            .lines()
            .join(" ")
            .trim()
            .trim_start_matches('/')
            .trim()
            .to_string()
    }

    pub fn update_disk(&mut self) {
        if let Some(ref name) = self.disk_name.clone() {
            if let Some(curr) = crate::processes::read_disk_stats(name) {
                let elapsed = self.disk_prev_time.elapsed().as_secs_f64();
                if let Some(ref prev) = self.disk_prev {
                    let (r, w) = crate::processes::compute_disk_delta(prev, &curr, elapsed);
                    self.disk_read_mbs = r;
                    self.disk_write_mbs = w;
                    self.disk_read_history.push((r * 10.0) as u64);
                    if self.disk_read_history.len() > 30 {
                        self.disk_read_history.remove(0);
                    }
                    self.disk_write_history.push((w * 10.0) as u64);
                    if self.disk_write_history.len() > 30 {
                        self.disk_write_history.remove(0);
                    }
                }
                self.disk_prev = Some(curr);
                self.disk_prev_time = Instant::now();
            }
        }
    }

    #[cfg(feature = "profiling-ui")]
    pub fn reset_profile_flow(&mut self) {
        self.profiling_advisory = None;
        self.profiling_pending_action = None;
        self.profiling_progress_title = "Preparing".into();
        self.profiling_progress_current = 0;
        self.profiling_progress_total = 0;
        self.profiling_progress.clear();
        self.profiling_choice_index = 0;
        self.profiling_success = None;
        self.profiling_failure = None;
        self.profiling_event_rx = None;
        self.profiling_cancel = None;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn start_profile_workflow(
        &mut self,
        rx: UnboundedReceiver<WorkflowEvent>,
        cancel: CancellationToken,
    ) {
        self.profiling_event_rx = Some(rx);
        self.profiling_cancel = Some(cancel);
        self.profiling_progress_title = "Preparing".into();
        self.profiling_progress_current = 0;
        self.profiling_progress_total = 0;
        self.profiling_progress.clear();
        self.push_profile_progress("Preparing workflow...".into());
        self.profiling_choice_index = 0;
        self.screen = Screen::ProfileRunning;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn reset_profile_and_open_launcher(&mut self) {
        self.reset_profile_flow();
        self.screen = Screen::Launcher;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn open_profile_advisory(&mut self, advisory: ProfilingAdvisory) {
        self.profiling_advisory = Some(advisory);
        self.profiling_choice_index = 0;
        self.screen = Screen::ProfileAdvisory;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn open_profile_failure(&mut self, report: ProfilingFailureReport) {
        self.profiling_failure = Some(report);
        self.profiling_choice_index = 0;
        self.screen = Screen::ProfileFailure;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn open_confirm_with_plan(&mut self, plan: LaunchPlan) {
        self.current_plan = Some(plan);
        self.configure_recommended_plan = None;
        self.screen = Screen::Confirm;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn clear_profile_success_and_open_configure_hub(&mut self) {
        self.profiling_pending_action = None;
        self.profiling_success = None;
        // Refresh profiles to pick up any auto-saved ones from the profiling run
        if let Some(plan) = self.current_plan.as_ref() {
            let model_name = plan.model_name.clone();
            crate::ui::configure_profile_flow::refresh_configure_profiles(self, &model_name);
        }
        self.screen = Screen::ConfigureHub;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn clear_profile_failure_and_open_configure_hub(&mut self) {
        self.profiling_pending_action = None;
        self.profiling_failure = None;
        self.screen = Screen::ConfigureHub;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn push_profile_progress(&mut self, line: String) {
        self.profiling_progress.push(line);
        if self.profiling_progress.len() > 20 {
            self.profiling_progress.remove(0);
        }
    }

    pub(crate) fn start_bench_eval_workflow(
        &mut self,
        rx: tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>,
        model: String,
        preset: crate::eval::EvalPreset,
        limit: u32,
        command_preview: String,
    ) {
        self.bench_eval_event_rx = Some(rx);
        self.bench_eval_running_model = Some(model);
        self.bench_eval_running_preset = Some(preset.cli_name().to_string());
        self.bench_eval_running_limit = Some(limit);
        self.bench_eval_running_command = Some(command_preview);
        self.bench_eval_progress_title = "Launching eval".into();
        self.bench_eval_progress.clear();
        self.bench_eval_progress
            .push("Preparing evaluation subprocess...".into());
        self.screen = Screen::BenchEvalRunning;
    }

    pub(crate) fn store_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.bench_eval_report_title = report.title;
        self.bench_eval_report_markdown = report.markdown;
        self.bench_eval_report_source = Some(report.source_path);
        self.bench_eval_report_markdown_path = Some(report.markdown_path);
        self.bench_eval_report_scroll = 0;
    }

    pub(crate) fn open_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.store_bench_eval_report(report);
        self.screen = Screen::BenchEvalReport;
    }

    pub(super) fn discover_result_files(&mut self) {
        self.bench_eval_results_files.clear();
        let data_dir = ozone_core::paths::data_dir();

        // Scan data dir for sweep CSVs
        if let Some(ref data) = data_dir {
            if let Ok(entries) = std::fs::read_dir(data) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if fname.starts_with("sweep_") && fname.ends_with(".csv") {
                        // Parse model name from filename: sweep_{model}_{timestamp}.csv
                        let rest = fname.strip_prefix("sweep_").unwrap_or(fname);
                        let model = rest
                            .rsplit_once('_')
                            .map(|(prefix, _suffix)| prefix)
                            .unwrap_or(rest)
                            .to_string();
                        let summary = first_csv_summary(&path).unwrap_or_default();
                        self.bench_eval_results_files.push(ResultFile {
                            path,
                            kind: ResultFileKind::Sweep,
                            model,
                            summary,
                        });
                    }
                }
            }
        }

        // Scan results for eval CSVs and markdown reports
        if let Ok(root) = crate::eval::resolve_project_root() {
            let artifacts = root.join("results");
            if artifacts.exists() {
                scan_result_dir(&artifacts, &mut self.bench_eval_results_files);
            }
        }
    }

    pub(crate) fn load_result_file_content(&mut self, index: usize) {
        if let Some(file) = self.bench_eval_results_files.get(index) {
            self.bench_eval_results_viewing = true;
            self.bench_eval_results_scroll = 0;
            let content = match std::fs::read_to_string(&file.path) {
                Ok(text) => format_result_text(&file.path, &text, &file.kind),
                Err(e) => format!("Could not read {}: {e}", file.path.display()),
            };
            self.bench_eval_results_content = content;
        }
    }

    pub(crate) fn push_bench_eval_progress(&mut self, line: String) {
        self.bench_eval_progress.push(line);
        if self.bench_eval_progress.len() > 24 {
            self.bench_eval_progress.remove(0);
        }
    }

    /// Returns the filtered catalog based on the current model_filter.
    pub fn filtered_catalog(&self) -> Vec<&crate::catalog::CatalogRecord> {
        if self.model_filter.is_empty() {
            self.catalog.iter().collect()
        } else {
            let filter_lower = self.model_filter.to_lowercase();
            self.catalog
                .iter()
                .filter(|r| r.model_name.to_lowercase().contains(&filter_lower))
                .collect()
        }
    }

    pub fn filtered_catalog_len(&self) -> usize {
        self.filtered_catalog().len()
    }

    pub fn filtered_catalog_get(&self, index: usize) -> Option<crate::catalog::CatalogRecord> {
        self.filtered_catalog().get(index).map(|r| (*r).clone())
    }
}

pub(crate) fn next_screen_after_splash(app: &App) -> Screen {
    if app.prefs.preferred_tier.is_none() {
        Screen::TierPicker
    } else {
        Screen::Launcher
    }
}

pub(crate) fn queue_launch(app: &mut App) {
    app.pending_launch_choice = Some(0);
}

pub(crate) enum LauncherActionOutcome {
    Continue,
    Exit,
}

pub(super) fn selected_record(app: &App) -> Option<CatalogRecord> {
    app.current_plan.as_ref().and_then(|plan| {
        app.catalog
            .iter()
            .find(|record| record.model_name == plan.model_name)
            .cloned()
    })
}