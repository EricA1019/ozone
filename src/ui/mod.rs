#[cfg(feature = "profiling-ui")]
use std::collections::BTreeMap;
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::planner::LaunchPlan;
use crate::prefs::{Preferences, SavedLaunchProfile};
use crate::processes::{DiskSnapshot, ServiceStatus};
#[cfg(feature = "profiling-ui")]
use crate::profiling::{
    ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent,
};
use anyhow::Result;
#[cfg(test)]
use crossterm::event::KeyEvent;
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::Clear, Terminal};
use serde::{de, Deserialize, Deserializer, Serialize};
#[cfg(feature = "profiling-ui")]
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};
#[cfg(feature = "profiling-ui")]
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;

mod backend_args;
mod bench_eval;
mod bench_eval_flow;
mod bench_eval_workflow;
mod bench_launcher;
mod catalog_flow;
mod command_overlay_flow;
mod configure_hub_flow;
mod configure_plan_flow;
mod configure_profile_flow;
mod confirm_flow;
mod eval_launcher;
pub mod eval_run_workflow;
mod exit_confirm_flow;
mod launch_execution_flow;
pub mod launcher;
mod launcher_screen_flow;
mod model_picker_flow;
pub mod monitor;
mod monitor_flow;
#[cfg(feature = "profiling-ui")]
mod profiling_entry_flow;
#[cfg(feature = "profiling-ui")]
mod profiling_result_flow;
mod settings_flow;
mod settings_screen_flow;
pub mod splash;
mod splash_flow;
pub mod tier_install;
pub mod tier_picker;
mod tier_picker_flow;

use self::bench_eval_flow::{handle_bench_eval_key, BenchEvalOutcome};
use self::bench_eval_workflow::{apply_bench_eval_event, BenchEvalWorkflowEvent};
use self::bench_launcher::{handle_key as handle_bench_launcher_key, BenchLauncherOutcome};
use self::catalog_flow::apply_catalog_report;
#[cfg(test)]
use self::catalog_flow::{apply_catalog_refresh, selected_catalog_name};
#[cfg(test)]
use self::command_overlay_flow::normalize_command_overlay;
use self::command_overlay_flow::{
    handle_command_overlay_key, new_command_overlay, open_command_overlay, overlay_supported,
};
use self::configure_hub_flow::handle_configure_hub_key;
#[cfg(test)]
use self::configure_plan_flow::{adjust_configure_plan, reset_configure_plan};
#[cfg(test)]
use self::configure_profile_flow::build_effective_plan;
use self::confirm_flow::handle_confirm_key;
use self::eval_launcher::{handle_key as handle_eval_launcher_key, EvalLauncherOutcome};
use self::eval_run_workflow::{apply_eval_run_event, EvalRunEvent};
use self::exit_confirm_flow::{handle_exit_confirm_key, ExitConfirmOutcome};
use self::launch_execution_flow::{
    handle_pending_frontend_launch, run_launcher_action, PendingFrontendLaunchOutcome,
};
use self::launcher_screen_flow::handle_launcher_screen_key;
use self::model_picker_flow::handle_model_picker_key;
use self::monitor_flow::{handle_monitor_key, MonitorOutcome};
#[cfg(feature = "profiling-ui")]
use self::profiling_entry_flow::{handle_profile_advisory_key, handle_profile_confirm_key};
#[cfg(feature = "profiling-ui")]
use self::profiling_result_flow::{
    apply_workflow_event, handle_profile_failure_key, handle_profile_running_key,
    handle_profile_success_key, ProfilingResultOutcome,
};
#[cfg(test)]
use self::settings_flow::back_from_confirm;
use self::settings_flow::{open_exit_confirm, open_settings, sync_settings_from_prefs};
use self::settings_screen_flow::handle_settings_key;
use self::splash_flow::handle_splash_key;
use self::tier_picker_flow::{handle_tier_picker_key, TierPickerOutcome};

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

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPickerMode {
    Launch,
    Configure,
    #[cfg(feature = "profiling-ui")]
    Profile,
    /// Opened from Bench+Eval — returns to BenchEval after selection.
    BenchEval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherActionId {
    Launch,
    ConfigureModel,
    #[cfg(feature = "profiling-ui")]
    ProfileModel,
    BenchLauncher,
    EvalLauncher,
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

/// A discovered result file (CSV or markdown) from an eval/sweep/creative-writing run.
#[derive(Debug, Clone)]
pub struct ResultFile {
    pub path: std::path::PathBuf,
    pub kind: ResultFileKind,
    pub model: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultFileKind {
    Sweep,
    Eval,
    CreativeWriting,
    Report,
}

impl ResultFileKind {
    fn label(&self) -> &'static str {
        match self {
            ResultFileKind::Sweep => "Sweep",
            ResultFileKind::Eval => "Eval",
            ResultFileKind::CreativeWriting => "Creative",
            ResultFileKind::Report => "Report",
        }
    }
}

/// Read the second line of a CSV (first data row) as a summary.
fn first_csv_summary(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().nth(1)?;
    let parts: Vec<&str> = line.split(',').take(4).collect();
    if parts.len() >= 3 {
        Some(format!(
            "{} ctx={} → {} tok/s",
            parts[0],
            parts.get(1).unwrap_or(&"?"),
            parts.get(2).unwrap_or(&"?")
        ))
    } else {
        Some(line.chars().take(80).collect())
    }
}

/// Recursively scan a directory for CSV and MD result files.
fn scan_result_dir(dir: &std::path::Path, out: &mut Vec<ResultFile>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                scan_result_dir(&path, out);
            } else if fname.ends_with(".csv")
                && (fname.contains("eval") || fname.contains("sweep") || fname.contains("creative"))
            {
                let kind = if fname.contains("creative")
                    || path.to_string_lossy().contains("creative_writing")
                {
                    ResultFileKind::CreativeWriting
                } else if fname.contains("sweep") {
                    ResultFileKind::Sweep
                } else {
                    ResultFileKind::Eval
                };
                let summary = first_csv_summary(&path).unwrap_or_default();
                let model = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                out.push(ResultFile {
                    path,
                    kind,
                    model,
                    summary,
                });
            } else if fname.ends_with(".md") && fname.contains("creative") {
                let summary = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| {
                        t.lines()
                            .next()
                            .map(|l| l.trim_start_matches("# ").to_string())
                    })
                    .unwrap_or_default();
                let model = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                out.push(ResultFile {
                    path,
                    kind: ResultFileKind::Report,
                    model,
                    summary,
                });
            }
        }
    }
}

/// Format a result file's contents for display in the viewer.
fn format_result_text(path: &std::path::Path, text: &str, kind: &ResultFileKind) -> String {
    match kind {
        ResultFileKind::Report | ResultFileKind::CreativeWriting
            if path.extension().is_some_and(|e| e == "md") =>
        {
            text.to_string()
        }
        _ => {
            // CSV → aligned table
            let mut out = String::new();
            let lines: Vec<&str> = text.lines().collect();
            if lines.is_empty() {
                return "(empty file)".into();
            }
            // Compute column widths
            let headers: Vec<&str> = lines[0].split(',').collect();
            let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for line in &lines[1..] {
                let cols: Vec<&str> = line.split(',').collect();
                for (i, col) in cols.iter().enumerate() {
                    if i < widths.len() {
                        widths[i] = widths[i].max(col.len());
                    }
                }
            }
            // Header
            let header_line: String = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:width$}", h, width = widths[i] + 2))
                .collect();
            out.push_str(&header_line);
            out.push('\n');
            out.push_str(&"-".repeat(header_line.len().min(120)));
            out.push('\n');
            // Data rows (limit to 50)
            for line in lines[1..].iter().take(50) {
                let cols: Vec<&str> = line.split(',').collect();
                let row: String = cols
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let w = widths.get(i).copied().unwrap_or(10) + 2;
                        let s = if c.len() > 20 {
                            format!("{}…", &c[..19])
                        } else {
                            c.to_string()
                        };
                        format!("{:width$}", s, width = w)
                    })
                    .collect();
                out.push_str(&row);
                out.push('\n');
            }
            if lines.len() > 51 {
                out.push_str(&format!("\n... {} more rows", lines.len() - 51));
            }
            out
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
    bench_eval_selected: usize,
    eval_launcher_selected: usize,
    bench_launcher_selected: usize,
    bench_eval_progress_title: String,
    bench_eval_progress: Vec<String>,
    bench_eval_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>>,
    eval_run_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<EvalRunEvent>>,
    eval_run_stage: String,
    eval_run_progress: Vec<String>,
    eval_run_running: bool,
    eval_run_tasks_run: usize,
    eval_run_tasks_passed: usize,
    eval_run_model: Option<String>,
    bench_eval_running_model: Option<String>,
    bench_eval_running_preset: Option<String>,
    bench_eval_running_limit: Option<u32>,
    bench_eval_running_command: Option<String>,
    bench_eval_report_title: String,
    bench_eval_report_markdown: String,
    bench_eval_report_source: Option<PathBuf>,
    bench_eval_report_markdown_path: Option<PathBuf>,
    bench_eval_report_scroll: u16,
    bench_eval_results_files: Vec<ResultFile>,
    bench_eval_results_selected: usize,
    bench_eval_results_content: String,
    bench_eval_results_scroll: u16,
    bench_eval_results_viewing: bool,
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

    fn start_bench_eval_workflow(
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

    fn store_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.bench_eval_report_title = report.title;
        self.bench_eval_report_markdown = report.markdown;
        self.bench_eval_report_source = Some(report.source_path);
        self.bench_eval_report_markdown_path = Some(report.markdown_path);
        self.bench_eval_report_scroll = 0;
    }

    fn open_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.store_bench_eval_report(report);
        self.screen = Screen::BenchEvalReport;
    }

    fn discover_result_files(&mut self) {
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

    fn load_result_file_content(&mut self, index: usize) {
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

    fn push_bench_eval_progress(&mut self, line: String) {
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

fn next_screen_after_splash(app: &App) -> Screen {
    if app.prefs.preferred_tier.is_none() {
        Screen::TierPicker
    } else {
        Screen::Launcher
    }
}

fn queue_launch(app: &mut App) {
    app.pending_launch_choice = Some(0);
}

enum LauncherActionOutcome {
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


// ── Shared hint bar ───────────────────────────────────────────────────────────

/// Render a consistent bottom hint bar showing available key bindings.
/// Each element in `pairs` is a `(key_label, description)` tuple.
/// Example: `render_hint_bar(f, area, &[("j/k", "navigate"), ("Enter", "select"), ("q", "quit")])`
pub fn render_hint_bar(f: &mut ratatui::Frame, area: ratatui::layout::Rect, pairs: &[(&'static str, &'static str)]) {
    use ratatui::widgets::Paragraph;
    use ratatui::text::{Line, Span};

    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, crate::theme::style_hint_key()));
        spans.push(Span::styled(format!(" {}", desc), crate::theme::style_muted()));
    }
    let bar = Paragraph::new(Line::from(spans));
    f.render_widget(bar, area);
}


struct TerminalRestoreGuard {
    raw_mode_enabled: bool,
    alt_screen_entered: bool,
}

impl TerminalRestoreGuard {
    fn new() -> Self {
        Self {
            raw_mode_enabled: false,
            alt_screen_entered: false,
        }
    }

    fn mark_raw_mode_enabled(&mut self) {
        self.raw_mode_enabled = true;
    }

    fn mark_alt_screen_entered(&mut self) {
        self.alt_screen_entered = true;
    }

    fn restore(&mut self) -> io::Result<()> {
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

pub async fn run_launcher(
    no_browser: bool,
    tier_override: Option<crate::prefs::Tier>,
    force_picker: bool,
) -> Result<()> {
    let (mut prefs, startup_error) = match crate::prefs::load_prefs().await {
        Ok(prefs) => (prefs, None),
        Err(error) => (
            Preferences::default(),
            Some(format!("Failed to load preferences: {error}")),
        ),
    };
    prefs.no_browser = prefs.no_browser || no_browser;

    // Apply tier override if given
    if let Some(tier) = tier_override {
        prefs.preferred_tier = Some(tier);
    }

    // Apply model directory override from preferences
    if let Some(ref dir) = prefs.models_dir {
        ozone_core::paths::set_models_dir_override(std::path::Path::new(dir));
    }

    let mut app = App::new(prefs);
    if let Some(error) = startup_error {
        app.set_error(error);
    }

    // If --pick flag, clear the tier preference so picker shows
    if force_picker {
        app.prefs.preferred_tier = None;
    }

    // Sync settings indices from persisted prefs
    sync_settings_from_prefs(&mut app);

    enable_raw_mode()?;
    let mut terminal_restore = TerminalRestoreGuard::new();
    terminal_restore.mark_raw_mode_enabled();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    terminal_restore.mark_alt_screen_entered();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Spawn hardware loading
    let (hw_tx, mut hw_rx) = tokio::sync::oneshot::channel::<HardwareProfile>();
    tokio::spawn(async move {
        let hw = tokio::task::spawn_blocking(crate::hardware::load_hardware)
            .await
            .unwrap_or_default();
        let _ = hw_tx.send(hw);
    });

    // Spawn catalog loading
    let model_dir = ozone_core::paths::models_dir();
    let preset_file = ozone_core::paths::catalog_preset_path();
    let bench_file = model_dir.join("bench-results.txt");
    let catalog_model_dir = model_dir.clone();
    let catalog_preset_file = preset_file.clone();
    let catalog_bench_file = bench_file.clone();
    let (cat_tx, mut cat_rx) =
        tokio::sync::oneshot::channel::<(u64, Result<crate::catalog::CatalogLoadReport>)>();
    tokio::spawn(async move {
        let signature = crate::catalog::catalog_signature(
            &catalog_model_dir,
            &catalog_preset_file,
            &catalog_bench_file,
        )
        .await
        .unwrap_or_default();
        let report = crate::catalog::load_catalog_report(
            &catalog_model_dir,
            &catalog_preset_file,
            &catalog_bench_file,
        )
        .await;
        let _ = cat_tx.send((signature, report));
    });

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    let mut last_fast_refresh = Instant::now();
    let mut last_catalog_signature: Option<u64> = None;
    let mut catalog_bootstrap_complete = false;

    let result = loop {
        // Check incoming async data
        if let Ok(hw) = hw_rx.try_recv() {
            app.hardware = Some(hw);
            if catalog_bootstrap_complete {
                app.splash_ready = true;
            }
        }
        if let Ok((signature, report)) = cat_rx.try_recv() {
            last_catalog_signature = Some(signature);
            catalog_bootstrap_complete = true;
            match report {
                Ok(report) => apply_catalog_report(&mut app, report),
                Err(error) => app.set_error(format!("Failed to load catalog: {error}")),
            }
            if app.hardware.is_some() {
                app.splash_ready = true;
            }
        }
        if app.hardware.is_some() && catalog_bootstrap_complete {
            app.splash_ready = true;
        }

        // Poll tier install result from background thread
        if app.screen == Screen::TierPicker {
            let install_result = app
                .tier_picker
                .install_rx
                .as_ref()
                .and_then(|rx| rx.try_recv().ok());
            if let Some(result) = install_result {
                app.tier_picker.install_rx = None;
                if let tier_picker::TierPickerPhase::Installing { tier, .. } =
                    &app.tier_picker.phase
                {
                    let tier = *tier;
                    app.tier_picker.phase = match result {
                        Ok(path) => tier_picker::TierPickerPhase::InstallDone { tier, path },
                        Err(msg) => tier_picker::TierPickerPhase::InstallError { _tier: tier, msg },
                    };
                }
            }
        }

        // Drain profiling workflow events (only compiled when profiling-ui is enabled).
        #[cfg(feature = "profiling-ui")]
        loop {
            let lost = app.profiling_event_rx.is_none();
            let event = match app.profiling_event_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(event) => Some(event),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => {
                        app.profiling_event_rx = None;
                        None
                    }
                },
                None => None,
            };
            let Some(event) = event else {
                // If the channel dropped without sending Completed/Failed, and we're
                // on ProfileRunning, the task silently crashed — bail to launcher.
                if lost && app.screen == Screen::ProfileRunning {
                    app.profiling_cancel = None;
                    app.reset_profile_and_open_launcher();
                    app.set_status("Profiling task exited unexpectedly — check crash.log".into());
                }
                break;
            };
            apply_workflow_event(&mut app, event);
        }

        loop {
            let event = match app.bench_eval_event_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(event) => Some(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        app.bench_eval_event_rx = None;
                        None
                    }
                },
                None => None,
            };
            let Some(event) = event else {
                break;
            };
            apply_bench_eval_event(&mut app, event);
        }

        // Eval run event processing
        loop {
            let event = match app.eval_run_event_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(event) => Some(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        app.eval_run_event_rx = None;
                        None
                    }
                },
                None => None,
            };
            let Some(event) = event else {
                break;
            };
            apply_eval_run_event(&mut app, event);
        }

        // Execute a pending launch request queued by the confirm flow.
        if let Some(choice_idx) = app.pending_launch_choice.take() {
            match handle_pending_frontend_launch(&mut app, choice_idx).await {
                PendingFrontendLaunchOutcome::Continue => {}
                PendingFrontendLaunchOutcome::SkipTick => continue,
                PendingFrontendLaunchOutcome::ExitLauncher => break Ok(()),
            }
        }

        // Draw
        terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            match app.screen {
                Screen::Splash => splash::render(f, &app),
                Screen::TierPicker => {
                    tier_picker::render_tier_picker(f, f.area(), &app.tier_picker, app.ticker)
                }
                Screen::Launcher => launcher::render(f, &app),
                Screen::ExitConfirm => launcher::render_exit_confirm(f, &app),
                Screen::ModelPicker => launcher::render_model_picker(f, &app),
                Screen::ConfigureHub => launcher::render_configure_hub(f, &app),
                Screen::Confirm => launcher::render_confirm(f, &app),
                Screen::Launching => launcher::render_launching(f, &app),
                #[cfg(feature = "profiling-ui")]
                Screen::ProfileAdvisory => launcher::render_profile_advisory(f, &app),
                #[cfg(feature = "profiling-ui")]
                Screen::ProfileConfirm => launcher::render_profile_confirm(f, &app),
                #[cfg(feature = "profiling-ui")]
                Screen::ProfileRunning => launcher::render_profile_running(f, &app),
                #[cfg(feature = "profiling-ui")]
                Screen::ProfileSuccess => launcher::render_profile_success(f, &app),
                #[cfg(feature = "profiling-ui")]
                Screen::ProfileFailure => launcher::render_profile_failure(f, &app),
                Screen::BenchEval => bench_eval::render(f, &app),
                Screen::EvalLauncher => eval_launcher::render(f, &app),
                Screen::BenchLauncher => bench_launcher::render(f, &app),
                Screen::BenchEvalRunning => bench_eval::render_running(f, &app),
                Screen::EvalRunRunning => bench_eval::render_running(f, &app),
                Screen::BenchEvalReport => bench_eval::render_report(f, &app),
                Screen::BenchEvalResults => bench_eval::render_results(f, &app),
                Screen::Settings => launcher::render_settings(f, &app),
                Screen::Monitor => monitor::render(f, &app),
            }
            if app.command_overlay_open {
                launcher::render_command_overlay(f, &app);
            }
        })?;

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.command_overlay_open {
                    match handle_command_overlay_key(&mut app, key, &mut last_refresh).await? {
                        LauncherActionOutcome::Continue => continue,
                        LauncherActionOutcome::Exit => break Ok(()),
                    }
                }
                if matches!(key.code, KeyCode::Char('/')) && overlay_supported(&app.screen) {
                    open_command_overlay(&mut app);
                    continue;
                }
                match app.screen {
                    Screen::Splash => handle_splash_key(&mut app),
                    Screen::TierPicker => match handle_tier_picker_key(&mut app, key) {
                        TierPickerOutcome::Continue => {}
                        TierPickerOutcome::ExitLauncher => break Ok(()),
                    },
                    Screen::Launcher => {
                        match handle_launcher_screen_key(&mut app, key, &mut last_refresh).await {
                            LauncherActionOutcome::Continue => {}
                            LauncherActionOutcome::Exit => break Ok(()),
                        }
                    }
                    Screen::ExitConfirm => match handle_exit_confirm_key(&mut app, key) {
                        ExitConfirmOutcome::Continue => {}
                        ExitConfirmOutcome::ExitLauncher => break Ok(()),
                    },
                    Screen::Settings => {
                        handle_settings_key(&mut app, key).await;
                    }
                    Screen::ModelPicker => {
                        handle_model_picker_key(&mut app, key);
                    }
                    Screen::Confirm => {
                        handle_confirm_key(&mut app, key);
                    }
                    Screen::ConfigureHub => {
                        handle_configure_hub_key(&mut app, key).await;
                    }
                    #[cfg(feature = "profiling-ui")]
                    Screen::ProfileAdvisory => {
                        handle_profile_advisory_key(&mut app, key);
                    }
                    #[cfg(feature = "profiling-ui")]
                    Screen::ProfileConfirm => {
                        handle_profile_confirm_key(&mut app, key);
                    }
                    #[cfg(feature = "profiling-ui")]
                    Screen::ProfileRunning => {
                        handle_profile_running_key(&mut app, key);
                    }
                    #[cfg(feature = "profiling-ui")]
                    Screen::ProfileSuccess => {
                        if matches!(
                            handle_profile_success_key(&mut app, key),
                            ProfilingResultOutcome::RestartLoop
                        ) {
                            continue;
                        }
                    }
                    #[cfg(feature = "profiling-ui")]
                    Screen::ProfileFailure => {
                        if matches!(
                            handle_profile_failure_key(&mut app, key),
                            ProfilingResultOutcome::RestartLoop
                        ) {
                            continue;
                        }
                    }
                    Screen::EvalLauncher => {
                        if let EvalLauncherOutcome::ExitLauncher =
                            handle_eval_launcher_key(&mut app, key).await
                        {}
                    }
                    Screen::BenchLauncher => {
                        if let BenchLauncherOutcome::ExitLauncher =
                            handle_bench_launcher_key(&mut app, key).await
                        {}
                    }
                    Screen::BenchEval => match handle_bench_eval_key(&mut app, key).await {
                        BenchEvalOutcome::Continue => {}
                        BenchEvalOutcome::ExitLauncher => break Ok(()),
                    },
                    Screen::BenchEvalRunning => {
                        self::bench_eval_flow::handle_bench_eval_running_key(&mut app, key);
                    }
                    Screen::BenchEvalReport => {
                        self::bench_eval_flow::handle_bench_eval_report_key(&mut app, key);
                    }
                    Screen::BenchEvalResults => {
                        self::bench_eval_flow::handle_bench_eval_results_key(&mut app, key);
                    }
                    Screen::Monitor => match handle_monitor_key(&mut app, key).await {
                        MonitorOutcome::Continue => {}
                        MonitorOutcome::ExitLauncher => break Ok(()),
                    },
                    _ => {}
                }
            }
        }

        // Tick every 100ms
        if last_tick.elapsed() >= Duration::from_millis(100) {
            last_tick = Instant::now();
            app.tick();
        }

        // Fast path (500ms): service status, GPU/RAM stats
        if last_fast_refresh.elapsed() >= Duration::from_millis(500) {
            last_fast_refresh = Instant::now();
            if matches!(app.screen, Screen::Monitor) {
                app.services = crate::processes::get_service_status().await;
                app.tokens_per_sec = None;
                if let Some(ref mut hw) = app.hardware {
                    *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware_live)
                        .await
                        .unwrap_or_default();
                }
            } else if matches!(app.screen, Screen::Launcher) {
                app.services = crate::processes::get_service_status().await;
            }
        }

        // Slow path (2s): disk usage, catalog refresh
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            if matches!(app.screen, Screen::Monitor) {
                app.update_disk();
            }

            let need_catalog_refresh = matches!(
                app.screen,
                Screen::Launcher
                    | Screen::BenchEval
                    | Screen::BenchEvalRunning
                    | Screen::ModelPicker
                    | Screen::ConfigureHub
                    | Screen::Confirm
                    | Screen::Settings
                    | Screen::ExitConfirm
            ) || {
                #[cfg(feature = "profiling-ui")]
                {
                    matches!(
                        app.screen,
                        Screen::ProfileAdvisory
                            | Screen::ProfileConfirm
                            | Screen::ProfileSuccess
                            | Screen::ProfileFailure
                    )
                }
                #[cfg(not(feature = "profiling-ui"))]
                {
                    false
                }
            };
            if need_catalog_refresh {
                let signature =
                    crate::catalog::catalog_signature(&model_dir, &preset_file, &bench_file)
                        .await
                        .unwrap_or_default();
                if last_catalog_signature != Some(signature) {
                    last_catalog_signature = Some(signature);
                    match crate::catalog::load_catalog_report(&model_dir, &preset_file, &bench_file)
                        .await
                    {
                        Ok(report) => apply_catalog_report(&mut app, report),
                        Err(error) => {
                            if app.error_msg.is_none() {
                                app.set_error(format!("Failed to refresh catalog: {error}"));
                            }
                        }
                    }
                }
            }
        }

        // Auto-advance from splash after data is ready
        if app.screen == Screen::Splash && app.splash_ready && app.ticker > 25 {
            app.screen = next_screen_after_splash(&app);
        }
    };

    terminal_restore.restore()?;
    result
}

pub async fn run_monitor() -> Result<()> {
    let (prefs, startup_error) = match crate::prefs::load_prefs().await {
        Ok(prefs) => (prefs, None),
        Err(error) => (
            Preferences::default(),
            Some(format!("Failed to load preferences: {error}")),
        ),
    };
    let mut app = App::new(prefs);
    if let Some(error) = startup_error {
        app.set_error(error);
    }
    app.screen = Screen::Monitor;
    app.hardware = Some(
        tokio::task::spawn_blocking(crate::hardware::load_hardware)
            .await
            .unwrap_or_default(),
    );
    app.services = crate::processes::get_service_status().await;

    enable_raw_mode()?;
    let mut terminal_restore = TerminalRestoreGuard::new();
    terminal_restore.mark_raw_mode_enabled();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    terminal_restore.mark_alt_screen_entered();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    let mut last_fast_refresh = Instant::now();

    loop {
        terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            monitor::render(f, &app);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('s') => {
                        let _ = crate::processes::clear_gpu_backends().await;
                        app.services = crate::processes::get_service_status().await;
                        terminal.draw(|f| monitor::render(f, &app))?;
                        break;
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= Duration::from_millis(100) {
            last_tick = Instant::now();
            app.tick();
        }

        // Fast path (500ms): service status, GPU/RAM stats
        if last_fast_refresh.elapsed() >= Duration::from_millis(500) {
            last_fast_refresh = Instant::now();
            app.services = crate::processes::get_service_status().await;
            app.tokens_per_sec = None;
            if let Some(ref mut hw) = app.hardware {
                *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware_live)
                    .await
                    .unwrap_or_default();
            }
        }

        // Slow path (2s): disk usage
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            app.update_disk();
        }
    }

    terminal_restore.restore()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_screen_syncs_from_saved_preferences() {
        let mut app = App::new(Preferences {
            preferred_backend: Some(BackendMode::LlamaCpp),
            ..Preferences::default()
        });
        app.settings_section = 1;
        app.settings_backend_index = 0;

        sync_settings_from_prefs(&mut app);

        assert_eq!(app.settings_section, 0);
        assert_eq!(app.settings_backend_index, 0);
    }

    #[test]
    fn terminal_restore_guard_tracks_terminal_state() {
        let mut guard = TerminalRestoreGuard::new();

        assert!(!guard.raw_mode_enabled);
        assert!(!guard.alt_screen_entered);

        guard.mark_raw_mode_enabled();
        guard.mark_alt_screen_entered();

        assert!(guard.raw_mode_enabled);
        assert!(guard.alt_screen_entered);
    }

    #[test]
    fn terminal_restore_guard_restore_without_state_is_a_noop() {
        let mut guard = TerminalRestoreGuard::new();

        guard
            .restore()
            .expect("restore without state should succeed");

        assert!(!guard.raw_mode_enabled);
        assert!(!guard.alt_screen_entered);
    }

    #[test]
    fn confirm_back_returns_to_configure_hub_when_manual_plan_active() {
        let mut app = App::new(Preferences::default());
        app.configure_recommended_plan = Some(LaunchPlan {
            model_name: "alpha.gguf".into(),
            context_size: 4096,
            gpu_layers: 24,
            total_layers: 32,
            cpu_layers: 8,
            quant_k: 1,
            quant_v: 1,
            n_parallel: 1,
            threads: None,
            blas_threads: None,
            mode: crate::planner::RecommendationMode::MixedMemory,
            rationale: "test".into(),
            estimated: false,
            estimated_vram_mb: 0,
            estimated_ram_mb: 0,
            source: "test".into(),
            layer_source_label: "heuristic".into(),
            layer_source_note: None,
        });

        assert_eq!(back_from_confirm(&app), Screen::ConfigureHub);
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn confirm_back_returns_to_last_relevant_screen() {
        let mut app = App::new(Preferences::default());
        assert_eq!(back_from_confirm(&app), Screen::ModelPicker);

        app.profiling_advisory = Some(ProfilingAdvisory {
            model_name: "test.gguf".into(),
            source_label: "heuristic".into(),
            benchmark_count: 0,
            ok_benchmark_count: 0,
            profile_count: 0,
            rationale: "review".into(),
            recommended_action: ProfilingAction::SingleBenchmark,
            estimated_vram_mb: None,
            gpu_budget_mb: None,
            recommended_profile: None,
            warnings: Vec::new(),
            available_actions: Vec::new(),
            launch_plan: None,
        });
        assert_eq!(back_from_confirm(&app), Screen::ProfileAdvisory);

        app.profiling_success = Some(ProfilingSuccessReport {
            model_name: "test.gguf".into(),
            action: ProfilingAction::QuickSweep,
            summary: "done".into(),
            benchmark_count: 0,
            ok_benchmark_count: 0,
            profile_count: 0,
            auto_saved_profile: None,
            best_tokens_per_sec: None,
            recommended_profile: None,
            saved_profile_report: None,
            suggestions: Vec::new(),
            export_detail: None,
        });
        assert_eq!(back_from_confirm(&app), Screen::ProfileSuccess);
    }

    fn test_record(name: &str) -> CatalogRecord {
        CatalogRecord {
            model_name: name.into(),
            model_path: std::path::PathBuf::from(format!("/tmp/{name}")),
            model_size_gb: 7.0,
            recommendation: crate::catalog::Recommendation {
                context_size: 4096,
                gpu_layers: -1,
                quant_k: 1,
                quant_v: 1,
                note: "test".into(),
                source: crate::catalog::RecSource::Heuristic,
            },
            benchmark: None,
            benchmark_count: 0,
            source_priority: 2,
        }
    }

    #[test]
    fn splash_routes_to_tier_picker_when_preference_missing() {
        let app = App::new(Preferences::default());
        assert_eq!(next_screen_after_splash(&app), Screen::TierPicker);
    }

    #[test]
    fn splash_routes_to_launcher_when_preference_exists() {
        let app = App::new(Preferences {
            preferred_tier: Some(crate::prefs::Tier::Base),
            ..Preferences::default()
        });
        assert_eq!(next_screen_after_splash(&app), Screen::Launcher);
    }

    #[test]
    fn launcher_actions_include_explicit_configure_entry() {
        let app = App::new(Preferences {
            preferred_tier: Some(crate::prefs::Tier::Base),
            ..Preferences::default()
        });

        let actions = launcher::visible_launcher_actions(&app);
        assert!(actions
            .iter()
            .any(|action| action.id == LauncherActionId::ConfigureModel));
    }

    #[test]
    fn build_effective_plan_prefers_default_saved_profile() {
        let mut prefs = Preferences::default();
        prefs.upsert_saved_launch_profile(
            "alpha.gguf",
            SavedLaunchProfile {
                profile_name: "custom-1".into(),
                context_size: 16384,
                gpu_layers: 12,
                quant_k: 1,
                quant_v: 1,
                threads: Some(6),
            },
        );
        prefs.set_default_saved_launch_profile("alpha.gguf", "custom-1");

        let mut app = App::new(prefs);
        app.hardware = Some(crate::hardware::HardwareProfile {
            gpu: Some(crate::hardware::GpuMemory {
                used_mb: 1000,
                free_mb: 12000,
                total_mb: 16000,
            }),
            ram_total_mb: 32000,
            ram_free_mb: 20000,
            ram_used_mb: 12000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        });

        let record = test_record("alpha.gguf");
        let recommended = crate::planner::plan_launch(&record, app.hardware.as_ref().unwrap());
        let effective = build_effective_plan(&app, &record, &recommended).expect("effective plan");

        assert_eq!(effective.context_size, 16384);
        assert_eq!(effective.gpu_layers, 12);
        assert_eq!(effective.threads, Some(6));
    }

    const TEST_MODEL_NAME: &str = "alpha.gguf";
    const TEST_CONTEXT_BASE: u32 = 4096;
    const TEST_TOTAL_LAYERS: u32 = 32;
    const TEST_RECOMMENDED_GPU_LAYERS: i32 = 24;
    const TEST_RECOMMENDED_CPU_LAYERS: u32 = 8;
    const TEST_QUANT_K: u8 = 1;
    const TEST_QUANT_V: u8 = 1;
    const TEST_GPU_USED_MB: u64 = 1000;
    const TEST_GPU_FREE_MB: u64 = 12000;
    const TEST_GPU_TOTAL_MB: u64 = 16000;
    const TEST_RAM_TOTAL_MB: u64 = 32000;
    const TEST_RAM_FREE_MB: u64 = 20000;
    const TEST_RAM_USED_MB: u64 = 12000;
    const TEST_CPU_LOGICAL: usize = 8;
    const TEST_CPU_PHYSICAL: usize = 4;

    fn configured_app_for_plan_mutation(configure_field_index: usize) -> App {
        let mut app = App::new(Preferences::default());
        app.hardware = Some(crate::hardware::HardwareProfile {
            gpu: Some(crate::hardware::GpuMemory {
                used_mb: TEST_GPU_USED_MB,
                free_mb: TEST_GPU_FREE_MB,
                total_mb: TEST_GPU_TOTAL_MB,
            }),
            ram_total_mb: TEST_RAM_TOTAL_MB,
            ram_free_mb: TEST_RAM_FREE_MB,
            ram_used_mb: TEST_RAM_USED_MB,
            cpu_logical: TEST_CPU_LOGICAL,
            cpu_physical: TEST_CPU_PHYSICAL,
            ..Default::default()
        });
        app.catalog = vec![test_record(TEST_MODEL_NAME)];
        app.current_plan = Some(LaunchPlan {
            model_name: TEST_MODEL_NAME.into(),
            context_size: TEST_CONTEXT_BASE,
            gpu_layers: TEST_RECOMMENDED_GPU_LAYERS,
            total_layers: TEST_TOTAL_LAYERS,
            cpu_layers: TEST_RECOMMENDED_CPU_LAYERS,
            quant_k: TEST_QUANT_K,
            quant_v: TEST_QUANT_V,
            n_parallel: 1,
            threads: None,
            blas_threads: None,
            mode: crate::planner::RecommendationMode::MixedMemory,
            rationale: "test".into(),
            estimated: false,
            estimated_vram_mb: 0,
            estimated_ram_mb: 0,
            source: "test".into(),
            layer_source_label: "heuristic".into(),
            layer_source_note: None,
        });
        app.configure_recommended_plan = app.current_plan.clone();
        app.configure_field_index = configure_field_index;
        app
    }

    #[test]
    fn adjust_configure_plan_steps_context_size_when_context_selected() {
        let mut app = configured_app_for_plan_mutation(0);

        adjust_configure_plan(&mut app, 1);

        let adjusted = app.current_plan.expect("plan should exist");
        assert_eq!(
            adjusted.context_size,
            crate::planner::step_context_size(TEST_CONTEXT_BASE, 1)
        );
        assert_eq!(adjusted.gpu_layers, TEST_RECOMMENDED_GPU_LAYERS);
    }

    #[test]
    fn adjust_configure_plan_clamps_gpu_layers_when_layers_selected() {
        let mut app = configured_app_for_plan_mutation(1);
        let negative_delta = -(TEST_TOTAL_LAYERS as i32) * 2;

        adjust_configure_plan(&mut app, negative_delta);

        let adjusted = app.current_plan.expect("plan should exist");
        assert_eq!(adjusted.gpu_layers, 0);
        assert_eq!(adjusted.context_size, TEST_CONTEXT_BASE);
    }

    #[test]
    fn reset_configure_plan_restores_recommended_plan() {
        let mut app = configured_app_for_plan_mutation(1);
        let increase_layers = 2;
        adjust_configure_plan(&mut app, increase_layers);

        reset_configure_plan(&mut app);

        let reset_plan = app.current_plan.expect("plan should exist");
        assert_eq!(reset_plan.context_size, TEST_CONTEXT_BASE);
        assert_eq!(reset_plan.gpu_layers, TEST_RECOMMENDED_GPU_LAYERS);
    }

    fn test_launch_plan_for_model(
        model_name: &str,
        context_size: u32,
        gpu_layers: i32,
    ) -> LaunchPlan {
        LaunchPlan {
            model_name: model_name.into(),
            context_size,
            gpu_layers,
            total_layers: TEST_TOTAL_LAYERS,
            cpu_layers: TEST_RECOMMENDED_CPU_LAYERS,
            quant_k: TEST_QUANT_K,
            quant_v: TEST_QUANT_V,
            n_parallel: 1,
            threads: None,
            blas_threads: None,
            mode: crate::planner::RecommendationMode::MixedMemory,
            rationale: "test".into(),
            estimated: false,
            estimated_vram_mb: 0,
            estimated_ram_mb: 0,
            source: "test".into(),
            layer_source_label: "heuristic".into(),
            layer_source_note: None,
        }
    }

    #[test]
    fn configure_hub_escape_clears_state_and_returns_to_model_picker() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ConfigureHub;
        app.current_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            12,
        ));
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));
        app.configure_saved_profiles = vec![SavedLaunchProfile {
            profile_name: "custom-1".into(),
            context_size: TEST_CONTEXT_BASE,
            gpu_layers: TEST_RECOMMENDED_GPU_LAYERS,
            quant_k: TEST_QUANT_K,
            quant_v: TEST_QUANT_V,
            threads: None,
        }];
        app.configure_profile_index = 1;

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(handle_configure_hub_key(&mut app, key));

        assert!(app.current_plan.is_none());
        assert!(app.configure_recommended_plan.is_none());
        assert!(app.configure_saved_profiles.is_empty());
        assert_eq!(app.configure_profile_index, 0);
        assert_eq!(app.screen, Screen::ModelPicker);
    }

    #[test]
    fn configure_hub_enter_persists_override_and_moves_to_confirm() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ConfigureHub;
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));
        app.current_plan = Some(test_launch_plan_for_model(TEST_MODEL_NAME, 8192, 20));

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(handle_configure_hub_key(&mut app, key));

        let override_state = app
            .prefs
            .launch_override_for(TEST_MODEL_NAME)
            .expect("override should be stored");
        assert_eq!(override_state.context_size, Some(8192));
        assert_eq!(override_state.gpu_layers, Some(20));
        assert_eq!(app.screen, Screen::Confirm);
    }

    #[test]
    fn model_picker_escape_clears_filter_before_navigation() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ModelPicker;
        app.model_filter = "alp".into();
        app.current_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        handle_model_picker_key(&mut app, key);

        assert!(app.model_filter.is_empty());
        assert!(app.current_plan.is_some());
        assert_eq!(app.screen, Screen::ModelPicker);
    }

    #[test]
    fn model_picker_escape_without_filter_returns_to_launcher() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ModelPicker;
        app.current_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        handle_model_picker_key(&mut app, key);

        assert!(app.current_plan.is_none());
        assert!(app.configure_recommended_plan.is_none());
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn model_picker_enter_in_launch_mode_opens_configure_hub() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ModelPicker;
        app.model_picker_mode = ModelPickerMode::Launch;
        app.catalog = vec![test_record(TEST_MODEL_NAME)];
        app.selected_model = 0;
        app.hardware = Some(crate::hardware::HardwareProfile {
            gpu: Some(crate::hardware::GpuMemory {
                used_mb: TEST_GPU_USED_MB,
                free_mb: TEST_GPU_FREE_MB,
                total_mb: TEST_GPU_TOTAL_MB,
            }),
            ram_total_mb: TEST_RAM_TOTAL_MB,
            ram_free_mb: TEST_RAM_FREE_MB,
            ram_used_mb: TEST_RAM_USED_MB,
            cpu_logical: TEST_CPU_LOGICAL,
            cpu_physical: TEST_CPU_PHYSICAL,
            ..Default::default()
        });

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        handle_model_picker_key(&mut app, key);

        assert!(app.current_plan.is_some());
        assert!(app.configure_recommended_plan.is_some());
        assert_eq!(app.configure_field_index, 0);
        assert_eq!(app.screen, Screen::ConfigureHub);
    }

    #[test]
    fn confirm_escape_uses_back_navigation_target() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Confirm;
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        handle_confirm_key(&mut app, key);

        assert_eq!(app.screen, Screen::ConfigureHub);
    }

    #[test]
    fn confirm_enter_queues_launch_request() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Confirm;
        app.current_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        handle_confirm_key(&mut app, key);

        assert_eq!(app.pending_launch_choice, Some(0));
    }

    #[test]
    fn exit_confirm_enter_on_yes_requests_exit() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ExitConfirm;
        app.exit_confirm_index = 0;

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        let outcome = handle_exit_confirm_key(&mut app, key);

        assert!(matches!(outcome, ExitConfirmOutcome::ExitLauncher));
    }

    #[test]
    fn exit_confirm_escape_returns_to_launcher() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ExitConfirm;

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let outcome = handle_exit_confirm_key(&mut app, key);

        assert!(matches!(outcome, ExitConfirmOutcome::Continue));
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn settings_enter_saves_selection_and_returns_launcher() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Settings;
        app.settings_backend_index = 2;

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(handle_settings_key(&mut app, key));

        assert_eq!(app.prefs.preferred_backend, Some(BackendMode::LlamaCpp));
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn settings_escape_discards_changes_and_returns_launcher() {
        let mut app = App::new(Preferences {
            preferred_backend: Some(BackendMode::LlamaCpp),
            ..Preferences::default()
        });
        app.screen = Screen::Settings;
        app.settings_backend_index = 2;

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(handle_settings_key(&mut app, key));

        assert_eq!(app.settings_backend_index, 0);
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn tier_picker_q_requests_exit_in_picking_phase() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::TierPicker;
        app.tier_picker.phase = tier_picker::TierPickerPhase::Picking;

        let key = KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE);
        let outcome = handle_tier_picker_key(&mut app, key);

        assert!(matches!(outcome, TierPickerOutcome::ExitLauncher));
    }

    #[test]
    fn tier_picker_enter_on_lite_selects_launcher_without_download() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::TierPicker;
        app.tier_picker.phase = tier_picker::TierPickerPhase::Picking;
        app.tier_picker.selected = 0;

        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome = runtime.block_on(async { handle_tier_picker_key(&mut app, key) });

        assert!(matches!(outcome, TierPickerOutcome::Continue));
        assert_eq!(app.prefs.preferred_tier, Some(crate::prefs::Tier::Lite));
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn launcher_q_requests_exit() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Launcher;
        let mut last_refresh = Instant::now();

        let key = KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome =
            runtime.block_on(handle_launcher_screen_key(&mut app, key, &mut last_refresh));

        assert!(matches!(outcome, LauncherActionOutcome::Exit));
    }

    #[test]
    fn launcher_escape_opens_exit_confirm_screen() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Launcher;
        let mut last_refresh = Instant::now();

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome =
            runtime.block_on(handle_launcher_screen_key(&mut app, key, &mut last_refresh));

        assert!(matches!(outcome, LauncherActionOutcome::Continue));
        assert_eq!(app.screen, Screen::ExitConfirm);
        assert_eq!(app.exit_confirm_index, 1);
    }

    #[test]
    fn splash_key_advances_to_tier_picker_without_preferred_tier() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Splash;
        app.splash_ready = true;

        handle_splash_key(&mut app);

        assert_eq!(app.screen, Screen::TierPicker);
    }

    #[test]
    fn splash_key_advances_to_launcher_with_preferred_tier() {
        let mut app = App::new(Preferences {
            preferred_tier: Some(crate::prefs::Tier::Base),
            ..Preferences::default()
        });
        app.screen = Screen::Splash;
        app.splash_ready = true;

        handle_splash_key(&mut app);

        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn monitor_q_requests_exit() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Monitor;

        let key = KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome = runtime.block_on(handle_monitor_key(&mut app, key));

        assert!(matches!(outcome, MonitorOutcome::ExitLauncher));
    }

    #[test]
    fn monitor_escape_returns_to_launcher() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Monitor;

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome = runtime.block_on(handle_monitor_key(&mut app, key));

        assert!(matches!(outcome, MonitorOutcome::Continue));
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    fn monitor_r_returns_to_launcher() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::Monitor;

        let key = KeyEvent::new(KeyCode::Char('r'), crossterm::event::KeyModifiers::NONE);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let outcome = runtime.block_on(handle_monitor_key(&mut app, key));

        assert!(matches!(outcome, MonitorOutcome::Continue));
        assert_eq!(app.screen, Screen::Launcher);
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn profile_running_q_cancels_active_workflow() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileRunning;
        let cancel = CancellationToken::new();
        app.profiling_cancel = Some(cancel.clone());

        let key = KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE);
        handle_profile_running_key(&mut app, key);

        assert!(cancel.is_cancelled());
        assert!(app
            .profiling_progress
            .iter()
            .any(|line| line.contains("Cancelling")));
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn profile_success_escape_from_saved_profile_returns_configure_hub() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileSuccess;
        app.current_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));
        app.profiling_pending_action = Some(ProfilingAction::BenchmarkSavedProfile);
        app.profiling_success = Some(ProfilingSuccessReport {
            model_name: TEST_MODEL_NAME.into(),
            action: ProfilingAction::BenchmarkSavedProfile,
            summary: "saved".into(),
            benchmark_count: 0,
            auto_saved_profile: None,
            ok_benchmark_count: 0,
            profile_count: 0,
            best_tokens_per_sec: None,
            recommended_profile: None,
            saved_profile_report: None,
            suggestions: Vec::new(),
            export_detail: None,
        });

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        let outcome = handle_profile_success_key(&mut app, key);

        assert!(matches!(outcome, ProfilingResultOutcome::RestartLoop));
        assert_eq!(app.screen, Screen::ConfigureHub);
        assert!(app.profiling_pending_action.is_none());
        assert!(app.profiling_success.is_none());
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn profile_advisory_escape_returns_to_model_picker() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileAdvisory;

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        handle_profile_advisory_key(&mut app, key);

        assert_eq!(app.screen, Screen::ModelPicker);
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn start_profile_workflow_sets_running_state_cluster() {
        let mut app = App::new(Preferences::default());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(tx);
        let cancel = CancellationToken::new();

        app.start_profile_workflow(rx, cancel.clone());

        assert_eq!(app.screen, Screen::ProfileRunning);
        assert_eq!(app.profiling_progress_title, "Preparing");
        assert_eq!(app.profiling_progress_current, 0);
        assert_eq!(app.profiling_progress_total, 0);
        assert_eq!(app.profiling_choice_index, 0);
        assert!(app
            .profiling_cancel
            .as_ref()
            .is_some_and(|token| !token.is_cancelled()));
        assert!(app
            .profiling_progress
            .iter()
            .any(|line| line.contains("Preparing workflow")));
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn reset_profile_and_open_launcher_resets_cluster_and_screen() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileSuccess;
        app.profiling_pending_action = Some(ProfilingAction::QuickSweep);
        app.profiling_progress.push("line".into());

        app.reset_profile_and_open_launcher();

        assert_eq!(app.screen, Screen::Launcher);
        assert!(app.profiling_pending_action.is_none());
        assert!(app.profiling_progress.is_empty());
        assert!(app.profiling_success.is_none());
        assert!(app.profiling_failure.is_none());
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn profile_confirm_escape_returns_to_configure_hub_for_saved_profile_benchmark() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileConfirm;
        app.profiling_pending_action = Some(ProfilingAction::BenchmarkSavedProfile);
        app.configure_recommended_plan = Some(test_launch_plan_for_model(
            TEST_MODEL_NAME,
            TEST_CONTEXT_BASE,
            TEST_RECOMMENDED_GPU_LAYERS,
        ));

        let key = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        handle_profile_confirm_key(&mut app, key);

        assert_eq!(app.screen, Screen::ConfigureHub);
    }

    #[test]
    fn catalog_refresh_preserves_selected_model_name() {
        let mut app = App::new(Preferences::default());
        app.catalog = vec![test_record("alpha.gguf"), test_record("beta.gguf")];
        app.selected_model = 1;

        apply_catalog_refresh(
            &mut app,
            vec![
                test_record("gamma.gguf"),
                test_record("beta.gguf"),
                test_record("delta.gguf"),
            ],
        );

        assert_eq!(selected_catalog_name(&app).as_deref(), Some("beta.gguf"));
    }

    #[test]
    fn catalog_refresh_clears_removed_launch_plan() {
        let mut app = App::new(Preferences::default());
        app.catalog = vec![test_record("alpha.gguf")];
        app.current_plan = Some(LaunchPlan {
            model_name: "alpha.gguf".into(),
            context_size: 4096,
            gpu_layers: -1,
            total_layers: 32,
            cpu_layers: 0,
            quant_k: 1,
            quant_v: 1,
            n_parallel: 1,
            threads: None,
            blas_threads: None,
            mode: crate::planner::RecommendationMode::VramFirst,
            rationale: "test".into(),
            estimated: false,
            estimated_vram_mb: 0,
            estimated_ram_mb: 0,
            source: "test".into(),
            layer_source_label: "heuristic".into(),
            layer_source_note: None,
        });
        app.screen = Screen::Confirm;

        apply_catalog_refresh(&mut app, vec![test_record("beta.gguf")]);

        assert!(app.current_plan.is_none());
        assert!(app.configure_recommended_plan.is_none());
        assert_eq!(app.screen, Screen::ModelPicker);
        assert_eq!(
            app.error_msg.as_deref(),
            Some("Selected model is no longer available.")
        );
    }

    #[test]
    fn command_overlay_query_normalizes_prefix_and_newlines() {
        let mut app = App::new(Preferences::default());
        app.command_overlay.insert_str("/clear\ngpu  ");
        normalize_command_overlay(&mut app);

        assert_eq!(app.command_overlay_query(), "clear gpu");
    }

    #[test]
    fn overlay_support_targets_launcher_facing_screens() {
        assert!(overlay_supported(&Screen::Launcher));
        assert!(overlay_supported(&Screen::BenchEval));
        assert!(overlay_supported(&Screen::BenchEvalRunning));
        assert!(overlay_supported(&Screen::Settings));
        assert!(overlay_supported(&Screen::ConfigureHub));
        assert!(overlay_supported(&Screen::Confirm));
        assert!(overlay_supported(&Screen::Monitor));
        assert!(!overlay_supported(&Screen::Splash));
        assert!(!overlay_supported(&Screen::ExitConfirm));
    }

    #[test]
    fn command_overlay_can_execute_settings_action() {
        let mut app = App::new(Preferences::default());
        let mut last_refresh = Instant::now();

        let outcome = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run_launcher_action(
                &mut app,
                LauncherActionId::Settings,
                &mut last_refresh,
            ));

        assert!(matches!(outcome, LauncherActionOutcome::Continue));
        assert_eq!(app.screen, Screen::Settings);
    }
}
