use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};
use crate::hybrid_search::HybridSearchService;
use ozone_memory::RetrievalResultSet;

pub fn handle_search_command(command: SearchCommand) -> Result<(), String> {
    match command {
        SearchCommand::Session(args) => search_session(args),
        SearchCommand::Global(args) => search_global(args),
    }
}

pub fn search_session(args: SessionSearchArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let query = crate::cli::util::require_non_empty("query", args.query)?;
    let memory = crate::cli::util::load_memory_config(&repo, Some(&session_id))?;
    let result = HybridSearchService::new(&repo, &memory).search_session(&session_id, &query)?;

    println!(
        "{}",
        crate::cli::print::format_search_report("Session search", Some(&session_id), &result, false)
    );

    Ok(())
}

pub fn search_global(args: GlobalSearchArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let query = crate::cli::util::require_non_empty("query", args.query)?;
    let memory = crate::cli::util::load_memory_config(&repo, None)?;
    let result = HybridSearchService::new(&repo, &memory).search_global(&query)?;

    println!(
        "{}",
        crate::cli::print::format_search_report("Global search", None, &result, true)
    );

    Ok(())
}

pub fn format_search_result(result: &RetrievalResultSet, include_session_details: bool) -> String {
    crate::cli::print::format_search_report("", None, result, include_session_details)
}