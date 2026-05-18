use crate::cli::args::*;

pub fn show_transcript(args: TranscriptArgs) -> Result<(), String> {
    let engine = crate::cli::util::open_cli_engine()?;
    let (branch, transcript) = engine.transcript(args)?;

    println!("Transcript");
    match branch {
        Some(branch) => crate::cli::print::print_branch_record_from_engine(&branch, true),
        None => println!("  active branch    none"),
    }
    println!();
    crate::cli::print::print_transcript(&transcript);
    Ok(())
}