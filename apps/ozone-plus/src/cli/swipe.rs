use crate::cli::args::*;
use crate::cli::util::parse_session_id;
use ozone_engine::ConversationStore;

pub fn handle_swipe_command(command: SwipeCommand) -> Result<(), String> {
    match command {
        SwipeCommand::List(args) => list_swipes(args),
        SwipeCommand::Add(args) => add_swipe_candidate(args),
        SwipeCommand::Activate(args) => activate_swipe(args),
    }
}

pub fn list_swipes(args: SessionArgs) -> Result<(), String> {
    let engine = crate::cli::util::open_cli_engine()?;
    let session_id = parse_session_id(&args.session_id)?;
    let swipe_groups = engine.list_swipes(&session_id)?;

    println!("Swipe groups");
    if swipe_groups.is_empty() {
        println!("  none yet");
        return Ok(());
    }
    for snapshot in swipe_groups {
        crate::cli::print::print_swipe_group_snapshot(&snapshot);
        println!();
    }
    Ok(())
}

pub fn add_swipe_candidate(args: SwipeAddArgs) -> Result<(), String> {
    let mut engine = crate::cli::util::open_cli_engine()?;
    let (group, candidate) = engine.add_swipe_candidate(args)?;

    println!("Added swipe candidate.");
    println!("  group id         {}", group.swipe_group_id);
    println!("  parent message   {}", group.parent_message_id);
    println!("  active ordinal   {}", group.active_ordinal);
    println!("  candidate ord    {}", candidate.ordinal);
    println!("  candidate id     {}", candidate.message_id);
    println!("  state            {}", candidate.state);
    Ok(())
}

pub fn activate_swipe(args: SwipeActivateArgs) -> Result<(), String> {
    let session_id = parse_session_id(&args.session_id)?;
    let mut engine = crate::cli::util::open_cli_engine()?;
    let group = engine.activate_swipe(args)?;
    let transcript = engine
        .engine
        .store()
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    println!("Activated swipe candidate.");
    println!("  group id         {}", group.swipe_group_id);
    println!("  active ordinal   {}", group.active_ordinal);
    println!();
    println!("Updated active transcript");
    crate::cli::print::print_transcript(&transcript);
    Ok(())
}