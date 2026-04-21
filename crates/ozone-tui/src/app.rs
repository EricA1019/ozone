use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ozone_core::{engine::CancelReason, session::SessionId};
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::TextArea;

use crate::input::{
    dispatch_command_palette_key, dispatch_form_key, dispatch_key, dispatch_menu_key, InputMode,
    KeyAction,
};

/// Create a fresh TextArea with ozone+ theme styling.
pub(crate) fn new_themed_textarea() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    textarea.set_cursor_line_style(Style::default());
    textarea.set_block(ratatui::widgets::Block::default());
    textarea.set_style(Style::default().fg(crate::theme::cyan(crate::theme::active_preset())));
    textarea.set_cursor_style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::REVERSED),
    );
    textarea
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    MainMenu,
    SessionList,
    CharacterManager,
    CharacterCreate,
    CharacterEdit,
    CharacterImport,
    Settings,
    ModelIntelligence,
    Conversation,
    Help,
    Quit,
}

/// The top-level categories shown in the Settings menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCategory {
    Backend,
    Model,
    Display,
    Keybindings,
    Session,
    Appearance,
    Launch,
}

impl SettingsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsCategory::Backend => "Backend",
            SettingsCategory::Model => "Model",
            SettingsCategory::Display => "Display",
            SettingsCategory::Keybindings => "Keybindings",
            SettingsCategory::Session => "Session",
            SettingsCategory::Appearance => "Appearance",
            SettingsCategory::Launch => "Launch",
        }
    }
}

/// Describes how a settings entry behaves when the user presses Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// Read-only diagnostic value — no interaction.
    ReadOnly,
    /// Boolean toggle — Enter flips the value.
    Toggle(bool),
    /// Cycle through a list of options — Enter advances to the next.
    Cycle {
        options: Vec<String>,
        current: usize,
    },
}

impl EntryKind {
    /// Returns the current human-readable value, or `None` for `ReadOnly`
    /// (caller should use `SettingsEntry::value` instead).
    pub fn current_value(&self) -> Option<String> {
        match self {
            EntryKind::ReadOnly => None,
            EntryKind::Toggle(v) => Some(v.to_string()),
            EntryKind::Cycle { options, current } => options.get(*current).cloned(),
        }
    }

    /// Advance to the next state and return the new value string.
    /// Returns `None` for `ReadOnly` entries (no change).
    pub fn activate(&mut self) -> Option<String> {
        match self {
            EntryKind::ReadOnly => None,
            EntryKind::Toggle(v) => {
                *v = !*v;
                Some(v.to_string())
            }
            EntryKind::Cycle { options, current } => {
                if options.is_empty() {
                    return None;
                }
                *current = (*current + 1) % options.len();
                options.get(*current).cloned()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    pub categories: Vec<SettingsCategory>,
    pub selected_category: usize,
    pub selected_entry: usize,
    /// `false` = category list visible; `true` = entry list for selected category.
    pub drill_down: bool,
    // Entries loaded from runtime (Backend / Model); Display & Keybindings are
    // provided statically by `entries_for_category`.
    raw_entries: Vec<SettingsEntry>,
    loaded: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            categories: vec![
                SettingsCategory::Backend,
                SettingsCategory::Model,
                SettingsCategory::Display,
                SettingsCategory::Keybindings,
                SettingsCategory::Session,
                SettingsCategory::Appearance,
                SettingsCategory::Launch,
            ],
            selected_category: 0,
            selected_entry: 0,
            drill_down: false,
            raw_entries: Vec::new(),
            loaded: false,
        }
    }
}

impl SettingsState {
    /// True once the runtime has provided Backend / Model config entries.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Store runtime-provided entries (replaces any previous Backend/Model data).
    pub fn load(&mut self, entries: Vec<SettingsEntry>) {
        self.raw_entries = entries;
        self.loaded = true;
    }

    /// Returns `(label, value, kind)` triples for the given category.
    /// Display and Keybindings have built-in read-only entries; the editable
    /// Display rows and the Appearance / Launch / Backend / Model / Session
    /// entries come from `raw_entries`.
    pub fn entries_for_category(&self, cat: &SettingsCategory) -> Vec<(String, String, EntryKind)> {
        match cat {
            SettingsCategory::Display => {
                let mut entries: Vec<(String, String, EntryKind)> = vec![
                    (
                        "Color theme".into(),
                        "ozone-dark".into(),
                        EntryKind::ReadOnly,
                    ),
                    (
                        "Wide mode threshold".into(),
                        "120 cols".into(),
                        EntryKind::ReadOnly,
                    ),
                ];
                for e in &self.raw_entries {
                    if e.category == "Display" {
                        let val = e.kind.current_value().unwrap_or_else(|| e.value.clone());
                        entries.push((e.key.clone(), val, e.kind.clone()));
                    }
                }
                entries
            }
            SettingsCategory::Keybindings => vec![
                ("Move up".into(), "↑ / k".into(), EntryKind::ReadOnly),
                ("Move down".into(), "↓ / j".into(), EntryKind::ReadOnly),
                ("Select / open".into(), "Enter".into(), EntryKind::ReadOnly),
                (
                    "Back / cancel".into(),
                    "Esc / q".into(),
                    EntryKind::ReadOnly,
                ),
                ("Insert mode".into(), "i".into(), EntryKind::ReadOnly),
                (
                    "Send message".into(),
                    "Enter (insert)".into(),
                    EntryKind::ReadOnly,
                ),
                (
                    "Command palette".into(),
                    "/ or :".into(),
                    EntryKind::ReadOnly,
                ),
                (
                    "Toggle inspector".into(),
                    "Ctrl+I".into(),
                    EntryKind::ReadOnly,
                ),
            ],
            other => {
                let cat_name = other.label();
                self.raw_entries
                    .iter()
                    .filter(|e| e.category == cat_name)
                    .map(|e| {
                        let val = e.kind.current_value().unwrap_or_else(|| e.value.clone());
                        (e.key.clone(), val, e.kind.clone())
                    })
                    .collect()
            }
        }
    }

    /// Reference to the currently selected category.
    pub fn current_category(&self) -> &SettingsCategory {
        self.categories
            .get(self.selected_category)
            .unwrap_or_else(|| &self.categories[0])
    }

    /// Move selection down (wraps). Operates at the current navigation level.
    pub fn nav_down(&mut self) {
        if self.drill_down {
            let cat = self.categories[self.selected_category].clone();
            let count = self.entries_for_category(&cat).len();
            if count > 0 {
                self.selected_entry = (self.selected_entry + 1) % count;
            }
        } else {
            self.selected_category = (self.selected_category + 1) % self.categories.len();
        }
    }

    /// Move selection up (wraps). Operates at the current navigation level.
    pub fn nav_up(&mut self) {
        if self.drill_down {
            let cat = self.categories[self.selected_category].clone();
            let count = self.entries_for_category(&cat).len();
            if count > 0 {
                if self.selected_entry == 0 {
                    self.selected_entry = count - 1;
                } else {
                    self.selected_entry -= 1;
                }
            }
        } else if self.selected_category == 0 {
            self.selected_category = self.categories.len() - 1;
        } else {
            self.selected_category -= 1;
        }
    }

    /// Drill into the selected category (Enter at category level) OR activate the
    /// selected entry when already drilled in.  Returns a `RuntimeCommand` when
    /// an editable entry was toggled/cycled so the caller can persist the change.
    pub fn enter(&mut self) -> Option<RuntimeCommand> {
        if self.drill_down {
            self.activate_entry()
        } else {
            self.drill_down = true;
            self.selected_entry = 0;
            None
        }
    }

    /// Activate the currently selected entry if it is editable.
    /// Mutates the entry's kind in `raw_entries` and returns a `PrefChanged`
    /// command when the value changed, or `None` for read-only entries.
    pub fn activate_entry(&mut self) -> Option<RuntimeCommand> {
        if !self.drill_down {
            return None;
        }
        let cat = &self.categories[self.selected_category];
        let cat_name = cat.label();

        // Some categories prepend hardcoded static/read-only rows that are NOT
        // in `raw_entries`.  We must subtract that count so `selected_entry`
        // (which indexes the *merged* visual list) maps to the correct position
        // inside `raw_entries`.
        let static_count = self.static_entry_count(cat);
        // Selected entry is within the static rows — always read-only.
        let idx = self.selected_entry.checked_sub(static_count)?;

        let mut pos = 0usize;
        for entry in &mut self.raw_entries {
            if entry.category == cat_name {
                if pos == idx {
                    if let Some(new_val) = entry.kind.activate() {
                        return Some(RuntimeCommand::PrefChanged {
                            pref_key: entry.pref_key.clone(),
                            value: new_val,
                        });
                    }
                    return None;
                }
                pos += 1;
            }
        }
        None
    }

    /// Number of hardcoded static entries that `entries_for_category` prepends
    /// before the runtime-provided `raw_entries` for the given category.
    fn static_entry_count(&self, cat: &SettingsCategory) -> usize {
        match cat {
            SettingsCategory::Display => 2, // Color theme, Wide mode threshold
            SettingsCategory::Keybindings => 8, // all keybinding rows are static
            _ => 0,
        }
    }

    /// Go back one level.  Returns `true` if handled (was in entry list);
    /// returns `false` if already at category list (caller navigates to main menu).
    pub fn back(&mut self) -> bool {
        if self.drill_down {
            self.drill_down = false;
            self.selected_entry = 0;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEntry {
    pub category: String,
    /// Display label shown in the settings entry list.
    pub key: String,
    /// Display value for `ReadOnly` entries. For `Toggle`/`Cycle`, the value is
    /// derived from `kind` at render time; this field can be left empty.
    pub value: String,
    /// Interaction kind — controls rendering and Enter-key behaviour.
    pub kind: EntryKind,
    /// JSON field name in the preferences file (e.g. `"theme_preset"`).
    /// Empty string means the entry is not persisted.
    pub pref_key: String,
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
                    id: "model-intel",
                    label: "Model Intel",
                    description: "Launch advisory & resource plan",
                    shortcut: Some('m'),
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
    pub folder: Option<String>,
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
    /// Pre-formatted display timestamp, e.g. "2:15 PM".
    pub timestamp: Option<String>,
}

impl TranscriptItem {
    pub fn new(author: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            message_id: None,
            author: author.into(),
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
        Self {
            message_id: Some(message_id.into()),
            author: author.into(),
            content: content.into(),
            is_bookmarked,
            timestamp: None,
        }
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
    CreateSession,
    SendDraft {
        prompt: String,
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
pub struct RuntimeSessionLoad {
    pub session_id: String,
    pub session_name: String,
    pub bootstrap: AppBootstrap,
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
                name: "session character".into(),
                alias: vec![],
                description: "Set session character".into(),
            },
            CommandEntry {
                name: "memory list".into(),
                alias: vec![],
                description: "List pinned memories".into(),
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
            .filter(|c| {
                c.name.to_lowercase().contains(&query) || c.alias.iter().any(|a| a.contains(&query))
            })
            .collect()
    }

    pub fn selected_command(&self) -> Option<CommandEntry> {
        let cmds = self.filtered_commands();
        cmds.into_iter().nth(self.selected)
    }
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
    /// Index of the highlighted slash suggestion (`None` = popup not navigated).
    pub slash_selected: Option<usize>,
    /// True when the user explicitly dismissed the slash popup for the current query.
    pub slash_dismissed: bool,
    /// Monotonically increasing counter incremented each event-loop tick for animations.
    pub tick_count: u64,
    /// Ephemeral toast notification: (message, created_at).
    pub toast: Option<(String, Instant)>,
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
            slash_selected: None,
            slash_dismissed: false,
            tick_count: 0,
            toast: None,
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
        self.session.runtime = RuntimePhase::Idle;

        if let Some(status_line) = bootstrap.status_line {
            self.status_line = Some(status_line);
        }

        let draft = bootstrap.draft.unwrap_or_default();
        if !draft.text.is_empty() {
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Insert;
            self.sync_textarea_from_draft(&draft);
        } else {
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Normal;
            self.textarea = new_themed_textarea();
        }
        self.draft = draft;
        self.command_palette.close();
        self.slash_selected = None;
        self.slash_dismissed = false;

        if let Some(screen) = bootstrap.screen {
            self.screen = screen;
        }

        self.session_metadata = bootstrap.session_metadata;
        self.session_stats = bootstrap.session_stats;
        self.context_preview = bootstrap.context_preview;
        self.context_dry_run = bootstrap.context_dry_run;
        self.recall_browser = bootstrap.recall_browser;
        if let Some(plan) = bootstrap.active_launch_plan {
            self.active_launch_plan = Some(plan);
        }
    }

    /// Transition from a menu screen into the conversation view for the current session.
    pub fn enter_conversation(&mut self) {
        self.screen = ScreenState::Conversation;
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Normal;
    }

    pub fn reset_for_new_conversation(&mut self) {
        self.session.transcript.clear();
        self.session.branches.clear();
        self.session.selected_message = None;
        self.session.selected_branch = None;
        self.session.runtime = RuntimePhase::Idle;
        self.draft = DraftState::default();
        self.textarea = new_themed_textarea();
        self.session_metadata = None;
        self.session_stats = None;
        self.context_preview = None;
        self.context_dry_run = None;
        self.recall_browser = None;
        self.command_palette.close();
        self.slash_selected = None;
        self.slash_dismissed = false;
    }

    /// Show an ephemeral toast notification that auto-expires after 3 seconds.
    pub fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Return the toast message if it is still within its display window.
    pub fn active_toast(&self) -> Option<&str> {
        if let Some((msg, created)) = &self.toast {
            if created.elapsed().as_secs() < 3 {
                return Some(msg.as_str());
            }
        }
        None
    }

    /// Return to the main menu from any screen.
    pub fn return_to_menu(&mut self) {
        self.screen = ScreenState::MainMenu;
        self.input_mode = InputMode::Normal;
        self.focus = FocusTarget::Transcript;
    }

    /// Replace textarea contents from a DraftState (for history navigation).
    fn sync_textarea_from_draft(&mut self, draft: &DraftState) {
        let lines: Vec<String> = if draft.text.is_empty() {
            vec![String::new()]
        } else {
            draft.text.lines().map(str::to_owned).collect()
        };
        self.textarea = TextArea::new(lines);
        self.textarea.set_cursor_line_style(Style::default());
        self.textarea.set_block(ratatui::widgets::Block::default());
        self.textarea
            .set_style(Style::default().fg(crate::theme::CYAN));
        self.textarea.set_cursor_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::REVERSED),
        );
    }

    // ── Slash-popup helpers ─────────────────────────────────────────────────

    /// Command names (with leading `/`) that match the current draft query.
    pub fn slash_completion_names(&self) -> Vec<String> {
        if !self.draft.text.starts_with('/') || self.draft.text.contains(' ') {
            return Vec::new();
        }
        let query = self
            .draft
            .text
            .get(1..)
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        CommandEntry::all()
            .into_iter()
            .filter(|cmd| {
                query.is_empty()
                    || cmd.name.to_lowercase().starts_with(&query)
                    || cmd.alias.iter().any(|a| a.starts_with(&query))
            })
            .map(|cmd| format!("/{}", cmd.name))
            .collect()
    }

    /// True when the slash popup should be visible to the user.
    pub fn slash_popup_active(&self) -> bool {
        self.input_mode == InputMode::Insert
            && !self.command_palette.open
            && !self.slash_dismissed
            && !self.slash_completion_names().is_empty()
    }

    /// Move popup highlight up (wraps from top to bottom).
    pub fn slash_move_up(&mut self) {
        let len = self.slash_completion_names().len();
        if len == 0 {
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(i) if i > 0 => i - 1,
            _ => len - 1,
        });
    }

    /// Move popup highlight down (wraps from bottom to top).
    pub fn slash_move_down(&mut self) {
        let len = self.slash_completion_names().len();
        if len == 0 {
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(i) if i + 1 < len => i + 1,
            _ => 0,
        });
    }

    /// Fill the draft with the currently highlighted suggestion.
    /// Returns `true` if a suggestion was accepted, `false` if nothing was selected.
    pub fn slash_accept(&mut self) -> bool {
        if let Some(idx) = self.slash_selected {
            let names = self.slash_completion_names();
            if let Some(name) = names.get(idx) {
                let filled = name.clone() + " ";
                let len = filled.len();
                self.draft.text = filled;
                self.draft.cursor = len;
                self.draft.dirty = true;
                self.slash_selected = None;
                self.slash_dismissed = false;
                return true;
            }
        }
        false
    }

    /// Keep `slash_selected` / `slash_dismissed` consistent with the current draft.
    /// Called automatically at the end of every `apply_action`.
    fn sync_slash_state(&mut self) {
        let names = self.slash_completion_names();
        let has_suggestions = !names.is_empty() && !self.command_palette.open;

        if !has_suggestions {
            // No applicable suggestions — reset everything.
            self.slash_selected = None;
            self.slash_dismissed = false;
        } else if self.slash_dismissed {
            // Popup was dismissed; keep selected = None but don't reopen.
            self.slash_selected = None;
        } else {
            // Suggestions exist and popup is not dismissed.
            if self.slash_selected.is_none() {
                // Auto-highlight the first item when popup first appears.
                self.slash_selected = Some(0);
            } else {
                // Clamp in case the list shrank (e.g., user typed more).
                let len = names.len();
                if let Some(idx) = self.slash_selected {
                    if idx >= len {
                        self.slash_selected = Some(0);
                    }
                }
            }
        }
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

        // Slash-popup navigation: intercept arrow/Enter/Esc when popup is visible.
        if self.slash_selected.is_some()
            && matches!(
                self.screen,
                ScreenState::Conversation | ScreenState::Help | ScreenState::Quit
            )
        {
            let slash_action = match key.code {
                KeyCode::Up => Some(KeyAction::SlashUp),
                KeyCode::Down => Some(KeyAction::SlashDown),
                KeyCode::Enter => Some(KeyAction::SlashAccept),
                KeyCode::Esc => Some(KeyAction::SlashDismiss),
                _ => None,
            };
            if let Some(action) = slash_action {
                self.apply_action(action);
                return action;
            }
        }

        // Tab in Insert mode: slash tab-completion when draft starts with '/'.
        if self.input_mode == InputMode::Insert
            && !self.command_palette.open
            && key.code == KeyCode::Tab
            && key.modifiers.is_empty()
            && self.draft.text.starts_with('/')
        {
            let action = if self.slash_selected.is_some() {
                KeyAction::SlashAccept
            } else {
                KeyAction::SlashTabComplete
            };
            self.apply_action(action);
            return action;
        }

        // Folder picker intercept when picker is open (SessionList only)
        if self.screen == ScreenState::SessionList && self.folder_picker.visible {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.folder_picker.close();
                }
                KeyCode::Up | KeyCode::Char('k') => self.folder_picker.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.folder_picker.move_down(),
                KeyCode::Enter => {
                    if self.folder_picker.creating {
                        let name = self.folder_picker.new_folder_input.trim().to_owned();
                        self.folder_picker.close();
                        if !name.is_empty() {
                            let session_id_opt = self
                                .session_list
                                .selected_entry()
                                .map(|e| e.session_id.clone());
                            if let Some(session_id) = session_id_opt {
                                self.runtime_commands
                                    .push(RuntimeCommand::SetSessionFolder {
                                        session_id,
                                        folder: Some(name),
                                    });
                            }
                        }
                    } else if self.folder_picker.selected == self.folder_picker.new_folder_index() {
                        self.folder_picker.creating = true;
                    } else {
                        let folder = self.folder_picker.selected_folder().map(|s| s.to_owned());
                        self.folder_picker.close();
                        let session_id_opt = self
                            .session_list
                            .selected_entry()
                            .map(|e| e.session_id.clone());
                        if let Some(session_id) = session_id_opt {
                            self.runtime_commands
                                .push(RuntimeCommand::SetSessionFolder { session_id, folder });
                        }
                    }
                }
                KeyCode::Char(c) if self.folder_picker.creating => {
                    self.folder_picker.new_folder_input.push(c);
                }
                KeyCode::Backspace if self.folder_picker.creating => {
                    self.folder_picker.new_folder_input.pop();
                }
                _ => {}
            }
            return KeyAction::Noop; // consume the key — don't pass to session list
        }

        let action = match self.screen {
            ScreenState::CharacterManager => {
                // Intercept n/i/e for create/import/edit before normal menu dispatch
                match key.code {
                    crossterm::event::KeyCode::Char('n') => KeyAction::CharacterCreate,
                    crossterm::event::KeyCode::Char('e') => KeyAction::CharacterEditSelected,
                    crossterm::event::KeyCode::Char('i') => KeyAction::CharacterImportPrompt,
                    _ => dispatch_menu_key(key, false),
                }
            }
            ScreenState::CharacterCreate
            | ScreenState::CharacterEdit
            | ScreenState::CharacterImport => dispatch_form_key(key),
            ScreenState::SessionList => {
                // Intercept f/F for folder management before normal menu dispatch
                match key.code {
                    KeyCode::Char('f') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                        if self.session_list.selected_entry().is_some() {
                            let folders: Vec<String> = self
                                .session_list
                                .entries
                                .iter()
                                .filter_map(|e| e.folder.clone())
                                .collect();
                            self.folder_picker.open(folders);
                        }
                        KeyAction::Noop
                    }
                    KeyCode::Char('F') => {
                        // Shift+F: immediately remove from folder
                        let session_id_opt = self
                            .session_list
                            .selected_entry()
                            .map(|e| e.session_id.clone());
                        if let Some(session_id) = session_id_opt {
                            self.runtime_commands
                                .push(RuntimeCommand::SetSessionFolder {
                                    session_id,
                                    folder: None,
                                });
                        }
                        KeyAction::Noop
                    }
                    _ => dispatch_menu_key(key, false),
                }
            }
            ScreenState::MainMenu | ScreenState::Settings => {
                let is_root = self.screen == ScreenState::MainMenu;
                dispatch_menu_key(key, is_root)
            }
            ScreenState::Conversation | ScreenState::Help | ScreenState::Quit => {
                dispatch_key(self.input_mode, key)
            }
            ScreenState::ModelIntelligence => dispatch_menu_key(key, false),
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
                    self.slash_dismissed = false;
                    self.focus = FocusTarget::Draft;
                    self.input_mode = InputMode::Insert;
                    self.sync_textarea_from_draft(&draft);
                    self.draft = draft;
                }
            }
            KeyAction::HistoryNext => {
                if let Some(draft) = self.history.next_entry() {
                    self.slash_dismissed = false;
                    self.focus = FocusTarget::Draft;
                    self.input_mode = InputMode::Insert;
                    self.sync_textarea_from_draft(&draft);
                    self.draft = draft;
                }
            }
            KeyAction::TextAreaInput(key_event) => {
                self.slash_dismissed = false;
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
                self.history.reset_navigation();
                self.textarea.input(key_event);
                let text = self.textarea.lines().join("\n");
                self.draft.text = text;
                self.draft.cursor = self.textarea.cursor().1;
                self.draft.dirty = true;
            }
            KeyAction::DraftInsertChar(ch) => {
                self.slash_dismissed = false;
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
                self.history.reset_navigation();
                self.draft.insert_char(ch);
            }
            KeyAction::DraftBackspace => {
                self.slash_dismissed = false;
                self.history.reset_navigation();
                self.draft.backspace();
            }
            KeyAction::DraftDelete => {
                self.slash_dismissed = false;
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
            KeyAction::ConfirmQuit => match self.screen {
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
            },
            KeyAction::MenuUp => match self.screen {
                ScreenState::MainMenu => self.menu.move_up(),
                ScreenState::SessionList => self.session_list.move_up(),
                ScreenState::CharacterManager => self.character_list.move_up(),
                ScreenState::Settings => self.settings.nav_up(),
                _ => {}
            },
            KeyAction::MenuDown => match self.screen {
                ScreenState::MainMenu => self.menu.move_down(),
                ScreenState::SessionList => self.session_list.move_down(),
                ScreenState::CharacterManager => self.character_list.move_down(),
                ScreenState::Settings => self.settings.nav_down(),
                _ => {}
            },
            KeyAction::PageUp => match self.screen {
                ScreenState::SessionList => self.session_list.page_up(),
                ScreenState::CharacterManager => self.character_list.page_up(),
                _ => {}
            },
            KeyAction::PageDown => match self.screen {
                ScreenState::SessionList => self.session_list.page_down(),
                ScreenState::CharacterManager => self.character_list.page_down(),
                _ => {}
            },
            KeyAction::MenuSelect => match self.screen {
                ScreenState::Settings => {
                    if let Some(cmd) = self.settings.enter() {
                        self.runtime_commands.push(cmd);
                    }
                }
                ScreenState::MainMenu => {
                    if let Some(item) = self.menu.selected_item() {
                        match item.id {
                            "new-chat" => {
                                self.reset_for_new_conversation();
                                self.runtime_commands.push(RuntimeCommand::CreateSession);
                                self.enter_conversation();
                                self.status_line = Some("Starting new conversation…".into());
                            }
                            "sessions" => {
                                self.screen = ScreenState::SessionList;
                                self.status_line = Some("Loading sessions…".into());
                            }
                            "characters" => {
                                self.screen = ScreenState::CharacterManager;
                                self.status_line = Some("Browsing characters".into());
                            }
                            "settings" => {
                                self.screen = ScreenState::Settings;
                                self.status_line = Some("Viewing settings".into());
                            }
                            "model-intel" => {
                                self.screen = ScreenState::ModelIntelligence;
                                self.status_line = Some("Viewing model intelligence".into());
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
                        let session_id = entry.session_id.clone();
                        let session_name = entry.name.clone();
                        self.status_line = Some(format!("Opening: {}", session_name));
                        self.runtime_commands.push(RuntimeCommand::OpenSession {
                            session_id,
                            session_name,
                        });
                        self.enter_conversation();
                    }
                }
                _ => {}
            },
            KeyAction::MenuBack => {
                match self.screen {
                    ScreenState::Settings => {
                        // If drilled into a category, go back to category list.
                        // Otherwise, navigate to main menu.
                        if !self.settings.back() {
                            self.screen = ScreenState::MainMenu;
                            self.status_line = Some("Returned to main menu".into());
                        }
                    }
                    ScreenState::SessionList
                    | ScreenState::CharacterManager
                    | ScreenState::Conversation
                    | ScreenState::ModelIntelligence => {
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
                if self.screen == ScreenState::MainMenu && self.menu.select_by_shortcut(ch) {
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
            KeyAction::CharacterEditSelected => {
                if let Some(entry) = self.character_list.selected_entry() {
                    let card_id = entry.card_id.clone();
                    self.runtime_commands
                        .push(RuntimeCommand::EditCharacter { card_id });
                }
            }
            KeyAction::CharacterImportPrompt => {
                self.character_import = CharacterImportState::default();
                self.screen = ScreenState::CharacterImport;
            }
            KeyAction::FormInsertChar(ch) => match self.screen {
                ScreenState::CharacterCreate | ScreenState::CharacterEdit => {
                    self.character_create.active_draft_mut().insert_char(ch);
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.insert_char(ch);
                }
                _ => {}
            },
            KeyAction::FormBackspace => match self.screen {
                ScreenState::CharacterCreate | ScreenState::CharacterEdit => {
                    self.character_create.active_draft_mut().backspace();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.backspace();
                }
                _ => {}
            },
            KeyAction::FormMoveCursorLeft => match self.screen {
                ScreenState::CharacterCreate | ScreenState::CharacterEdit => {
                    self.character_create.active_draft_mut().move_cursor_left();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.move_cursor_left();
                }
                _ => {}
            },
            KeyAction::FormMoveCursorRight => match self.screen {
                ScreenState::CharacterCreate | ScreenState::CharacterEdit => {
                    self.character_create.active_draft_mut().move_cursor_right();
                }
                ScreenState::CharacterImport => {
                    self.character_import.path.move_cursor_right();
                }
                _ => {}
            },
            KeyAction::FormToggleField => {
                if matches!(
                    self.screen,
                    ScreenState::CharacterCreate | ScreenState::CharacterEdit
                ) {
                    self.character_create.toggle_field();
                }
            }
            KeyAction::FormSubmit => match self.screen {
                ScreenState::CharacterCreate => {
                    let name = self.character_create.name.text.trim().to_string();
                    if name.is_empty() {
                        self.status_line = Some("Character name cannot be empty".into());
                    } else {
                        self.runtime_commands.push(RuntimeCommand::CreateCharacter {
                            name,
                            description: self.character_create.description.text.trim().to_string(),
                            system_prompt: self
                                .character_create
                                .system_prompt
                                .text
                                .trim()
                                .to_string(),
                            personality: self.character_create.personality.text.trim().to_string(),
                            scenario: self.character_create.scenario.text.trim().to_string(),
                            greeting: self.character_create.greeting.text.trim().to_string(),
                            example_dialogue: self
                                .character_create
                                .example_dialogue
                                .text
                                .trim()
                                .to_string(),
                        });
                        self.character_create = CharacterCreateState::default();
                        self.screen = ScreenState::CharacterManager;
                    }
                }
                ScreenState::CharacterEdit => {
                    let name = self.character_create.name.text.trim().to_string();
                    let card_id = self.character_create.editing_card_id.clone();
                    if name.is_empty() {
                        self.status_line = Some("Character name cannot be empty".into());
                    } else if let Some(card_id) = card_id {
                        self.runtime_commands.push(RuntimeCommand::UpdateCharacter {
                            card_id,
                            name,
                            description: self.character_create.description.text.trim().to_string(),
                            system_prompt: self
                                .character_create
                                .system_prompt
                                .text
                                .trim()
                                .to_string(),
                            personality: self.character_create.personality.text.trim().to_string(),
                            scenario: self.character_create.scenario.text.trim().to_string(),
                            greeting: self.character_create.greeting.text.trim().to_string(),
                            example_dialogue: self
                                .character_create
                                .example_dialogue
                                .text
                                .trim()
                                .to_string(),
                        });
                        self.character_create = CharacterCreateState::default();
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
                ScreenState::CharacterCreate
                | ScreenState::CharacterEdit
                | ScreenState::CharacterImport => {
                    self.character_create = CharacterCreateState::default();
                    self.screen = ScreenState::CharacterManager;
                }
                _ => {}
            },
            // Slash-popup actions
            KeyAction::SlashUp => {
                self.slash_dismissed = false;
                self.slash_move_up();
            }
            KeyAction::SlashDown => {
                self.slash_dismissed = false;
                self.slash_move_down();
            }
            KeyAction::SlashAccept => {
                self.slash_accept();
            }
            KeyAction::SlashDismiss => {
                self.slash_selected = None;
                self.slash_dismissed = true;
            }
            KeyAction::SlashTabComplete => {
                let names = self.slash_completion_names();
                match names.len() {
                    0 => {}
                    1 => {
                        let filled = names[0].clone() + " ";
                        let len = filled.len();
                        self.draft.text = filled;
                        self.draft.cursor = len;
                        self.draft.dirty = true;
                        self.slash_selected = None;
                        self.slash_dismissed = false;
                    }
                    _ => {
                        self.slash_dismissed = false;
                        self.slash_selected = Some(self.slash_selected.unwrap_or(0));
                    }
                }
            }
        }

        // Keep slash_selected / slash_dismissed in sync with current draft state.
        self.sync_slash_state();
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
            "quit" => match self.screen {
                ScreenState::MainMenu => {
                    self.screen = ScreenState::Quit;
                    self.should_quit = true;
                }
                _ => self.return_to_menu(),
            },
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
        let prompt = self.textarea.lines().join("\n");
        if prompt.trim().is_empty() {
            self.status_line = Some("Draft is empty".into());
            return;
        }

        if is_shell_command(&prompt) {
            self.history.push(prompt.clone());
            self.runtime_commands
                .push(RuntimeCommand::RunCommand { input: prompt });
            self.draft = DraftState::default();
            self.textarea = new_themed_textarea();
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
        self.textarea = new_themed_textarea();
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
        self.show_toast("★ Bookmark toggled");
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
        self.show_toast("📌 Pinned to memory");
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
            active_launch_plan: None,
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
    fn hydrate_without_draft_clears_existing_draft_and_runtime_state() {
        let mut app = ShellState::new(session_context());
        app.enter_conversation();
        app.draft = DraftState::with_text("stale draft");
        app.sync_textarea_from_draft(&app.draft.clone());
        app.session.runtime = RuntimePhase::Generating {
            request_id: "req-stale".into(),
            prompt: "old prompt".into(),
            partial_content: Some("partial".into()),
        };

        app.hydrate(AppBootstrap {
            transcript: Vec::new(),
            branches: Vec::new(),
            status_line: Some("fresh session".into()),
            draft: None,
            screen: Some(ScreenState::Conversation),
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            active_launch_plan: None,
        });

        assert!(app.draft.text.is_empty());
        assert_eq!(app.textarea.lines(), vec![String::new()]);
        assert!(matches!(app.session.runtime, RuntimePhase::Idle));
        assert_eq!(app.status_line.as_deref(), Some("fresh session"));
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
            active_launch_plan: None,
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
            active_launch_plan: None,
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
            folder: None,
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
        assert_eq!(state.status_line.as_deref(), Some("Starting new conversation…"));
        assert_eq!(state.take_runtime_commands(), vec![RuntimeCommand::CreateSession]);
    }

    #[test]
    fn new_chat_clears_existing_conversation_state() {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.session.transcript = vec![TranscriptItem::new("assistant", "stale message")];
        state.session.selected_message = Some(0);
        state.session.branches = vec![BranchItem::new("main", "main", true)];
        state.session.selected_branch = Some(0);
        state.session.runtime = RuntimePhase::Generating {
            request_id: "req-old".into(),
            prompt: "old prompt".into(),
            partial_content: Some("still streaming".into()),
        };
        state.draft = DraftState::with_text("stale draft");
        state.session_metadata = Some(SessionMetadata {
            character_name: Some("Stale".into()),
            tags: vec!["stale".into()],
        });
        state.session_stats = Some(SessionStats {
            message_count: 1,
            branch_count: 1,
            bookmark_count: 0,
        });

        state.screen = ScreenState::MainMenu;
        state.apply_action(KeyAction::MenuSelect);

        assert!(state.session.transcript.is_empty());
        assert!(state.session.branches.is_empty());
        assert_eq!(state.session.selected_message, None);
        assert_eq!(state.session.selected_branch, None);
        assert!(matches!(state.session.runtime, RuntimePhase::Idle));
        assert!(state.draft.text.is_empty());
        assert!(state.session_metadata.is_none());
        assert!(state.session_stats.is_none());
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

        state.menu.selected = 5; // "Quit"
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
                folder: None,
            },
            SessionListEntry {
                session_id: "2".into(),
                name: "Beta Chat".into(),
                character_name: None,
                message_count: 5,
                last_active: None,
                folder: None,
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
                folder: None,
            },
            SessionListEntry {
                session_id: "2".into(),
                name: "Second".into(),
                character_name: None,
                message_count: 0,
                last_active: None,
                folder: None,
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
        assert!(!filtered.is_empty());
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
        assert!(
            state.command_palette.selected > 0
                || state.command_palette.filtered_commands().len() <= 1
        );

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

        let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = state.handle_key_event(key);
        assert_eq!(action, KeyAction::TextAreaInput(key));
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
        assert!(model
            .composer
            .slash_suggestions
            .iter()
            .any(|s| s.name == "/help"));

        // Type `/he` — filtered suggestions.
        state.apply_action(KeyAction::DraftInsertChar('h'));
        state.apply_action(KeyAction::DraftInsertChar('e'));
        let model = build_render_model(&state, &layout);
        assert!(model
            .composer
            .slash_suggestions
            .iter()
            .all(|s| s.name.contains("help")));

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
        assert_eq!(
            state.character_create.active_field,
            CharacterFormField::Name
        );
        state.apply_action(KeyAction::FormToggleField);
        assert_eq!(
            state.character_create.active_field,
            CharacterFormField::Description
        );
        state.apply_action(KeyAction::FormToggleField);
        assert_eq!(
            state.character_create.active_field,
            CharacterFormField::SystemPrompt
        );
        // Cycle through remaining fields back to Name (4 more fields then wrap)
        for _ in 0..5 {
            state.apply_action(KeyAction::FormToggleField);
        }
        assert_eq!(
            state.character_create.active_field,
            CharacterFormField::Name
        );
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
            RuntimeCommand::CreateCharacter {
                name,
                system_prompt,
                ..
            } => {
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

    // ── SettingsState unit tests ──────────────────────────────────────────

    #[test]
    fn settings_state_enter_sets_drill_down() {
        let mut s = super::SettingsState::default();
        assert!(!s.drill_down);
        assert_eq!(s.selected_category, 0);
        s.enter();
        assert!(s.drill_down);
        assert_eq!(s.selected_entry, 0);
    }

    #[test]
    fn settings_state_back_returns_to_category_list() {
        let mut s = super::SettingsState::default();
        s.enter();
        assert!(s.drill_down);
        let handled = s.back();
        assert!(handled);
        assert!(!s.drill_down);
    }

    #[test]
    fn settings_state_back_returns_false_at_top_level() {
        let mut s = super::SettingsState::default();
        assert!(!s.drill_down);
        let handled = s.back();
        assert!(!handled);
    }

    #[test]
    fn settings_state_nav_down_wraps_category_list() {
        let mut s = super::SettingsState::default();
        let last = s.categories.len() - 1;
        s.selected_category = last;
        s.nav_down();
        assert_eq!(s.selected_category, 0);
    }

    #[test]
    fn settings_state_nav_up_wraps_category_list() {
        let mut s = super::SettingsState {
            selected_category: 0,
            ..Default::default()
        };
        s.nav_up();
        assert_eq!(s.selected_category, s.categories.len() - 1);
    }

    #[test]
    fn settings_state_nav_down_wraps_entry_list() {
        let mut s = super::SettingsState {
            selected_category: 3,
            ..Default::default()
        };
        s.enter();
        let count = s
            .entries_for_category(&super::SettingsCategory::Keybindings)
            .len();
        s.selected_entry = count - 1;
        s.nav_down();
        assert_eq!(s.selected_entry, 0);
    }

    #[test]
    fn settings_state_nav_up_wraps_entry_list() {
        let mut s = super::SettingsState {
            selected_category: 3,
            ..Default::default()
        };
        s.enter();
        s.selected_entry = 0;
        s.nav_up();
        let count = s
            .entries_for_category(&super::SettingsCategory::Keybindings)
            .len();
        assert_eq!(s.selected_entry, count - 1);
    }

    #[test]
    fn settings_menu_back_when_drilled_stays_on_settings_screen() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        state.settings.enter();
        assert!(state.settings.drill_down);

        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::Settings);
        assert!(!state.settings.drill_down);
    }

    #[test]
    fn settings_menu_back_at_category_list_goes_to_main_menu() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        assert!(!state.settings.drill_down);

        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
    }

    #[test]
    fn settings_menu_select_drills_into_category() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        assert!(!state.settings.drill_down);

        state.apply_action(KeyAction::MenuSelect);
        assert!(state.settings.drill_down);
        assert_eq!(state.screen, ScreenState::Settings);
    }

    #[test]
    fn settings_nav_down_up_moves_selection_on_settings_screen() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::Settings;
        assert_eq!(state.settings.selected_category, 0);

        state.apply_action(KeyAction::MenuDown);
        assert_eq!(state.settings.selected_category, 1);

        state.apply_action(KeyAction::MenuUp);
        assert_eq!(state.settings.selected_category, 0);
    }

    #[test]
    fn settings_display_and_keybindings_always_have_static_entries() {
        let s = super::SettingsState::default();
        let disp = s.entries_for_category(&super::SettingsCategory::Display);
        assert!(!disp.is_empty());
        let keys = s.entries_for_category(&super::SettingsCategory::Keybindings);
        assert!(!keys.is_empty());
    }

    #[test]
    fn settings_load_populates_backend_and_model_entries() {
        let mut s = super::SettingsState::default();
        assert!(!s.is_loaded());
        s.load(vec![
            super::SettingsEntry {
                category: "Backend".into(),
                key: "Type".into(),
                value: "koboldcpp".into(),
                kind: super::EntryKind::ReadOnly,
                pref_key: String::new(),
            },
            super::SettingsEntry {
                category: "Backend".into(),
                key: "URL".into(),
                value: "http://localhost:5001".into(),
                kind: super::EntryKind::ReadOnly,
                pref_key: String::new(),
            },
        ]);
        assert!(s.is_loaded());
        let entries = s.entries_for_category(&super::SettingsCategory::Backend);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "Type");
        assert_eq!(entries[0].1, "koboldcpp");
    }

    // ── Slash popup helper tests ─────────────────────────────────────────

    fn slash_insert_state() -> ShellState {
        let mut state = ShellState::new(session_context());
        state.enter_conversation();
        state.input_mode = InputMode::Insert;
        state
    }

    #[test]
    fn slash_popup_auto_highlights_first_item_when_typing_slash() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        assert_eq!(state.slash_selected, Some(0), "first item auto-selected");
    }

    #[test]
    fn slash_move_down_wraps_at_end() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        let len = state.slash_completion_names().len();
        assert!(len > 1, "need at least 2 suggestions for wrap test");
        // Move to the last item
        state.slash_selected = Some(len - 1);
        state.slash_move_down();
        assert_eq!(state.slash_selected, Some(0), "should wrap to first item");
    }

    #[test]
    fn slash_move_up_wraps_at_beginning() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        let len = state.slash_completion_names().len();
        assert!(len > 1, "need at least 2 suggestions for wrap test");
        // Move from first item upward
        state.slash_selected = Some(0);
        state.slash_move_up();
        assert_eq!(
            state.slash_selected,
            Some(len - 1),
            "should wrap to last item"
        );
    }

    #[test]
    fn slash_accept_fills_draft_with_command_and_space() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        // Select the first suggestion
        let names = state.slash_completion_names();
        let expected = names[0].clone() + " ";
        state.slash_selected = Some(0);
        let accepted = state.slash_accept();
        assert!(accepted, "slash_accept should return true");
        assert_eq!(state.draft.text, expected);
        assert_eq!(state.draft.cursor, expected.len());
        assert!(state.draft.dirty);
        assert_eq!(state.slash_selected, None, "selection cleared after accept");
    }

    #[test]
    fn slash_dismiss_hides_popup_until_next_edit() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        assert_eq!(state.slash_selected, Some(0));
        // Dismiss
        state.apply_action(KeyAction::SlashDismiss);
        assert_eq!(state.slash_selected, None);
        assert!(state.slash_dismissed);
        // Any draft edit should re-enable the popup
        state.apply_action(KeyAction::DraftInsertChar('h'));
        assert_eq!(state.slash_selected, Some(0), "popup reopens after edit");
    }

    #[test]
    fn slash_tab_complete_fills_immediately_when_single_match() {
        let mut state = slash_insert_state();
        // "/qui" should match exactly "quit"
        state.apply_action(KeyAction::DraftInsertChar('/'));
        state.apply_action(KeyAction::DraftInsertChar('q'));
        state.apply_action(KeyAction::DraftInsertChar('u'));
        state.apply_action(KeyAction::DraftInsertChar('i'));
        let names = state.slash_completion_names();
        assert_eq!(names.len(), 1, "expect exactly one match for /qui");
        let expected = names[0].clone() + " ";
        // Reset selected to None so tab triggers TabComplete path
        state.slash_selected = None;
        state.apply_action(KeyAction::SlashTabComplete);
        assert_eq!(state.draft.text, expected);
    }

    #[test]
    fn slash_tab_complete_opens_popup_when_multiple_matches() {
        let mut state = slash_insert_state();
        state.apply_action(KeyAction::DraftInsertChar('/'));
        let names = state.slash_completion_names();
        assert!(names.len() > 1, "expect multiple matches for /");
        // Reset selected so we test the multi-match code path
        state.slash_selected = None;
        state.slash_dismissed = false;
        state.apply_action(KeyAction::SlashTabComplete);
        assert_eq!(state.slash_selected, Some(0), "first item highlighted");
    }

    // ── Model Intelligence screen tests ───────────────────────────────

    #[test]
    fn navigating_to_model_intelligence_sets_screen() {
        // Via menu shortcut 'm'
        let mut state = ShellState::new(session_context());
        assert_eq!(state.screen, ScreenState::MainMenu);
        state.apply_action(KeyAction::MenuShortcut('m'));
        assert_eq!(state.screen, ScreenState::ModelIntelligence);
    }

    #[test]
    fn escape_from_model_intelligence_returns_to_main_menu() {
        let mut state = ShellState::new(session_context());
        state.screen = ScreenState::ModelIntelligence;
        state.apply_action(KeyAction::MenuBack);
        assert_eq!(state.screen, ScreenState::MainMenu);
        assert!(!state.should_quit);
    }

    #[test]
    fn render_model_intelligence_with_no_plan() {
        use crate::layout::build_layout_for_area;
        use crate::render::build_render_model;
        use ratatui::layout::Rect;

        let state = ShellState::new(session_context());
        assert!(state.active_launch_plan.is_none());
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        assert!(!model.model_intelligence.has_plan);
    }

    // ── Settings crash regression tests ──────────────────────────────

    #[test]
    fn settings_category_navigation_never_panics_on_oob_index() {
        use super::{SettingsCategory, SettingsState};

        let state = SettingsState {
            selected_category: 999,
            ..Default::default()
        };
        // Must not panic — falls back to categories[0]
        let cat = state.current_category();
        assert_eq!(cat, &SettingsCategory::Backend);
    }

    #[test]
    fn settings_category_navigation_never_panics_on_empty_entries() {
        use super::{SettingsCategory, SettingsState};

        let mut state = SettingsState {
            selected_category: 0,
            drill_down: true,
            ..Default::default()
        };
        let entries = state.entries_for_category(&SettingsCategory::Backend);
        assert!(entries.is_empty());
        // nav_down / nav_up with empty entry list must not panic
        state.nav_down();
        state.nav_up();
        assert_eq!(state.selected_entry, 0);
    }

    #[test]
    fn settings_session_and_model_categories_visible() {
        use super::{EntryKind, SettingsCategory, SettingsEntry, SettingsState};

        let mut state = SettingsState::default();
        // Session and Model categories should be in the list
        assert!(state.categories.contains(&SettingsCategory::Session));
        assert!(state.categories.contains(&SettingsCategory::Model));

        // Load entries as runtime would provide them
        state.load(vec![
            SettingsEntry {
                category: "Session".into(),
                key: "Session ID".into(),
                value: "abc-123".into(),
                kind: EntryKind::ReadOnly,
                pref_key: String::new(),
            },
            SettingsEntry {
                category: "Model".into(),
                key: "Max tokens".into(),
                value: "4096".into(),
                kind: EntryKind::ReadOnly,
                pref_key: String::new(),
            },
        ]);

        let session_entries = state.entries_for_category(&SettingsCategory::Session);
        assert_eq!(session_entries.len(), 1);
        assert_eq!(session_entries[0].0, "Session ID");

        let model_entries = state.entries_for_category(&SettingsCategory::Model);
        assert_eq!(model_entries.len(), 1);
        assert_eq!(model_entries[0].0, "Max tokens");
    }

    // ── New category tests ────────────────────────────────────────────────

    #[test]
    fn settings_appearance_and_launch_categories_present() {
        let s = super::SettingsState::default();
        assert!(s.categories.contains(&super::SettingsCategory::Appearance));
        assert!(s.categories.contains(&super::SettingsCategory::Launch));
    }

    #[test]
    fn settings_entry_kind_toggle_activates() {
        let mut kind = super::EntryKind::Toggle(false);
        let new_val = kind.activate();
        assert_eq!(new_val, Some("true".to_string()));
        assert_eq!(kind, super::EntryKind::Toggle(true));

        let new_val2 = kind.activate();
        assert_eq!(new_val2, Some("false".to_string()));
        assert_eq!(kind, super::EntryKind::Toggle(false));
    }

    #[test]
    fn settings_entry_kind_cycle_advances_and_wraps() {
        let mut kind = super::EntryKind::Cycle {
            options: vec!["a".into(), "b".into(), "c".into()],
            current: 0,
        };
        let v1 = kind.activate();
        assert_eq!(v1, Some("b".to_string()));
        let v2 = kind.activate();
        assert_eq!(v2, Some("c".to_string()));
        let v3 = kind.activate();
        assert_eq!(v3, Some("a".to_string())); // wraps
    }

    #[test]
    fn settings_entry_kind_readonly_does_not_activate() {
        let mut kind = super::EntryKind::ReadOnly;
        assert_eq!(kind.activate(), None);
    }

    #[test]
    fn settings_activate_entry_on_editable_raw_entry_emits_pref_changed() {
        let mut s = super::SettingsState::default();
        // Add a Toggle entry in Appearance category
        s.load(vec![super::SettingsEntry {
            category: "Appearance".into(),
            key: "Theme".into(),
            value: String::new(),
            kind: super::EntryKind::Cycle {
                options: vec!["dark-mint".into(), "ozone-dark".into()],
                current: 0,
            },
            pref_key: "theme_preset".into(),
        }]);

        // Navigate to Appearance (index 5) and drill in
        let appearance_idx = s
            .categories
            .iter()
            .position(|c| *c == super::SettingsCategory::Appearance)
            .unwrap();
        s.selected_category = appearance_idx;
        s.selected_entry = 0;
        s.drill_down = true;

        let cmd = s.activate_entry();
        assert!(cmd.is_some());
        if let Some(super::RuntimeCommand::PrefChanged { pref_key, value }) = cmd {
            assert_eq!(pref_key, "theme_preset");
            assert_eq!(value, "ozone-dark");
        } else {
            panic!("expected PrefChanged");
        }
    }

    #[test]
    fn settings_activate_entry_on_readonly_returns_none() {
        let mut s = super::SettingsState::default();
        s.load(vec![super::SettingsEntry {
            category: "Backend".into(),
            key: "Type".into(),
            value: "koboldcpp".into(),
            kind: super::EntryKind::ReadOnly,
            pref_key: String::new(),
        }]);
        // Navigate to Backend and drill in
        s.selected_category = 0; // Backend
        s.selected_entry = 0;
        s.drill_down = true;
        assert!(s.activate_entry().is_none());
    }

    #[test]
    fn settings_enter_drills_in_at_category_level() {
        let mut s = super::SettingsState::default();
        assert!(!s.drill_down);
        let cmd = s.enter();
        assert!(s.drill_down);
        assert!(cmd.is_none()); // no pref change on drill-in
    }

    #[test]
    fn settings_enter_activates_when_drilled_into_editable_category() {
        let mut s = super::SettingsState::default();
        s.load(vec![super::SettingsEntry {
            category: "Launch".into(),
            key: "Side-by-side monitor".into(),
            value: String::new(),
            kind: super::EntryKind::Toggle(false),
            pref_key: "side_by_side_monitor".into(),
        }]);
        let launch_idx = s
            .categories
            .iter()
            .position(|c| *c == super::SettingsCategory::Launch)
            .unwrap();
        s.selected_category = launch_idx;
        s.selected_entry = 0;
        s.enter(); // drill in
        let cmd = s.enter(); // activate
        assert!(matches!(
            cmd,
            Some(super::RuntimeCommand::PrefChanged { .. })
        ));
    }

    #[test]
    fn display_category_has_editable_entries_when_loaded() {
        let mut s = super::SettingsState::default();
        s.load(vec![
            super::SettingsEntry {
                category: "Display".into(),
                key: "Timestamp style".into(),
                value: String::new(),
                kind: super::EntryKind::Cycle {
                    options: vec!["relative".into(), "absolute".into(), "off".into()],
                    current: 0,
                },
                pref_key: "timestamp_style".into(),
            },
            super::SettingsEntry {
                category: "Display".into(),
                key: "Message density".into(),
                value: String::new(),
                kind: super::EntryKind::Cycle {
                    options: vec!["comfortable".into(), "compact".into()],
                    current: 0,
                },
                pref_key: "message_density".into(),
            },
        ]);
        let entries = s.entries_for_category(&super::SettingsCategory::Display);
        // 2 static + 2 editable
        assert_eq!(entries.len(), 4);
        // Static entries are ReadOnly
        assert_eq!(entries[0].2, super::EntryKind::ReadOnly);
        assert_eq!(entries[1].2, super::EntryKind::ReadOnly);
        // Editable entries are Cycle
        assert!(matches!(entries[2].2, super::EntryKind::Cycle { .. }));
        assert!(matches!(entries[3].2, super::EntryKind::Cycle { .. }));
    }

    #[test]
    fn display_static_entries_are_skipped_when_activating() {
        // Regression test: selecting the first visible Display row (index 0 =
        // "Color theme", a static ReadOnly entry) must NOT accidentally activate
        // the first *raw* entry ("Timestamp style").
        let mut s = super::SettingsState::default();
        s.load(vec![
            super::SettingsEntry {
                category: "Display".into(),
                key: "Timestamp style".into(),
                value: String::new(),
                kind: super::EntryKind::Cycle {
                    options: vec!["relative".into(), "absolute".into(), "off".into()],
                    current: 0,
                },
                pref_key: "timestamp_style".into(),
            },
            super::SettingsEntry {
                category: "Display".into(),
                key: "Message density".into(),
                value: String::new(),
                kind: super::EntryKind::Cycle {
                    options: vec!["comfortable".into(), "compact".into()],
                    current: 0,
                },
                pref_key: "message_density".into(),
            },
        ]);

        let display_idx = s
            .categories
            .iter()
            .position(|c| *c == super::SettingsCategory::Display)
            .unwrap();
        s.selected_category = display_idx;
        s.drill_down = true;

        // Index 0 = static "Color theme" → must be no-op
        s.selected_entry = 0;
        assert!(
            s.activate_entry().is_none(),
            "static Color theme must be no-op"
        );

        // Index 1 = static "Wide mode threshold" → must be no-op
        s.selected_entry = 1;
        assert!(
            s.activate_entry().is_none(),
            "static Wide mode must be no-op"
        );

        // Index 2 = runtime "Timestamp style" → must emit PrefChanged
        s.selected_entry = 2;
        let cmd = s.activate_entry();
        match cmd {
            Some(super::RuntimeCommand::PrefChanged { pref_key, .. }) => {
                assert_eq!(pref_key, "timestamp_style");
            }
            other => panic!("Expected PrefChanged for timestamp_style, got {:?}", other),
        }

        // Index 3 = runtime "Message density" → must emit PrefChanged
        s.selected_entry = 3;
        let cmd = s.activate_entry();
        match cmd {
            Some(super::RuntimeCommand::PrefChanged { pref_key, .. }) => {
                assert_eq!(pref_key, "message_density");
            }
            other => panic!("Expected PrefChanged for message_density, got {:?}", other),
        }
    }

    #[test]
    fn selecting_session_emits_open_session_command() {
        let session_id =
            ozone_core::session::SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let mut app =
            super::ShellState::new(super::SessionContext::new(session_id, "Original Session"));

        // Populate session list and navigate to it.
        app.session_list.entries = vec![super::SessionListEntry {
            session_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            name: "Test Session Alpha".into(),
            character_name: None,
            message_count: 5,
            last_active: None,
            folder: None,
        }];
        app.screen = super::ScreenState::SessionList;
        app.session_list.selected = 0;

        // Select the session.
        app.apply_action(super::super::input::KeyAction::MenuSelect);

        let cmds = app.take_runtime_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            super::RuntimeCommand::OpenSession {
                session_id,
                session_name,
            } => {
                assert_eq!(session_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
                assert_eq!(session_name, "Test Session Alpha");
            }
            other => panic!("Expected OpenSession, got {:?}", other),
        }
        assert_eq!(app.screen, super::ScreenState::Conversation);
    }
}
