//! Launcher event loop — extracted from `mod.rs`.

use std::{
    io,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::Clear, Terminal};
use tokio::sync::mpsc::error::TryRecvError;

use crate::hardware::HardwareProfile;
use crate::prefs::Preferences;


use super::*;
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
            let lost = app.profiling.event_rx.is_none();
            let event = match app.profiling.event_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(event) => Some(event),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => {
                        app.profiling.event_rx = None;
                        None
                    }
                },
                None => None,
            };
            let Some(event) = event else {
                // If the channel dropped without sending Completed/Failed, and we're
                // on ProfileRunning, the task silently crashed — bail to launcher.
                if lost && app.screen == Screen::ProfileRunning {
                    app.profiling.cancel = None;
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

