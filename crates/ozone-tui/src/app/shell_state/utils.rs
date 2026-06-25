use crate::state::*;
use crate::input::{InputMode, KeyAction};
use crate::app::textareas::{new_themed_textarea, themed_textarea_from_text};
use crate::app::TextAreaSurface;

/// Split textarea text into lines.
pub fn textarea_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        vec![String::new()]
    } else {
        text.split('\n').map(str::to_owned).collect()
    }
}

/// Calculate character offset from line/col.
pub fn textarea_cursor_offset(lines: &[String], row: usize, col: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }

    let row = row.min(lines.len().saturating_sub(1));
    let mut offset = 0usize;
    for line in lines.iter().take(row) {
        offset += line.chars().count() + 1;
    }

    offset + col.min(lines[row].chars().count())
}

/// Convert cursor offset to line/col position.
pub fn textarea_cursor_position(lines: &[String], cursor: usize) -> (u16, u16) {
    let mut remaining = cursor;
    for (row, line) in lines.iter().enumerate() {
        let line_len = line.chars().count();
        if remaining <= line_len {
            return (row as u16, remaining as u16);
        }
        remaining = remaining.saturating_sub(line_len + 1);
    }

    let row = lines.len().saturating_sub(1) as u16;
    let col = remaining as u16;
    (row, col)
}

/// Check if prompt starts with shell command prefix.
pub(crate) fn is_shell_command(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    trimmed.starts_with('/') || trimmed.starts_with(':')
}

/// Parse local shell command (e.g., /session reroll).
pub(crate) fn parse_local_shell_command(prompt: &str) -> Option<&'static str> {
    let trimmed = prompt.trim();
    let command = trimmed.strip_prefix('/').or_else(|| trimmed.strip_prefix(':'))?;
    let mut parts = command.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("session"), Some("reroll"), None) => Some("shell reroll"),
        _ => None,
    }
}

fn opens_memories_overlay(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), ":memories" | "/memories" | "/memory list")
}

/// Parse an `/attach <path>` command. Returns the file path if valid.
pub(crate) fn parse_attach_command(prompt: &str) -> Option<std::path::PathBuf> {
    let trimmed = prompt.trim();
    let command = trimmed.strip_prefix('/').or_else(|| trimmed.strip_prefix(':'))?;
    let mut parts = command.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("attach" | "a"), Some(path)) if parts.next().is_none() => {
            Some(std::path::PathBuf::from(path))
        }
        _ => None,
    }
}

/// Read a file and return its contents as a string, or an error message.
fn read_file_for_attach(path: &std::path::Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()));
    }
    let metadata = std::fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if metadata.len() > 1024 * 1024 {
        return Err(format!("file too large ({} bytes): {}", metadata.len(), path.display()));
    }
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

impl ShellState {
    pub fn persistable_draft(&self) -> Option<DraftCheckpoint> {
        let checkpoint = self
            .message_edit
            .as_ref()
            .map(|edit_state| edit_state.previous_draft.checkpoint())
            .unwrap_or_else(|| self.draft.checkpoint());
        (!checkpoint.text.is_empty() || self.draft.dirty).then_some(checkpoint)
    }

    pub fn take_pending_actions(&mut self) -> Vec<KeyAction> {
        std::mem::take(&mut self.pending_actions)
    }

    pub fn take_runtime_commands(&mut self) -> Vec<RuntimeCommand> {
        std::mem::take(&mut self.runtime_commands)
    }

    pub(crate) fn prefill_shell_command(&mut self, command: String) {
        self.enter_conversation();
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.history.reset_navigation();
        self.replace_draft(DraftState::with_text(command.clone()));
        self.status_line = Some(format!("{command}— continue typing or press Enter"));
    }

    pub fn enqueue_shell_command(&mut self, prompt: String) {
        let show_memories_overlay = opens_memories_overlay(&prompt);
        self.enter_conversation();
        if show_memories_overlay {
            self.screen = ScreenState::MemoriesOverlay;
            self.focus = FocusTarget::Transcript;
            self.input_mode = InputMode::Normal;
        }
        self.history.push(prompt.clone());
        self.runtime_commands
            .push(RuntimeCommand::RunCommand { input: prompt });
        self.draft = DraftState::default();
        self.textarea = new_themed_textarea();
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.status_line = Some(if show_memories_overlay {
            "Loading memories…".into()
        } else {
            "Running shell command…".into()
        });
        if show_memories_overlay {
            self.focus = FocusTarget::Transcript;
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn cancel_generation(&mut self) {
        if !self.session.runtime.is_inflight() {
            self.status_line = Some("No generation is active".into());
            return;
        }

        let prompt = self.session.runtime.prompt().unwrap_or_default().to_owned();
        let request_id = self.session.runtime.request_id().map(str::to_owned);

        self.session.runtime = RuntimePhase::Cancelling { request_id, prompt };
        self.runtime_commands.push(RuntimeCommand::CancelGeneration);
        self.status_line = Some("Cancelling generation…".into());
    }

    pub fn begin_selected_message_edit(&mut self) {
        let Some(index) = self.session.selected_message else {
            self.status_line = Some("No transcript message is selected".into());
            return;
        };
        let Some(item) = self.session.transcript.get(index).cloned() else {
            self.status_line = Some("Selected transcript entry is no longer available".into());
            return;
        };
        let Some(message_id) = item.message_id else {
            self.status_line = Some("Only persisted transcript messages can be edited".into());
            return;
        };

        self.message_edit = Some(MessageEditState {
            message_id,
            previous_draft: self.draft.clone(),
            previous_focus: self.focus,
            previous_input_mode: self.input_mode,
            previous_history: self.history.clone(),
            previous_slash_selected: self.slash_selected,
            previous_slash_dismissed: self.slash_dismissed,
        });

        let cursor = item.content.chars().count();
        self.draft = DraftState::restore(DraftCheckpoint::new(item.content, cursor));
        let text = self.draft.text.clone();
        let cursor = self.draft.cursor;
        self.sync_textarea_from_draft(&text, cursor);
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.history.reset_navigation();
        self.command_palette.close();
        self.slash_selected = None;
        self.slash_dismissed = false;
        self.status_line = Some("Editing selected message…".into());
    }

    pub fn cancel_message_edit(&mut self) {
        let Some(edit_state) = self.message_edit.take() else {
            return;
        };
        self.restore_post_edit_state(edit_state);
        self.status_line = Some("Message edit cancelled".into());
    }

    pub fn restore_post_edit_state(&mut self, edit_state: MessageEditState) {
        let previous_draft = edit_state.previous_draft;
        self.focus = edit_state.previous_focus;
        self.input_mode = edit_state.previous_input_mode;
        self.history = edit_state.previous_history;
        self.slash_selected = edit_state.previous_slash_selected;
        self.slash_dismissed = edit_state.previous_slash_dismissed;
        self.draft = previous_draft.clone();
        if previous_draft.text.is_empty() {
            self.textarea = new_themed_textarea();
        } else {
            self.sync_textarea_from_draft(&previous_draft.text, previous_draft.cursor);
        }
    }

    pub fn trigger_context_dry_run(&mut self) {
        self.runtime_commands
            .push(RuntimeCommand::BuildContextDryRun);
        self.status_line = Some("Building context dry run…".into());
        self.inspector.focus = InspectorFocus::Summary;
    }

    pub fn trigger_bookmark_toggle(&mut self) {
        let Some(index) = self.session.selected_message else {
            self.status_line = Some("No transcript message is selected".into());
            return;
        };
        let Some(item) = self.session.transcript.get(index) else {
            self.status_line = Some("Selected transcript entry is no longer available".into());
            return;
        };
        let Some(message_id) = item.message_id.clone() else {
            self.status_line = Some("Only persisted transcript messages can be bookmarked".into());
            return;
        };

        self.runtime_commands
            .push(RuntimeCommand::ToggleBookmark { message_id });
        self.status_line = Some("Updating bookmark…".into());
        self.show_toast("★ Bookmark toggled");
        self.inspector.focus = InspectorFocus::Message;
    }

    pub fn trigger_pinned_memory_toggle(&mut self) {
        let Some(index) = self.session.selected_message else {
            self.status_line = Some("No transcript message is selected".into());
            return;
        };
        let Some(item) = self.session.transcript.get(index) else {
            self.status_line = Some("Selected transcript entry is no longer available".into());
            return;
        };
        let Some(message_id) = item.message_id.clone() else {
            self.status_line =
                Some("Only persisted transcript messages can be pinned to memory".into());
            return;
        };

        self.runtime_commands
            .push(RuntimeCommand::TogglePinnedMemory { message_id });
        self.status_line = Some("Updating pinned memory…".into());
        self.show_toast("📌 Pinned to memory");
        self.inspector.focus = InspectorFocus::Recall;
    }

    pub fn trigger_reroll_selected_message(&mut self) {
        let Some(index) = self.session.selected_message else {
            self.status_line = Some("No transcript message is selected".into());
            return;
        };
        let Some(item) = self.session.transcript.get(index).cloned() else {
            self.status_line = Some("Selected transcript entry is no longer available".into());
            return;
        };
        let Some(message_id) = item.message_id.clone() else {
            self.status_line = Some("Only persisted transcript messages can be rerolled".into());
            return;
        };
        if item.author_kind != "assistant" {
            self.status_line = Some("Only assistant messages can be rerolled".into());
            return;
        }
        if self.session.runtime.is_inflight() {
            self.status_line = Some("Cannot reroll while generation is active".into());
            return;
        }

        self.runtime_commands
            .push(RuntimeCommand::RerollMessage { message_id });
        self.status_line = Some("Rerolling reply…".into());
        self.show_toast("↺ Reroll started");
        self.inspector.focus = InspectorFocus::Message;
    }

    pub(crate) fn push_transcript_item(&mut self, item: TranscriptItem) {
        self.session.transcript.push(item);
        self.session.selected_message = Some(self.session.transcript.len() - 1);
    }

    pub fn sync_pending_user_message(&mut self, item: TranscriptItem) {
        if let Some(message_id) = item.message_id.as_deref() {
            if let Some(index) = self
                .session
                .transcript
                .iter()
                .position(|existing| existing.message_id.as_deref() == Some(message_id))
            {
                self.session.selected_message = Some(index);
                return;
            }
        }

        self.push_transcript_item(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_attach_command_extracts_path() {
        assert_eq!(
            parse_attach_command("/attach /home/user/notes.txt"),
            Some(std::path::PathBuf::from("/home/user/notes.txt"))
        );
        assert_eq!(
            parse_attach_command(":attach /home/user/notes.txt"),
            Some(std::path::PathBuf::from("/home/user/notes.txt"))
        );
        assert_eq!(
            parse_attach_command("/a /home/user/notes.txt"),
            Some(std::path::PathBuf::from("/home/user/notes.txt"))
        );
    }

    #[test]
    fn parse_attach_command_rejects_extra_args() {
        assert_eq!(parse_attach_command("/attach /a /b"), None);
        assert_eq!(parse_attach_command("/attach"), None);
    }

    #[test]
    fn parse_attach_command_rejects_non_attach_commands() {
        assert_eq!(parse_attach_command("/memory list"), None);
        assert_eq!(parse_attach_command("/session reroll"), None);
        assert_eq!(parse_attach_command("just a normal message"), None);
    }

    #[test]
    fn read_file_for_attach_rejects_missing_files() {
        assert!(read_file_for_attach(std::path::Path::new("/nonexistent/path/file.txt")).is_err());
    }

    #[test]
    fn read_file_for_attach_reads_valid_file() {
        let tmp = std::env::temp_dir().join("ozone-attach-test.txt");
        std::fs::write(&tmp, "hello world").unwrap();
        let result = read_file_for_attach(&tmp);
        assert_eq!(result.unwrap(), "hello world");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn read_file_for_attach_rejects_too_large_files() {
        let tmp = std::env::temp_dir().join("ozone-attach-large.txt");
        let content = "x".repeat(1024 * 1024 + 1);
        std::fs::write(&tmp, content).unwrap();
        let result = read_file_for_attach(&tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
        std::fs::remove_file(&tmp).ok();
    }
}
