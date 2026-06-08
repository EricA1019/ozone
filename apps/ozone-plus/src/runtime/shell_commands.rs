use ozone_engine::ThinkingDisplayMode;
use ozone_persist::MemoryArtifactId;

use super::{
    HooksCommand, MemoryCommand, SafeModeCommand, SearchCommand, SessionCommand, ShellCommand,
    SummarizeShellCommand, ThinkingCommand, TierBCommand,
};

pub(super) fn parse_shell_command(input: &str) -> Result<ShellCommand, String> {
    let trimmed = input.trim();

    if let Some(alias) = trimmed.strip_prefix(':') {
        return match alias.trim() {
            "memories" => Ok(ShellCommand::Memory(MemoryCommand::List)),
            _ => Err(unknown_shell_command_message()),
        };
    }

    let command = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = command.splitn(2, char::is_whitespace);
    let root = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();

    match root {
        "session" => parse_session_subcommand(remainder).map(ShellCommand::Session),
        "memory" => parse_memory_subcommand(remainder).map(ShellCommand::Memory),
        "memories" if remainder.is_empty() => Ok(ShellCommand::Memory(MemoryCommand::List)),
        "search" => parse_search_subcommand(remainder).map(ShellCommand::Search),
        "summarize" => parse_summarize_subcommand(remainder).map(ShellCommand::Summarize),
        "thinking" => parse_thinking_subcommand(remainder).map(ShellCommand::Thinking),
        "tierb" => parse_tierb_subcommand(remainder).map(ShellCommand::TierB),
        "hooks" => parse_hooks_subcommand(remainder).map(ShellCommand::Hooks),
        "safemode" => parse_safemode_subcommand(remainder).map(ShellCommand::SafeMode),
        _ => Err(unknown_shell_command_message()),
    }
}

pub(super) fn require_non_empty(label: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }

    Ok(trimmed.to_owned())
}

pub(super) fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".to_owned()
    } else {
        tags.join(", ")
    }
}

fn parse_session_subcommand(remainder: &str) -> Result<SessionCommand, String> {
    if remainder.eq_ignore_ascii_case("show") {
        return Ok(SessionCommand::Show);
    }

    let mut subcommand_parts = remainder.splitn(2, char::is_whitespace);
    let subcommand = subcommand_parts.next().unwrap_or_default();
    let argument = subcommand_parts.next().unwrap_or_default().trim();

    match subcommand {
        "rename" => Ok(SessionCommand::Rename(require_non_empty(
            "session name",
            argument.to_owned(),
        )?)),
        "retitle" => Ok(SessionCommand::Retitle),
        "reroll" if argument.is_empty() => Ok(SessionCommand::Reroll),
        "character" => {
            if argument.eq_ignore_ascii_case("clear") || argument == "-" {
                Ok(SessionCommand::Character(None))
            } else {
                Ok(SessionCommand::Character(Some(require_non_empty(
                    "character name",
                    argument.to_owned(),
                )?)))
            }
        }
        "tags" => {
            if argument.eq_ignore_ascii_case("clear") || argument == "-" {
                Ok(SessionCommand::Tags(Vec::new()))
            } else {
                let tags = normalize_tags(argument);
                if tags.is_empty() {
                    Err("Session tags command expects comma-separated tags or `clear`".to_owned())
                } else {
                    Ok(SessionCommand::Tags(tags))
                }
            }
        }
        _ => Err(
            "Unknown session command. Try show, rename, retitle, reroll, character, or tags"
                .to_owned(),
        ),
    }
}

fn parse_memory_subcommand(remainder: &str) -> Result<MemoryCommand, String> {
    if remainder.is_empty() || remainder.eq_ignore_ascii_case("list") {
        return Ok(MemoryCommand::List);
    }

    let mut parts = remainder.splitn(2, char::is_whitespace);
    let subcommand = parts.next().unwrap_or_default();
    let argument = parts.next().unwrap_or_default().trim();

    match subcommand {
        "note" => Ok(MemoryCommand::Note(require_non_empty(
            "memory note",
            argument.to_owned(),
        )?)),
        "unpin" => Ok(MemoryCommand::Unpin(
            MemoryArtifactId::parse(require_non_empty("artifact id", argument.to_owned())?)
                .map_err(|error| error.to_string())?,
        )),
        _ => Err(
            "Unknown memory command. Try /memory list | /memory note TEXT | /memory unpin <artifact-id> | Ctrl+K to pin the selected message | :memories"
                .to_owned(),
        ),
    }
}

fn parse_search_subcommand(remainder: &str) -> Result<SearchCommand, String> {
    let mut parts = remainder.splitn(2, char::is_whitespace);
    let scope = parts.next().unwrap_or_default();
    let query = parts.next().unwrap_or_default().trim();

    match scope {
        "session" => Ok(SearchCommand::Session(require_non_empty(
            "search query",
            query.to_owned(),
        )?)),
        "global" => Ok(SearchCommand::Global(require_non_empty(
            "search query",
            query.to_owned(),
        )?)),
        _ => Err(
            "Unknown search command. Try /search session QUERY | /search global QUERY"
                .to_owned(),
        ),
    }
}

fn parse_summarize_subcommand(remainder: &str) -> Result<SummarizeShellCommand, String> {
    match remainder.trim() {
        "session" | "" => Ok(SummarizeShellCommand::Session),
        _ => Err("Usage: /summarize session".to_string()),
    }
}

fn parse_thinking_subcommand(remainder: &str) -> Result<ThinkingCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(ThinkingCommand::Status),
        "hidden" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Hidden)),
        "assisted" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Assisted)),
        "debug" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Debug)),
        _ => Err("Usage: /thinking [status|hidden|assisted|debug]".to_string()),
    }
}

fn parse_tierb_subcommand(remainder: &str) -> Result<TierBCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(TierBCommand::Status),
        "toggle" => Ok(TierBCommand::Toggle),
        _ => Err("Usage: /tierb [status|toggle]".to_string()),
    }
}

fn parse_hooks_subcommand(remainder: &str) -> Result<HooksCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(HooksCommand::Status),
        "list" => Ok(HooksCommand::List),
        _ => Err("Usage: /hooks [status|list]".to_string()),
    }
}

fn parse_safemode_subcommand(remainder: &str) -> Result<SafeModeCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(SafeModeCommand::Status),
        "on" => Ok(SafeModeCommand::On),
        "off" => Ok(SafeModeCommand::Off),
        "toggle" => Ok(SafeModeCommand::Toggle),
        _ => Err("Usage: /safemode [status|on|off|toggle]".to_string()),
    }
}

fn normalize_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|tag| {
            let trimmed = tag.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .collect()
}

pub(super) fn unknown_shell_command_message() -> String {
    "Unknown command. Try /session show|rename|retitle|reroll|character|tags | /memory list|note|unpin (+ Ctrl+K to pin) | \
/search session|global | /summarize session | /thinking status|hidden|assisted|debug | \
/tierb status|toggle | /hooks status|list | /safemode status|on|off|toggle | :memories"
        .to_owned()
}
