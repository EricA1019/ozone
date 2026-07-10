use super::helpers::*;
use crate::app::ShellState;
use crate::input::InputMode;
use crate::state::{FocusTarget, RuntimePhase, ScreenState};

pub fn input_mode_label(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Insert => "INSERT",
        InputMode::Command => "COMMAND",
        InputMode::Visual => "VISUAL",
    }
}

pub fn screen_label(screen: &ScreenState) -> &'static str {
    match screen {
        ScreenState::MainMenu => "main menu",
        ScreenState::SessionList => "sessions",
        ScreenState::CharacterManager => "characters",
        ScreenState::CharacterCreate => "character create",
        ScreenState::CharacterEdit => "character edit",
        ScreenState::CharacterImport => "character import",
        ScreenState::Settings => "settings",
        ScreenState::Conversation => "conversation",
        ScreenState::Help => "help",
        ScreenState::Quit => "quit",
        ScreenState::ModelIntelligence => "model intelligence",
        ScreenState::MemoriesOverlay => "memories",
        ScreenState::CharacterOverlay(_) => "character overlay",
    }
}

pub fn focus_label(focus: FocusTarget) -> &'static str {
    match focus {
        FocusTarget::Transcript => "conversation",
        FocusTarget::Draft => "composer",
        FocusTarget::Status => "status",
        FocusTarget::Inspector => "inspector",
    }
}

pub fn selection_label(state: &ShellState) -> String {
    match (
        state.session.selected_message,
        state.session.transcript.len(),
    ) {
        (_, 0) => "0 messages".into(),
        (Some(index), total) => format!("message {}/{}", index + 1, total),
        (None, total) => format!("{} messages", total),
    }
}

pub fn branch_label(state: &ShellState) -> String {
    state
        .session
        .selected_branch
        .and_then(|index| state.session.branches.get(index))
        .or_else(|| {
            state
                .session
                .branches
                .iter()
                .find(|branch| branch.is_active)
        })
        .map(|branch| format!("branch {}", branch.label))
        .unwrap_or_else(|| "no branches loaded".into())
}

pub fn runtime_label(runtime: &RuntimePhase) -> String {
    match runtime {
        RuntimePhase::Idle => "runtime idle".into(),
        RuntimePhase::Queued { .. } => "runtime queued".into(),
        RuntimePhase::Generating { request_id, .. } => {
            format!("runtime generating · {}", request_id)
        }
        RuntimePhase::Cancelling { request_id, .. } => match request_id {
            Some(request_id) => format!("runtime cancelling · {}", request_id),
            None => "runtime cancelling".into(),
        },
        RuntimePhase::Cancelled {
            request_id, reason, ..
        } => match request_id {
            Some(request_id) => format!("runtime cancelled · {} · {}", request_id, reason),
            None => format!("runtime cancelled · {}", reason),
        },
        RuntimePhase::Failed {
            request_id,
            message,
            ..
        } => match request_id {
            Some(request_id) => format!("runtime failed · {} · {}", request_id, message),
            None => format!("runtime failed · {}", message),
        },
    }
}

pub fn context_status_line(state: &ShellState) -> String {
    state
        .context_preview
        .as_ref()
        .map(|preview| format!("context {}", preview.inline_status))
        .unwrap_or_else(|| "context preview pending".into())
}

pub fn status_short_runtime(summary: &str) -> String {
    if summary.contains("generating") {
        "⟳ generating\u{2026}".into()
    } else if summary.contains("queued") {
        "⟳ queued".into()
    } else if summary.contains("cancelling") {
        "\u{2715} cancelling".into()
    } else if summary.contains("cancelled") {
        "\u{2715} cancelled".into()
    } else if summary.contains("failed") {
        "\u{26a0} error".into()
    } else if summary == "runtime idle" {
        String::new()
    } else {
        truncate_str(summary, 36)
    }
}

