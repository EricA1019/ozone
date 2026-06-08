use crate::cli::args::*;
use crate::cli::util::parse_session_id;

pub fn handle_branch_command(command: BranchCommand) -> Result<(), String> {
    match command {
        BranchCommand::List(args) => list_branches(args),
        BranchCommand::Create(args) => create_branch(args),
        BranchCommand::Activate(args) => activate_branch(args),
    }
}

pub fn list_branches(args: SessionArgs) -> Result<(), String> {
    let engine = crate::cli::util::open_cli_engine()?;
    let session_id = parse_session_id(&args.session_id)?;
    let branches = engine.list_branches(&session_id)?;

    println!("Branches");
    if branches.is_empty() {
        println!("  none yet — send the first message to bootstrap the active branch");
        return Ok(());
    }
    for branch in branches {
        crate::cli::print::print_branch_record_from_engine(&branch, true);
        println!();
    }
    Ok(())
}

pub fn create_branch(args: BranchCreateArgs) -> Result<(), String> {
    let mut engine = crate::cli::util::open_cli_engine()?;
    let branch = engine.create_branch(args)?;

    println!("Created branch.");
    crate::cli::print::print_branch_record_from_engine(&branch, true);
    Ok(())
}

pub fn activate_branch(args: BranchActivateArgs) -> Result<(), String> {
    let mut engine = crate::cli::util::open_cli_engine()?;
    let branch = engine.activate_branch(args)?;

    println!("Activated branch.");
    crate::cli::print::print_branch_record_from_engine(&branch, true);
    Ok(())
}