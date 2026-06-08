use crate::cli::args::*;
use crate::cli::util::{open_repository, require_existing_file, read_utf8_file, optional_value, normalize_tags};
use ozone_persist::{CharacterCard, ImportCharacterCardRequest};

pub fn handle_import_command(command: ImportCommand) -> Result<(), String> {
    match command {
        ImportCommand::Card(args) => import_character_card(args),
    }
}

pub fn import_character_card(args: ImportCharacterCardArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let input_path = require_existing_file(&args.input, "character card JSON")?;
    let contents = read_utf8_file(&input_path, "character card JSON")?;
    let card = CharacterCard::from_json_str(&contents).map_err(|error| error.to_string())?;
    let imported = repo
        .import_character_card(ImportCharacterCardRequest {
            card: card.clone(),
            session_name: optional_value(args.session_name),
            tags: normalize_tags(args.tags),
            provenance: input_path.display().to_string(),
        })
        .map_err(|error| error.to_string())?;

    println!("Imported character card.");
    println!("  card name       {}", card.name);
    println!("  source format   {}", card.source_format);
    println!(
        "  greeting seeded {}",
        if imported.seeded_message_id.is_some() {
            "yes"
        } else {
            "no"
        }
    );
    println!();
    crate::cli::print::print_session_details(&imported.session);
    println!();
    println!("Paths");
    crate::cli::print::print_session_paths(repo.paths(), &imported.session.session_id);

    if let Some(branch_id) = imported.seeded_branch_id {
        println!();
        println!("Seeded branch");
        println!("  branch id       {}", branch_id);
    }

    if let Some(message_id) = imported.seeded_message_id {
        println!("  greeting id     {}", message_id);
    }

    Ok(())
}