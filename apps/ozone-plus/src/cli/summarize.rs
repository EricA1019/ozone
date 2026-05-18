use crate::cli::args::*;
use crate::cli::util::{open_repository, parse_session_id, parse_message_id};
use ozone_memory::summary;

pub fn handle_summarize_command(command: SummarizeCommand) -> Result<(), String> {
    match command {
        SummarizeCommand::Session { session_id } => summarize_session(session_id),
        SummarizeCommand::Chunk {
            session_id,
            start_message_id,
            end_message_id,
        } => summarize_chunk(session_id, start_message_id, end_message_id),
    }
}

pub fn summarize_session(session_id_raw: String) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&session_id_raw)?;
    let messages = repo
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    let turns: Vec<summary::SummaryInputTurn> = messages
        .iter()
        .map(|msg| summary::SummaryInputTurn {
            role: msg.author_kind.clone(),
            content: msg.content.clone(),
        })
        .collect();

    let config = summary::SummaryConfig::default();
    match summary::generate_session_synopsis(&turns, &config) {
        Some(synopsis) => {
            println!("Session synopsis");
            println!("  session         {session_id}");
            println!("  messages        {}", messages.len());
            println!();
            println!("{synopsis}");
            match repo.store_session_synopsis(&session_id, &synopsis, messages.len(), 0) {
                Ok(record) => println!("  stored as       {}", record.artifact_id),
                Err(err) => eprintln!("  warning: failed to persist synopsis: {err}"),
            }
        }
        None => {
            println!(
                "Not enough content to generate a synopsis ({} messages, minimum {}).",
                messages.len(),
                config.synopsis_min_messages
            );
        }
    }

    Ok(())
}

pub fn summarize_chunk(
    session_id_raw: String,
    start_message_id_raw: String,
    end_message_id_raw: String,
) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&session_id_raw)?;
    let start_id = parse_message_id(&start_message_id_raw)?;
    let end_id = parse_message_id(&end_message_id_raw)?;
    let messages = repo
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    let start_idx = messages
        .iter()
        .position(|m| m.message_id == start_id)
        .ok_or_else(|| format!("start message {start_id} not found in active branch transcript"))?;
    let end_idx = messages
        .iter()
        .position(|m| m.message_id == end_id)
        .ok_or_else(|| format!("end message {end_id} not found in active branch transcript"))?;

    if end_idx < start_idx {
        return Err("end message must come after start message in the transcript".to_owned());
    }

    let chunk = &messages[start_idx..=end_idx];
    let turns: Vec<summary::SummaryInputTurn> = chunk
        .iter()
        .map(|msg| summary::SummaryInputTurn {
            role: msg.author_kind.clone(),
            content: msg.content.clone(),
        })
        .collect();

    let config = summary::SummaryConfig::default();
    match summary::generate_chunk_summary(&turns, &config) {
        Some(summary) => {
            println!("Chunk summary");
            println!("  session         {session_id}");
            println!("  range           {start_id} → {end_id}");
            println!("  messages        {}", chunk.len());
            println!();
            println!("{summary}");
            match repo.store_chunk_summary(
                &session_id,
                &summary,
                chunk.len(),
                &start_id,
                &end_id,
                0,
            ) {
                Ok(record) => println!("  stored as       {}", record.artifact_id),
                Err(err) => eprintln!("  warning: failed to persist chunk summary: {err}"),
            }
        }
        None => {
            println!(
                "Not enough content to generate a chunk summary ({} messages in range).",
                chunk.len()
            );
        }
    }

    Ok(())
}