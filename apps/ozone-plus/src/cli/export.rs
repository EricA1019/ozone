use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id};
use ozone_core::engine::BranchId;

pub fn handle_export_command(command: ExportCommand) -> Result<(), String> {
    match command {
        ExportCommand::Session(args) => export_session(args),
        ExportCommand::Transcript(args) => export_transcript(args),
    }
}

pub fn export_session(args: ExportSessionArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let export = repo
        .export_session(&session_id)
        .map_err(|error| error.to_string())?;
    let contents = match args.format {
        SessionExportFormat::Json => export.to_pretty_json().map_err(|error| error.to_string())?,
    };
    let output_path = crate::cli::print::write_output_file(&args.output, &contents)?;

    println!("Exported session.");
    println!("  session id      {}", session_id);
    println!("  format          {:?}", args.format);
    println!("  output          {}", output_path.display());
    Ok(())
}

pub fn export_transcript(args: ExportTranscriptArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let branch_id = args
        .branch_id
        .as_deref()
        .map(|s| {
            BranchId::parse(s).map_err(|e| format!("invalid branch id: {e}"))
        })
        .transpose()?;
    let export = repo
        .export_transcript(&session_id, branch_id.as_ref())
        .map_err(|error| error.to_string())?;
    let contents = match args.format {
        TranscriptExportFormat::Json => {
            export.to_pretty_json().map_err(|error| error.to_string())?
        }
        TranscriptExportFormat::Text => crate::cli::print::render_transcript_text(&export),
    };
    let output_path = crate::cli::print::write_output_file(&args.output, &contents)?;

    println!("Exported transcript.");
    println!("  session id      {}", session_id);
    println!(
        "  branch id       {}",
        branch_id.map(|id| id.to_string()).unwrap_or_else(|| export
            .branch
            .as_ref()
            .map(|branch| branch.branch_id.clone())
            .unwrap_or_else(|| "active branch unavailable".to_owned()))
    );
    println!("  format          {:?}", args.format);
    println!("  output          {}", output_path.display());
    Ok(())
}