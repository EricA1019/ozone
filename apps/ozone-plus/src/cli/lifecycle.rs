use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id, load_memory_config};
use ozone_memory::{StorageTierPolicy, check_disk_space, DiskMonitorPolicy};

pub fn handle_lifecycle_command(command: LifecycleCommand) -> Result<(), String> {
    match command {
        LifecycleCommand::Inspect { session_id } => lifecycle_inspect(session_id),
        LifecycleCommand::DiskStatus => lifecycle_disk_status(),
    }
}

pub fn lifecycle_inspect(session_id_raw: Option<String>) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let config = load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let policy = StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let records = repo
        .inspect_derived_artifacts(
            session_id.as_ref(),
            &policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
        )
        .map_err(|error| error.to_string())?;

    if records.is_empty() {
        println!("No derived artifacts found.");
        return Ok(());
    }

    println!("Derived artifacts  ({} total)", records.len());
    println!();
    for record in &records {
        println!(
            "  {}  {}  {}",
            record.artifact_id, record.kind, record.session_id
        );
        println!("    tier       {}", record.storage_tier);
        println!(
            "    stale      {}",
            if record.staleness.is_stale {
                "yes ⚠"
            } else {
                "no"
            }
        );
        println!(
            "    age        {} messages  {} hours",
            record.age_messages, record.staleness.age_hours
        );
        println!(
            "    source     {}",
            if record.source_exists {
                "present"
            } else {
                "missing ⚠"
            }
        );
        println!("    created    {}", record.created_at);
        println!();
    }
    Ok(())
}

pub fn lifecycle_disk_status() -> Result<(), String> {
    let repo = open_repository()?;
    let data_dir = repo.paths().data_dir();
    let policy = DiskMonitorPolicy::default();
    match check_disk_space(data_dir, &policy) {
        Some(result) => {
            println!("Disk status");
            println!("  path            {}", data_dir.display());
            println!(
                "  free            {} MiB",
                result.free_bytes / (1024 * 1024)
            );
            println!("  status          {}", result.status);
            if result.status.should_pause_background_jobs() {
                println!("  ⚠ emergency: background artifact jobs should be paused");
            }
        }
        None => println!("Disk space check not available on this platform."),
    }
    Ok(())
}