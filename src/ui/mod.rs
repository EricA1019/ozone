use std::io;
use std::time::{Duration, Instant};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::planner::LaunchPlan;
use crate::prefs::Preferences;
use crate::processes::{DiskSnapshot, ServiceStatus};

pub mod launcher;
pub mod monitor;
pub mod splash;

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Splash,
    Launcher,
    ModelPicker,
    Confirm,
    Launching,
    Monitor,
}

pub struct App {
    pub screen: Screen,
    pub hardware: Option<HardwareProfile>,
    pub catalog: Vec<CatalogRecord>,
    pub selected_model: usize,
    pub selected_action: usize,
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
    pub last_refresh: Instant,
    pub ticker: u64,
    pub error_msg: Option<String>,
    pub status_msg: Option<String>,
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
            current_plan: None,
            prefs,
            services: ServiceStatus { kobold_running: false, kobold_model: None, st_running: false },
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
        }
    }

    pub fn tick(&mut self) {
        self.ticker += 1;
        self.splash_pulse = (self.ticker / 5) % 2 == 0;
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
                    if self.disk_read_history.len() > 30 { self.disk_read_history.remove(0); }
                    self.disk_write_history.push((w * 10.0) as u64);
                    if self.disk_write_history.len() > 30 { self.disk_write_history.remove(0); }
                }
                self.disk_prev = Some(curr);
                self.disk_prev_time = Instant::now();
            }
        }
    }
}

pub async fn run_launcher(no_browser: bool) -> Result<()> {
    let mut prefs = crate::prefs::load_prefs().await;
    prefs.no_browser = prefs.no_browser || no_browser;

    let mut app = App::new(prefs);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Spawn hardware loading
    let (hw_tx, mut hw_rx) = tokio::sync::oneshot::channel::<HardwareProfile>();
    tokio::spawn(async move {
        let hw = tokio::task::spawn_blocking(crate::hardware::load_hardware).await.unwrap_or_default();
        let _ = hw_tx.send(hw);
    });

    // Spawn catalog loading
    let home = std::env::var("HOME").unwrap_or_default();
    let model_dir = std::path::PathBuf::from(&home).join("models");
    let preset_file = model_dir.join("koboldcpp-presets.conf");
    let bench_file = model_dir.join("bench-results.txt");
    let (cat_tx, mut cat_rx) = tokio::sync::oneshot::channel::<Vec<CatalogRecord>>();
    tokio::spawn(async move {
        let records = crate::catalog::load_catalog(&model_dir, &preset_file, &bench_file)
            .await.unwrap_or_default();
        let _ = cat_tx.send(records);
    });

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();

    let result = loop {
        // Check incoming async data
        if let Ok(hw) = hw_rx.try_recv() {
            app.hardware = Some(hw);
            if !app.catalog.is_empty() { app.splash_ready = true; }
        }
        if let Ok(catalog) = cat_rx.try_recv() {
            let last = app.prefs.last_model_name.clone();
            app.selected_model = catalog.iter().position(|r| r.model_name == last).unwrap_or(0);
            app.catalog = catalog;
            if app.hardware.is_some() { app.splash_ready = true; }
        }
        if app.hardware.is_some() && !app.catalog.is_empty() {
            app.splash_ready = true;
        }

        // Draw
        terminal.draw(|f| {
            match app.screen {
                Screen::Splash => splash::render(f, &app),
                Screen::Launcher => launcher::render(f, &app),
                Screen::ModelPicker => launcher::render_model_picker(f, &app),
                Screen::Confirm => launcher::render_confirm(f, &app),
                Screen::Monitor => monitor::render(f, &app),
                Screen::Launching => launcher::render_launching(f, &app),
            }
        })?;

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match app.screen {
                    Screen::Splash => {
                        if app.splash_ready {
                            app.screen = Screen::Launcher;
                        }
                    }
                    Screen::Launcher => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                            KeyCode::Up => { if app.selected_action > 0 { app.selected_action -= 1; } }
                            KeyCode::Down => { if app.selected_action < 5 { app.selected_action += 1; } }
                            KeyCode::Enter => {
                                match app.selected_action {
                                    0 => {
                                        if !app.catalog.is_empty() {
                                            app.screen = Screen::ModelPicker;
                                        }
                                    }
                                    1 => {
                                        app.status_msg = Some("Launching SillyTavern...".into());
                                    }
                                    2 => {
                                        let _ = crate::processes::clear_gpu_backends().await;
                                        // Immediately reflect cleared state — don't wait for 2s refresh timer
                                        app.services.kobold_running = false;
                                        app.services.kobold_model = None;
                                        app.services.st_running = false;
                                        last_refresh = Instant::now();
                                        app.status_msg = Some("GPU backends cleared.".into());
                                    }
                                    3 => {
                                        app.screen = Screen::Monitor;
                                        app.launch_start = Some(Instant::now());
                                    }
                                    4 => {
                                        app.screen = Screen::ModelPicker;
                                    }
                                    5 => break Ok(()),
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    Screen::ModelPicker => {
                        match key.code {
                            KeyCode::Esc => app.screen = Screen::Launcher,
                            KeyCode::Up => { if app.selected_model > 0 { app.selected_model -= 1; } }
                            KeyCode::Down => {
                                if app.selected_model + 1 < app.catalog.len() { app.selected_model += 1; }
                            }
                            KeyCode::Enter => {
                                if let Some(hw) = &app.hardware {
                                    if let Some(record) = app.catalog.get(app.selected_model) {
                                        let plan = crate::planner::plan_launch(record, hw);
                                        app.current_plan = Some(plan);
                                        app.screen = Screen::Confirm;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Screen::Confirm => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('n') => app.screen = Screen::Launcher,
                            KeyCode::Enter | KeyCode::Char('y') => {
                                if let Some(plan) = app.current_plan.clone() {
                                    app.screen = Screen::Launching;
                                    app.launch_start = Some(Instant::now());
                                    terminal.draw(|f| launcher::render_launching(f, &app))?;

                                    let home = std::env::var("HOME").unwrap_or_default();
                                    let launcher_path = std::path::PathBuf::from(&home).join("models/launch-koboldcpp.sh");
                                    let model_path = std::path::PathBuf::from(&home).join("models").join(&plan.model_name);
                                    let mut kc_args = vec![
                                        format!("--gpulayers"),
                                        format!("{}", plan.gpu_layers),
                                        format!("--contextsize"),
                                        format!("{}", plan.context_size),
                                        format!("--quantkv"),
                                        format!("{}", plan.quant_kv),
                                    ];
                                    if let Some(t) = plan.threads {
                                        kc_args.push("--threads".to_string());
                                        kc_args.push(t.to_string());
                                    }
                                    if let Some(bt) = plan.blas_threads {
                                        kc_args.push("--blasthreads".to_string());
                                        kc_args.push(bt.to_string());
                                    }
                                    match crate::processes::start_kobold(&launcher_path, &model_path.to_string_lossy(), &kc_args).await {
                                        Ok(_) => {
                                            let mut updated_prefs = app.prefs.clone();
                                            updated_prefs.last_model_name = plan.model_name.clone();
                                            updated_prefs.last_context_size = Some(plan.context_size);
                                            updated_prefs.last_gpu_layers = Some(plan.gpu_layers);
                                            updated_prefs.last_quant_kv = Some(plan.quant_kv);
                                            let _ = crate::prefs::save_prefs(&updated_prefs).await;
                                            app.prefs = updated_prefs;
                                            if !app.prefs.no_browser {
                                                crate::processes::open_browser_app("http://localhost:8000");
                                            }
                                            app.screen = Screen::Monitor;
                                        }
                                        Err(e) => {
                                            app.error_msg = Some(format!("Launch failed: {e}"));
                                            app.screen = Screen::Launcher;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Screen::Monitor => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                            KeyCode::Char('s') => {
                                let _ = crate::processes::clear_gpu_backends().await;
                                // Return to launcher with updated status rather than quitting
                                app.services.kobold_running = false;
                                app.services.kobold_model = None;
                                app.services.st_running = false;
                                app.status_msg = Some("GPU backends cleared.".into());
                                app.screen = Screen::Launcher;
                            }
                            KeyCode::Char('r') => {
                                app.screen = Screen::Launcher;
                            }
                            _ => {}
                        }
                    }
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
        if last_refresh.elapsed() >= Duration::from_secs(2)
            && matches!(app.screen, Screen::Monitor | Screen::Launcher)
        {
            last_refresh = Instant::now();
            app.services = crate::processes::get_service_status().await;
            if matches!(app.screen, Screen::Monitor) {
                app.update_disk();
                app.tokens_per_sec = crate::processes::get_kobold_perf().await;
                if let Some(ref mut hw) = app.hardware {
                    *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware).await.unwrap_or_default();
                }
            }
        }

        // Auto-advance from splash after data is ready
        if app.screen == Screen::Splash && app.splash_ready && app.ticker > 25 {
            app.screen = Screen::Launcher;
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

pub async fn run_monitor() -> Result<()> {
    let prefs = crate::prefs::load_prefs().await;
    let mut app = App::new(prefs);
    app.screen = Screen::Monitor;
    app.hardware = Some(tokio::task::spawn_blocking(crate::hardware::load_hardware).await.unwrap_or_default());
    app.services = crate::processes::get_service_status().await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|f| monitor::render(f, &app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('s') => {
                        let _ = crate::processes::clear_gpu_backends().await;
                        // Draw one final frame showing cleared state before exit
                        app.services.kobold_running = false;
                        app.services.kobold_model = None;
                        app.services.st_running = false;
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
            app.tokens_per_sec = crate::processes::get_kobold_perf().await;
            if let Some(ref mut hw) = app.hardware {
                *hw = tokio::task::spawn_blocking(crate::hardware::load_hardware).await.unwrap_or_default();
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
