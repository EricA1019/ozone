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
    pub author: String,
    pub content: String,
}

impl TranscriptItem {
    pub fn new(author: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            author: author.into(),
            content: content.into(),
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RuntimePhase {
    #[default]
    Idle,
    Queued {
        prompt: String,
    },
    Generating {
        request_id: String,
        prompt: String,
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
            | Self::Cancelled { prompt, .. } => Some(prompt),
            Self::Idle => None,
        }
    }

    fn request_id(&self) -> Option<&str> {
        match self {
            Self::Generating { request_id, .. } => Some(request_id.as_str()),
            Self::Cancelling { request_id, .. } | Self::Cancelled { request_id, .. } => {
                request_id.as_deref()
            }
            Self::Idle | Self::Queued { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    SendDraft { prompt: String },
    CancelGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSendReceipt {
    pub request_id: String,
    pub user_message: TranscriptItem,
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
        self.push_transcript_item(receipt.user_message);
        self.session.runtime = RuntimePhase::Generating {
            request_id: receipt.request_id,
            prompt,
        };
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some("Mock generation in progress".into());
    }

    pub fn apply_runtime_completion(&mut self, completion: RuntimeCompletion) {
        self.push_transcript_item(completion.assistant_message);
        self.session.runtime = RuntimePhase::Idle;
        self.inspector.focus = InspectorFocus::Message;
        self.status_line = Some("Mock generation completed".into());
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
        self.status_line = Some(format!(
            "Mock generation cancelled ({})",
            cancellation.reason
        ));
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

        self.history.push(prompt.clone());
        self.session.runtime = RuntimePhase::Queued {
            prompt: prompt.clone(),
        };
        self.runtime_commands
            .push(RuntimeCommand::SendDraft { prompt });
        self.draft = DraftState::default();
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.status_line = Some("Sending mock prompt…".into());
    }

    fn cancel_generation(&mut self) {
        if !self.session.runtime.is_inflight() {
            self.status_line = Some("No mock generation is active".into());
            return;
        }

        let prompt = self.session.runtime.prompt().unwrap_or_default().to_owned();
        let request_id = self.session.runtime.request_id().map(str::to_owned);

        self.session.runtime = RuntimePhase::Cancelling { request_id, prompt };
        self.runtime_commands.push(RuntimeCommand::CancelGeneration);
        self.status_line = Some("Cancelling mock generation…".into());
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
        AppBootstrap, BranchItem, DraftCheckpoint, DraftState, FocusTarget, InspectorFocus,
        RuntimeCancellation, RuntimeCommand, RuntimePhase, RuntimeSendReceipt, ScreenState,
        SessionContext, ShellState, TranscriptItem,
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
}
