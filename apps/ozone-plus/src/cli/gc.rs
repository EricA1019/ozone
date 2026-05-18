use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};
use ozone_persist::{
    GarbageCollectionPolicy, SessionId,
};
use ozone_memory::StorageTierPolicy;

pub fn handle_gc_command(command: GcCommand) -> Result<(), String> {
    match command {
        GcCommand::Plan {
            session_id,
            max_embeddings,
            purge_orphans,
        } => gc_plan(session_id, max_embeddings, purge_orphans),
        GcCommand::Run {
            session_id,
            max_embeddings,
            purge_orphans,
            apply,
        } => gc_run(session_id, max_embeddings, purge_orphans, apply),
    }
}

pub fn build_gc_policy_and_session(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
) -> Result<(Option<SessionId>, GarbageCollectionPolicy), String> {
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let policy = GarbageCollectionPolicy::new(max_embeddings, purge_orphans);
    Ok((session_id, policy))
}

pub fn gc_plan(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
) -> Result<(), String> {
    let (session_id, policy) =
        build_gc_policy_and_session(session_id_raw, max_embeddings, purge_orphans)?;
    let repo = open_repository()?;
    let config = crate::cli::util::load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let storage_policy = StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let plan = repo
        .plan_garbage_collection(
            session_id.as_ref(),
            &storage_policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
            &policy,
        )
        .map_err(|error| error.to_string())?;
    crate::cli::print::print_gc_plan(&plan);
    Ok(())
}

pub fn gc_run(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
    apply: bool,
) -> Result<(), String> {
    let (session_id, policy) =
        build_gc_policy_and_session(session_id_raw, max_embeddings, purge_orphans)?;
    let repo = open_repository()?;
    let config = crate::cli::util::load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let storage_policy = StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let plan = repo
        .plan_garbage_collection(
            session_id.as_ref(),
            &storage_policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
            &policy,
        )
        .map_err(|error| error.to_string())?;

    crate::cli::print::print_gc_plan(&plan);

    if !apply {
        println!();
        println!("Dry-run mode — no artifacts deleted. Pass --apply to commit.");
        return Ok(());
    }

    if plan.candidate_count == 0 {
        println!();
        println!("Nothing to delete.");
        return Ok(());
    }

    let outcome = repo
        .apply_garbage_collection_plan(&plan)
        .map_err(|error| error.to_string())?;
    crate::cli::print::print_gc_outcome(&outcome);
    Ok(())
}