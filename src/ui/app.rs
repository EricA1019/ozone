//! App state struct and related types for the ozone TUI.
//!
//! Contains the main `App` struct (the root UI state object),
//! `BenchEvalState`, `ProfilingState`, and their constructors and
//! pure-state manipulation methods. Event dispatch and flow glue
//! live in `super::mod` (the parent module).

#[cfg(feature = "profiling-ui")]
use std::collections::BTreeMap;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use tui_textarea::TextArea;

use crate::catalog::CatalogRecord;
use crate::disk::DiskSnapshot;
use crate::hardware::HardwareProfile;
use crate::launch_config::LaunchPlan;
use crate::llamacpp::ServiceStatus;
use crate::prefs::{Preferences, SavedLaunchProfile};
#[cfg(feature = "profiling-ui")]
use crate::profiling::{
    ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent,
};
#[cfg(feature = "profiling-ui")]
use tokio::sync::mpsc::UnboundedReceiver;
#[cfg(feature = "profiling-ui")]
use tokio_util::sync::CancellationToken;

#[cfg(feature = "eval")]
use super::bench_eval_workflow::BenchEvalWorkflowEvent;
use super::command_overlay_flow::new_command_overlay;
#[cfg(feature = "eval")]
use super::eval_run_workflow::EvalRunEvent;
use super::results::{first_csv_summary, scan_result_dir, ResultFile, ResultFileKind};
use super::screen::Screen;
use super::tier_picker;
use super::ModelPickerMode;

/// Tracks profiling workflow state — only present with `profiling-ui` feature.
#[cfg(feature = "profiling-ui")]
pub struct ProfilingState {
    pub advisory: Option<ProfilingAdvisory>,
    pub pending_action: Option<ProfilingAction>,
    pub progress_title: String,
    pub progress_current: u32,
    pub progress_total: u32,
    pub progress: Vec<String>,
    pub choice_index: usize,
    pub success: Option<ProfilingSuccessReport>,
    pub failure: Option<ProfilingFailureReport>,
    pub event_rx: Option<UnboundedReceiver<WorkflowEvent>>,
    pub cancel: Option<CancellationToken>,
}

/// Tracks bench/eval screen state.
#[derive(Debug, Default)]
pub struct BenchEvalState {
    pub selected: usize,
    pub eval_launcher_selected: usize,
    pub bench_launcher_selected: usize,
    pub progress_title: String,
    pub progress: Vec<String>,
    #[cfg(feature = "eval")]
    pub event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>>,
    #[cfg(feature = "eval")]
    pub eval_run_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<EvalRunEvent>>,
    pub eval_run_stage: String,
    pub eval_run_progress: Vec<String>,
    pub eval_run_running: bool,
    pub eval_run_tasks_run: usize,
    pub eval_run_tasks_passed: usize,
    pub eval_run_model: Option<String>,
    pub running_model: Option<String>,
    pub running_preset: Option<String>,
    pub running_limit: Option<u32>,
    pub running_command: Option<String>,
    pub report_title: String,
    pub report_markdown: String,
    pub report_source: Option<PathBuf>,
    pub report_markdown_path: Option<PathBuf>,
    pub report_scroll: u16,
    pub results_files: Vec<ResultFile>,
    pub results_selected: usize,
    pub results_content: String,
    pub results_scroll: u16,
    pub results_viewing: bool,
}

/// Root UI state object for the ozone TUI.
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
    pub bench_eval: BenchEvalState,
    pub command_overlay_open: bool,
    pub command_overlay: TextArea<'static>,
    pub command_overlay_selected: usize,
    #[cfg(feature = "profiling-ui")]
    pub profiling: ProfilingState,
    pub tier_picker: tier_picker::TierPickerState,
}

impl App {
    pub fn new(prefs: Preferences) -> Self {
        let disk_name = crate::disk::get_root_disk_name();
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
            #[cfg(feature = "profiling-ui")]
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
            bench_eval: BenchEvalState {
                progress_title: "Ready".into(),
                ..Default::default()
            },
            command_overlay_open: false,
            command_overlay: new_command_overlay(),
            command_overlay_selected: 0,
            #[cfg(feature = "profiling-ui")]
            profiling: ProfilingState {
                advisory: None,
                pending_action: None,
                progress_title: "Preparing".into(),
                progress_current: 0,
                progress_total: 0,
                progress: Vec::new(),
                choice_index: 0,
                success: None,
                failure: None,
                event_rx: None,
                cancel: None,
            },
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
            if let Some(curr) = crate::disk::read_disk_stats(name) {
                let elapsed = self.disk_prev_time.elapsed().as_secs_f64();
                if let Some(ref prev) = self.disk_prev {
                    let (r, w) = crate::disk::compute_disk_delta(prev, &curr, elapsed);
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
        self.profiling.advisory = None;
        self.profiling.pending_action = None;
        self.profiling.progress_title = "Preparing".into();
        self.profiling.progress_current = 0;
        self.profiling.progress_total = 0;
        self.profiling.progress.clear();
        self.profiling.choice_index = 0;
        self.profiling.success = None;
        self.profiling.failure = None;
        self.profiling.event_rx = None;
        self.profiling.cancel = None;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn start_profile_workflow(
        &mut self,
        rx: UnboundedReceiver<WorkflowEvent>,
        cancel: CancellationToken,
    ) {
        self.profiling.event_rx = Some(rx);
        self.profiling.cancel = Some(cancel);
        self.profiling.progress_title = "Preparing".into();
        self.profiling.progress_current = 0;
        self.profiling.progress_total = 0;
        self.profiling.progress.clear();
        self.push_profile_progress("Preparing workflow...".into());
        self.profiling.choice_index = 0;
        self.screen = Screen::ProfileRunning;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn reset_profile_and_open_launcher(&mut self) {
        self.reset_profile_flow();
        self.screen = Screen::Launcher;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn open_profile_advisory(&mut self, advisory: ProfilingAdvisory) {
        self.profiling.advisory = Some(advisory);
        self.profiling.choice_index = 0;
        self.screen = Screen::ProfileAdvisory;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn open_profile_failure(&mut self, report: ProfilingFailureReport) {
        self.profiling.failure = Some(report);
        self.profiling.choice_index = 0;
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
        self.profiling.pending_action = None;
        self.profiling.success = None;
        if let Some(plan) = self.current_plan.as_ref() {
            let model_name = plan.model_name.clone();
            super::configure_profile_flow::refresh_configure_profiles(self, &model_name);
        }
        self.screen = Screen::ConfigureHub;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn clear_profile_failure_and_open_configure_hub(&mut self) {
        self.profiling.pending_action = None;
        self.profiling.failure = None;
        self.screen = Screen::ConfigureHub;
    }

    #[cfg(feature = "profiling-ui")]
    pub fn push_profile_progress(&mut self, line: String) {
        self.profiling.progress.push(line);
        if self.profiling.progress.len() > 20 {
            self.profiling.progress.remove(0);
        }
    }

    #[cfg(feature = "eval")]
    pub(super) fn start_bench_eval_workflow(
        &mut self,
        rx: tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>,
        model: String,
        preset: crate::eval::EvalPreset,
        limit: u32,
        command_preview: String,
    ) {
        self.bench_eval.event_rx = Some(rx);
        self.bench_eval.running_model = Some(model);
        self.bench_eval.running_preset = Some(preset.cli_name().to_string());
        self.bench_eval.running_limit = Some(limit);
        self.bench_eval.running_command = Some(command_preview);
        self.bench_eval.progress_title = "Launching eval".into();
        self.bench_eval.progress.clear();
        self.bench_eval.progress.push("Preparing evaluation subprocess...".into());
        self.screen = Screen::BenchEvalRunning;
    }

    #[cfg(feature = "eval")]
    pub(super) fn store_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.bench_eval.report_title = report.title;
        self.bench_eval.report_markdown = report.markdown;
        self.bench_eval.report_source = Some(report.source_path);
        self.bench_eval.report_markdown_path = Some(report.markdown_path);
        self.bench_eval.report_scroll = 0;
    }

    #[cfg(feature = "eval")]
    pub(super) fn open_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.store_bench_eval_report(report);
        self.screen = Screen::BenchEvalReport;
    }

    pub(super) fn discover_result_files(&mut self) {
        self.bench_eval.results_files.clear();
        let data_dir = ozone_core::paths::data_dir();

        if let Some(ref data) = data_dir {
            if let Ok(entries) = std::fs::read_dir(data) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if fname.starts_with("sweep_") && fname.ends_with(".csv") {
                        let rest = fname.strip_prefix("sweep_").unwrap_or(fname);
                        let model = rest
                            .rsplit_once('_')
                            .map(|(prefix, _suffix)| prefix)
                            .unwrap_or(rest)
                            .to_string();
                        let summary = first_csv_summary(&path).unwrap_or_default();
                        self.bench_eval.results_files.push(ResultFile {
                            path,
                            kind: ResultFileKind::Sweep,
                            model,
                            summary,
                        });
                    }
                }
            }
        }

        #[cfg(feature = "eval")]
        if let Ok(root) = crate::eval::resolve_project_root() {
            let artifacts = root.join("results");
            if artifacts.exists() {
                scan_result_dir(&artifacts, &mut self.bench_eval.results_files);
            }
        }
    }

    pub(super) fn load_result_file_content(&mut self, index: usize) {
        if let Some(file) = self.bench_eval.results_files.get(index) {
            self.bench_eval.results_viewing = true;
            self.bench_eval.results_scroll = 0;
            let content = match std::fs::read_to_string(&file.path) {
                Ok(text) => super::results::format_result_text(&file.path, &text, &file.kind),
                Err(e) => format!("Could not read {}: {e}", file.path.display()),
            };
            self.bench_eval.results_content = content;
        }
    }

    pub(super) fn push_bench_eval_progress(&mut self, line: String) {
        self.bench_eval.progress.push(line);
        if self.bench_eval.progress.len() > 24 {
            self.bench_eval.progress.remove(0);
        }
    }

    pub fn filtered_catalog(&self) -> Vec<&CatalogRecord> {
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

    pub fn filtered_catalog_get(&self, index: usize) -> Option<CatalogRecord> {
        self.filtered_catalog().get(index).map(|r| (*r).clone())
    }
}
