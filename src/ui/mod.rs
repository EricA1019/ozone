use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::Clear, Terminal};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};

use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::planner::LaunchPlan;
use crate::prefs::Preferences;
use crate::processes::{DiskSnapshot, ServiceStatus};
use crate::profiling::{
    self, ProfilingAction, ProfilingAdvisory, ProfilingFailureReport, ProfilingSuccessReport,
    WorkflowEvent, WorkflowRequest,
};
use tokio_util::sync::CancellationToken;

pub mod launcher;
pub mod monitor;
pub mod splash;
pub mod tier_picker;

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Splash,
    TierPicker,
    Launcher,
    ExitConfirm,
    ModelPicker,
    Confirm,
    FrontendChoice,
    Launching,
    ProfileAdvisory,
    ProfileConfirm,
    ProfileRunning,
    ProfileSuccess,
    ProfileFailure,
    Settings,
    Monitor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelPickerMode {
    Launch,
    Profile,
}

/// Which frontend the user wants to launch (or `--frontend` CLI bypass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum FrontendMode {
    /// Open browser to the SillyTavern web UI (default existing behaviour)
    #[value(name = "sillyTavern")]
    SillyTavern,
    /// Launch the ozone+ conversation shell
    #[value(name = "ozonePlus")]
    OzonePlus,
}

/// Which backend the user wants to launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendMode {
    KoboldCpp,
    LlamaCpp,
    Ollama,
}

pub struct App {
    pub screen: Screen,
    pub hardware: Option<HardwareProfile>,
    pub catalog: Vec<CatalogRecord>,
    pub selected_model: usize,
    pub selected_action: usize,
    pub model_picker_mode: ModelPickerMode,
    pub current_plan: Option<LaunchPlan>,
    pub prefs: Preferences,
    pub services: ServiceStatus,
    pub splash_pulse: bool,
    pub splash_ready: bool,
    // Monitor state
    pub disk_name: Option<String>,
    pub disk_prev: Option<DiskSnapshot>,
    pub disk_prev_time: Instant,
    pub disk_read_history: Vec<u64>,
    pub disk_write_history: Vec<u64>,
    pub disk_read_mbs: f64,
    pub disk_write_mbs: f64,
    pub tokens_per_sec: Option<f64>,
    pub launch_start: Option<Instant>,
    // UI state
    /// Stored at construction; event loop uses a local `Instant` instead.
    #[allow(dead_code)]
    pub last_refresh: Instant,
    pub ticker: u64,
    pub error_msg: Option<String>,
    pub status_msg: Option<String>,
    pub status_set_at: Option<Instant>,
    // Model picker filter
    pub model_filter: String,
    // Frontend choice state
    pub preferred_frontend: Option<FrontendMode>,
    pub frontend_choice_index: usize,
    pub ozone_plus_handoff: bool,
    pub pending_launch_choice: Option<usize>,
    pub exit_confirm_index: usize,
    // Settings screen state
    pub settings_section: usize,        // 0=backend, 1=frontend
    pub settings_backend_index: usize,  // 0=KoboldCpp, 1=LlamaCpp, 2=Ollama
    pub settings_frontend_index: usize, // 0=SillyTavern, 1=OzonePlus
    // Profiling flow state
    pub profiling_advisory: Option<ProfilingAdvisory>,
    pub profiling_pending_action: Option<ProfilingAction>,
    pub profiling_progress_title: String,
    pub profiling_progress_current: u32,
    pub profiling_progress_total: u32,
    pub profiling_progress: Vec<String>,
    pub profiling_choice_index: usize,
    pub profiling_success: Option<ProfilingSuccessReport>,
    pub profiling_failure: Option<ProfilingFailureReport>,
    pub profiling_event_rx: Option<UnboundedReceiver<WorkflowEvent>>,
    pub profiling_cancel: Option<CancellationToken>,
    // Tier picker state
    pub tier_picker: tier_picker::TierPickerState,
}

impl App {
    pub fn new(prefs: Preferences) -> Self {
        let disk_name = crate::processes::get_root_disk_name();
        App {
            screen: Screen::Splash,
            hardware: None,
            catalog: Vec::new(),
            selected_model: 0,
            selected_action: 0,
            model_picker_mode: ModelPickerMode::Launch,
            current_plan: None,
            prefs,
            services: ServiceStatus {
                kobold_running: false,
                kobold_model: None,
                llamacpp_running: false,
                llamacpp_model: None,
                ollama_running: false,
                st_running: false,
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
            last_refresh: Instant::now(),
            ticker: 0,
            error_msg: None,
            status_msg: None,
            status_set_at: None,
            model_filter: String::new(),
            preferred_frontend: None,
            frontend_choice_index: 0,
            ozone_plus_handoff: false,
            pending_launch_choice: None,
            exit_confirm_index: 1,
            settings_section: 0,
            settings_backend_index: 0,
            settings_frontend_index: 0,
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

    pub fn push_profile_progress(&mut self, line: String) {
        self.profiling_progress.push(line);
        if self.profiling_progress.len() > 20 {
            self.profiling_progress.remove(0);
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

fn build_kc_args(plan: &LaunchPlan) -> Vec<String> {
    let mut args = vec![
        "--gpulayers".to_string(),
        plan.gpu_layers.to_string(),
        "--contextsize".to_string(),
        plan.context_size.to_string(),
        "--quantkv".to_string(),
        plan.quant_kv.to_string(),
    ];
    if let Some(t) = plan.threads {
        args.push("--threads".to_string());
        args.push(t.to_string());
    }
    if let Some(bt) = plan.blas_threads {
        args.push("--blasthreads".to_string());
        args.push(bt.to_string());
    }
    args
}

fn build_llama_args(plan: &LaunchPlan) -> Vec<String> {
    let gpu_layers = if plan.gpu_layers < 0 {
        "all".to_string()
    } else {
        plan.gpu_layers.to_string()
    };
    let mut args = vec![
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        "8080".to_string(),
        "--ctx-size".to_string(),
        plan.context_size.to_string(),
        "--gpu-layers".to_string(),
        gpu_layers,
        "--no-webui".to_string(),
    ];
    if let Some(t) = plan.threads {
        args.push("--threads".to_string());
        args.push(t.to_string());
    }
    args
}

fn next_screen_after_splash(app: &App) -> Screen {
    if app.prefs.preferred_tier.is_none() {
        Screen::TierPicker
    } else {
        Screen::Launcher
    }
}

fn selected_catalog_name(app: &App) -> Option<String> {
    app.filtered_catalog_get(app.selected_model)
        .map(|record| record.model_name)
}

fn select_catalog_index(app: &App, preferred_name: Option<&str>) -> usize {
    preferred_name
        .and_then(|name| {
            app.filtered_catalog()
                .iter()
                .position(|record| record.model_name == name)
        })
        .unwrap_or(0)
}

fn apply_catalog_refresh(app: &mut App, catalog: Vec<CatalogRecord>) {
    let preferred_name = selected_catalog_name(app)
        .or_else(|| {
            app.current_plan
                .as_ref()
                .map(|plan| plan.model_name.clone())
        })
        .or_else(|| {
            (!app.prefs.last_model_name.is_empty()).then(|| app.prefs.last_model_name.clone())
        });

    app.catalog = catalog;
    app.selected_model = select_catalog_index(app, preferred_name.as_deref());

    let plan_missing = app.current_plan.as_ref().is_some_and(|plan| {
        !app.catalog
            .iter()
            .any(|record| record.model_name == plan.model_name)
    });
    if plan_missing {
        app.current_plan = None;
        if matches!(app.screen, Screen::Confirm | Screen::FrontendChoice) {
            app.screen = Screen::ModelPicker;
            app.set_error("Selected model is no longer available.".into());
        }
    }
}

fn queue_frontend_launch(app: &mut App) {
    match app.preferred_frontend {
        Some(FrontendMode::SillyTavern) => {
            app.pending_launch_choice = Some(0);
        }
        Some(FrontendMode::OzonePlus) => {
            app.pending_launch_choice = Some(1);
        }
        None => {
            app.frontend_choice_index = match app.prefs.preferred_frontend {
                Some(FrontendMode::OzonePlus) => 1,
                _ => 0,
            };
            app.screen = Screen::FrontendChoice;
        }
    }
}

fn sync_settings_from_prefs(app: &mut App) {
    app.settings_section = 0;
    app.settings_backend_index = match app.prefs.preferred_backend {
        Some(BackendMode::LlamaCpp) => 1,
        Some(BackendMode::Ollama) => 2,
        _ => 0,
    };
    app.settings_frontend_index = match app.prefs.preferred_frontend {
        Some(FrontendMode::OzonePlus) => 1,
        _ => 0,
    };
}

fn open_settings(app: &mut App) {
    sync_settings_from_prefs(app);
    app.screen = Screen::Settings;
}

fn open_exit_confirm(app: &mut App) {
    app.exit_confirm_index = 1;
    app.screen = Screen::ExitConfirm;
}

fn back_from_confirm(app: &App) -> Screen {
    if app.profiling_success.is_some() {
        Screen::ProfileSuccess
    } else if app.profiling_failure.is_some() {
        Screen::ProfileFailure
    } else if app.profiling_advisory.is_some() {
        Screen::ProfileAdvisory
    } else {
        Screen::ModelPicker
    }
}

pub async fn run_launcher(
    no_browser: bool,
    preferred_frontend: Option<FrontendMode>,
    tier_override: Option<crate::prefs::Tier>,
    force_picker: bool,
) -> Result<()> {
    let mut prefs = crate::prefs::load_prefs().await;
    prefs.no_browser = prefs.no_browser || no_browser;

    // Apply tier override if given
    if let Some(tier) = tier_override {
        prefs.preferred_tier = Some(tier);
    }

    let mut app = App::new(prefs);
    app.preferred_frontend = preferred_frontend.or(app.prefs.preferred_frontend);

    // If --pick flag, clear the tier preference so picker shows
    if force_picker {
        app.prefs.preferred_tier = None;
    }

    // Sync settings indices from persisted prefs
    sync_settings_from_prefs(&mut app);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
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
    let preset_file = ozone_core::paths::presets_path();
    let bench_file = model_dir.join("bench-results.txt");
    let catalog_model_dir = model_dir.clone();
    let catalog_preset_file = preset_file.clone();
    let catalog_bench_file = bench_file.clone();
    let (cat_tx, mut cat_rx) = tokio::sync::oneshot::channel::<(u64, Vec<CatalogRecord>)>();
    tokio::spawn(async move {
        let signature = crate::catalog::catalog_signature(
            &catalog_model_dir,
            &catalog_preset_file,
            &catalog_bench_file,
        )
        .await
        .unwrap_or_default();
        let records = crate::catalog::load_catalog(
            &catalog_model_dir,
            &catalog_preset_file,
            &catalog_bench_file,
        )
        .await
        .unwrap_or_default();
        let _ = cat_tx.send((signature, records));
    });

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();
    let mut last_catalog_signature: Option<u64> = None;

    let result = loop {
        // Check incoming async data
        if let Ok(hw) = hw_rx.try_recv() {
            app.hardware = Some(hw);
            if !app.catalog.is_empty() {
                app.splash_ready = true;
            }
        }
        if let Ok((signature, catalog)) = cat_rx.try_recv() {
            last_catalog_signature = Some(signature);
            apply_catalog_refresh(&mut app, catalog);
            if app.hardware.is_some() {
                app.splash_ready = true;
            }
        }
        if app.hardware.is_some() && !app.catalog.is_empty() {
            app.splash_ready = true;
        }

        loop {
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
                break;
            };
            match event {
                WorkflowEvent::Status { title, detail } => {
                    app.profiling_progress_title = title;
                    app.push_profile_progress(detail);
                }
                WorkflowEvent::Progress {
                    title,
                    detail,
                    current,
                    total,
                } => {
                    app.profiling_progress_title = title;
                    app.profiling_progress_current = current;
                    app.profiling_progress_total = total;
                    app.push_profile_progress(detail);
                }
                WorkflowEvent::Completed(report) => {
                    app.profiling_event_rx = None;
                    app.profiling_cancel = None;
                    app.profiling_success = Some(report);
                    app.profiling_failure = None;
                    app.profiling_choice_index = 0;
                    app.screen = Screen::ProfileSuccess;
                }
                WorkflowEvent::Failed(report) => {
                    app.profiling_event_rx = None;
                    app.profiling_cancel = None;
                    app.profiling_failure = Some(report);
                    app.profiling_success = None;
                    app.profiling_choice_index = 0;
                    app.screen = Screen::ProfileFailure;
                }
                WorkflowEvent::Cancelled => {
                    app.profiling_event_rx = None;
                    app.profiling_cancel = None;
                    app.set_status("Profiling cancelled.".into());
                    app.screen = Screen::Launcher;
                }
            }
        }

        // Execute a pending frontend launch choice (triggered by FrontendChoice Enter or --frontend bypass).
        if let Some(choice_idx) = app.pending_launch_choice.take() {
            match app.prefs.preferred_backend {
                Some(BackendMode::KoboldCpp) => {
                    if let Some(plan) = app.current_plan.clone() {
                        app.screen = Screen::Launching;
                        app.launch_start = Some(Instant::now());

                        let home = std::env::var("HOME").unwrap_or_default();
                        let launcher_path = crate::processes::resolved_kobold_launcher_path();
                        let model_path = std::path::PathBuf::from(&home)
                            .join("models")
                            .join(&plan.model_name);
                        let kc_args = build_kc_args(&plan);
                        match crate::processes::start_kobold(
                            &launcher_path,
                            &model_path.to_string_lossy(),
                            &kc_args,
                        )
                        .await
                        {
                            Ok(_) => {
                                let mut updated_prefs = app.prefs.clone();
                                updated_prefs.last_model_name = plan.model_name.clone();
                                updated_prefs.last_context_size = Some(plan.context_size);
                                updated_prefs.last_gpu_layers = Some(plan.gpu_layers);
                                updated_prefs.last_quant_kv = Some(plan.quant_kv);
                                let _ = crate::prefs::save_prefs(&updated_prefs).await;
                                app.prefs = updated_prefs;
                                if choice_idx == 0 {
                                    if !app.prefs.no_browser {
                                        crate::processes::open_browser_app("http://localhost:8000");
                                    }
                                    app.screen = Screen::Monitor;
                                } else {
                                    app.ozone_plus_handoff = true;
                                    break Ok(());
                                }
                            }
                            Err(error) => {
                                app.set_error(format!("Launch failed: {error}"));
                                app.screen = Screen::Launcher;
                            }
                        }
                    } else {
                        app.set_error("No launch plan selected.".into());
                        app.screen = Screen::Launcher;
                    }
                }
                Some(BackendMode::LlamaCpp) => {
                    if let Some(plan) = app.current_plan.clone() {
                        app.screen = Screen::Launching;
                        app.launch_start = Some(Instant::now());

                        let home = std::env::var("HOME").unwrap_or_default();
                        let server_path = match crate::processes::resolved_llamacpp_server_path() {
                            Ok(path) => path,
                            Err(error) => {
                                app.set_error(format!("Launch failed: {error}"));
                                app.screen = Screen::Launcher;
                                continue;
                            }
                        };
                        let model_path = std::path::PathBuf::from(&home)
                            .join("models")
                            .join(&plan.model_name);
                        let llama_args = build_llama_args(&plan);
                        match crate::processes::start_llamacpp(
                            &server_path,
                            &model_path.to_string_lossy(),
                            &llama_args,
                        )
                        .await
                        {
                            Ok(_) => {
                                let mut updated_prefs = app.prefs.clone();
                                updated_prefs.last_model_name = plan.model_name.clone();
                                updated_prefs.last_context_size = Some(plan.context_size);
                                updated_prefs.last_gpu_layers = Some(plan.gpu_layers);
                                updated_prefs.last_quant_kv = Some(plan.quant_kv);
                                let _ = crate::prefs::save_prefs(&updated_prefs).await;
                                app.prefs = updated_prefs;
                                if choice_idx == 0 {
                                    if !app.prefs.no_browser {
                                        crate::processes::open_browser_app("http://localhost:8000");
                                    }
                                    app.screen = Screen::Monitor;
                                } else {
                                    app.ozone_plus_handoff = true;
                                    break Ok(());
                                }
                            }
                            Err(error) => {
                                app.set_error(format!("Launch failed: {error}"));
                                app.screen = Screen::Launcher;
                            }
                        }
                    } else {
                        app.set_error("No launch plan selected.".into());
                        app.screen = Screen::Launcher;
                    }
                }
                Some(BackendMode::Ollama) => {
                    if choice_idx == 0 {
                        if !app.prefs.no_browser {
                            crate::processes::open_browser_app("http://localhost:8000");
                        }
                        app.screen = Screen::Monitor;
                    } else {
                        app.set_error(
                            "ozone+ handoff currently requires KoboldCpp. Use SillyTavern for Ollama-backed launches.".into(),
                        );
                        app.screen = Screen::Launcher;
                    }
                }
                None => {
                    app.set_error("Configure backend in Settings first".into());
                    app.screen = Screen::Launcher;
                }
            }
        }

        // Draw
        terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            match app.screen {
                Screen::Splash => splash::render(f, &app),
                Screen::TierPicker => {
                    tier_picker::render_tier_picker(f, f.area(), &app.tier_picker)
                }
                Screen::Launcher => launcher::render(f, &app),
                Screen::ExitConfirm => launcher::render_exit_confirm(f, &app),
                Screen::ModelPicker => launcher::render_model_picker(f, &app),
                Screen::Confirm => launcher::render_confirm(f, &app),
                Screen::FrontendChoice => launcher::render_frontend_choice(f, &app),
                Screen::Launching => launcher::render_launching(f, &app),
                Screen::ProfileAdvisory => launcher::render_profile_advisory(f, &app),
                Screen::ProfileConfirm => launcher::render_profile_confirm(f, &app),
                Screen::ProfileRunning => launcher::render_profile_running(f, &app),
                Screen::ProfileSuccess => launcher::render_profile_success(f, &app),
                Screen::ProfileFailure => launcher::render_profile_failure(f, &app),
                Screen::Settings => launcher::render_settings(f, &app),
                Screen::Monitor => monitor::render(f, &app),
            }
        })?;

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.screen {
                    Screen::Splash if app.splash_ready => {
                        app.screen = next_screen_after_splash(&app);
                    }
                    Screen::TierPicker => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Up => app.tier_picker.up(),
                        KeyCode::Down => app.tier_picker.down(),
                        KeyCode::Enter => {
                            let tier = app.tier_picker.selected_tier();
                            app.prefs.preferred_tier = Some(tier);
                            // Save preference
                            let prefs_clone = app.prefs.clone();
                            tokio::spawn(async move {
                                let _ = crate::prefs::save_prefs(&prefs_clone).await;
                            });
                            app.screen = Screen::Launcher;
                        }
                        _ => {}
                    },
                    Screen::Launcher => match key.code {
                        KeyCode::Char('q') => break Ok(()),
                        KeyCode::Esc => open_exit_confirm(&mut app),
                        KeyCode::Up if app.selected_action > 0 => {
                            app.selected_action -= 1;
                        }
                        KeyCode::Down if app.selected_action < 6 => {
                            app.selected_action += 1;
                        }
                        KeyCode::Enter => match app.selected_action {
                            0 => {
                                // Launch configured stack
                                match app.prefs.preferred_backend {
                                    None => {
                                        app.set_error("Configure backend in Settings first".into());
                                    }
                                    Some(BackendMode::KoboldCpp) => {
                                        if !app.catalog.is_empty() {
                                            app.reset_profile_flow();
                                            app.model_picker_mode = ModelPickerMode::Launch;
                                            app.screen = Screen::ModelPicker;
                                        }
                                    }
                                    Some(BackendMode::LlamaCpp) => {
                                        if !app.catalog.is_empty() {
                                            app.reset_profile_flow();
                                            app.model_picker_mode = ModelPickerMode::Launch;
                                            app.screen = Screen::ModelPicker;
                                        }
                                    }
                                    Some(BackendMode::Ollama) => {
                                        if crate::processes::is_url_ready(
                                            "http://127.0.0.1:11434/api/tags",
                                        )
                                        .await
                                        {
                                            app.set_status("Ollama backend ready.".into());
                                            queue_frontend_launch(&mut app);
                                        } else {
                                            app.set_error("Ollama not running on :11434".into());
                                        }
                                    }
                                }
                            }
                            1
                                // Profile / recommend model
                                if !app.catalog.is_empty() => {
                                    app.reset_profile_flow();
                                    app.model_picker_mode = ModelPickerMode::Profile;
                                    app.screen = Screen::ModelPicker;
                                }
                            2 => {
                                // Open ozone+ shell (direct handoff)
                                app.ozone_plus_handoff = true;
                                break Ok(());
                            }
                            3 => {
                                // Settings
                                open_settings(&mut app);
                            }
                            4 => {
                                // Clear GPU backends
                                let _ = crate::processes::clear_gpu_backends().await;
                                app.services = crate::processes::get_service_status().await;
                                last_refresh = Instant::now();
                                app.set_status("GPU backends cleared.".into());
                            }
                            5 => {
                                // Monitor
                                app.screen = Screen::Monitor;
                                app.launch_start = Some(Instant::now());
                            }
                            6 => open_exit_confirm(&mut app),
                            _ => {}
                        },
                        _ => {}
                    },
                    Screen::ExitConfirm => match key.code {
                        KeyCode::Esc | KeyCode::Char('n') => app.screen = Screen::Launcher,
                        KeyCode::Left | KeyCode::Up if app.exit_confirm_index > 0 => {
                            app.exit_confirm_index -= 1;
                        }
                        KeyCode::Right | KeyCode::Down if app.exit_confirm_index < 1 => {
                            app.exit_confirm_index += 1;
                        }
                        KeyCode::Enter | KeyCode::Char('y') => {
                            if app.exit_confirm_index == 0 {
                                break Ok(());
                            }
                            app.screen = Screen::Launcher;
                        }
                        _ => {}
                    },
                    Screen::Settings => match key.code {
                        KeyCode::Tab | KeyCode::Right => {
                            app.settings_section = (app.settings_section + 1) % 2;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            app.settings_section = (app.settings_section + 1) % 2;
                        }
                        KeyCode::Up => match app.settings_section {
                            0 => {
                                if app.settings_backend_index > 0 {
                                    app.settings_backend_index -= 1;
                                }
                            }
                            _ => {
                                if app.settings_frontend_index > 0 {
                                    app.settings_frontend_index -= 1;
                                }
                            }
                        },
                        KeyCode::Down => match app.settings_section {
                            0 => {
                                if app.settings_backend_index < 2 {
                                    app.settings_backend_index += 1;
                                }
                            }
                            _ => {
                                if app.settings_frontend_index < 1 {
                                    app.settings_frontend_index += 1;
                                }
                            }
                        },
                        KeyCode::Enter => {
                            app.prefs.preferred_backend = match app.settings_backend_index {
                                0 => Some(BackendMode::KoboldCpp),
                                1 => Some(BackendMode::LlamaCpp),
                                2 => Some(BackendMode::Ollama),
                                _ => None,
                            };
                            app.prefs.preferred_frontend = match app.settings_frontend_index {
                                0 => Some(FrontendMode::SillyTavern),
                                1 => Some(FrontendMode::OzonePlus),
                                _ => None,
                            };
                            let _ = crate::prefs::save_prefs(&app.prefs).await;
                            app.preferred_frontend =
                                preferred_frontend.or(app.prefs.preferred_frontend);
                            app.set_status("Settings saved.".into());
                            app.screen = Screen::Launcher;
                        }
                        KeyCode::Esc => {
                            sync_settings_from_prefs(&mut app);
                            app.screen = Screen::Launcher;
                        }
                        _ => {}
                    },
                    Screen::ModelPicker => match key.code {
                        KeyCode::Esc => {
                            if !app.model_filter.is_empty() {
                                app.model_filter.clear();
                            } else {
                                app.screen = Screen::Launcher;
                            }
                        }
                        KeyCode::Up => {
                            let count = app.filtered_catalog_len();
                            if app.selected_model > 0 {
                                app.selected_model -= 1;
                            }
                            let _ = count; // keep borrow checker happy
                        }
                        KeyCode::Down => {
                            let count = app.filtered_catalog_len();
                            if app.selected_model + 1 < count {
                                app.selected_model += 1;
                            }
                        }
                        KeyCode::Backspace => {
                            app.model_filter.pop();
                            app.selected_model = 0;
                        }
                        KeyCode::Enter => {
                            if let Some(record) = app.filtered_catalog_get(app.selected_model) {
                                match app.model_picker_mode {
                                    ModelPickerMode::Launch => {
                                        if let Some(hw) = &app.hardware {
                                            let plan = crate::planner::plan_launch(&record, hw);
                                            app.current_plan = Some(plan);
                                            app.screen = Screen::Confirm;
                                        }
                                    }
                                    ModelPickerMode::Profile => {
                                        match profiling::build_advisory(
                                            &record,
                                            app.hardware.as_ref(),
                                            &app.services,
                                        ) {
                                            Ok(advisory) => {
                                                app.profiling_advisory = Some(advisory);
                                                app.profiling_choice_index = 0;
                                                app.profiling_success = None;
                                                app.profiling_failure = None;
                                                app.screen = Screen::ProfileAdvisory;
                                            }
                                            Err(error) => {
                                                app.set_error(format!(
                                                    "Could not prepare profiling advice: {error}"
                                                ));
                                                app.screen = Screen::Launcher;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char(c)
                            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' =>
                        {
                            app.model_filter.push(c);
                            app.selected_model = 0;
                        }
                        _ => {}
                    },
                    Screen::Confirm => match key.code {
                        KeyCode::Esc | KeyCode::Char('n') => app.screen = back_from_confirm(&app),
                        KeyCode::Enter | KeyCode::Char('y') if app.current_plan.is_some() => {
                            queue_frontend_launch(&mut app);
                        }
                        _ => {}
                    },
                    Screen::FrontendChoice => match key.code {
                        KeyCode::Esc => app.screen = Screen::Confirm,
                        KeyCode::Up if app.frontend_choice_index > 0 => {
                            app.frontend_choice_index -= 1;
                        }
                        KeyCode::Down if app.frontend_choice_index < 1 => {
                            app.frontend_choice_index += 1;
                        }
                        KeyCode::Enter if app.current_plan.is_some() => {
                            app.pending_launch_choice = Some(app.frontend_choice_index);
                        }
                        _ => {}
                    },
                    Screen::ProfileAdvisory => match key.code {
                        KeyCode::Esc => app.screen = Screen::ModelPicker,
                        KeyCode::Up if app.profiling_choice_index > 0 => {
                            app.profiling_choice_index -= 1;
                        }
                        KeyCode::Down => {
                            let count = app
                                .profiling_advisory
                                .as_ref()
                                .map(|advisory| advisory.available_actions.len())
                                .unwrap_or(0);
                            if app.profiling_choice_index + 1 < count {
                                app.profiling_choice_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(advisory) = &app.profiling_advisory {
                                if let Some(action) = advisory
                                    .available_actions
                                    .get(app.profiling_choice_index)
                                    .cloned()
                                {
                                    match action {
                                        ProfilingAction::LaunchRecommended => {
                                            if let (Some(record), Some(hw)) = (
                                                app.filtered_catalog_get(app.selected_model),
                                                app.hardware.as_ref(),
                                            ) {
                                                match profiling::preferred_launch_plan(&record, hw)
                                                {
                                                    Ok(plan) => {
                                                        app.current_plan = Some(plan);
                                                        app.screen = Screen::Confirm;
                                                    }
                                                    Err(error) => {
                                                        app.set_error(format!(
                                                            "Could not build launch plan: {error}"
                                                        ));
                                                        app.screen = Screen::Launcher;
                                                    }
                                                }
                                            }
                                        }
                                        ProfilingAction::ReviewIssue => {
                                            if let Some(record) =
                                                app.filtered_catalog_get(app.selected_model)
                                            {
                                                app.profiling_failure =
                                                    Some(profiling::blocking_issue_report(&record));
                                                app.profiling_choice_index = 0;
                                                app.screen = Screen::ProfileFailure;
                                            }
                                        }
                                        action => {
                                            app.profiling_pending_action = Some(action);
                                            app.screen = Screen::ProfileConfirm;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    Screen::ProfileConfirm => match key.code {
                        KeyCode::Esc => app.screen = Screen::ProfileAdvisory,
                        KeyCode::Enter => {
                            if let (Some(record), Some(action)) = (
                                app.filtered_catalog_get(app.selected_model),
                                app.profiling_pending_action.clone(),
                            ) {
                                let request = WorkflowRequest {
                                    record,
                                    hardware: app.hardware.clone().unwrap_or_default(),
                                    action,
                                };
                                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                                let cancel = CancellationToken::new();
                                let cancel_clone = cancel.clone();
                                app.profiling_event_rx = Some(rx);
                                app.profiling_cancel = Some(cancel);
                                app.profiling_progress_title = "Preparing".into();
                                app.profiling_progress_current = 0;
                                app.profiling_progress_total = 0;
                                app.profiling_progress.clear();
                                app.push_profile_progress("Preparing workflow…".into());
                                app.profiling_choice_index = 0;
                                app.screen = Screen::ProfileRunning;
                                tokio::spawn(async move {
                                    let _ =
                                        profiling::run_workflow(request, tx, cancel_clone).await;
                                });
                            }
                        }
                        _ => {}
                    },
                    Screen::ProfileRunning => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            if let Some(token) = &app.profiling_cancel {
                                token.cancel();
                                app.push_profile_progress("⏳ Cancelling…".into());
                            }
                        }
                        _ => {}
                    },
                    Screen::ProfileSuccess => match key.code {
                        KeyCode::Esc => {
                            // Back to advisory — re-run build_advisory to refresh state
                            if let Some(record) = app.filtered_catalog_get(app.selected_model) {
                                match profiling::build_advisory(
                                    &record,
                                    app.hardware.as_ref(),
                                    &app.services,
                                ) {
                                    Ok(advisory) => {
                                        app.profiling_advisory = Some(advisory);
                                        app.profiling_choice_index = 0;
                                        app.screen = Screen::ProfileAdvisory;
                                    }
                                    Err(_) => {
                                        app.reset_profile_flow();
                                        app.screen = Screen::Launcher;
                                    }
                                }
                            } else {
                                app.reset_profile_flow();
                                app.screen = Screen::Launcher;
                            }
                        }
                        KeyCode::Char('q') => {
                            app.reset_profile_flow();
                            app.screen = Screen::Launcher;
                        }
                        KeyCode::Up if app.profiling_choice_index > 0 => {
                            app.profiling_choice_index -= 1;
                        }
                        KeyCode::Down => {
                            let count = app
                                .profiling_success
                                .as_ref()
                                .map(|report| report.available_actions().len())
                                .unwrap_or(0);
                            if app.profiling_choice_index + 1 < count {
                                app.profiling_choice_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(report) = &app.profiling_success {
                                let actions = report.available_actions();
                                if let Some(action) =
                                    actions.get(app.profiling_choice_index).cloned()
                                {
                                    match action {
                                        ProfilingAction::LaunchRecommended => {
                                            if let (Some(record), Some(hw)) = (
                                                app.filtered_catalog_get(app.selected_model),
                                                app.hardware.as_ref(),
                                            ) {
                                                match profiling::preferred_launch_plan(&record, hw)
                                                {
                                                    Ok(plan) => {
                                                        app.current_plan = Some(plan);
                                                        app.screen = Screen::Confirm;
                                                    }
                                                    Err(error) => {
                                                        app.set_error(format!(
                                                            "Could not build launch plan: {error}"
                                                        ));
                                                        app.screen = Screen::Launcher;
                                                    }
                                                }
                                            }
                                        }
                                        action => {
                                            app.profiling_pending_action = Some(action);
                                            app.screen = Screen::ProfileConfirm;
                                        }
                                    }
                                } else {
                                    app.reset_profile_flow();
                                    app.screen = Screen::Launcher;
                                }
                            }
                        }
                        _ => {}
                    },
                    Screen::ProfileFailure => match key.code {
                        KeyCode::Esc => {
                            // Back to advisory
                            if let Some(record) = app.filtered_catalog_get(app.selected_model) {
                                match profiling::build_advisory(
                                    &record,
                                    app.hardware.as_ref(),
                                    &app.services,
                                ) {
                                    Ok(advisory) => {
                                        app.profiling_advisory = Some(advisory);
                                        app.profiling_choice_index = 0;
                                        app.screen = Screen::ProfileAdvisory;
                                    }
                                    Err(_) => {
                                        app.reset_profile_flow();
                                        app.screen = Screen::Launcher;
                                    }
                                }
                            } else {
                                app.reset_profile_flow();
                                app.screen = Screen::Launcher;
                            }
                        }
                        KeyCode::Char('q') => {
                            app.reset_profile_flow();
                            app.screen = Screen::Launcher;
                        }
                        KeyCode::Up if app.profiling_choice_index > 0 => {
                            app.profiling_choice_index -= 1;
                        }
                        KeyCode::Down => {
                            let count = app
                                .profiling_failure
                                .as_ref()
                                .map(|report| report.available_actions().len())
                                .unwrap_or(0);
                            if app.profiling_choice_index + 1 < count {
                                app.profiling_choice_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(report) = &app.profiling_failure {
                                let actions = report.available_actions();
                                if let Some(action) =
                                    actions.get(app.profiling_choice_index).cloned()
                                {
                                    app.profiling_pending_action = Some(action);
                                    app.screen = Screen::ProfileConfirm;
                                }
                            }
                        }
                        _ => {}
                    },
                    Screen::Monitor => match key.code {
                        KeyCode::Char('q') => break Ok(()),
                        KeyCode::Esc => {
                            app.screen = Screen::Launcher;
                        }
                        KeyCode::Char('s') => {
                            let _ = crate::processes::clear_gpu_backends().await;
                            app.services = crate::processes::get_service_status().await;
                            app.set_status("GPU backends cleared.".into());
                            app.screen = Screen::Launcher;
                        }
                        KeyCode::Char('r') => {
                            app.screen = Screen::Launcher;
                        }
                        _ => {}
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

        // 2s refresh for service status and monitor data
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            if matches!(app.screen, Screen::Monitor) {
                app.services = crate::processes::get_service_status().await;
                app.update_disk();
                app.tokens_per_sec = if app.services.kobold_running {
                    crate::processes::get_kobold_perf().await
                } else {
                    None
                };
                if let Some(ref mut hw) = app.hardware {
                    *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware)
                        .await
                        .unwrap_or_default();
                }
            } else if matches!(app.screen, Screen::Launcher) {
                app.services = crate::processes::get_service_status().await;
            }

            if matches!(
                app.screen,
                Screen::Launcher
                    | Screen::ModelPicker
                    | Screen::Confirm
                    | Screen::FrontendChoice
                    | Screen::Settings
                    | Screen::ExitConfirm
                    | Screen::ProfileAdvisory
                    | Screen::ProfileConfirm
                    | Screen::ProfileSuccess
                    | Screen::ProfileFailure
            ) {
                let signature =
                    crate::catalog::catalog_signature(&model_dir, &preset_file, &bench_file)
                        .await
                        .unwrap_or_default();
                if last_catalog_signature != Some(signature) {
                    let catalog =
                        crate::catalog::load_catalog(&model_dir, &preset_file, &bench_file)
                            .await
                            .unwrap_or_default();
                    last_catalog_signature = Some(signature);
                    apply_catalog_refresh(&mut app, catalog);
                }
            }
        }

        // Auto-advance from splash after data is ready
        if app.screen == Screen::Splash && app.splash_ready && app.ticker > 25 {
            app.screen = next_screen_after_splash(&app);
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    if app.ozone_plus_handoff {
        let ozone_plus_bin = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|dir| dir.join("ozone-plus")))
            .filter(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("ozone-plus"));
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new(ozone_plus_bin);
        command.arg("handoff").arg("--launcher-session");
        match app.prefs.preferred_backend {
            Some(BackendMode::KoboldCpp) => {
                command.env("OZONE__BACKEND__TYPE", "koboldcpp");
                command.env(
                    "OZONE__BACKEND__URL",
                    ozone_core::paths::koboldcpp_base_url(),
                );
            }
            Some(BackendMode::LlamaCpp) => {
                command.env("OZONE__BACKEND__TYPE", "llamacpp");
                command.env(
                    "OZONE__BACKEND__URL",
                    ozone_core::paths::llamacpp_base_url(),
                );
            }
            Some(BackendMode::Ollama) | None => {}
        }
        let err = command.exec();
        return Err(anyhow::anyhow!("Failed to exec ozone-plus: {err}"));
    }
    result
}

pub async fn run_monitor() -> Result<()> {
    let prefs = crate::prefs::load_prefs().await;
    let mut app = App::new(prefs);
    app.screen = Screen::Monitor;
    app.hardware = Some(
        tokio::task::spawn_blocking(crate::hardware::load_hardware)
            .await
            .unwrap_or_default(),
    );
    app.services = crate::processes::get_service_status().await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();

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

        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            app.services = crate::processes::get_service_status().await;
            app.update_disk();
            app.tokens_per_sec = if app.services.kobold_running {
                crate::processes::get_kobold_perf().await
            } else {
                None
            };
            if let Some(ref mut hw) = app.hardware {
                *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware)
                    .await
                    .unwrap_or_default();
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_screen_syncs_from_saved_preferences() {
        let mut app = App::new(Preferences {
            preferred_backend: Some(BackendMode::Ollama),
            preferred_frontend: Some(FrontendMode::OzonePlus),
            ..Preferences::default()
        });
        app.settings_section = 1;
        app.settings_backend_index = 0;
        app.settings_frontend_index = 0;

        sync_settings_from_prefs(&mut app);

        assert_eq!(app.settings_section, 0);
        assert_eq!(app.settings_backend_index, 2);
        assert_eq!(app.settings_frontend_index, 1);
    }

    #[test]
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
            best_tokens_per_sec: None,
            recommended_profile: None,
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
                quant_kv: 1,
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
            quant_kv: 1,
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
        assert_eq!(app.screen, Screen::ModelPicker);
        assert_eq!(
            app.error_msg.as_deref(),
            Some("Selected model is no longer available.")
        );
    }
}
