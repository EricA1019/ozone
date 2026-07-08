#[cfg(feature = "profiling-ui")]
use std::collections::BTreeMap;
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::launch_config::LaunchPlan;
use crate::prefs::{Preferences, SavedLaunchProfile};
use crate::disk::DiskSnapshot;
use crate::processes::ServiceStatus;
#[cfg(feature = "profiling-ui")]
use crate::profiling::{
    ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent,
};
use anyhow::Result;
#[cfg(test)]
use crossterm::event::KeyEvent;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::Clear, Terminal};
use serde::{de, Deserialize, Deserializer, Serialize};
#[cfg(feature = "profiling-ui")]
use tokio::sync::mpsc::UnboundedReceiver;
#[cfg(feature = "profiling-ui")]
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;

mod backend_args;
mod bench_eval;
mod bench_eval_flow;
pub mod bench_eval_workflow;
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
mod launcher_loop;
pub use self::launcher_loop::run_launcher;
mod launcher_screens;
mod launcher_settings;
#[cfg(feature = "profiling-ui")]
mod launcher_profile_views;
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
pub mod results;
use self::results::*;

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

pub mod screen;
pub use self::screen::Screen;

pub mod terminal;
use self::terminal::*;

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

#[cfg(feature = "profiling-ui")]
#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
pub struct BenchEvalState {
    pub selected: usize,
    pub eval_launcher_selected: usize,
    pub bench_launcher_selected: usize,
    pub progress_title: String,
    pub progress: Vec<String>,
    pub event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BenchEvalWorkflowEvent>>,
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
                        bench_eval: BenchEvalState {
                progress_title: "Ready".into(),
                ..Default::default()
            },
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
                        bench_eval: BenchEvalState {
                progress_title: "Ready".into(),
                ..Default::default()
            },
            command_overlay_open: false,
            command_overlay: new_command_overlay(),
            command_overlay_selected: 0,
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
        // Refresh profiles to pick up any auto-saved ones from the profiling run
        if let Some(plan) = self.current_plan.as_ref() {
            let model_name = plan.model_name.clone();
            crate::ui::configure_profile_flow::refresh_configure_profiles(self, &model_name);
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

    fn start_bench_eval_workflow(
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
        self.bench_eval.progress
            .push("Preparing evaluation subprocess...".into());
        self.screen = Screen::BenchEvalRunning;
    }

    fn store_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.bench_eval.report_title = report.title;
        self.bench_eval.report_markdown = report.markdown;
        self.bench_eval.report_source = Some(report.source_path);
        self.bench_eval.report_markdown_path = Some(report.markdown_path);
        self.bench_eval.report_scroll = 0;
    }

    fn open_bench_eval_report(&mut self, report: crate::eval_report::EvalMarkdownReport) {
        self.store_bench_eval_report(report);
        self.screen = Screen::BenchEvalReport;
    }

    pub(super) fn discover_result_files(&mut self) {
        self.bench_eval.results_files.clear();
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

        // Scan results for eval CSVs and markdown reports
        if let Ok(root) = crate::eval::resolve_project_root() {
            let artifacts = root.join("results");
            if artifacts.exists() {
                scan_result_dir(&artifacts, &mut self.bench_eval.results_files);
            }
        }
    }

    fn load_result_file_content(&mut self, index: usize) {
        if let Some(file) = self.bench_eval.results_files.get(index) {
            self.bench_eval.results_viewing = true;
            self.bench_eval.results_scroll = 0;
            let content = match std::fs::read_to_string(&file.path) {
                Ok(text) => format_result_text(&file.path, &text, &file.kind),
                Err(e) => format!("Could not read {}: {e}", file.path.display()),
            };
            self.bench_eval.results_content = content;
        }
    }

    fn push_bench_eval_progress(&mut self, line: String) {
        self.bench_eval.progress.push(line);
        if self.bench_eval.progress.len() > 24 {
            self.bench_eval.progress.remove(0);
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

        assert!(!guard.is_raw_mode_enabled());
        assert!(!guard.is_alt_screen_entered());

        guard.mark_raw_mode_enabled();
        guard.mark_alt_screen_entered();

        assert!(guard.is_raw_mode_enabled());
        assert!(guard.is_alt_screen_entered());
    }

    #[test]
    fn terminal_restore_guard_restore_without_state_is_a_noop() {
        let mut guard = TerminalRestoreGuard::new();

        guard
            .restore()
            .expect("restore without state should succeed");

        assert!(!guard.is_raw_mode_enabled());
        assert!(!guard.is_alt_screen_entered());
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
            mode: crate::launch_config::RecommendationMode::MixedMemory,
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

        app.profiling.advisory = Some(ProfilingAdvisory {
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

        app.profiling.success = Some(ProfilingSuccessReport {
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
        let recommended = crate::launch_config::plan_launch(&record, app.hardware.as_ref().unwrap());
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
            mode: crate::launch_config::RecommendationMode::MixedMemory,
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
            crate::launch_config::step_context_size(TEST_CONTEXT_BASE, 1)
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
            mode: crate::launch_config::RecommendationMode::MixedMemory,
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
        app.profiling.cancel = Some(cancel.clone());

        let key = KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE);
        handle_profile_running_key(&mut app, key);

        assert!(cancel.is_cancelled());
        assert!(app
            .profiling.progress
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
        app.profiling.pending_action = Some(ProfilingAction::BenchmarkSavedProfile);
        app.profiling.success = Some(ProfilingSuccessReport {
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
        assert!(app.profiling.pending_action.is_none());
        assert!(app.profiling.success.is_none());
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
        assert_eq!(app.profiling.progress_title, "Preparing");
        assert_eq!(app.profiling.progress_current, 0);
        assert_eq!(app.profiling.progress_total, 0);
        assert_eq!(app.profiling.choice_index, 0);
        assert!(app
            .profiling.cancel
            .as_ref()
            .is_some_and(|token| !token.is_cancelled()));
        assert!(app
            .profiling.progress
            .iter()
            .any(|line| line.contains("Preparing workflow")));
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn reset_profile_and_open_launcher_resets_cluster_and_screen() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileSuccess;
        app.profiling.pending_action = Some(ProfilingAction::QuickSweep);
        app.profiling.progress.push("line".into());

        app.reset_profile_and_open_launcher();

        assert_eq!(app.screen, Screen::Launcher);
        assert!(app.profiling.pending_action.is_none());
        assert!(app.profiling.progress.is_empty());
        assert!(app.profiling.success.is_none());
        assert!(app.profiling.failure.is_none());
    }

    #[test]
    #[cfg(feature = "profiling-ui")]
    fn profile_confirm_escape_returns_to_configure_hub_for_saved_profile_benchmark() {
        let mut app = App::new(Preferences::default());
        app.screen = Screen::ProfileConfirm;
        app.profiling.pending_action = Some(ProfilingAction::BenchmarkSavedProfile);
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
            mode: crate::launch_config::RecommendationMode::VramFirst,
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
