use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};

pub fn handle_events_command(command: EventsCommand) -> Result<(), String> {
    match command {
        EventsCommand::Compact {
            session_id,
            retention_days,
        } => events_compact(session_id, retention_days),
    }
}

pub fn events_compact(session_id_raw: Option<String>, retention_days: u64) -> Result<(), String> {
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let now_ms = u64::try_from(crate::cli::util::now_timestamp_ms()).unwrap_or(0);
    let older_than_ms = now_ms.saturating_sub(retention_days * 24 * 3600 * 1000);
    let repo = open_repository()?;
    let count = repo
        .compact_events(session_id.as_ref(), older_than_ms)
        .map_err(|e| e.to_string())?;
    println!("Events compacted");
    println!("  deleted  {count}");
    println!("  older than  {retention_days} days");
    Ok(())
}