use super::enums_core::ScreenState;
use super::enums_core::SettingsState;
use super::enums_focus::FocusTarget;
use super::enums_focus::InspectorState;
use super::enums_focus::MenuState;
use super::protocol::*;
use crate::input::InputMode;
use crate::input::KeyAction;
use crate::app::TextAreaSurface;
use crate::app::textareas::{new_themed_textarea, new_themed_textarea_for, themed_textarea_from_text};
use ozone_core::session::SessionId;
use ozone_core::engine::CancelReason;
use std::time::Instant;
use tui_textarea::TextArea;
use crossterm::event::KeyEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntry {
    pub session_id: String,
    pub name: String,
    pub character_name: Option<String>,
    pub message_count: usize,
    pub last_active: Option<String>,
    pub folder: Option<String>,
    /// Truncated preview of the most recent message content (~40 chars), if available.
    pub last_message_preview: Option<String>,
}

/// An item in the visible session list — either a folder header row or a session entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibleSessionItem {
    FolderHeader {
        name: String,
    },
    /// `visual_index` counts only Entry items (headers skipped), mapping `selected` to entries.
    Entry {
        entry: SessionListEntry,
        visual_index: usize,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionListState {
    pub entries: Vec<SessionListEntry>,
    pub selected: usize,
    pub filter: String,
    pub loading: bool,
}

impl SessionListState {
    pub fn new() -> Self {
        Self {
            entries: vec![],
            selected: 0,
            filter: String::new(),
            loading: false,
        }
    }

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

    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected = (self.selected + 10).min(count - 1);
        }
    }

    pub fn visible_count(&self) -> usize {
        if self.filter.is_empty() {
            self.entries.len()
        } else {
            let lower = self.filter.to_lowercase();
            self.entries
                .iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&lower)
                        || e.character_name
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&lower)
                })
                .count()
        }
    }

    pub fn visible_entries(&self) -> Vec<&SessionListEntry> {
        if self.filter.is_empty() {
            self.entries.iter().collect()
        } else {
            let lower = self.filter.to_lowercase();
            self.entries
                .iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&lower)
                        || e.character_name
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&lower)
                })
                .collect()
        }
    }

    /// Returns items for rendering, grouped by folder with section headers.
    /// Order: named folders alphabetically → [Unfiled] last.
    /// Headers are not selectable; `visual_index` counts only Entry items.
    pub fn grouped_visible_items(&self) -> Vec<VisibleSessionItem> {
        let visible = self.visible_entries();

        let mut folders: std::collections::BTreeMap<String, Vec<SessionListEntry>> =
            std::collections::BTreeMap::new();
        let mut unfiled: Vec<SessionListEntry> = Vec::new();

        for entry in visible {
            match &entry.folder {
                Some(f) => folders.entry(f.clone()).or_default().push(entry.clone()),
                None => unfiled.push(entry.clone()),
            }
        }

        let mut items = Vec::new();
        let mut visual_index = 0usize;

        for (folder_name, entries) in &folders {
            items.push(VisibleSessionItem::FolderHeader {
                name: folder_name.clone(),
            });
            for entry in entries {
                items.push(VisibleSessionItem::Entry {
                    entry: entry.clone(),
                    visual_index,
                });
                visual_index += 1;
            }
        }

        if !unfiled.is_empty() {
            if !folders.is_empty() {
                items.push(VisibleSessionItem::FolderHeader {
                    name: "[Unfiled]".to_string(),
                });
            }
            for entry in unfiled {
                items.push(VisibleSessionItem::Entry {
                    entry,
                    visual_index,
                });
                visual_index += 1;
            }
        }

        items
    }

    pub fn selected_entry(&self) -> Option<&SessionListEntry> {
        let visible = self.visible_entries();

        let mut folders: std::collections::BTreeMap<&str, Vec<&SessionListEntry>> =
            std::collections::BTreeMap::new();
        let mut unfiled: Vec<&SessionListEntry> = Vec::new();

        for entry in &visible {
            match &entry.folder {
                Some(f) => folders.entry(f.as_str()).or_default().push(entry),
                None => unfiled.push(entry),
            }
        }

        let mut ordered: Vec<&SessionListEntry> = Vec::new();
        for entries in folders.values() {
            for entry in entries {
                ordered.push(entry);
            }
        }
        for entry in &unfiled {
            ordered.push(entry);
        }

        ordered.get(self.selected).copied()
    }
}

/// State for the inline folder assignment picker.
#[derive(Debug, Clone, Default)]
pub struct FolderPickerState {
    /// Whether the picker is open.
    pub visible: bool,
    /// Existing folder names, alphabetically sorted.
    pub folders: Vec<String>,
    /// Index into `folders` (+ 1 for the "New folder" option at the end).
    pub selected: usize,
    /// When true, user is typing a new folder name.
    pub creating: bool,
    /// Input buffer for new folder name (only used when `creating` is true).
    pub new_folder_input: String,
}

impl FolderPickerState {
    /// Total number of options: existing folders + "[+ New folder]"
    pub fn option_count(&self) -> usize {
        self.folders.len() + 1
    }

    /// The index of the "[+ New folder]" option.
    pub fn new_folder_index(&self) -> usize {
        self.folders.len()
    }

    /// Returns the selected folder name, or None if "[+ New folder]" is selected.
    pub fn selected_folder(&self) -> Option<&str> {
        self.folders.get(self.selected).map(|s| s.as_str())
    }

    pub fn move_up(&mut self) {
        if self.option_count() > 0 && self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.option_count() {
            self.selected += 1;
        }
    }

    /// Open the picker, populating folder list from existing session entries.
    pub fn open(&mut self, current_folders: Vec<String>) {
        self.folders = current_folders;
        self.folders.sort();
        self.folders.dedup();
        self.selected = 0;
        self.creating = false;
        self.new_folder_input = String::new();
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.creating = false;
        self.new_folder_input = String::new();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEntry {
    pub card_id: String,
    pub name: String,
    pub description: String,
    pub greeting: String,
    pub session_count: usize,
}

/// Full character card data for editing (all ST-style fields).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterDetail {
    pub card_id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality: String,
    pub scenario: String,
    pub greeting: String,
    pub example_dialogue: String,
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

    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        let count = self.entries.len();
        if count > 0 {
            self.selected = (self.selected + 10).min(count - 1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterFormField {
    #[default]
    Name,
    Description,
    SystemPrompt,
    Personality,
    Scenario,
    Greeting,
    ExampleDialogue,
}

impl CharacterFormField {
    /// All fields in display order.
    pub const ALL: [CharacterFormField; 7] = [
        CharacterFormField::Name,
        CharacterFormField::Description,
        CharacterFormField::SystemPrompt,
        CharacterFormField::Personality,
        CharacterFormField::Scenario,
        CharacterFormField::Greeting,
        CharacterFormField::ExampleDialogue,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Description => "Description",
            Self::SystemPrompt => "System Prompt",
            Self::Personality => "Personality",
            Self::Scenario => "Scenario",
            Self::Greeting => "Greeting",
            Self::ExampleDialogue => "Example Dialogue",
        }
    }

    fn ordinal(self) -> usize {
        match self {
            Self::Name => 0,
            Self::Description => 1,
            Self::SystemPrompt => 2,
            Self::Personality => 3,
            Self::Scenario => 4,
            Self::Greeting => 5,
            Self::ExampleDialogue => 6,
        }
    }

    fn from_ordinal(n: usize) -> Self {
        Self::ALL[n % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterCreateState {
    pub name: DraftState,
    pub description: DraftState,
    pub system_prompt: DraftState,
    pub personality: DraftState,
    pub scenario: DraftState,
    pub greeting: DraftState,
    pub example_dialogue: DraftState,
    pub active_field: CharacterFormField,
    /// When editing an existing character, holds the card_id.
    pub editing_card_id: Option<String>,
}

impl CharacterCreateState {
    pub fn active_draft(&self) -> &DraftState {
        match self.active_field {
            CharacterFormField::Name => &self.name,
            CharacterFormField::Description => &self.description,
            CharacterFormField::SystemPrompt => &self.system_prompt,
            CharacterFormField::Personality => &self.personality,
            CharacterFormField::Scenario => &self.scenario,
            CharacterFormField::Greeting => &self.greeting,
            CharacterFormField::ExampleDialogue => &self.example_dialogue,
        }
    }

    pub fn active_draft_mut(&mut self) -> &mut DraftState {
        match self.active_field {
            CharacterFormField::Name => &mut self.name,
            CharacterFormField::Description => &mut self.description,
            CharacterFormField::SystemPrompt => &mut self.system_prompt,
            CharacterFormField::Personality => &mut self.personality,
            CharacterFormField::Scenario => &mut self.scenario,
            CharacterFormField::Greeting => &mut self.greeting,
            CharacterFormField::ExampleDialogue => &mut self.example_dialogue,
        }
    }

    pub fn toggle_field(&mut self) {
        let next = (self.active_field.ordinal() + 1) % CharacterFormField::ALL.len();
        self.active_field = CharacterFormField::from_ordinal(next);
    }

    /// Populate form from an existing character for editing.
    pub fn load_from_character(&mut self, detail: &CharacterDetail) {
        self.editing_card_id = Some(detail.card_id.clone());
        self.name = DraftState::with_text(&detail.name);
        self.description = DraftState::with_text(&detail.description);
        self.system_prompt = DraftState::with_text(&detail.system_prompt);
        self.personality = DraftState::with_text(&detail.personality);
        self.scenario = DraftState::with_text(&detail.scenario);
        self.greeting = DraftState::with_text(&detail.greeting);
        self.example_dialogue = DraftState::with_text(&detail.example_dialogue);
        self.active_field = CharacterFormField::Name;
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

    pub fn sync_dirty(&mut self) {
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
    pub author_kind: String,
    pub content: String,
    pub is_bookmarked: bool,
    /// Pre-formatted display timestamp, e.g. "2:15 PM".
    pub timestamp: Option<String>,
}

impl TranscriptItem {
    pub fn new(author: impl Into<String>, content: impl Into<String>) -> Self {
        let author = author.into();
        Self {
            message_id: None,
            author_kind: author.clone(),
            author,
            content: content.into(),
            is_bookmarked: false,
            timestamp: None,
        }
    }

    pub fn persisted(
        message_id: impl Into<String>,
        author: impl Into<String>,
        content: impl Into<String>,
        is_bookmarked: bool,
    ) -> Self {
        let author = author.into();
        Self {
            message_id: Some(message_id.into()),
            author_kind: author.clone(),
            author,
            content: content.into(),
            is_bookmarked,
            timestamp: None,
        }
    }

    pub fn with_author_kind(mut self, author_kind: impl Into<String>) -> Self {
        self.author_kind = author_kind.into();
        self
    }

    /// Set a pre-formatted display timestamp on this item.
    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = Some(ts.into());
        self
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
        reason: String,
    },
}

impl RuntimePhase {
    pub fn is_inflight(&self) -> bool {
        matches!(
            self,
            Self::Queued { .. } | Self::Generating { .. } | Self::Cancelling { .. }
        )
    }

    pub fn prompt(&self) -> Option<&str> {
        match self {
            Self::Queued { prompt }
            | Self::Generating { prompt, .. }
            | Self::Cancelling { prompt, .. }
            | Self::Cancelled { prompt, .. }
            | Self::Failed { prompt, .. } => Some(prompt),
            Self::Idle => None,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
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
    CreateSession {
        character_name: Option<String>,
    },
    SendDraft {
        prompt: String,
    },
    /// Generate a new assistant message reusing the parent user prompt from
    /// the specified persisted assistant message. The runtime is responsible
    /// for branching/forking and swipe persistence rules.
    RerollMessage {
        message_id: String,
    },
    EditMessage {
        message_id: String,
        content: String,
    },
    CancelGeneration,
    BuildContextDryRun,
    ToggleBookmark {
        message_id: String,
    },
    TogglePinnedMemory {
        message_id: String,
    },
    RunCommand {
        input: String,
    },
    CreateCharacter {
        name: String,
        description: String,
        system_prompt: String,
        personality: String,
        scenario: String,
        greeting: String,
        example_dialogue: String,
    },
    UpdateCharacter {
        card_id: String,
        name: String,
        description: String,
        system_prompt: String,
        personality: String,
        scenario: String,
        greeting: String,
        example_dialogue: String,
    },
    /// Load a character's full details and enter edit mode.
    EditCharacter {
        card_id: String,
    },
    ImportCharacter {
        path: String,
    },
    /// A user-editable preference was changed from the settings screen.
    /// `pref_key` is the JSON field name; `value` is the new serialised value.
    PrefChanged {
        pref_key: String,
        value: String,
    },
    /// Assign or remove the folder for a session.
    SetSessionFolder {
        session_id: String,
        folder: Option<String>,
    },
    /// Switch to a different session — load its transcript, branches, and metadata.
    OpenSession {
        session_id: String,
        session_name: String,
    },
    SaveSession,
    DeleteSession,
}

/// Ephemeral event recorded when the context engine compresses messages due to budget exceeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCompressionEvent {
    /// Number of tokens freed by the compression.
    pub freed_tokens: usize,
    /// Remaining tokens in the budget after compression.
    pub remaining_tokens: usize,
    /// When the compression occurred, for fade-out timing.
    pub timestamp: Instant,
}

impl ContextCompressionEvent {
    pub fn new(freed_tokens: usize, remaining_tokens: usize) -> Self {
        Self {
            freed_tokens,
            remaining_tokens,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSendReceipt {
    pub request_id: String,
    pub user_message: TranscriptItem,
    pub context_preview: Option<ContextPreview>,
    pub context_dry_run: Option<ContextDryRunPreview>,
    pub refresh: Option<RuntimeContextRefresh>,
    /// Compression event emitted when context was truncated due to budget.
    /// Tuple is (freed_tokens, remaining_tokens).
    pub context_compression: Option<(usize, usize)>,
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
pub struct RuntimeSessionLoad {
    pub session_id: String,
    pub session_name: String,
    pub bootstrap: AppBootstrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCompletion {
    pub request_id: String,
    pub message: TranscriptItem,
    pub session_title: Option<String>,
    pub refresh: Option<RuntimeContextRefresh>,
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
    pub content: String,
}

/// An unrecoverable generation failure reported by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    pub request_id: String,
    pub prompt: String,
    pub message: String,
    pub reason: String,
}

/// The result of a single `poll_generation` call. The runtime returns this to
/// tell the TUI shell whether generation is still in progress, completed, or
/// failed — replacing the fixed-delay timer approach.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
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
    pub active_launch_plan: Option<ozone_core::planner::LaunchPlan>,
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
            CommandEntry {
                name: "new".into(),
                alias: vec!["n".into()],
                description: "Start new chat".into(),
            },
            CommandEntry {
                name: "sessions".into(),
                alias: vec!["s".into()],
                description: "Browse sessions".into(),
            },
            CommandEntry {
                name: "characters".into(),
                alias: vec!["c".into()],
                description: "Manage characters".into(),
            },
            CommandEntry {
                name: "settings".into(),
                alias: vec![],
                description: "Open settings".into(),
            },
            CommandEntry {
                name: "session show".into(),
                alias: vec![],
                description: "Show session metadata".into(),
            },
            CommandEntry {
                name: "session rename".into(),
                alias: vec![],
                description: "Rename current session".into(),
            },
            CommandEntry {
                name: "session retitle".into(),
                alias: vec![],
                description: "Generate a title from the current chat".into(),
            },
            CommandEntry {
                name: "session character".into(),
                alias: vec![],
                description: "Set session character".into(),
            },
            CommandEntry {
                name: "session reroll".into(),
                alias: vec![],
                description: "Reroll the selected assistant reply".into(),
            },
            CommandEntry {
                name: "memories".into(),
                alias: vec!["memory list".into()],
                description: "Open recall browser".into(),
            },
            CommandEntry {
                name: "memory note".into(),
                alias: vec![],
                description: "Create a note memory".into(),
            },
            CommandEntry {
                name: "search session".into(),
                alias: vec![],
                description: "Search this session".into(),
            },
            CommandEntry {
                name: "search global".into(),
                alias: vec![],
                description: "Search all sessions".into(),
            },
            CommandEntry {
                name: "attach".into(),
                alias: vec!["a".into()],
                description: "Attach a text file to the prompt".into(),
            },
            CommandEntry {
                name: "help".into(),
                alias: vec!["h".into(), "?".into()],
                description: "Show help".into(),
            },
            CommandEntry {
                name: "quit".into(),
                alias: vec!["q".into()],
                description: "Quit / back to menu".into(),
            },
            CommandEntry {
                name: "menu".into(),
                alias: vec!["m".into()],
                description: "Return to main menu".into(),
            },
        ]
    }

    pub fn palette_matches(query: &str) -> Vec<CommandEntry> {
        ranked_command_matches(query, true)
    }

    pub fn slash_matches(query: &str) -> Vec<CommandEntry> {
        ranked_command_matches(query, false)
    }

}

fn ranked_command_matches(query: &str, include_description: bool) -> Vec<CommandEntry> {
    let all = CommandEntry::all();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return all;
    }

    let mut matches: Vec<_> = all
        .into_iter()
        .filter_map(|entry| {
            command_match_rank(&entry, &query, include_description).map(|rank| (rank, entry))
        })
        .collect();

    matches.sort_by(|left, right| left.0.cmp(&right.0));
    matches.into_iter().map(|(_, entry)| entry).collect()
}

fn command_match_rank(
    entry: &CommandEntry,
    query: &str,
    include_description: bool,
) -> Option<(u8, usize, String)> {
    let mut ranks = Vec::new();

    if let Some(rank) = match_bucket(&entry.name, query, 0, 2, 4, 6) {
        ranks.push((rank.0, rank.1, entry.name.to_lowercase()));
    }

    for alias in &entry.alias {
        if let Some(rank) = match_bucket(alias, query, 1, 3, 5, 7) {
            ranks.push((rank.0, rank.1, entry.name.to_lowercase()));
        }
    }

    if include_description {
        let description = entry.description.to_lowercase();
        if description.contains(query) {
            ranks.push((8, description.len(), entry.name.to_lowercase()));
        }
    }

    ranks.into_iter().min()
}

fn match_bucket(
    text: &str,
    query: &str,
    exact_bucket: u8,
    prefix_bucket: u8,
    word_prefix_bucket: u8,
    contains_bucket: u8,
) -> Option<(u8, usize)> {
    let lower = text.to_lowercase();
    if lower == query {
        Some((exact_bucket, lower.len()))
    } else if lower.starts_with(query) {
        Some((prefix_bucket, lower.len()))
    } else if lower.split_whitespace().any(|word| word.starts_with(query)) {
        Some((word_prefix_bucket, lower.len()))
    } else if lower.contains(query) {
        Some((contains_bucket, lower.len()))
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    pub open: bool,
    pub textarea: TextArea<'static>,
    pub selected: usize,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            open: false,
            textarea: new_themed_textarea_for(TextAreaSurface::CommandPalette),
            selected: 0,
        }
    }
}

impl CommandPaletteState {
    pub fn open(&mut self, textarea: TextArea<'static>) {
        self.open = true;
        self.textarea = textarea;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.textarea = new_themed_textarea_for(TextAreaSurface::CommandPalette);
        self.selected = 0;
    }

    pub fn input_text(&self) -> String {
        self.textarea.lines().join(" ")
    }

    pub(crate) fn restore_input_text(&mut self, text: &str, cursor: usize) {
        self.textarea = themed_textarea_from_text(
            TextAreaSurface::CommandPalette,
            text,
            cursor.min(text.chars().count()),
        );
    }

    fn normalize_single_line(&mut self) {
        let lines = self.textarea.lines();
        if lines.len() <= 1 {
            return;
        }

        let cursor = self.textarea.cursor();
        let text_before_cursor = lines
            .iter()
            .take(cursor.0)
            .map(|line| line.chars().count())
            .sum::<usize>()
            + cursor.0
            + cursor.1;
        let normalized = lines.join(" ");
        self.restore_input_text(
            &normalized,
            text_before_cursor.min(normalized.chars().count()),
        );
    }

    pub fn handle_textarea_input(&mut self, key: KeyEvent) -> bool {
        let modified = self.textarea.input(key);
        self.normalize_single_line();
        if modified {
            self.selected = 0;
        }
        modified
    }

    /// Return commands matching the current input, ranked by exact, prefix, word-prefix,
    /// and substring matches.
    pub fn filtered_commands(&self) -> Vec<CommandEntry> {
        let input = self.input_text();
        CommandEntry::palette_matches(&input)
    }

    pub fn selected_command(&self) -> Option<CommandEntry> {
        let cmds = self.filtered_commands();
        cmds.into_iter().nth(self.selected)
    }
}

#[derive(Debug, Clone)]
pub struct MessageEditState {
    pub message_id: String,
    pub previous_draft: DraftState,
    pub previous_focus: FocusTarget,
    pub previous_input_mode: InputMode,
    pub previous_history: InputHistoryState,
    pub previous_slash_selected: Option<usize>,
    pub previous_slash_dismissed: bool,
}

#[derive(Debug, Clone)]
pub struct ShellState {
    pub screen: ScreenState,
    pub input_mode: InputMode,
    pub focus: FocusTarget,
    pub inspector: InspectorState,
    pub menu: MenuState,
    pub session_list: SessionListState,
    pub folder_picker: FolderPickerState,
    pub character_list: CharacterListState,
    pub character_create: CharacterCreateState,
    pub character_import: CharacterImportState,
    pub settings: SettingsState,
    pub session: SessionState,
    pub draft: DraftState,
    pub message_edit: Option<MessageEditState>,
    pub textarea: TextArea<'static>,
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
    /// Active model's launch plan, populated from `OZONE__LAUNCH_PLAN` env var on handoff.
    pub active_launch_plan: Option<ozone_core::planner::LaunchPlan>,
    /// Manual transcript viewport offset in visual rows; `None` follows selection.
    pub conversation_scroll: Option<usize>,
    /// Index of the highlighted slash suggestion (`None` = popup not navigated).
    pub slash_selected: Option<usize>,
    /// True when the user explicitly dismissed the slash popup for the current query.
    pub slash_dismissed: bool,
    /// Monotonically increasing counter incremented each event-loop tick for animations.
    pub tick_count: u64,
    /// Ephemeral toast notification: (message, created_at).
    pub toast: Option<(String, Instant)>,
    /// Count prefix accumulated in Normal mode (e.g., "3j" → count = 3).
    pub normal_mode_count: Option<u32>,
    /// Whether Ctrl+W pane-prefix mode is active — next key dispatches pane focus.
    pub pane_prefix_active: bool,
    /// Last context compression event for displaying the freed-tokens flash.
    pub last_context_compression: Option<ContextCompressionEvent>,
    /// Buffer for jj/jk escape sequence detection in Insert mode.
    pub insert_escape_buffer: Vec<char>,
    /// Last polled hardware stats (VRAM/RAM). Updated every 600 ticks (~30s).
    pub hardware: crate::hardware::HardwareInfo,
    /// Counter that tracks when to next poll hardware (every 600 ticks).
    pub hardware_poll_interval: u64,
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
            folder_picker: FolderPickerState::default(),
            character_list: CharacterListState::default(),
            character_create: CharacterCreateState::default(),
            character_import: CharacterImportState::default(),
            settings: SettingsState::default(),
            session: SessionState::new(context),
            draft: DraftState::default(),
            message_edit: None,
            textarea: new_themed_textarea(),
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
            active_launch_plan: None,
            conversation_scroll: None,
            slash_selected: None,
            slash_dismissed: false,
            tick_count: 0,
            toast: None,
            normal_mode_count: None,
            pane_prefix_active: false,
            last_context_compression: None,
            insert_escape_buffer: Vec::new(),
            hardware: crate::hardware::HardwareInfo::default(),
            hardware_poll_interval: 0,
        }
    }

}

// ── Helper functions ──────────────────────────────────────────────────────

fn clamp_cursor(text: &str, cursor: usize) -> usize {
    cursor.min(text.chars().count())
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .map(|(idx, _)| idx)
        .nth(char_index)
        .unwrap_or(text.len())
}
