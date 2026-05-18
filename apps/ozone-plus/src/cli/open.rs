use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};
use crate::runtime::Phase1dRuntime;
use ozone_persist::{
    CreateSessionRequest, PersistError, SessionId, SessionSummary,
    SqliteRepository, UpdateSessionRequest,
};
use ozone_tui::run_terminal_session;
use ozone_tui::state::SessionContext as TuiSessionContext;

pub fn open_session(args: OpenArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;

    if args.force {
        eprintln!("Force-clearing session lock for {session_id}...");
        repo.force_clear_session_lock(&session_id)
            .map_err(|error| error.to_string())?;
    }

    let session = repo
        .get_session(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {session_id} was not found"))?;

    if args.metadata {
        return open_session_metadata(repo, &session, &session_id);
    }

    open_session_record(repo, session)
}

pub fn handoff_session(args: HandoffArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let candidates = handoff_candidates(&repo, args)?;
    let mut last_lock_error = None;

    for session in candidates {
        match open_session_record(repo.clone(), session) {
            Ok(()) => return Ok(()),
            Err(error) if is_session_locked_error(&error) => {
                last_lock_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    let fallback = create_handoff_session(&repo)?;
    match open_session_record(repo, fallback) {
        Ok(()) => Ok(()),
        Err(error) => match last_lock_error {
            Some(lock_error) if is_session_locked_error(&error) => Err(format!(
                "{lock_error}; also could not open a fresh launcher session: {error}"
            )),
            _ => Err(error),
        },
    }
}

pub fn handoff_candidates(
    repo: &SqliteRepository,
    args: HandoffArgs,
) -> Result<Vec<SessionSummary>, String> {
    if args.launcher_session {
        if let Some(session) = repo
            .list_sessions()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|session| session.name == LAUNCHER_SESSION_NAME)
        {
            return Ok(vec![session]);
        }
        return Ok(vec![create_handoff_session(repo)?]);
    }

    let sessions = repo.list_sessions().map_err(|error| error.to_string())?;
    if !sessions.is_empty() {
        return Ok(sessions);
    }

    Ok(vec![create_handoff_session(repo)?])
}

pub fn create_handoff_session(repo: &SqliteRepository) -> Result<SessionSummary, String> {
    repo.create_session(CreateSessionRequest::new(LAUNCHER_SESSION_NAME))
        .map_err(|error| error.to_string())
}

pub fn open_session_record(repo: SqliteRepository, session: SessionSummary) -> Result<(), String> {
    run_session_shell(repo, session.session_id, session.name)
}

pub fn run_session_shell(
    repo: SqliteRepository,
    session_id: SessionId,
    session_name: String,
) -> Result<(), String> {
    // Initialise the TUI theme from the shared preferences file.
    ozone_tui::theme::set_preset(load_prefs());

    let mut runtime = Phase1dRuntime::open(repo.clone(), session_id.clone())?;
    if let Err(error) = repo
        .update_session_metadata(&session_id, UpdateSessionRequest::default())
        .map_err(|error| error.to_string())
    {
        let release_result = runtime.release_lock();
        return match release_result {
            Ok(()) => Err(error),
            Err(release_error) => Err(format!(
                "{error}; also failed to release session lock cleanly: {release_error}"
            )),
        };
    }

    let context = TuiSessionContext::new(session_id.clone(), session_name);
    let session_result =
        run_terminal_session(context, &mut runtime).map_err(|error| error.to_string());
    let release_result = runtime.release_lock();

    match (session_result, release_result) {
        (Ok(_), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(session_error), Err(release_error)) => Err(format!(
            "{session_error}; also failed to release session lock cleanly: {release_error}"
        )),
    }
}

pub fn is_session_locked_error(error: &str) -> bool {
    error.contains("is locked by instance")
}

pub fn open_session_metadata(
    repo: SqliteRepository,
    session: &SessionSummary,
    session_id: &SessionId,
) -> Result<(), String> {
    let instance_id = format!("ozone-plus-phase1b-{}", std::process::id());
    let lock = repo
        .acquire_session_lock(session_id, &instance_id)
        .map_err(|error| match error {
            PersistError::SessionLocked {
                instance_id,
                acquired_at,
            } => format!(
                "session {session_id} is locked by instance {instance_id} (since {})",
                crate::cli::print::format_timestamp(acquired_at)
            ),
            other => other.to_string(),
        })?;

    if !repo
        .release_session_lock(session_id, &lock.instance_id)
        .map_err(|error| error.to_string())?
    {
        return Err(format!(
            "session {session_id} lock was acquired but could not be released cleanly"
        ));
    }

    println!("Resolved persisted ozone+ session.");
    crate::cli::print::print_session_details(session);
    println!();
    if let Some(active_branch) = repo
        .get_active_branch(session_id)
        .map_err(|error| error.to_string())?
    {
        println!("Active branch");
        crate::cli::print::print_branch_record(&active_branch, true);
        let transcript = repo
            .get_active_branch_transcript(session_id)
            .map_err(|error| error.to_string())?;
        println!("  transcript messages  {}", transcript.len());
    } else {
        println!("Active branch");
        println!("  none yet — send the first message to bootstrap the conversation");
    }
    println!();
    println!("Session open check");
    println!("  advisory lock instance   {}", lock.instance_id);
    println!(
        "  acquired at              {}",
        crate::cli::print::format_timestamp(lock.acquired_at)
    );
    println!(
        "  heartbeat at             {}",
        crate::cli::print::format_timestamp(lock.heartbeat_at)
    );
    println!("  lock release             ok");
    println!();
    println!("Paths");
    crate::cli::print::print_session_paths(repo.paths(), &session.session_id);

    Ok(())
}

fn load_prefs() -> ozone_tui::ThemePreset {
    crate::cli::prefs::load_theme_preset()
}