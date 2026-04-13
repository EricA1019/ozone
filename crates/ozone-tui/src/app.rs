use crossterm::event::KeyEvent;
use ozone_core::{engine::CancelReason, session::SessionId};

use crate::input::{dispatch_key, InputMode, KeyAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    Conversation,
    Help,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Transcript,
    Draft,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorFocus {
    Summary,
    Branches,
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorState {
    pub visible: bool,
    pub focus: InspectorFocus,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            visible: false,
            focus: InspectorFocus::Summary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionContext {
    pub session_id: SessionId,
    pub title: String,
}

impl SessionContext {
    pub fn new(session_id: SessionId, title: impl Into<String>) -> Self {
        Self {
            session_id,
            title: title.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftCheckpoint {
    pub text: String,
    pub cursor: usize,
}

impl DraftCheckpoint {
    pub fn new(text: impl Into<String>, cursor: usize) -> Self {
        let text = text.into();
        let cursor = clamp_cursor(&text, cursor);

        Self { text, cursor }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftState {
    pub text: String,
    pub cursor: usize,
    pub dirty: bool,
    pub persisted: Option<DraftCheckpoint>,
}

impl DraftState {
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        let dirty = !text.is_empty();

        Self {
            text,
            cursor,
            dirty,
            persisted: None,
        }
    }

    pub fn restore(checkpoint: DraftCheckpoint) -> Self {
        Self {
            text: checkpoint.text.clone(),
            cursor: checkpoint.cursor,
            dirty: false,
            persisted: Some(checkpoint),
        }
    }

    pub fn checkpoint(&self) -> DraftCheckpoint {
        DraftCheckpoint::new(self.text.clone(), self.cursor)
    }

    pub fn insert_char(&mut self, ch: char) {
        let byte_index = byte_index_for_char(&self.text, self.cursor);
        self.text.insert(byte_index, ch);
        self.cursor += 1;
        self.sync_dirty();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let end = byte_index_for_char(&self.text, self.cursor);
        let start = byte_index_for_char(&self.text, self.cursor - 1);
        self.text.drain(start..end);
        self.cursor -= 1;
        self.sync_dirty();
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }

        let start = byte_index_for_char(&self.text, self.cursor);
        let end = byte_index_for_char(&self.text, self.cursor + 1);
        self.text.drain(start..end);
        self.sync_dirty();
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.char_len();
    }

    fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    fn sync_dirty(&mut self) {
        self.cursor = clamp_cursor(&self.text, self.cursor);
        self.dirty = match &self.persisted {
            Some(checkpoint) => {
                checkpoint.text != self.text
                    || clamp_cursor(&self.text, checkpoint.cursor) != self.cursor
            }
            None => !self.text.is_empty(),
        };
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputHistoryState {
    pub entries: Vec<String>,
    pub browsing: Option<usize>,
    pub draft_before_browse: Option<DraftCheckpoint>,
}

impl InputHistoryState {
    pub fn push(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.trim().is_empty() {
            return;
        }

        if self.entries.last() != Some(&entry) {
            self.entries.push(entry);
        }
        self.reset_navigation();
    }

    pub fn previous(&mut self, current: &DraftState) -> Option<DraftState> {
        if self.entries.is_empty() {
            return None;
        }

        let index = match self.browsing {
            Some(index) if index > 0 => index - 1,
            Some(index) => index,
            None => {
                self.draft_before_browse = Some(current.checkpoint());
                self.entries.len() - 1
            }
        };

        self.browsing = Some(index);
        Some(DraftState::with_text(self.entries[index].clone()))
    }

    pub fn next_entry(&mut self) -> Option<DraftState> {
        let current = self.browsing?;

        if current + 1 < self.entries.len() {
            self.browsing = Some(current + 1);
            return Some(DraftState::with_text(self.entries[current + 1].clone()));
        }

        self.browsing = None;
        Some(
            self.draft_before_browse
                .take()
                .map(DraftState::restore)
                .unwrap_or_default(),
        )
    }

    pub fn reset_navigation(&mut self) {
        self.browsing = None;
        self.draft_before_browse = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptItem {
    pub message_id: Option<String>,
    pub author: String,
    pub content: String,
    pub is_bookmarked: bool,
}

impl TranscriptItem {
    pub fn new(author: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            message_id: None,
            author: author.into(),
            content: content.into(),
            is_bookmarked: false,
        }
    }

    pub fn persisted(
        message_id: impl Into<String>,
        author: impl Into<String>,
        content: impl Into<String>,
        is_bookmarked: bool,
    ) -> Self {
        Self {
            message_id: Some(message_id.into()),
            author: author.into(),
            content: content.into(),
            is_bookmarked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchItem {
    pub id: String,
    pub label: String,
    pub is_active: bool,
}

impl BranchItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>, is_active: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTokenBudget {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPreview {
    pub source: String,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudget>,
    pub inline_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDryRunPreview {
    pub summary: String,
    pub built_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub character_name: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionStats {
    pub message_count: usize,
    pub branch_count: usize,
    pub bookmark_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RuntimePhase {
    #[default]
    Idle,
    Queued {
        prompt: String,
    },
    /// Actively generating. `partial_content` holds streamed text received so far.
    Generating {
        request_id: String,
        prompt: String,
        partial_content: Option<String>,
    },
    Cancelling {
        request_id: Option<String>,
        prompt: String,
    },
    Cancelled {
        request_id: Option<String>,
        prompt: String,
        reason: CancelReason,
    },
    Failed {
        request_id: Option<String>,
        prompt: String,
        message: String,
    },
}

impl RuntimePhase {
    pub fn is_inflight(&self) -> bool {
        matches!(
            self,
            Self::Queued { .. } | Self::Generating { .. } | Self::Cancelling { .. }
        )
    }

    fn prompt(&self) -> Option<&str> {
        match self {
            Self::Queued { prompt }
            | Self::Generating { prompt, .. }
            | Self::Cancelling { prompt, .. }
            | Self::Cancelled { prompt, .. }
            | Self::Failed { prompt, .. } => Some(prompt),
            Self::Idle => None,
        }
    }

    fn request_id(&self) -> Option<&str> {
        match self {
            Self::Generating { request_id, .. } => Some(request_id.as_str()),
            Self::Cancelling { request_id, .. }
            | Self::Cancelled { request_id, .. }
            | Self::Failed { request_id, .. } => request_id.as_deref(),
            Self::Idle | Self::Queued { .. } => None,
        }
    }

    /// Returns streamed partial content if currently in the `Generating` phase.
    pub fn partial_content(&self) -> Option<&str> {
        match self {
            Self::Generating {
                partial_content: Some(text),
                ..
            } => Some(text.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    SendDraft { prompt: String },
    CancelGeneration,
    BuildContextDryRun,
    ToggleBookmark { message_id: String },
    RunCommand { input: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSendReceipt {
    pub request_id: String,
    pub user_message: TranscriptItem,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeContextRefresh {
    pub status_line: Option<String>,
    pub session_title: Option<String>,
    pub transcript: Option<Vec<TranscriptItem>>,
    pub session_metadata: Option<SessionMetadata>,
    pub session_stats: Option<SessionStats>,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCompletion {
    pub request_id: String,
    pub assistant_message: TranscriptItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCancellation {
    pub request_id: String,
    pub reason: CancelReason,
    pub partial_assistant_message: Option<TranscriptItem>,
}

/// Partial content streamed from a running generation. `partial_content` is the
/// full accumulated text so far (not an incremental delta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProgress {
    pub request_id: String,
    pub partial_content: String,
}

/// An unrecoverable generation failure reported by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    pub request_id: String,
    pub message: String,
}

/// The result of a single `poll_generation` call. The runtime returns this to
/// tell the TUI shell whether generation is still in progress, completed, or
/// failed — replacing the fixed-delay timer approach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationPoll {
    /// Generation is still running. Optionally carries a partial-content update.
    Pending { partial: Option<RuntimeProgress> },
    /// Generation finished successfully.
    Completed(RuntimeCompletion),
    /// Generation failed unrecoverably.
    Failed(RuntimeFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    pub context: SessionContext,
    pub transcript: Vec<TranscriptItem>,
    pub branches: Vec<BranchItem>,
    pub selected_message: Option<usize>,
    pub selected_branch: Option<usize>,
    pub runtime: RuntimePhase,
}

impl SessionState {
    pub fn new(context: SessionContext) -> Self {
        Self {
            context,
            transcript: Vec::new(),
            branches: Vec::new(),
            selected_message: None,
            selected_branch: None,
            runtime: RuntimePhase::Idle,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppBootstrap {
    pub transcript: Vec<TranscriptItem>,
    pub branches: Vec<BranchItem>,
    pub status_line: Option<String>,
    pub draft: Option<DraftState>,
    pub screen: Option<ScreenState>,
    pub session_metadata: Option<SessionMetadata>,
    pub session_stats: Option<SessionStats>,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellState {
    pub screen: ScreenState,
    pub input_mode: InputMode,
    pub focus: FocusTarget,
    pub inspector: InspectorState,
    pub session: SessionState,
    pub draft: DraftState,
    pub history: InputHistoryState,
    pub status_line: Option<String>,
    pub session_metadata: Option<SessionMetadata>,
    pub session_stats: Option<SessionStats>,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
    pub pending_actions: Vec<KeyAction>,
    pub runtime_commands: Vec<RuntimeCommand>,
    pub should_quit: bool,
}

impl ShellState {
    pub fn new(context: SessionContext) -> Self {
        Self {
            screen: ScreenState::Conversation,
            input_mode: InputMode::Normal,
            focus: FocusTarget::Transcript,
            inspector: InspectorState::default(),
            session: SessionState::new(context),
            draft: DraftState::default(),
            history: InputHistoryState::default(),
            status_line: Some("ozone+ TUI shell skeleton ready".into()),
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            pending_actions: Vec::new(),
            runtime_commands: Vec::new(),
            should_quit: false,
        }
    }

    pub fn hydrate(&mut self, bootstrap: AppBootstrap) {
        self.session.transcript = bootstrap.transcript;
        self.session.selected_message = (!self.session.transcript.is_empty())
            .then_some(self.session.transcript.len().saturating_sub(1));

        self.session.branches = bootstrap.branches;
        self.session.selected_branch = self
            .session
            .branches
            .iter()
            .position(|branch| branch.is_active)
            .or_else(|| (!self.session.branches.is_empty()).then_some(0));

        if let Some(status_line) = bootstrap.status_line {
            self.status_line = Some(status_line);
        }

        if let Some(draft) = bootstrap.draft {
            if !draft.text.is_empty() {
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
            }
            self.draft = draft;
        }

        if let Some(screen) = bootstrap.screen {
            self.screen = screen;
        }

        self.session_metadata = bootstrap.session_metadata;
        self.session_stats = bootstrap.session_stats;
        self.context_preview = bootstrap.context_preview;
        self.context_dry_run = bootstrap.context_dry_run;
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> KeyAction {
        let action = dispatch_key(self.input_mode, key);
        if action != KeyAction::Noop {
            self.apply_action(action);
        }
        action
    }

    pub fn apply_action(&mut self, action: KeyAction) {
        self.pending_actions.push(action);

        match action {
            KeyAction::Noop => {}
            KeyAction::MoveSelectionUp => {
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
                self.history.reset_navigation();
                if let Some(index) = self.session.selected_message {
                    if index > 0 {
                        self.session.selected_message = Some(index - 1);
                    }
                } else if !self.session.transcript.is_empty() {
                    self.session.selected_message = Some(0);
                }
            }
            KeyAction::MoveSelectionDown => {
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
                self.history.reset_navigation();
                match self.session.selected_message {
                    Some(index) if index + 1 < self.session.transcript.len() => {
                        self.session.selected_message = Some(index + 1);
                    }
                    None if !self.session.transcript.is_empty() => {
                        self.session.selected_message = Some(0);
                    }
                    _ => {}
                }
            }
            KeyAction::FocusTranscript => {
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
                self.history.reset_navigation();
            }
            KeyAction::FocusDraft | KeyAction::EnterInsert => {
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
            }
            KeyAction::LeaveInputMode => {
                self.input_mode = InputMode::Normal;
                self.history.reset_navigation();
            }
            KeyAction::SubmitDraft => self.submit_draft(),
            KeyAction::CancelGeneration => self.cancel_generation(),
            KeyAction::ToggleInspector => {
                self.inspector.visible = !self.inspector.visible;
                self.status_line = Some(if self.inspector.visible {
                    "Inspector opened".into()
                } else {
                    "Inspector hidden".into()
                });
            }
            KeyAction::TriggerContextDryRun => self.trigger_context_dry_run(),
            KeyAction::ToggleBookmark => self.trigger_bookmark_toggle(),
            KeyAction::HistoryPrevious => {
                if let Some(draft) = self.history.previous(&self.draft) {
                    self.focus = FocusTarget::Draft;
                    self.input_mode = InputMode::Insert;
                    self.draft = draft;
                }
            }
            KeyAction::HistoryNext => {
                if let Some(draft) = self.history.next_entry() {
                    self.focus = FocusTarget::Draft;
                    self.input_mode = InputMode::Insert;
                    self.draft = draft;
                }
            }
            KeyAction::DraftInsertChar(ch) => {
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
                self.history.reset_navigation();
                self.draft.insert_char(ch);
            }
            KeyAction::DraftBackspace => {
                self.history.reset_navigation();
                self.draft.backspace();
            }
            KeyAction::DraftDelete => {
                self.history.reset_navigation();
                self.draft.delete();
            }
            KeyAction::MoveCursorLeft => {
                self.draft.move_cursor_left();
            }
            KeyAction::MoveCursorRight => {
                self.draft.move_cursor_right();
            }
            KeyAction::MoveCursorHome => {
                self.draft.move_cursor_home();
            }
            KeyAction::MoveCursorEnd => {
                self.draft.move_cursor_end();
            }
            KeyAction::ToggleHelp => {
                self.screen = match self.screen {
                    ScreenState::Help => ScreenState::Conversation,
                    _ => ScreenState::Help,
                };
            }
            KeyAction::ConfirmQuit => {
                self.screen = ScreenState::Quit;
                self.should_quit = true;
            }
        }
    }

    pub fn apply_send_receipt(&mut self, receipt: RuntimeSendReceipt) {
        let prompt = receipt.user_message.content.clone();
        if let Some(context_preview) = receipt.context_preview {
            self.context_preview = Some(context_preview);
        }
        if let Some(context_dry_run) = receipt.context_dry_run {
            self.context_dry_run = Some(context_dry_run);
        }
        self.push_transcript_item(receipt.user_message);
        self.session.runtime = RuntimePhase::Generating {
            request_id: receipt.request_id,
            prompt,
            partial_content: None,
        };
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some(
            self.context_preview
                .as_ref()
                .map(|preview| format!("Generation in progress · {}", preview.inline_status))
                .unwrap_or_else(|| "Generation in progress".into()),
        );
    }

    pub fn apply_runtime_completion(&mut self, completion: RuntimeCompletion) {
        self.push_transcript_item(completion.assistant_message);
        self.session.runtime = RuntimePhase::Idle;
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some("Generation completed".into());
    }

    pub fn apply_runtime_cancellation(&mut self, cancellation: RuntimeCancellation) {
        let prompt = self.session.runtime.prompt().unwrap_or_default().to_owned();

        if let Some(partial_message) = cancellation.partial_assistant_message {
            self.push_transcript_item(partial_message);
        }

        self.session.runtime = RuntimePhase::Cancelled {
            request_id: Some(cancellation.request_id),
            prompt,
            reason: cancellation.reason,
        };
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some(format!("Generation cancelled ({})", cancellation.reason));
    }

    /// Updates the partial content shown for a streaming generation in progress.
    pub fn apply_runtime_progress(&mut self, progress: RuntimeProgress) {
        if let RuntimePhase::Generating {
            partial_content, ..
        } = &mut self.session.runtime
        {
            *partial_content = Some(progress.partial_content);
        }
    }

    /// Transitions the runtime to the `Failed` terminal state.
    pub fn apply_runtime_failure(&mut self, failure: RuntimeFailure) {
        let prompt = self.session.runtime.prompt().unwrap_or_default().to_owned();
        let request_id = self.session.runtime.request_id().map(str::to_owned);
        self.session.runtime = RuntimePhase::Failed {
            request_id,
            prompt,
            message: failure.message.clone(),
        };
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some(format!("Generation failed: {}", failure.message));
    }

    pub fn apply_context_refresh(&mut self, refresh: RuntimeContextRefresh) {
        let selected_message_id = self
            .session
            .selected_message
            .and_then(|index| self.session.transcript.get(index))
            .and_then(|item| item.message_id.clone());
        if let Some(session_title) = refresh.session_title {
            self.session.context.title = session_title;
        }
        if let Some(transcript) = refresh.transcript {
            self.session.transcript = transcript;
            self.session.selected_message = selected_message_id
                .as_deref()
                .and_then(|message_id| {
                    self.session
                        .transcript
                        .iter()
                        .position(|item| item.message_id.as_deref() == Some(message_id))
                })
                .or_else(|| {
                    (!self.session.transcript.is_empty())
                        .then_some(self.session.transcript.len().saturating_sub(1))
                });
        }
        if let Some(session_metadata) = refresh.session_metadata {
            self.session_metadata = Some(session_metadata);
        }
        if let Some(session_stats) = refresh.session_stats {
            self.session_stats = Some(session_stats);
        }
        if let Some(context_preview) = refresh.context_preview {
            self.context_preview = Some(context_preview);
        }
        if let Some(context_dry_run) = refresh.context_dry_run {
            self.context_dry_run = Some(context_dry_run);
        }
        if let Some(status_line) = refresh.status_line {
            self.status_line = Some(status_line);
        }
    }

    pub fn persistable_draft(&self) -> Option<DraftCheckpoint> {
        let checkpoint = self.draft.checkpoint();
        (!checkpoint.text.is_empty() || self.draft.dirty).then_some(checkpoint)
    }

    pub fn take_pending_actions(&mut self) -> Vec<KeyAction> {
        std::mem::take(&mut self.pending_actions)
    }

    pub fn take_runtime_commands(&mut self) -> Vec<RuntimeCommand> {
        std::mem::take(&mut self.runtime_commands)
    }

    fn submit_draft(&mut self) {
        let prompt = self.draft.text.clone();
        if prompt.trim().is_empty() {
            self.status_line = Some("Draft is empty".into());
            return;
        }

        if prompt.trim_start().starts_with('/') {
            self.history.push(prompt.clone());
            self.runtime_commands
                .push(RuntimeCommand::RunCommand { input: prompt });
            self.draft = DraftState::default();
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Insert;
            self.status_line = Some("Running session command…".into());
            return;
        }

        self.history.push(prompt.clone());
        self.session.runtime = RuntimePhase::Queued {
            prompt: prompt.clone(),
        };
        self.runtime_commands
            .push(RuntimeCommand::SendDraft { prompt });
        self.draft = DraftState::default();
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.status_line = Some("Sending prompt…".into());
    }

    fn cancel_generation(&mut self) {
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

    fn trigger_context_dry_run(&mut self) {
        self.runtime_commands
            .push(RuntimeCommand::BuildContextDryRun);
        self.status_line = Some("Building context dry run…".into());
        self.inspector.focus = InspectorFocus::Summary;
    }

    fn trigger_bookmark_toggle(&mut self) {
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
        self.inspector.focus = InspectorFocus::Message;
    }

    fn push_transcript_item(&mut self, item: TranscriptItem) {
        self.session.transcript.push(item);
        self.session.selected_message = Some(self.session.transcript.len() - 1);
    }
}

fn clamp_cursor(text: &str, cursor: usize) -> usize {
    cursor.min(text.chars().count())
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .map(|(idx, _)| idx)
        .nth(char_index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ozone_core::engine::CancelReason;
    use ozone_core::session::SessionId;

    use super::{
        AppBootstrap, BranchItem, ContextDryRunPreview, ContextPreview, DraftCheckpoint,
        DraftState, FocusTarget, GenerationPoll, InspectorFocus, RuntimeCancellation,
        RuntimeCommand, RuntimeContextRefresh, RuntimeFailure, RuntimePhase, RuntimeProgress,
        RuntimeSendReceipt, ScreenState, SessionContext, SessionMetadata, SessionStats, ShellState,
        TranscriptItem,
    };
    use crate::input::{InputMode, KeyAction};

    fn session_context() -> SessionContext {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        SessionContext::new(session_id, "Phase 1C")
    }

    #[test]
    fn hydrate_seeds_transcript_branch_and_restored_draft() {
        let mut app = ShellState::new(session_context());
        let checkpoint = DraftCheckpoint::new("pending draft", 7);
        let bootstrap = AppBootstrap {
            transcript: vec![TranscriptItem::new("assistant", "hello")],
            branches: vec![
                BranchItem::new("branch-a", "main", false),
                BranchItem::new("branch-b", "fork", true),
            ],
            status_line: Some("hydrated".into()),
            draft: Some(DraftState::restore(checkpoint.clone())),
            screen: None,
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
        };

        app.hydrate(bootstrap);

        assert_eq!(app.session.selected_message, Some(0));
        assert_eq!(app.session.selected_branch, Some(1));
        assert_eq!(app.focus, FocusTarget::Draft);
        assert_eq!(app.input_mode, InputMode::Insert);
        assert_eq!(app.status_line.as_deref(), Some("hydrated"));
        assert_eq!(app.persistable_draft(), Some(checkpoint));
    }

    #[test]
    fn input_mode_transitions_follow_focus_changes() {
        let mut app = ShellState::new(session_context());

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.focus, FocusTarget::Transcript);

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            KeyAction::FocusDraft
        );
        assert_eq!(app.input_mode, InputMode::Insert);
        assert_eq!(app.focus, FocusTarget::Draft);

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            KeyAction::FocusTranscript
        );
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.focus, FocusTarget::Transcript);
    }

    #[test]
    fn draft_checkpoint_round_trips_cleanly() {
        let mut draft = DraftState::restore(DraftCheckpoint::new("persisted", 4));
        assert!(!draft.dirty);

        draft.insert_char('!');
        let checkpoint = draft.checkpoint();
        assert!(draft.dirty);

        let restored = DraftState::restore(checkpoint.clone());
        assert_eq!(restored.checkpoint(), checkpoint);
        assert!(!restored.dirty);
    }

    #[test]
    fn submitting_draft_queues_send_and_history_restores_working_copy() {
        let mut app = ShellState::new(session_context());

        app.apply_action(KeyAction::EnterInsert);
        for ch in ['h', 'i'] {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert_eq!(app.draft.text, "hi");

        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.draft.text, "");
        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::SendDraft {
                prompt: "hi".into()
            }]
        );
        assert!(matches!(app.session.runtime, RuntimePhase::Queued { .. }));

        for ch in ['w', 'i', 'p'] {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert_eq!(app.draft.text, "wip");

        app.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.draft.text, "hi");

        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.draft.text, "wip");
    }

    #[test]
    fn slash_command_routes_to_runtime_command_without_queuing_generation() {
        let mut app = ShellState::new(session_context());

        app.apply_action(KeyAction::EnterInsert);
        for ch in "/session show".chars() {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::RunCommand {
                input: "/session show".into()
            }]
        );
        assert!(matches!(app.session.runtime, RuntimePhase::Idle));
        assert_eq!(app.status_line.as_deref(), Some("Running session command…"));
    }

    #[test]
    fn ctrl_c_queues_cancel_for_active_generation() {
        let mut app = ShellState::new(session_context());

        app.apply_action(KeyAction::EnterInsert);
        for ch in ['h', 'e', 'l', 'l', 'o'] {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.take_runtime_commands().len(), 1);

        app.apply_send_receipt(RuntimeSendReceipt {
            request_id: "mock-request-1".into(),
            user_message: TranscriptItem::new("user", "hello"),
            context_preview: None,
            context_dry_run: None,
        });

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            KeyAction::CancelGeneration
        );
        assert!(matches!(
            app.session.runtime,
            RuntimePhase::Cancelling {
                request_id: Some(ref request_id),
                ..
            } if request_id == "mock-request-1"
        ));
        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::CancelGeneration]
        );
    }

    #[test]
    fn ctrl_i_toggles_inspector_and_runtime_updates_focus() {
        let mut app = ShellState::new(session_context());

        assert!(!app.inspector.visible);
        assert_eq!(app.inspector.focus, InspectorFocus::Summary);

        app.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL));
        assert!(app.inspector.visible);

        app.apply_send_receipt(RuntimeSendReceipt {
            request_id: "mock-request-2".into(),
            user_message: TranscriptItem::new("user", "inspect me"),
            context_preview: None,
            context_dry_run: None,
        });
        assert_eq!(app.inspector.focus, InspectorFocus::Message);

        app.apply_runtime_cancellation(RuntimeCancellation {
            request_id: "mock-request-2".into(),
            reason: CancelReason::UserRequested,
            partial_assistant_message: None,
        });
        assert!(matches!(
            app.session.runtime,
            RuntimePhase::Cancelled {
                reason: CancelReason::UserRequested,
                ..
            }
        ));
    }

    #[test]
    fn ctrl_d_queues_context_dry_run_command() {
        let mut app = ShellState::new(session_context());

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            KeyAction::TriggerContextDryRun
        );
        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::BuildContextDryRun]
        );
        assert_eq!(
            app.status_line.as_deref(),
            Some("Building context dry run…")
        );
    }

    #[test]
    fn bookmark_action_queues_toggle_for_selected_persisted_message() {
        let mut app = ShellState::new(session_context());
        app.hydrate(AppBootstrap {
            transcript: vec![TranscriptItem::persisted(
                "msg-1",
                "assistant",
                "hello",
                false,
            )],
            branches: vec![BranchItem::new("branch-a", "main", true)],
            status_line: None,
            draft: None,
            screen: None,
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
        });

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)),
            KeyAction::ToggleBookmark
        );
        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::ToggleBookmark {
                message_id: "msg-1".into()
            }]
        );
        assert_eq!(app.status_line.as_deref(), Some("Updating bookmark…"));
    }

    #[test]
    fn help_and_quit_actions_update_shell_state() {
        let mut app = ShellState::new(session_context());

        app.apply_action(KeyAction::ToggleHelp);
        assert_eq!(app.screen, ScreenState::Help);

        app.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(app.screen, ScreenState::Quit);
        assert!(app.should_quit);
        assert_eq!(
            app.take_pending_actions(),
            vec![KeyAction::ToggleHelp, KeyAction::ConfirmQuit]
        );
    }

    #[test]
    fn apply_runtime_progress_updates_partial_content_while_generating() {
        let mut app = ShellState::new(session_context());
        app.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-1".into(),
            user_message: TranscriptItem::new("user", "stream this"),
            context_preview: None,
            context_dry_run: None,
        });

        assert!(app.session.runtime.partial_content().is_none());
        assert_eq!(app.status_line.as_deref(), Some("Generation in progress"));

        app.apply_runtime_progress(RuntimeProgress {
            request_id: "req-1".into(),
            partial_content: "Hello, I am".into(),
        });

        assert_eq!(app.session.runtime.partial_content(), Some("Hello, I am"));

        app.apply_runtime_progress(RuntimeProgress {
            request_id: "req-1".into(),
            partial_content: "Hello, I am streaming text.".into(),
        });

        assert_eq!(
            app.session.runtime.partial_content(),
            Some("Hello, I am streaming text.")
        );
    }

    #[test]
    fn apply_runtime_progress_is_ignored_when_not_generating() {
        let mut app = ShellState::new(session_context());
        assert!(matches!(app.session.runtime, RuntimePhase::Idle));

        app.apply_runtime_progress(RuntimeProgress {
            request_id: "req-phantom".into(),
            partial_content: "should not stick".into(),
        });

        assert!(matches!(app.session.runtime, RuntimePhase::Idle));
    }

    #[test]
    fn apply_runtime_failure_transitions_to_failed_and_sets_status() {
        let mut app = ShellState::new(session_context());
        app.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-2".into(),
            user_message: TranscriptItem::new("user", "will fail"),
            context_preview: None,
            context_dry_run: None,
        });

        app.apply_runtime_failure(RuntimeFailure {
            request_id: "req-2".into(),
            message: "connection refused".into(),
        });

        assert!(matches!(
            app.session.runtime,
            RuntimePhase::Failed {
                ref message,
                ..
            } if message == "connection refused"
        ));
        assert_eq!(
            app.status_line.as_deref(),
            Some("Generation failed: connection refused")
        );
        assert!(!app.session.runtime.is_inflight());
    }

    #[test]
    fn apply_context_refresh_updates_status_and_previews() {
        let mut app = ShellState::new(session_context());
        app.apply_context_refresh(RuntimeContextRefresh {
            status_line: Some("Context dry run updated".into()),
            session_title: Some("Renamed Session".into()),
            transcript: Some(vec![TranscriptItem::persisted(
                "msg-1",
                "assistant",
                "hello",
                true,
            )]),
            session_metadata: Some(SessionMetadata {
                character_name: Some("Beatrice".into()),
                tags: vec!["story".into()],
            }),
            session_stats: Some(SessionStats {
                message_count: 1,
                branch_count: 1,
                bookmark_count: 1,
            }),
            context_preview: Some(ContextPreview {
                source: "transcript-fallback".into(),
                summary: "2 turns".into(),
                lines: vec!["user: hi".into()],
                selected_items: Some(2),
                omitted_items: Some(0),
                token_budget: None,
                inline_status: "transcript-fallback · 2 turns".into(),
            }),
            context_dry_run: Some(ContextDryRunPreview {
                summary: "2 turns".into(),
                built_at: 1_700_000_000_000,
            }),
        });

        assert_eq!(app.status_line.as_deref(), Some("Context dry run updated"));
        assert_eq!(app.session.context.title, "Renamed Session");
        assert_eq!(app.session.transcript.len(), 1);
        assert!(app.session.transcript[0].is_bookmarked);
        assert_eq!(
            app.session_metadata
                .as_ref()
                .and_then(|metadata| metadata.character_name.as_deref()),
            Some("Beatrice")
        );
        assert_eq!(
            app.session_stats.as_ref().map(|stats| stats.bookmark_count),
            Some(1)
        );
        assert!(app.context_preview.is_some());
        assert!(app.context_dry_run.is_some());
    }

    #[test]
    fn generation_poll_variants_are_equality_comparable() {
        use super::RuntimeCompletion;

        let completion = RuntimeCompletion {
            request_id: "r1".into(),
            assistant_message: TranscriptItem::new("assistant", "done"),
        };
        let poll = GenerationPoll::Completed(completion.clone());
        assert_eq!(poll, GenerationPoll::Completed(completion));

        let failure = RuntimeFailure {
            request_id: "r2".into(),
            message: "oops".into(),
        };
        let poll = GenerationPoll::Failed(failure.clone());
        assert_eq!(poll, GenerationPoll::Failed(failure));

        let pending = GenerationPoll::Pending {
            partial: Some(RuntimeProgress {
                request_id: "r3".into(),
                partial_content: "so far".into(),
            }),
        };
        assert_eq!(
            pending,
            GenerationPoll::Pending {
                partial: Some(RuntimeProgress {
                    request_id: "r3".into(),
                    partial_content: "so far".into(),
                })
            }
        );
    }
}
