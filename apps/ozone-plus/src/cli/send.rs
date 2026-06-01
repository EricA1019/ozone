use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};
use crate::runtime::OzonePlusRuntime;
use ozone_tui::mock::SessionRuntime;
use ozone_tui::GenerationPoll;
use ozone_tui::state::SessionContext as TuiSessionContext;
use std::time::Duration;

pub fn send_message(args: SendArgs) -> Result<(), String> {
    if !args.author_kind.eq_ignore_ascii_case("user") || args.author_name.is_some() {
        return send_message_legacy(args);
    }

    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let session = repo
        .get_session(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {session_id} was not found"))?;
    let mut runtime = OzonePlusRuntime::open(repo.clone(), session_id.clone())?;
    let context = TuiSessionContext::new(session_id.clone(), session.name);

    let send_result = (|| -> Result<(), String> {
        runtime.check_backend_health()?;

        runtime
            .send_draft(&context, &args.content)?
            .ok_or_else(|| "message content must not be empty".to_string())?;

        loop {
            match runtime.poll_generation(&context)? {
                Some(GenerationPoll::Completed(_)) => {
                    let transcript = repo
                        .get_active_branch_transcript(&session_id)
                        .map_err(|error| error.to_string())?;
                    println!("Completed runtime-backed turn.");
                    let start = transcript.len().saturating_sub(2);
                    crate::cli::print::print_transcript(&transcript[start..]);
                    return Ok(());
                }
                Some(GenerationPoll::Failed(failure)) => {
                    return Err(failure.message);
                }
                Some(GenerationPoll::Pending { .. }) | None => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    })();

    let release_result = runtime.release_lock();
    match (send_result, release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(session_error), Err(release_error)) => Err(format!(
            "{session_error}; also failed to release session lock cleanly: {release_error}"
        )),
    }
}

pub fn send_message_legacy(args: SendArgs) -> Result<(), String> {
    let mut engine = crate::cli::util::open_cli_engine()?;
    let (message, bootstrapped) = engine.send(args)?;

    println!("Committed engine-backed message without generation.");
    crate::cli::print::print_message(&message);
    if bootstrapped {
        println!();
        println!("Bootstrap note");
        println!("  This was the first transcript message, so the engine created the initial active branch.");
    }

    Ok(())
}