use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id, parse_message_id, parse_memory_artifact_id};
use ozone_persist::{
    AuthorId, PinMessageMemoryRequest, Provenance,
};

pub fn handle_memory_command(command: MemoryCommand) -> Result<(), String> {
    match command {
        MemoryCommand::Pin(args) => pin_memory(args),
        MemoryCommand::Note(args) => create_note_memory(args),
        MemoryCommand::List(args) => list_memories(args),
        MemoryCommand::Unpin(args) => unpin_memory(args),
    }
}

pub fn pin_memory(args: MemoryPinArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let message_id = parse_message_id(&args.message_id)?;
    let memory = repo
        .pin_message_memory(
            &session_id,
            &message_id,
            PinMessageMemoryRequest {
                pinned_by: AuthorId::User,
                expires_after_turns: args.expires_after_turns,
                provenance: Provenance::UserAuthored,
            },
        )
        .map_err(|error| error.to_string())?
        .into_view(
            repo.get_session(&session_id)
                .map_err(|error| error.to_string())?
                .map(|session| session.message_count)
                .ok_or_else(|| format!("session {session_id} was not found"))?,
        );

    println!("Pinned memory.");
    crate::cli::print::print_pinned_memory_view(&memory);
    Ok(())
}

pub fn create_note_memory(args: MemoryNoteArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let mut request = crate::cli::util::create_note_memory_request(
        crate::cli::util::require_non_empty("note text", args.text)?,
        AuthorId::User,
        Provenance::UserAuthored,
    );
    request.content.expires_after_turns = args.expires_after_turns;
    let memory = repo
        .create_note_memory(&session_id, request)
        .map_err(|error| error.to_string())?
        .into_view(
            repo.get_session(&session_id)
                .map_err(|error| error.to_string())?
                .map(|session| session.message_count)
                .ok_or_else(|| format!("session {session_id} was not found"))?,
        );

    println!("Created note memory.");
    crate::cli::print::print_pinned_memory_view(&memory);
    Ok(())
}

pub fn list_memories(args: SessionArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let memories = repo
        .list_saved_memories(&session_id)
        .map_err(|error| error.to_string())?;

    let pinned_count = memories
        .iter()
        .filter(|memory| memory.record.source_message_id.is_some())
        .count();
    let note_count = memories.len().saturating_sub(pinned_count);

    println!("Saved memories");
    println!("  session id      {}", session_id);
    println!("  pinned          {}", pinned_count);
    println!("  notes           {}", note_count);
    println!(
        "  active          {}",
        memories.iter().filter(|memory| memory.is_active).count()
    );
    println!(
        "  expired         {}",
        memories.iter().filter(|memory| memory.is_expired()).count()
    );

    if memories.is_empty() {
        println!("  none");
        return Ok(());
    }

    for memory in &memories {
        println!();
        crate::cli::print::print_pinned_memory_view(memory);
    }

    Ok(())
}

pub fn unpin_memory(args: MemoryUnpinArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let artifact_id = parse_memory_artifact_id(&args.artifact_id)?;
    let removed = repo
        .remove_saved_memory(&session_id, &artifact_id)
        .map_err(|error| error.to_string())?;

    if !removed {
        return Err(format!(
            "saved memory {} was not found in session {}",
            artifact_id, session_id
        ));
    }

    println!("Removed saved memory.");
    println!("  session id      {}", session_id);
    println!("  artifact id     {}", artifact_id);
    Ok(())
}