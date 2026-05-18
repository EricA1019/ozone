use crate::cli::args::*;
use crate::cli::util::open_repository;
use crate::index_rebuild::rebuild_index;

pub fn handle_index_command(command: IndexCommand) -> Result<(), String> {
    match command {
        IndexCommand::Rebuild => rebuild_vector_index(),
    }
}

pub fn rebuild_vector_index() -> Result<(), String> {
    let repo = open_repository()?;
    let result = rebuild_index(&repo)?;

    println!("Vector index rebuilt.");
    println!("  sessions        {}", result.session_count);
    println!("  sources         {}", result.source_count());
    println!("  message sources {}", result.message_source_count);
    println!("  memory sources  {}", result.memory_source_count);
    println!("  artifacts       {}", result.persisted_artifact_count);
    println!("  provider        {}", result.provider.provider);
    println!("  model           {}", result.provider.model);
    println!("  dimensions      {}", result.provider.dimensions);
    println!("  index path      {}", result.index_path().display());
    println!("  metadata path   {}", result.metadata_path().display());
    Ok(())
}