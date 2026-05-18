//! ozone+ — local-LLM chat shell with persistent memory and sessions
//!
//! Library crate providing all CLI logic. Binary entry point is in `main.rs`.

use clap::Parser;

pub mod cli;
pub mod config;
pub mod context_bridge;
pub mod hooks;
pub mod hybrid_search;
pub mod index_rebuild;
pub mod inference_adapter;
pub mod runtime;
pub mod session_title;
pub mod store;

// Re-export commonly used util functions for crate-internal access
pub use cli::util::{
    format_message_time, format_timestamp, format_timestamp_short, generate_branch_id,
    generate_message_id, generate_request_id, generate_swipe_group_id, generate_uuid_like,
    now_timestamp_ms, optional_value, parse_branch_id, parse_message_id, parse_session_id,
    parse_swipe_group_id, require_non_empty, require_existing_file, read_utf8_file,
    open_repository, open_cli_engine, normalize_tags, format_tags,
    map_branch_record, conversation_message_from_record, load_memory_config,
    create_note_memory_request,
};

use cli::args::{Command, Cli};
use ozone_core::install;

pub fn run() -> Result<(), String> {
    if install::maybe_prompt_for_local_install_update("ozone-plus")
        .map_err(|error| error.to_string())?
    {
        install::relaunch_current_process().map_err(|error| error.to_string())?;
    }

    run_cli(Cli::parse())
}

pub fn run_cli(cli: Cli) -> Result<(), String> {
    match cli.command {
        Some(Command::Identity) => {
            cli::identity::print_identity();
            Ok(())
        }
        Some(Command::Docs) => {
            cli::identity::print_docs();
            Ok(())
        }
        Some(Command::Paths) => {
            cli::identity::print_paths();
            Ok(())
        }
        Some(Command::Create(args)) => cli::create::create_session(args),
        Some(Command::List) => cli::create::list_sessions(),
        Some(Command::Handoff(args)) => cli::open::handoff_session(args),
        Some(Command::Open(args)) => cli::open::open_session(args),
        Some(Command::Send(args)) => cli::send::send_message(args),
        Some(Command::Transcript(args)) => cli::transcript::show_transcript(args),
        Some(Command::Edit(args)) => cli::edit::edit_message(args),
        Some(Command::Branch(args)) => cli::branch::handle_branch_command(args.command),
        Some(Command::Swipe(args)) => cli::swipe::handle_swipe_command(args.command),
        Some(Command::Import(args)) => cli::import::handle_import_command(args.command),
        Some(Command::Export(args)) => cli::export::handle_export_command(args.command),
        Some(Command::Memory(args)) => cli::memory::handle_memory_command(args.command),
        Some(Command::Search(args)) => cli::search::handle_search_command(args.command),
        Some(Command::Index(args)) => cli::index::handle_index_command(args.command),
        Some(Command::Summarize(args)) => cli::summarize::handle_summarize_command(args.command),
        Some(Command::Lifecycle(args)) => cli::lifecycle::handle_lifecycle_command(args.command),
        Some(Command::Gc(args)) => cli::gc::handle_gc_command(args.command),
        Some(Command::Events(args)) => cli::events::handle_events_command(args.command),
        None => {
            cli::identity::print_bootstrap_summary();
            Ok(())
        }
    }
}