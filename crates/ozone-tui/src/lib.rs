//! `ozone-tui` — chat-first TUI shell for ozone+.
//!
//! Provides the ratatui/crossterm terminal interface: session list, conversation
//! pane, input composer, status area, context inspector, and the runtime
//! integration seam for streaming generation.

pub mod app;
pub mod input;
pub mod layout;
pub mod mock;
pub mod render;
pub mod theme;

use std::{error::Error, fmt, io};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub use app::{
    AppBootstrap, BranchItem, CharacterDetail, CharacterEntry, CharacterListState, CommandEntry,
    CommandPaletteState, ContextDryRunPreview, ContextPreview, ContextTokenBudget, DraftState,
    EntryKind, FocusTarget, FolderPickerState, GenerationPoll, MenuItem, MenuState, RecallBrowser,
    RuntimeCancellation, RuntimeCompletion, RuntimeContextRefresh, RuntimeFailure, RuntimePhase,
    RuntimeProgress, RuntimeSendReceipt, RuntimeSessionLoad, ScreenState, SessionContext,
    SessionListEntry, SessionListState, SessionMetadata, SessionState, SessionStats,
    SettingsCategory, SettingsEntry, SettingsState, ShellState, TranscriptItem, VisibleSessionItem,
};
pub use input::{
    dispatch_command_palette_key, dispatch_edit_key, dispatch_key, dispatch_menu_key, InputMode,
    KeyAction,
};
pub use layout::{
    build_layout, build_layout_for_area, LayoutMode, LayoutModel, PaneId, PaneLayout,
};
pub use mock::{MockRuntime, SessionRuntime};
pub use render::{build_render_model, render_shell, FolderPickerRenderModel, RenderModel};
pub use theme::ThemePreset;

#[derive(Debug, Clone)]
pub struct RunSessionOutcome {
    pub app: ShellState,
    pub layout: LayoutModel,
    pub render: RenderModel,
}

#[derive(Debug)]
pub enum RunSessionError<E> {
    Bootstrap(E),
    Runtime(E),
    Io(io::Error),
}

impl<E: fmt::Display> fmt::Display for RunSessionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bootstrap(error) => write!(f, "failed to bootstrap TUI session: {error}"),
            Self::Runtime(error) => write!(f, "failed to run TUI session: {error}"),
            Self::Io(error) => write!(f, "TUI terminal I/O failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for RunSessionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bootstrap(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

pub fn run_session<R>(
    context: SessionContext,
    runtime: &mut R,
) -> Result<RunSessionOutcome, RunSessionError<R::Error>>
where
    R: SessionRuntime,
{
    let bootstrap = runtime
        .bootstrap(&context)
        .map_err(RunSessionError::Bootstrap)?;
    let mut app = ShellState::new(context);
    app.hydrate(bootstrap);
    let layout = build_layout(&app);
    let render = build_render_model(&app, &layout);

    Ok(RunSessionOutcome {
        app,
        layout,
        render,
    })
}

pub fn run_terminal_session<R>(
    context: SessionContext,
    runtime: &mut R,
) -> Result<RunSessionOutcome, RunSessionError<R::Error>>
where
    R: SessionRuntime,
{
    let bootstrap = runtime
        .bootstrap(&context)
        .map_err(RunSessionError::Bootstrap)?;
    let mut app = ShellState::new(context);
    app.hydrate(bootstrap);

    let mut terminal = TerminalGuard::enter().map_err(RunSessionError::Io)?;

    // Install a panic hook that restores the terminal before printing the panic.
    // Without this, a panic leaves the terminal in raw mode + alternate screen,
    // making it appear to "crash" with garbled output.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original_hook(info);
    }));

    let result = run_event_loop(&mut app, &mut terminal, runtime);

    // Restore the default panic hook now that the event loop is done.
    let _ = std::panic::take_hook();

    // Drop the terminal guard (restores terminal) before returning.
    drop(terminal);

    result
}

fn run_event_loop<R>(
    app: &mut ShellState,
    terminal: &mut TerminalGuard,
    runtime: &mut R,
) -> Result<RunSessionOutcome, RunSessionError<R::Error>>
where
    R: SessionRuntime,
{
    use std::time::Duration;
    const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(50);

    loop {
        app.tick_count = app.tick_count.wrapping_add(1);

        let (layout, render) = {
            let mut drawn_layout = None;
            let mut drawn_render = None;
            terminal
                .terminal
                .draw(|frame| {
                    let layout = build_layout_for_area(app, frame.area());
                    let render = build_render_model(app, &layout);
                    render_shell(
                        frame,
                        &layout,
                        &render,
                        Some(&app.textarea),
                        app.command_palette
                            .open
                            .then_some(&app.command_palette.textarea),
                    );
                    drawn_layout = Some(layout);
                    drawn_render = Some(render);
                })
                .map_err(RunSessionError::Io)?;

            (
                drawn_layout.expect("draw must capture layout"),
                drawn_render.expect("draw must capture render"),
            )
        };

        if app.should_quit {
            sync_draft(runtime, app)?;
            return Ok(RunSessionOutcome {
                app: app.clone(),
                layout,
                render,
            });
        }

        if event::poll(INPUT_POLL_INTERVAL).map_err(RunSessionError::Io)? {
            match event::read().map_err(RunSessionError::Io)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let action = app.handle_key_event_with_layout(key, &layout);

                    // Populate session list when entering the SessionList screen
                    if app.screen == ScreenState::SessionList && app.session_list.entries.is_empty()
                    {
                        if let Ok(entries) = runtime.list_sessions() {
                            app.session_list.entries = entries;
                            app.session_list.selected = 0;
                        }
                    }

                    // Populate character list when entering the CharacterManager screen
                    if app.screen == ScreenState::CharacterManager
                        && app.character_list.entries.is_empty()
                    {
                        if let Ok(entries) = runtime.list_characters() {
                            app.character_list.entries = entries;
                        }
                    }

                    // Populate settings when entering the Settings screen
                    if app.screen == ScreenState::Settings && !app.settings.is_loaded() {
                        if let Ok(entries) = runtime.get_settings() {
                            app.settings.load(entries);
                        }
                    }

                    if action != KeyAction::Noop {
                        runtime
                            .dispatch(&app.session.context, action)
                            .map_err(RunSessionError::Runtime)?;
                        sync_draft(runtime, app)?;
                    }

                    for command in app.take_runtime_commands() {
                        match command {
                            app::RuntimeCommand::CreateSession { character_name } => {
                                match runtime.create_session(character_name.as_deref()) {
                                    Ok(session) => {
                                        if let Ok(sid) = ozone_core::session::SessionId::parse(
                                            &session.session_id,
                                        ) {
                                            app.session.context =
                                                app::SessionContext::new(sid, session.session_name);
                                        }
                                        app.hydrate(session.bootstrap);
                                        app.enter_conversation();
                                        app.status_line = Some("New conversation started".into());
                                    }
                                    Err(error) => {
                                        app.status_line =
                                            Some(format!("Failed to create session: {:?}", error));
                                    }
                                }
                            }
                            app::RuntimeCommand::SendDraft { prompt } => {
                                if let Some(receipt) = runtime
                                    .send_draft(&app.session.context, &prompt)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_send_receipt(receipt);
                                }
                            }
                            app::RuntimeCommand::EditMessage {
                                message_id,
                                content,
                            } => {
                                if let Some(refresh) = runtime
                                    .edit_message(&app.session.context, &message_id, &content)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_context_refresh(refresh);
                                }
                            }
                            app::RuntimeCommand::CancelGeneration => {
                                if let Some(cancellation) = runtime
                                    .cancel_generation(&app.session.context)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_runtime_cancellation(cancellation);
                                }
                            }
                            app::RuntimeCommand::BuildContextDryRun => {
                                if let Some(refresh) = runtime
                                    .build_context_dry_run(&app.session.context)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_context_refresh(refresh);
                                }
                            }
                            app::RuntimeCommand::ToggleBookmark { message_id } => {
                                if let Some(refresh) = runtime
                                    .toggle_bookmark(&app.session.context, &message_id)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_context_refresh(refresh);
                                }
                            }
                            app::RuntimeCommand::TogglePinnedMemory { message_id } => {
                                if let Some(refresh) = runtime
                                    .toggle_pinned_memory(&app.session.context, &message_id)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_context_refresh(refresh);
                                }
                            }
                            app::RuntimeCommand::RunCommand { input } => {
                                if let Some(refresh) = runtime
                                    .run_command(&app.session.context, &input)
                                    .map_err(RunSessionError::Runtime)?
                                {
                                    app.apply_context_refresh(refresh);
                                }
                            }
                            app::RuntimeCommand::CreateCharacter {
                                name,
                                description,
                                system_prompt,
                                personality,
                                scenario,
                                greeting,
                                example_dialogue,
                            } => match runtime.create_character(app::CharacterDetail {
                                card_id: String::new(),
                                name,
                                description,
                                system_prompt,
                                personality,
                                scenario,
                                greeting,
                                example_dialogue,
                            }) {
                                Ok(entry) => {
                                    app.status_line =
                                        Some(format!("Created character: {}", entry.name));
                                    if let Ok(chars) = runtime.list_characters() {
                                        app.character_list.entries = chars;
                                    }
                                }
                                Err(e) => {
                                    app.status_line = Some(format!("Create failed: {:?}", e));
                                }
                            },
                            app::RuntimeCommand::UpdateCharacter {
                                card_id,
                                name,
                                description,
                                system_prompt,
                                personality,
                                scenario,
                                greeting,
                                example_dialogue,
                            } => match runtime.update_character(app::CharacterDetail {
                                card_id,
                                name,
                                description,
                                system_prompt,
                                personality,
                                scenario,
                                greeting,
                                example_dialogue,
                            }) {
                                Ok(entry) => {
                                    app.status_line =
                                        Some(format!("Updated character: {}", entry.name));
                                    if let Ok(chars) = runtime.list_characters() {
                                        app.character_list.entries = chars;
                                    }
                                }
                                Err(e) => {
                                    app.status_line = Some(format!("Update failed: {:?}", e));
                                }
                            },
                            app::RuntimeCommand::ImportCharacter { path } => {
                                match runtime.import_character(path) {
                                    Ok(entry) => {
                                        app.status_line =
                                            Some(format!("Imported character: {}", entry.name));
                                        if let Ok(chars) = runtime.list_characters() {
                                            app.character_list.entries = chars;
                                        }
                                    }
                                    Err(e) => {
                                        app.status_line = Some(format!("Import failed: {:?}", e));
                                    }
                                }
                            }
                            app::RuntimeCommand::EditCharacter { card_id } => {
                                match runtime.get_character(&card_id) {
                                    Ok(Some(detail)) => {
                                        app.character_create.load_from_character(&detail);
                                        app.screen = app::ScreenState::CharacterEdit;
                                    }
                                    Ok(None) => {
                                        app.status_line = Some("Character not found".into());
                                    }
                                    Err(e) => {
                                        app.status_line = Some(format!("Edit failed: {:?}", e));
                                    }
                                }
                            }
                            app::RuntimeCommand::PrefChanged { pref_key, value } => {
                                let _ = runtime.save_pref(&pref_key, &value);
                                // Apply theme change immediately at runtime.
                                if pref_key == "theme_preset" {
                                    crate::theme::set_preset(
                                        crate::theme::ThemePreset::from_pref_str(&value),
                                    );
                                }
                            }
                            app::RuntimeCommand::SetSessionFolder { session_id, folder } => {
                                match runtime.set_session_folder(&session_id, folder.as_deref()) {
                                    Ok(()) => {
                                        let status = match folder.as_deref() {
                                            Some(name) => {
                                                format!("Moved session into folder: {name}")
                                            }
                                            None => "Removed session from folder".to_owned(),
                                        };
                                        app.status_line = Some(status);
                                    }
                                    Err(error) => {
                                        app.status_line =
                                            Some(format!("Folder update failed: {:?}", error));
                                    }
                                }
                                if let Ok(entries) = runtime.list_sessions() {
                                    app.session_list.entries = entries;
                                }
                            }
                            app::RuntimeCommand::OpenSession {
                                session_id,
                                session_name: _,
                            } => match runtime.open_session(&session_id) {
                                Ok(Some(session)) => {
                                    if let Ok(sid) =
                                        ozone_core::session::SessionId::parse(&session.session_id)
                                    {
                                        app.session.context =
                                            app::SessionContext::new(sid, session.session_name);
                                    }
                                    app.hydrate(session.bootstrap);
                                }
                                Ok(None) => {
                                    app.status_line =
                                        Some(format!("Session not found: {session_id}"));
                                }
                                Err(error) => {
                                    app.status_line =
                                        Some(format!("Failed to open session: {:?}", error));
                                }
                            },
                        }
                        sync_draft(runtime, app)?;
                    }
                }
                Event::Mouse(mouse) => {
                    let action = app.handle_mouse_event(mouse, &layout);
                    if action != KeyAction::Noop {
                        runtime
                            .dispatch(&app.session.context, action)
                            .map_err(RunSessionError::Runtime)?;
                        sync_draft(runtime, app)?;
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        } else if matches!(app.session.runtime, app::RuntimePhase::Generating { .. }) {
            // The runtime drives when generation finishes; poll on every quiet
            // tick so real streaming backends can deliver partial content and
            // final completions without a fixed artificial delay.
            match runtime
                .poll_generation(&app.session.context)
                .map_err(RunSessionError::Runtime)?
            {
                Some(GenerationPoll::Completed(completion)) => {
                    app.apply_runtime_completion(completion);
                    sync_draft(runtime, app)?;
                }
                Some(GenerationPoll::Failed(failure)) => {
                    app.apply_runtime_failure(failure);
                    sync_draft(runtime, app)?;
                }
                Some(GenerationPoll::Pending {
                    partial: Some(progress),
                }) => {
                    app.apply_runtime_progress(progress);
                }
                Some(GenerationPoll::Pending { partial: None }) | None => {}
            }
        }
    }
}

fn sync_draft<R>(runtime: &mut R, app: &ShellState) -> Result<(), RunSessionError<R::Error>>
where
    R: SessionRuntime,
{
    let checkpoint = app.persistable_draft();
    runtime
        .persist_draft(
            &app.session.context,
            checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.text.as_str()),
        )
        .map_err(RunSessionError::Runtime)
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;

    use super::{
        run_session, AppBootstrap, BranchItem, GenerationPoll, MockRuntime, RuntimeCompletion,
        RuntimeFailure, RuntimeProgress, RuntimeSendReceipt, RuntimeSessionLoad, SessionContext,
        SessionRuntime, ShellState, TranscriptItem,
    };
    use crate::app::RuntimePhase;

    fn session_context() -> SessionContext {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        SessionContext::new(session_id, "Phase 1C")
    }

    #[test]
    fn run_session_bootstraps_the_shell_from_the_runtime() {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let context = SessionContext::new(session_id, "Phase 1C");
        let bootstrap = AppBootstrap {
            transcript: vec![TranscriptItem::new("user", "hello skeleton")],
            branches: vec![BranchItem::new("main", "main", true)],
            status_line: Some("mock runtime ready".into()),
            draft: None,
            screen: None,
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            active_launch_plan: None,
        };
        let mut runtime = MockRuntime::with_bootstrap(bootstrap);

        let outcome = run_session(context.clone(), &mut runtime).unwrap();

        assert_eq!(outcome.app.session.context, context);
        assert_eq!(outcome.app.session.transcript.len(), 1);
        assert_eq!(outcome.render.title, "⬡ ozone+ — Phase 1C");
        assert_eq!(
            runtime.bootstrapped_sessions,
            vec!["123e4567-e89b-12d3-a456-426614174000".to_string()]
        );
    }

    // ── Runtime-driven flow tests ──────────────────────────────────────────

    /// A runtime stub that returns `Pending` with partial content for N polls
    /// before yielding `Completed`. This exercises the streaming path through
    /// `run_session` without touching the terminal loop.
    struct StreamingStubRuntime {
        pending_ticks: usize,
        ticks_seen: usize,
        request_id: String,
        final_content: String,
    }

    impl StreamingStubRuntime {
        fn new(pending_ticks: usize) -> Self {
            Self {
                pending_ticks,
                ticks_seen: 0,
                request_id: "stub-req-1".into(),
                final_content: "stub final reply".into(),
            }
        }
    }

    impl SessionRuntime for StreamingStubRuntime {
        type Error = String;

        fn bootstrap(&mut self, _context: &SessionContext) -> Result<AppBootstrap, Self::Error> {
            Ok(AppBootstrap::default())
        }

        fn send_draft(
            &mut self,
            _context: &SessionContext,
            _prompt: &str,
        ) -> Result<Option<RuntimeSendReceipt>, Self::Error> {
            Ok(Some(RuntimeSendReceipt {
                request_id: self.request_id.clone(),
                user_message: TranscriptItem::new("user", "test prompt"),
                context_preview: None,
                context_dry_run: None,
            }))
        }

        fn poll_generation(
            &mut self,
            _context: &SessionContext,
        ) -> Result<Option<GenerationPoll>, Self::Error> {
            self.ticks_seen += 1;
            if self.ticks_seen <= self.pending_ticks {
                let partial = format!("partial content after {} tick(s)", self.ticks_seen);
                Ok(Some(GenerationPoll::Pending {
                    partial: Some(RuntimeProgress {
                        request_id: self.request_id.clone(),
                        partial_content: partial.clone(),
                    }),
                }))
            } else {
                Ok(Some(GenerationPoll::Completed(RuntimeCompletion {
                    request_id: self.request_id.clone(),
                    assistant_message: TranscriptItem::new("assistant", self.final_content.clone()),
                    session_title: None,
                })))
            }
        }

        fn create_character(
            &mut self,
            _detail: crate::app::CharacterDetail,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn update_character(
            &mut self,
            _detail: crate::app::CharacterDetail,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn get_character(
            &mut self,
            _card_id: &str,
        ) -> Result<Option<crate::app::CharacterDetail>, Self::Error> {
            Ok(None)
        }

        fn import_character(
            &mut self,
            _path: String,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn create_session(
            &mut self,
            _character_name: Option<&str>,
        ) -> Result<RuntimeSessionLoad, Self::Error> {
            Err("not implemented in stub".into())
        }
    }

    /// A runtime stub that always returns `Failed` on the first poll.
    struct FailingStubRuntime;

    impl SessionRuntime for FailingStubRuntime {
        type Error = String;

        fn bootstrap(&mut self, _context: &SessionContext) -> Result<AppBootstrap, Self::Error> {
            Ok(AppBootstrap::default())
        }

        fn send_draft(
            &mut self,
            _context: &SessionContext,
            _prompt: &str,
        ) -> Result<Option<RuntimeSendReceipt>, Self::Error> {
            Ok(Some(RuntimeSendReceipt {
                request_id: "fail-req-1".into(),
                user_message: TranscriptItem::new("user", "this will fail"),
                context_preview: None,
                context_dry_run: None,
            }))
        }

        fn poll_generation(
            &mut self,
            _context: &SessionContext,
        ) -> Result<Option<GenerationPoll>, Self::Error> {
            Ok(Some(GenerationPoll::Failed(RuntimeFailure {
                request_id: "fail-req-1".into(),
                message: "backend unavailable".into(),
            })))
        }

        fn create_character(
            &mut self,
            _detail: crate::app::CharacterDetail,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn update_character(
            &mut self,
            _detail: crate::app::CharacterDetail,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn get_character(
            &mut self,
            _card_id: &str,
        ) -> Result<Option<crate::app::CharacterDetail>, Self::Error> {
            Ok(None)
        }

        fn import_character(
            &mut self,
            _path: String,
        ) -> Result<crate::app::CharacterEntry, Self::Error> {
            Err("not implemented in stub".into())
        }

        fn create_session(
            &mut self,
            _character_name: Option<&str>,
        ) -> Result<RuntimeSessionLoad, Self::Error> {
            Err("not implemented in stub".into())
        }
    }

    #[test]
    fn shell_state_progresses_through_streaming_then_completes() {
        let context = session_context();
        let mut runtime = StreamingStubRuntime::new(2);
        let mut app = ShellState::new(context.clone());
        app.hydrate(runtime.bootstrap(&context).unwrap());

        // Simulate send
        let receipt = runtime
            .send_draft(&context, "test prompt")
            .unwrap()
            .unwrap();
        app.apply_send_receipt(receipt);
        assert!(matches!(
            app.session.runtime,
            RuntimePhase::Generating { .. }
        ));
        assert!(app.session.runtime.partial_content().is_none());

        // First poll → Pending with partial
        let poll1 = runtime.poll_generation(&context).unwrap().unwrap();
        match poll1 {
            GenerationPoll::Pending {
                partial: Some(ref p),
            } => {
                app.apply_runtime_progress(p.clone());
            }
            other => panic!("expected Pending, got {other:?}"),
        }
        assert_eq!(
            app.session.runtime.partial_content(),
            Some("partial content after 1 tick(s)")
        );

        // Second poll → Pending again with updated partial
        let poll2 = runtime.poll_generation(&context).unwrap().unwrap();
        match poll2 {
            GenerationPoll::Pending {
                partial: Some(ref p),
            } => {
                app.apply_runtime_progress(p.clone());
            }
            other => panic!("expected Pending, got {other:?}"),
        }
        assert_eq!(
            app.session.runtime.partial_content(),
            Some("partial content after 2 tick(s)")
        );

        // Third poll → Completed
        let poll3 = runtime.poll_generation(&context).unwrap().unwrap();
        match poll3 {
            GenerationPoll::Completed(completion) => {
                app.apply_runtime_completion(completion);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
        assert!(matches!(app.session.runtime, RuntimePhase::Idle));
        assert_eq!(
            app.session
                .transcript
                .last()
                .map(|item| item.content.as_str()),
            Some("stub final reply")
        );
        assert_eq!(app.status_line.as_deref(), Some("Generation completed"));
    }

    #[test]
    fn shell_state_handles_generation_failure() {
        let context = session_context();
        let mut runtime = FailingStubRuntime;
        let mut app = ShellState::new(context.clone());
        app.hydrate(runtime.bootstrap(&context).unwrap());

        let receipt = runtime
            .send_draft(&context, "this will fail")
            .unwrap()
            .unwrap();
        app.apply_send_receipt(receipt);

        let poll = runtime.poll_generation(&context).unwrap().unwrap();
        match poll {
            GenerationPoll::Failed(failure) => {
                app.apply_runtime_failure(failure);
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        assert!(matches!(app.session.runtime, RuntimePhase::Failed { .. }));
        assert!(!app.session.runtime.is_inflight());
        assert_eq!(
            app.status_line.as_deref(),
            Some("Generation failed: backend unavailable")
        );
    }

    #[test]
    fn mock_runtime_completes_on_first_poll_via_poll_generation() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        runtime.send_draft(&context, "quick poll test").unwrap();
        let poll = runtime.poll_generation(&context).unwrap().unwrap();

        assert!(matches!(poll, GenerationPoll::Completed(_)));
        assert!(runtime.active_generation.is_none());
        assert_eq!(runtime.polled_requests, vec!["mock-request-1"]);
        assert_eq!(runtime.completed_requests, vec!["mock-request-1"]);
    }
}
