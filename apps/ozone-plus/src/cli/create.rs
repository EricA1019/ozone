use crate::cli::args::*;
use crate::cli::util::{open_repository, require_non_empty};
use ozone_persist::CreateSessionRequest;

pub fn create_session(args: CreateArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let mut request = CreateSessionRequest::new(require_non_empty("session name", args.name)?);
    request.character_name = crate::cli::util::optional_value(args.character_name);
    request.tags = crate::cli::util::normalize_tags(args.tags);

    let session = repo
        .create_session(request)
        .map_err(|error| error.to_string())?;

    println!("Created persisted ozone+ session.");
    crate::cli::print::print_session_details(&session);
    println!();
    println!("Paths");
    crate::cli::print::print_session_paths(repo.paths(), &session.session_id);
    println!();
    println!("Next step");
    println!(
        "  Send the first message with `ozone-plus send {}`.",
        session.session_id
    );

    Ok(())
}

pub fn list_sessions() -> Result<(), String> {
    let repo = open_repository()?;
    let sessions = repo.list_sessions().map_err(|error| error.to_string())?;

    println!("Persisted ozone+ sessions");
    crate::cli::print::print_resolved_path("data dir", repo.paths().data_dir());
    crate::cli::print::print_resolved_path("global db", repo.paths().global_db_path());
    println!();

    if sessions.is_empty() {
        println!("No persisted sessions found yet.");
        println!("Create one with `ozone-plus create \"First Session\"`.");
        return Ok(());
    }

    for (index, session) in sessions.iter().enumerate() {
        if index != 0 {
            println!();
        }
        crate::cli::print::print_session_details(session);
    }

    println!();
    println!("Tip");
    println!("  Use `ozone-plus send <session-id> \"Hello\"` to bootstrap the active transcript.");

    Ok(())
}