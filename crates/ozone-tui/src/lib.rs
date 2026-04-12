pub mod app;
pub mod input;
pub mod layout;
pub mod mock;
pub mod render;

use std::{
    error::Error,
    fmt, io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub use app::{
    AppBootstrap, BranchItem, DraftState, FocusTarget, RuntimeCancellation, RuntimeCompletion,
    RuntimeSendReceipt, ScreenState, SessionContext, SessionState, ShellState, TranscriptItem,
};
pub use input::{dispatch_key, InputMode, KeyAction};
pub use layout::{
    build_layout, build_layout_for_area, LayoutMode, LayoutModel, PaneId, PaneLayout,
};
pub use mock::{MockRuntime, SessionRuntime};
pub use render::{build_render_model, render_shell, RenderModel};

#[derive(Debug, Clone, PartialEq)]
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
    const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(50);
    const MOCK_COMPLETION_DELAY: Duration = Duration::from_millis(250);

    let bootstrap = runtime
        .bootstrap(&context)
        .map_err(RunSessionError::Bootstrap)?;
    let mut app = ShellState::new(context);
    app.hydrate(bootstrap);

    let mut terminal = TerminalGuard::enter().map_err(RunSessionError::Io)?;
    let mut completion_deadline = None;

    loop {
        let (layout, render) = {
            let mut drawn_layout = None;
            let mut drawn_render = None;
            terminal
                .terminal
                .draw(|frame| {
                    let layout = build_layout_for_area(&app, frame.area());
                    let render = build_render_model(&app, &layout);
                    render_shell(frame, &layout, &render);
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
            sync_draft(runtime, &app)?;
            return Ok(RunSessionOutcome { app, layout, render });
        }

        if event::poll(INPUT_POLL_INTERVAL).map_err(RunSessionError::Io)? {
            match event::read().map_err(RunSessionError::Io)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let action = app.handle_key_event(key);
                    if action != KeyAction::Noop {
                        runtime
                            .dispatch(&app.session.context, action)
                            .map_err(RunSessionError::Runtime)?;
                        sync_draft(runtime, &app)?;

                        for command in app.take_runtime_commands() {
                            match command {
                                app::RuntimeCommand::SendDraft { prompt } => {
                                    if let Some(receipt) = runtime
                                        .send_draft(&app.session.context, &prompt)
                                        .map_err(RunSessionError::Runtime)?
                                    {
                                        app.apply_send_receipt(receipt);
                                        completion_deadline =
                                            Some(Instant::now() + MOCK_COMPLETION_DELAY);
                                    }
                                }
                                app::RuntimeCommand::CancelGeneration => {
                                    if let Some(cancellation) = runtime
                                        .cancel_generation(&app.session.context)
                                        .map_err(RunSessionError::Runtime)?
                                    {
                                        app.apply_runtime_cancellation(cancellation);
                                    }
                                    completion_deadline = None;
                                }
                            }
                            sync_draft(runtime, &app)?;
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        } else if completion_deadline.is_some()
            && matches!(app.session.runtime, app::RuntimePhase::Generating { .. })
            && Instant::now() >= completion_deadline.expect("deadline checked above")
        {
            if let Some(completion) = runtime
                .complete_generation(&app.session.context)
                .map_err(RunSessionError::Runtime)?
            {
                app.apply_runtime_completion(completion);
            }
            completion_deadline = None;
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
            checkpoint.as_ref().map(|checkpoint| checkpoint.text.as_str()),
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
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;

    use super::{
        run_session, AppBootstrap, BranchItem, MockRuntime, SessionContext, TranscriptItem,
    };

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
        };
        let mut runtime = MockRuntime::with_bootstrap(bootstrap);

        let outcome = run_session(context.clone(), &mut runtime).unwrap();

        assert_eq!(outcome.app.session.context, context);
        assert_eq!(outcome.app.session.transcript.len(), 1);
        assert_eq!(outcome.render.title, "ozone+ — Phase 1C");
        assert_eq!(
            runtime.bootstrapped_sessions,
            vec!["123e4567-e89b-12d3-a456-426614174000".to_string()]
        );
    }
}
