use crossterm::event::KeyEvent;
use ozone_core::{engine::CancelReason, session::SessionId};

use crate::input::{dispatch_command_palette_key, dispatch_form_key, dispatch_key, dispatch_menu_key, InputMode, KeyAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    MainMenu,
    SessionList,
    CharacterManager,
    CharacterCreate,
    CharacterImport,
    Settings,
    Conversation,
    Help,
    Quit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsState {
    pub entries: Vec<SettingsEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEntry {
    pub category: String,
    pub key: String,
    pub value: String,
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
    Recall,
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
pub struct MenuItem {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub shortcut: Option<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuState {
    pub items: Vec<MenuItem>,
    pub selected: usize,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            items: vec![
                MenuItem {
                    id: "new-chat",
                    label: "New Chat",
                    description: "Start a fresh conversation session",
                    shortcut: Some('1'),
                },
                MenuItem {
                    id: "sessions",
                    label: "Sessions",
                    description: "Browse and resume existing conversations",
                    shortcut: Some('2'),
                },
                MenuItem {
                    id: "characters",
                    label: "Characters",
                    description: "Manage character cards and personas",
                    shortcut: Some('3'),
                },
                MenuItem {
                    id: "settings",
                    label: "Settings",
                    description: "Configure backend, model, and preferences",
                    shortcut: Some('4'),
                },
                MenuItem {
                    id: "quit",
                    label: "Quit",
                    description: "Exit ozone+",
                    shortcut: Some('q'),
                },
            ],
            selected: 0,
        }
    }
}

impl MenuState {
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        self.items.get(self.selected)
    }

    pub fn select_by_shortcut(&mut self, ch: char) -> bool {
        if let Some(index) = self.items.iter().position(|item| item.shortcut == Some(ch)) {
            self.selected = index;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntry {
    pub session_id: String,
    pub name: String,
    pub character_name: Option<String>,
    pub message_count: usize,
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionListState {
    pub entries: Vec<SessionListEntry>,
    pub selected: usize,
    pub filter: String,
    pub loading: bool,
}

impl SessionListState {
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let count = self.visible_count();
        if count > 0 && self.selected + 1 < count {
            self.selected += 1;
        }
    }

    pub fn visible_count(&self) -> usize {
        if self.filter.is_empty() {
            self.entries.len()
        } else {
            let lower = self.filter.to_lowercase();
            self.entries.iter().filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.character_name.as_deref().unwrap_or("").to_lowercase().contains(&lower)
            }).count()
        }
    }

    pub fn visible_entries(&self) -> Vec<&SessionListEntry> {
        if self.filter.is_empty() {
            self.entries.iter().collect()
        } else {
            let lower = self.filter.to_lowercase();
            self.entries.iter().filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.character_name.as_deref().unwrap_or("").to_lowercase().contains(&lower)
            }).collect()
        }
    }

    pub fn selected_entry(&self) -> Option<&SessionListEntry> {
        let visible = self.visible_entries();
        visible.get(self.selected).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEntry {
    pub card_id: String,
    pub name: String,
    pub description: String,
    pub session_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterListState {
    pub entries: Vec<CharacterEntry>,
    pub selected: usize,
}

impl CharacterListState {
    pub fn selected_entry(&self) -> Option<&CharacterEntry> {
        self.entries.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterFormField {
    #[default]
    Name,
    SystemPrompt,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterCreateState {
    pub name: DraftState,
    pub system_prompt: DraftState,
    pub active_field: CharacterFormField,
}

impl CharacterCreateState {
    pub fn active_draft(&self) -> &DraftState {
        match self.active_field {
            CharacterFormField::Name => &self.name,
            CharacterFormField::SystemPrompt => &self.system_prompt,
        }
    }

    pub fn active_draft_mut(&mut self) -> &mut DraftState {
        match self.active_field {
            CharacterFormField::Name => &mut self.name,
            CharacterFormField::SystemPrompt => &mut self.system_prompt,
        }
    }

    pub fn toggle_field(&mut self) {
        self.active_field = match self.active_field {
            CharacterFormField::Name => CharacterFormField::SystemPrompt,
            CharacterFormField::SystemPrompt => CharacterFormField::Name,
        };
    }
}

/// State for the file-path import prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterImportState {
    pub path: DraftState,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallBrowser {
    pub title: String,
    pub summary: String,
    pub lines: Vec<String>,
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
    TogglePinnedMemory { message_id: String },
    RunCommand { input: String },
    CreateCharacter { name: String, system_prompt: String },
    ImportCharacter { path: String },
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
    pub recall_browser: Option<RecallBrowser>,
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
    pub recall_browser: Option<RecallBrowser>,
}

// ── Command palette ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEntry {
    pub name: String,
    pub alias: Vec<String>,
    pub description: String,
}

impl CommandEntry {
    pub fn all() -> Vec<CommandEntry> {
        vec![
            CommandEntry { name: "new".into(), alias: vec!["n".into()], description: "Start new chat".into() },
            CommandEntry { name: "sessions".into(), alias: vec!["s".into()], description: "Browse sessions".into() },
            CommandEntry { name: "characters".into(), alias: vec!["c".into()], description: "Manage characters".into() },
            CommandEntry { name: "settings".into(), alias: vec![], description: "Open settings".into() },
            CommandEntry { name: "session show".into(), alias: vec![], description: "Show session metadata".into() },
            CommandEntry { name: "session rename".into(), alias: vec![], description: "Rename current session".into() },
            CommandEntry { name: "session character".into(), alias: vec![], description: "Set session character".into() },
            CommandEntry { name: "memory list".into(), alias: vec![], description: "List pinned memories".into() },
            CommandEntry { name: "memory note".into(), alias: vec![], description: "Create a note memory".into() },
            CommandEntry { name: "search session".into(), alias: vec![], description: "Search this session".into() },
            CommandEntry { name: "search global".into(), alias: vec![], description: "Search all sessions".into() },
            CommandEntry { name: "help".into(), alias: vec!["h".into(), "?".into()], description: "Show help".into() },
            CommandEntry { name: "quit".into(), alias: vec!["q".into()], description: "Quit / back to menu".into() },
            CommandEntry { name: "menu".into(), alias: vec!["m".into()], description: "Return to main menu".into() },
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandPaletteState {
    pub open: bool,
    pub input: String,
    pub selected: usize,
}

impl CommandPaletteState {
    pub fn open(&mut self) {
        self.open = true;
        self.input.clear();
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.input.clear();
        self.selected = 0;
    }

    /// Return commands matching the current input prefix (case-insensitive).
    pub fn filtered_commands(&self) -> Vec<CommandEntry> {
        let all = CommandEntry::all();
        if self.input.is_empty() {
            return all;
        }
        let query = self.input.to_lowercase();
        all.into_iter()
            .filter(|c| c.name.to_lowercase().contains(&query) || c.alias.iter().any(|a| a.contains(&query)))
            .collect()
    }

    pub fn selected_command(&self) -> Option<CommandEntry> {
        let cmds = self.filtered_commands();
        cmds.into_iter().nth(self.selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellState {
    pub screen: ScreenState,
    pub input_mode: InputMode,
    pub focus: FocusTarget,
    pub inspector: InspectorState,
    pub menu: MenuState,
    pub session_list: SessionListState,
    pub character_list: CharacterListState,
    pub character_create: CharacterCreateState,
    pub character_import: CharacterImportState,
    pub settings: SettingsState,
    pub session: SessionState,
    pub draft: DraftState,
    pub history: InputHistoryState,
    pub status_line: Option<String>,
    pub session_metadata: Option<SessionMetadata>,
    pub session_stats: Option<SessionStats>,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
    pub recall_browser: Option<RecallBrowser>,
    pub pending_actions: Vec<KeyAction>,
    pub runtime_commands: Vec<RuntimeCommand>,
    pub should_quit: bool,
    pub command_palette: CommandPaletteState,
}

impl ShellState {
    pub fn new(context: SessionContext) -> Self {
        Self {
            screen: ScreenState::MainMenu,
            input_mode: InputMode::Normal,
            focus: FocusTarget::Transcript,
            inspector: InspectorState::default(),
            menu: MenuState::default(),
            session_list: SessionListState::default(),
            character_list: CharacterListState::default(),
            character_create: CharacterCreateState::default(),
            character_import: CharacterImportState::default(),
            settings: SettingsState::default(),
            session: SessionState::new(context),
            draft: DraftState::default(),
            history: InputHistoryState::default(),
            status_line: Some("ozone+ TUI shell skeleton ready".into()),
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            pending_actions: Vec::new(),
            runtime_commands: Vec::new(),
            should_quit: false,
            command_palette: CommandPaletteState::default(),
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
        self.recall_browser = bootstrap.recall_browser;
    }

    /// Transition from a menu screen into the conversation view for the current session.
    pub fn enter_conversation(&mut self) {
        self.screen = ScreenState::Conversation;
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Normal;
    }

    /// Return to the main menu from any screen.
    pub fn return_to_menu(&mut self) {
        self.screen = ScreenState::MainMenu;
        self.input_mode = InputMode::Normal;
        self.focus = FocusTarget::Transcript;
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> KeyAction {
        // Command palette takes priority when open
        if self.command_palette.open {
            if let Some(action) = dispatch_command_palette_key(key) {
                self.apply_action(action);
                return action;
            }
            return KeyAction::Noop;
        }

        let action = match self.screen {
            ScreenState::CharacterManager => {
                // Intercept n/i for create/import before normal menu dispatch
                match key.code {
                    crossterm::event::KeyCode::Char('n') => KeyAction::CharacterCreate,
                    crossterm::event::KeyCode::Char('i') => KeyAction::CharacterImportPrompt,
                    _ => dispatch_menu_key(key, false),
                }
            }
            ScreenState::CharacterCreate | ScreenState::CharacterImport => {
                dispatch_form_key(key)
            }
            ScreenState::MainMenu
            | ScreenState::SessionList
            | ScreenState::Settings => {
                let is_root = self.screen == ScreenState::MainMenu;
                dispatch_menu_key(key, is_root)
            }
            ScreenState::Conversation | ScreenState::Help | ScreenState::Quit => {
                dispatch_key(self.input_mode, key)
            }
        };
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
            KeyAction::TogglePinnedMemory => self.trigger_pinned_memory_toggle(),
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
                    ScreenState::Conversation => ScreenState::Help,
                    other => other,
                };
            }
            KeyAction::ConfirmQuit => {
                match self.screen {
                    ScreenState::MainMenu => {
                        self.screen = ScreenState::Quit;
                        self.should_quit = true;
                    }
                    ScreenState::Conversation | ScreenState::Help => {
                        self.return_to_menu();
                        self.status_line = Some("Returned to main menu".into());
                    }
                    _ => {
                        self.return_to_menu();
                        self.status_line = Some("Returned to main menu".into());
                    }
                }
            }
            KeyAction::MenuUp => {
                match self.screen {
                    ScreenState::MainMenu => self.menu.move_up(),
                    ScreenState::SessionList => self.session_list.move_up(),
                    ScreenState::CharacterManager => self.character_list.move_up(),
                    _ => {}
                }
            }
            KeyAction::MenuDown => {
                match self.screen {
                    ScreenState::MainMenu => self.menu.move_down(),
                    ScreenState::SessionList => self.session_list.move_down(),
                    ScreenState::CharacterManager => self.character_list.move_down(),
                    _ => {}
                }
            }
            KeyAction::MenuSelect => {
                match self.screen {
                    ScreenState::MainMenu => {
                        if let Some(item) = self.menu.selected_item() {
                            match item.id {
                                "new-chat" => {
                                    self.enter_conversation();
                                    self.status_line = Some("New conversation started".into());
                                }
                                "sessions" => {
                                    self.screen = ScreenState::SessionList;
                                    self.status_line =
                                        Some("Loading sessions…".into());
                                }
                                "characters" => {
                                    self.screen = ScreenState::CharacterManager;
                                    self.status_line =
                                        Some("Browsing characters".into());
                                }
                                "settings" => {
                                    self.screen = ScreenState::Settings;
                                    self.status_line =
                                        Some("Viewing settings".into());
                                }
                                "quit" => {
                                    self.screen = ScreenState::Quit;
                                    self.should_quit = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::SessionList => {
                        if let Some(entry) = self.session_list.selected_entry() {
                            self.status_line = Some(format!("Opening: {}", entry.name));
                            self.enter_conversation();
                        }
                    }
                    _ => {}
                }
            }
            KeyAction::MenuBack => {
                match self.screen {
                    ScreenState::SessionList
                    | ScreenState::CharacterManager
                    | ScreenState::Settings
                    | ScreenState::Conversation => {
                        self.screen = ScreenState::MainMenu;
                        self.status_line = Some("Returned to main menu".into());
                    }
                    ScreenState::MainMenu => {
                        // Already at top level — do nothing
                    }
                    _ => {}
                }
            }
            KeyAction::MenuShortcut(ch) => {
                if self.screen == ScreenState::MainMenu
                    && self.menu.select_by_shortcut(ch)
                {
                    self.apply_action(KeyAction::MenuSelect);
                }
            }
            KeyAction::OpenCommandPalette => {
                self.command_palette.open();
            }
            KeyAction::CommandPaletteClose => {
                self.command_palette.close();
            }
            KeyAction::CommandPaletteInput(c) => {
                self.command_palette.input.push(c);
                self.command_palette.selected = 0;
            }
            KeyAction::CommandPaletteBackspace => {
                self.command_palette.input.pop();
                self.command_palette.selected = 0;
            }
            KeyAction::CommandPaletteUp => {
                if self.command_palette.selected > 0 {
                    self.command_palette.selected -= 1;
                }
            }
            KeyAction::CommandPaletteDown => {
                let count = self.command_palette.filtered_commands().len();
                if self.command_palette.selected + 1 < count {
                    self.command_palette.selected += 1;
                }
            }
            KeyAction::CommandPaletteSelect => {
                if let Some(cmd) = self.command_palette.selected_command() {
                    self.command_palette.close();
                    self.execute_command(&cmd.name);
                }
            }
            // Character form actions
            KeyAction::CharacterCreate => {
                self.character_create = CharacterCreateState::default();
                self.screen = ScreenState::CharacterCreate;
            }
            KeyAction::CharacterImportPrompt => {
                self.character_import = CharacterImportState::default();
                self.screen = ScreenState::CharacterImport;
            }
            KeyAction::FormInsertChar(ch) => match self.screen {
                ScreenState::CharacterCreate => {
                    self.character_create.active_draft_mut().insert_char(ch);
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.insert_char(ch);
                }
                _ => {}
            },
            KeyAction::FormBackspace => match self.screen {
                ScreenState::CharacterCreate => {
                    self.character_create.active_draft_mut().backspace();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.backspace();
                }
                _ => {}
            },
            KeyAction::FormMoveCursorLeft => match self.screen {
                ScreenState::CharacterCreate => {
                    self.character_create.active_draft_mut().move_cursor_left();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.move_cursor_left();
                }
                _ => {}
            },
            KeyAction::FormMoveCursorRight => match self.screen {
                ScreenState::CharacterCreate => {
                    self.character_create.active_draft_mut().move_cursor_right();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.move_cursor_right();
                }
                _ => {}
            },
            KeyAction::FormToggleField => {
                if self.screen == ScreenState::CharacterCreate {
                    self.character_create.toggle_field();
                }
            }
            KeyAction::FormSubmit => match self.screen {
                ScreenState::CharacterCreate => {
                    let name = self.character_create.name.text.trim().to_string();
                    if name.is_empty() {
                        self.status_line = Some("Character name cannot be empty".into());
                    } else {
                        let system_prompt =
                            self.character_create.system_prompt.text.trim().to_string();
                        self.runtime_commands.push(RuntimeCommand::CreateCharacter {
                            name,
                            system_prompt,
                        });
                        self.screen = ScreenState::CharacterManager;
                    }
                }
                ScreenState::CharacterImport => {
                    let path = self.character_import.path.text.trim().to_string();
                    if path.is_empty() {
                        self.status_line = Some("File path cannot be empty".into());
                    } else {
                        self.runtime_commands
                            .push(RuntimeCommand::ImportCharacter { path });
                        self.screen = ScreenState::CharacterManager;
                    }
                }
                _ => {}
            },
            KeyAction::FormCancel => match self.screen {
                ScreenState::CharacterCreate | ScreenState::CharacterImport => {
                    self.screen = ScreenState::CharacterManager;
                }
                _ => {}
            },
        }
    }

    fn execute_command(&mut self, name: &str) {
        match name {
            "new" => {
                self.enter_conversation();
                self.status_line = Some("New conversation".into());
            }
            "sessions" => {
                self.screen = ScreenState::SessionList;
            }
            "characters" => {
                self.screen = ScreenState::CharacterManager;
            }
            "settings" => {
                self.screen = ScreenState::Settings;
            }
            "help" => {
                self.screen = ScreenState::Help;
            }
            "quit" => {
                match self.screen {
                    ScreenState::MainMenu => {
                        self.screen = ScreenState::Quit;
                        self.should_quit = true;
                    }
                    _ => self.return_to_menu(),
                }
            }
            "menu" => {
                self.return_to_menu();
            }
            // Slash commands: inject into draft and submit
            cmd if cmd.starts_with("session ")
                || cmd.starts_with("memory ")
                || cmd.starts_with("search ") =>
            {
                self.enter_conversation();
                self.draft.text = format!("/{cmd}");
                self.draft.cursor = self.draft.text.len();
                self.draft.dirty = true;
                self.status_line = Some(format!("/{cmd} — press Enter to run or keep typing"));
            }
            _ => {
                self.status_line = Some(format!("Unknown command: {}", name));
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
        if let Some(recall_browser) = refresh.recall_browser {
            self.recall_browser = Some(recall_browser);
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

        if is_shell_command(&prompt) {
            self.history.push(prompt.clone());
            self.runtime_commands
                .push(RuntimeCommand::RunCommand { input: prompt });
            self.draft = DraftState::default();
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Insert;
            self.status_line = Some("Running shell command…".into());
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

    fn trigger_pinned_memory_toggle(&mut self) {
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
        self.inspector.focus = InspectorFocus::Recall;
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

fn is_shell_command(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    trimmed.starts_with('/') || trimmed.starts_with(':')
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ozone_core::engine::CancelReason;
    use ozone_core::session::SessionId;

    use super::{
        AppBootstrap, BranchItem, CharacterEntry, CharacterListState, ContextDryRunPreview,
        ContextPreview, DraftCheckpoint, DraftState, FocusTarget, GenerationPoll, InspectorFocus,
        RecallBrowser, RuntimeCancellation, RuntimeCommand, RuntimeContextRefresh, RuntimeFailure,
        RuntimePhase, RuntimeProgress, RuntimeSendReceipt, ScreenState, SessionContext,
        SessionListEntry, SessionMetadata, SessionStats, ShellState, TranscriptItem,
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
            recall_browser: None,
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
        app.enter_conversation();

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.focus, FocusTarget::Draft);

        // Tab from Draft (Normal) → enters Insert mode (stays on Draft)
        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            KeyAction::FocusDraft
        );
        assert_eq!(app.input_mode, InputMode::Insert);
        assert_eq!(app.focus, FocusTarget::Draft);

        // Tab from Draft (Insert) → focus Transcript
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
        app.enter_conversation();

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
    fn shell_commands_route_to_runtime_without_queuing_generation() {
        let mut app = ShellState::new(session_context());
        app.enter_conversation();

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
        assert_eq!(app.status_line.as_deref(), Some("Running shell command…"));

        app.apply_action(KeyAction::EnterInsert);
        for ch in ":memories".chars() {
            app.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::RunCommand {
                input: ":memories".into()
            }]
        );
        assert!(matches!(app.session.runtime, RuntimePhase::Idle));
    }

    #[test]
    fn ctrl_c_queues_cancel_for_active_generation() {
        let mut app = ShellState::new(session_context());
        app.enter_conversation();

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
        app.enter_conversation();

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
        app.enter_conversation();

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
    fn ctrl_k_queues_pinned_memory_toggle_for_selected_persisted_message() {
        let mut app = ShellState::new(session_context());
        app.enter_conversation();
        app.hydrate(AppBootstrap {
            transcript: vec![TranscriptItem::persisted(
                "msg-1",
                "assistant",
                "pin me",
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
            recall_browser: None,
        });

        assert_eq!(
            app.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            KeyAction::TogglePinnedMemory
        );
        assert_eq!(
            app.take_runtime_commands(),
            vec![RuntimeCommand::TogglePinnedMemory {
                message_id: "msg-1".into()
            }]
        );
        assert_eq!(app.status_line.as_deref(), Some("Updating pinned memory…"));
        assert_eq!(app.inspector.focus, InspectorFocus::Recall);
    }

    #[test]
    fn bookmark_action_queues_toggle_for_selected_persisted_message() {
        let mut app = ShellState::new(session_context());
        app.enter_conversation();
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
            recall_browser: None,
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
        app.enter_conversation();

        // Toggle help from conversation
        app.apply_action(KeyAction::ToggleHelp);
        assert_eq!(app.screen, ScreenState::Help);

        // ConfirmQuit from Help returns to main menu instead of quitting
        app.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(app.screen, ScreenState::MainMenu);
        assert!(!app.should_quit);

        // ConfirmQuit from MainMenu actually quits
        app.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(app.screen, ScreenState::Quit);
        assert!(app.should_quit);
        assert_eq!(
            app.take_pending_actions(),
            vec![
                KeyAction::ToggleHelp,
                KeyAction::ConfirmQuit,
                KeyAction::ConfirmQuit,
            ]
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
            recall_browser: Some(RecallBrowser {
                title: "Recall".into(),
                summary: "1 active · 2 recent hits".into(),
                lines: vec!["active pinned 1".into()],
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
        assert_eq!(
            app.recall_browser
                .as_ref()
                .map(|browser| browser.summary.as_str()),
            Some("1 active · 2 recent hits")
        );
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

    #[test]
    fn session_list_navigation() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);

        // Navigate to sessions (item index 1)
        state.menu.selected = 1; // "Sessions"
        state.apply_action(KeyAction::MenuSelect);
        assert_eq!(state.screen, ScreenState::SessionList);

        // Add an entry and select it
        state.session_list.entries = vec![SessionListEntry {
            session_id: "test-1".into(),
            name: "Test Session".into(),
            character_name: None,
            message_count: 5,
            last_active: None,
        }];
        state.session_list.selected = 0;

        // Press enter to open
        state.apply_action(KeyAction::MenuSelect);
        assert_eq!(state.screen, ScreenState::Conversation);

        // Esc returns to menu
        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
    }

    // ── Menu navigation tests ──────────────────────────────────────────

    #[test]
    fn menu_navigation_up_down() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.menu.selected, 0);

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.menu.selected, 1);

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.menu.selected, 2);

        state.apply_action(KeyAction::MenuUp);
        assert_eq!(state.menu.selected, 1);

        // Up from 0 should stay at 0
        state.menu.selected = 0;
        state.apply_action(KeyAction::MenuUp);
        assert_eq!(state.menu.selected, 0);
    }

    #[test]
    fn menu_navigation_clamps_at_bounds() {
        let mut state = ShellState::new(session_context());
        let max = state.menu.items.len() - 1;

        for _ in 0..20 {
            state.apply_action(KeyAction::MenuDown);
        }
        assert_eq!(state.menu.selected, max);

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.menu.selected, max);
    }

    #[test]
    fn menu_shortcut_selects_correct_item() {
        let mut state = ShellState::new(session_context());

        // Shortcut '2' selects "Sessions" and triggers MenuSelect → SessionList
        state.apply_action(KeyAction::MenuShortcut('2'));
        assert_eq!(state.screen, ScreenState::SessionList);
    }

    // ── Screen transition tests ────────────────────────────────────────

    #[test]
    fn new_chat_enters_conversation() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);

        // "New Chat" is at index 0 (default selected)
        state.apply_action(KeyAction::MenuSelect);
        assert_eq!(state.screen, ScreenState::Conversation);
    }

    #[test]
    fn sessions_menu_enters_session_list() {
        let mut state = ShellState::new(session_context());
        state.menu.selected = 1; // "Sessions"
        state.apply_action(KeyAction::MenuSelect);
        assert_eq!(state.screen, ScreenState::SessionList);
    }

    #[test]
    fn back_from_session_list_returns_to_menu() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::SessionList;
        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
    }

    #[test]
    fn back_from_conversation_returns_to_menu() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        assert_eq!(state.screen, ScreenState::Conversation);

        state.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    // ── Quit behavior tests ───────────────────────────────────────────

    #[test]
    fn quit_from_menu_exits_app() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);

        state.menu.selected = 4; // "Quit"
        state.apply_action(KeyAction::MenuSelect);
        assert!(state.should_quit);
    }

    #[test]
    fn quit_shortcut_from_menu_exits() {
        let mut state = ShellState::new(session_context());
        state.apply_action(KeyAction::MenuShortcut('q'));
        assert!(state.should_quit);
    }

    #[test]
    fn confirm_quit_from_conversation_returns_to_menu() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    // ── SessionListState tests ────────────────────────────────────────

    #[test]
    fn session_list_filter_narrows_entries() {
        let mut state = ShellState::new(session_context());
        state.session_list.entries = vec![
            SessionListEntry {
                session_id: "1".into(),
                name: "Alpha Chat".into(),
                character_name: Some("Bot".into()),
                message_count: 10,
                last_active: None,
            },
            SessionListEntry {
                session_id: "2".into(),
                name: "Beta Chat".into(),
                character_name: None,
                message_count: 5,
                last_active: None,
            },
        ];

        state.session_list.filter = "alpha".into();
        let visible = state.session_list.visible_entries();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "Alpha Chat");
    }

    #[test]
    fn session_list_selected_entry_returns_correct() {
        let mut state = ShellState::new(session_context());
        state.session_list.entries = vec![
            SessionListEntry {
                session_id: "1".into(),
                name: "First".into(),
                character_name: None,
                message_count: 0,
                last_active: None,
            },
            SessionListEntry {
                session_id: "2".into(),
                name: "Second".into(),
                character_name: None,
                message_count: 0,
                last_active: None,
            },
        ];
        state.session_list.selected = 1;
        let entry = state.session_list.selected_entry();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "Second");
    }

    // ── Command palette tests ─────────────────────────────────────────

    #[test]
    fn command_palette_opens_and_closes() {
        let mut state = ShellState::new(session_context());
        assert!(!state.command_palette.open);

        state.apply_action(KeyAction::OpenCommandPalette);
        assert!(state.command_palette.open);

        state.apply_action(KeyAction::CommandPaletteClose);
        assert!(!state.command_palette.open);
    }

    #[test]
    fn command_palette_filters_commands() {
        let mut state = ShellState::new(session_context());
        state.command_palette.open();
        state.command_palette.input = "ses".into();
        let filtered = state.command_palette.filtered_commands();
        assert!(filtered.len() >= 1);
        assert!(filtered.iter().any(|c| c.name == "sessions"));

        // More specific filter
        state.command_palette.input = "settings".into();
        let filtered = state.command_palette.filtered_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "settings");
    }

    #[test]
    fn command_palette_executes_command() {
        let mut state = ShellState::new(session_context());
        state.apply_action(KeyAction::OpenCommandPalette);
        state.apply_action(KeyAction::CommandPaletteInput('n'));
        state.apply_action(KeyAction::CommandPaletteInput('e'));
        state.apply_action(KeyAction::CommandPaletteInput('w'));
        state.apply_action(KeyAction::CommandPaletteSelect);
        assert!(!state.command_palette.open);
        assert_eq!(state.screen, ScreenState::Conversation);
    }

    #[test]
    fn command_palette_navigation() {
        let mut state = ShellState::new(session_context());
        state.apply_action(KeyAction::OpenCommandPalette);
        assert_eq!(state.command_palette.selected, 0);

        state.apply_action(KeyAction::CommandPaletteDown);
        assert_eq!(state.command_palette.selected, 1);

        state.apply_action(KeyAction::CommandPaletteUp);
        assert_eq!(state.command_palette.selected, 0);

        // Up at 0 stays at 0
        state.apply_action(KeyAction::CommandPaletteUp);
        assert_eq!(state.command_palette.selected, 0);
    }

    #[test]
    fn command_palette_backspace_resets_selection() {
        let mut state = ShellState::new(session_context());
        state.apply_action(KeyAction::OpenCommandPalette);
        state.apply_action(KeyAction::CommandPaletteInput('s'));
        state.apply_action(KeyAction::CommandPaletteDown);
        assert!(state.command_palette.selected > 0 || state.command_palette.filtered_commands().len() <= 1);

        state.apply_action(KeyAction::CommandPaletteBackspace);
        assert_eq!(state.command_palette.selected, 0);
        assert!(state.command_palette.input.is_empty());
    }

    #[test]
    fn command_palette_intercepts_keys_in_handle_key_event() {
        let mut state = ShellState::new(session_context());
        state.apply_action(KeyAction::OpenCommandPalette);
        assert!(state.command_palette.open);

        // Typing should go to palette, not menu
        let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::CommandPaletteInput('h'));
        assert_eq!(state.command_palette.input, "h");

        // Esc should close palette, not quit
        let action = state.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, KeyAction::CommandPaletteClose);
        assert!(!state.command_palette.open);
    }

    #[test]
    fn slash_opens_command_palette_from_menu() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::OpenCommandPalette);
        assert!(state.command_palette.open);
    }

    #[test]
    fn slash_opens_command_palette_from_conversation_normal_mode() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        assert_eq!(state.input_mode, InputMode::Normal);

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::OpenCommandPalette);
        assert!(state.command_palette.open);
    }

    #[test]
    fn slash_does_not_open_palette_in_insert_mode() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.apply_action(KeyAction::EnterInsert);
        assert_eq!(state.input_mode, InputMode::Insert);

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::DraftInsertChar('/'));
        assert!(!state.command_palette.open);
    }

    #[test]
    fn command_palette_quit_from_menu_quits() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);
        state.apply_action(KeyAction::OpenCommandPalette);
        state.command_palette.input = "quit".into();
        state.apply_action(KeyAction::CommandPaletteSelect);
        assert!(state.should_quit);
    }

    #[test]
    fn command_palette_quit_from_conversation_returns_to_menu() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.apply_action(KeyAction::OpenCommandPalette);
        state.command_palette.input = "quit".into();
        state.apply_action(KeyAction::CommandPaletteSelect);
        assert!(!state.should_quit);
        assert_eq!(state.screen, ScreenState::MainMenu);
    }

    // ── CharacterListState tests ──────────────────────────────────────

    fn sample_characters() -> Vec<CharacterEntry> {
        vec![
            CharacterEntry {
                card_id: "c1".into(),
                name: "Alice".into(),
                description: "First".into(),
                session_count: 3,
            },
            CharacterEntry {
                card_id: "c2".into(),
                name: "Bob".into(),
                description: "Second".into(),
                session_count: 1,
            },
            CharacterEntry {
                card_id: "c3".into(),
                name: "Carol".into(),
                description: "Third".into(),
                session_count: 0,
            },
        ]
    }

    #[test]
    fn test_character_list_move_down() {
        let mut list = CharacterListState {
            entries: sample_characters(),
            selected: 0,
        };

        list.move_down();
        assert_eq!(list.selected, 1);

        list.move_down();
        assert_eq!(list.selected, 2);

        // At last entry, move_down clamps
        list.move_down();
        assert_eq!(list.selected, 2);
    }

    #[test]
    fn test_character_list_move_up() {
        let mut list = CharacterListState {
            entries: sample_characters(),
            selected: 2,
        };

        list.move_up();
        assert_eq!(list.selected, 1);

        list.move_up();
        assert_eq!(list.selected, 0);

        // At first entry, move_up clamps
        list.move_up();
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn test_character_list_selected_entry() {
        let list = CharacterListState {
            entries: sample_characters(),
            selected: 1,
        };

        let entry = list.selected_entry().unwrap();
        assert_eq!(entry.name, "Bob");
        assert_eq!(entry.card_id, "c2");

        // Empty list returns None
        let empty = CharacterListState::default();
        assert!(empty.selected_entry().is_none());
    }

    #[test]
    fn test_characters_screen_nav() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterManager;
        state.character_list.entries = sample_characters();
        state.character_list.selected = 0;

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.character_list.selected, 1);

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.character_list.selected, 2);

        state.apply_action(KeyAction::MenuUp);
        assert_eq!(state.character_list.selected, 1);
    }

    // ── Settings screen tests ─────────────────────────────────────────

    #[test]
    fn test_settings_screen_routing() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);

        // Shortcut '4' navigates to Settings
        state.apply_action(KeyAction::MenuShortcut('4'));
        assert_eq!(state.screen, ScreenState::Settings);
    }

    #[test]
    fn test_settings_screen_escape_returns() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;

        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
    }

    // ── All menu screens reachable ────────────────────────────────────

    #[test]
    fn test_all_menu_screens_reachable() {
        // Shortcut '1' → Conversation (New Chat)
        let mut s1 = ShellState::new(session_context());
        s1.apply_action(KeyAction::MenuShortcut('1'));
        assert_eq!(s1.screen, ScreenState::Conversation);

        // Shortcut '2' → SessionList
        let mut s2 = ShellState::new(session_context());
        s2.apply_action(KeyAction::MenuShortcut('2'));
        assert_eq!(s2.screen, ScreenState::SessionList);

        // Shortcut '3' → CharacterManager
        let mut s3 = ShellState::new(session_context());
        s3.apply_action(KeyAction::MenuShortcut('3'));
        assert_eq!(s3.screen, ScreenState::CharacterManager);

        // Shortcut '4' → Settings
        let mut s4 = ShellState::new(session_context());
        s4.apply_action(KeyAction::MenuShortcut('4'));
        assert_eq!(s4.screen, ScreenState::Settings);

        // Shortcut 'q' → Quit
        let mut sq = ShellState::new(session_context());
        sq.apply_action(KeyAction::MenuShortcut('q'));
        assert!(sq.should_quit);
        assert_eq!(sq.screen, ScreenState::Quit);
    }

    // ── Sub-screen q-back regression tests ────────────────────────────

    #[test]
    fn q_from_session_list_goes_back() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::SessionList;
        state.apply_action(KeyAction::MenuBack); // simulates q on sub-screen
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    #[test]
    fn q_from_characters_goes_back() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterManager;
        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    #[test]
    fn q_from_settings_goes_back() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    #[test]
    fn confirm_quit_from_sub_screen_returns_to_menu() {
        // Even if ConfirmQuit somehow reaches a sub-screen, it should go back
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        state.apply_action(KeyAction::ConfirmQuit);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    #[test]
    fn q_from_main_menu_still_quits() {
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);
        state.apply_action(KeyAction::ConfirmQuit);
        assert!(state.should_quit);
        assert_eq!(state.screen, ScreenState::Quit);
    }

    #[test]
    fn composer_show_cursor_when_draft_focused() {
        use crate::layout::build_layout;
        use crate::render::build_render_model;

        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        let layout = build_layout(&state);

        // Draft focused in Normal mode — cursor shown.
        let model = build_render_model(&state, &layout);
        assert!(model.composer.show_cursor);

        // Insert mode with draft focus — cursor still shown.
        state.apply_action(KeyAction::EnterInsert);
        let model = build_render_model(&state, &layout);
        assert!(model.composer.show_cursor);

        // Focus on Transcript — cursor hidden.
        state.focus = FocusTarget::Transcript;
        let layout = build_layout(&state);
        let model = build_render_model(&state, &layout);
        assert!(!model.composer.show_cursor);
    }

    #[test]
    fn slash_suggestions_appear_when_draft_starts_with_slash() {
        use crate::layout::build_layout;
        use crate::render::build_render_model;

        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.apply_action(KeyAction::EnterInsert);
        let layout = build_layout(&state);

        // Empty draft — no suggestions.
        let model = build_render_model(&state, &layout);
        assert!(model.composer.slash_suggestions.is_empty());

        // Type `/` — suggestions appear.
        state.apply_action(KeyAction::DraftInsertChar('/'));
        let model = build_render_model(&state, &layout);
        assert!(!model.composer.slash_suggestions.is_empty());
        assert!(model.composer.slash_suggestions.iter().any(|s| s.name == "/help"));

        // Type `/he` — filtered suggestions.
        state.apply_action(KeyAction::DraftInsertChar('h'));
        state.apply_action(KeyAction::DraftInsertChar('e'));
        let model = build_render_model(&state, &layout);
        assert!(model.composer.slash_suggestions.iter().all(|s| s.name.contains("help")));

        // Type a space — suggestions disappear (command complete).
        state.apply_action(KeyAction::DraftInsertChar('l'));
        state.apply_action(KeyAction::DraftInsertChar('p'));
        state.apply_action(KeyAction::DraftInsertChar(' '));
        let model = build_render_model(&state, &layout);
        assert!(model.composer.slash_suggestions.is_empty());
    }

    #[test]
    fn normal_text_has_no_slash_suggestions() {
        use crate::layout::build_layout;
        use crate::render::build_render_model;

        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.apply_action(KeyAction::EnterInsert);
        state.apply_action(KeyAction::DraftInsertChar('h'));
        state.apply_action(KeyAction::DraftInsertChar('i'));

        let layout = build_layout(&state);
        let model = build_render_model(&state, &layout);
        assert!(model.composer.slash_suggestions.is_empty());
    }

    // ── Character form tests ──────────────────────────────────────

    #[test]
    fn n_key_opens_character_create_form() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterManager;
        state.apply_action(KeyAction::CharacterCreate);
        assert_eq!(state.screen, ScreenState::CharacterCreate);
        assert!(state.character_create.name.text.is_empty());
        assert!(state.character_create.system_prompt.text.is_empty());
    }

    #[test]
    fn character_create_form_typing_updates_name() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterCreate;
        state.apply_action(KeyAction::FormInsertChar('H'));
        state.apply_action(KeyAction::FormInsertChar('i'));
        assert_eq!(state.character_create.name.text, "Hi");
        assert_eq!(state.character_create.name.cursor, 2);
    }

    #[test]
    fn character_create_tab_switches_field() {
        use super::CharacterFormField;
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterCreate;
        assert_eq!(state.character_create.active_field, CharacterFormField::Name);
        state.apply_action(KeyAction::FormToggleField);
        assert_eq!(state.character_create.active_field, CharacterFormField::SystemPrompt);
        state.apply_action(KeyAction::FormToggleField);
        assert_eq!(state.character_create.active_field, CharacterFormField::Name);
    }

    #[test]
    fn character_create_esc_cancels() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterCreate;
        state.apply_action(KeyAction::FormCancel);
        assert_eq!(state.screen, ScreenState::CharacterManager);
    }

    #[test]
    fn character_create_enter_submits() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterCreate;
        for c in "TestChar".chars() {
            state.apply_action(KeyAction::FormInsertChar(c));
        }
        state.apply_action(KeyAction::FormSubmit);
        let cmds = state.take_runtime_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            RuntimeCommand::CreateCharacter { name, system_prompt } => {
                assert_eq!(name, "TestChar");
                assert!(system_prompt.is_empty());
            }
            other => panic!("Expected CreateCharacter, got {:?}", other),
        }
        assert_eq!(state.screen, ScreenState::CharacterManager);
    }

    #[test]
    fn character_create_empty_name_rejected() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterCreate;
        state.apply_action(KeyAction::FormSubmit);
        assert!(state.take_runtime_commands().is_empty());
        assert_eq!(state.screen, ScreenState::CharacterCreate);
        assert!(state.status_line.as_ref().unwrap().contains("empty"));
    }

    #[test]
    fn i_key_opens_character_import_form() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterManager;
        state.apply_action(KeyAction::CharacterImportPrompt);
        assert_eq!(state.screen, ScreenState::CharacterImport);
    }

    #[test]
    fn character_import_esc_cancels() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterImport;
        state.apply_action(KeyAction::FormCancel);
        assert_eq!(state.screen, ScreenState::CharacterManager);
    }

    #[test]
    fn character_import_enter_submits() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::CharacterImport;
        for c in "/tmp/card.json".chars() {
            state.apply_action(KeyAction::FormInsertChar(c));
        }
        state.apply_action(KeyAction::FormSubmit);
        let cmds = state.take_runtime_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            RuntimeCommand::ImportCharacter { path } => {
                assert_eq!(path, "/tmp/card.json");
            }
            other => panic!("Expected ImportCharacter, got {:?}", other),
        }
    }
}
