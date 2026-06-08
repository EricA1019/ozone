use super::enums_runtime::CharacterDetail;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Full-screen overlay showing all session memories (pinned and notes).
    MemoriesOverlay,
    /// Full-screen overlay showing the active character card details.
    CharacterOverlay(CharacterDetail),
}

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
    pub fn label(&self) -> &str {
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

#[derive(Debug, Clone)]
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

impl SettingsState {
    pub fn new() -> Self {
        Self {
            categories: vec![],
            selected_category: 0,
            selected_entry: 0,
            drill_down: false,
            raw_entries: vec![],
            loaded: false,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn load(&mut self, entries: Vec<SettingsEntry>) {
        self.raw_entries = entries;
        self.loaded = true;
    }

    /// Returns the currently selected category name.
    pub fn current_category(&self) -> Option<&str> {
        self.categories.get(self.selected_category).map(|c| c.label())
    }

    /// Get entries for the currently selected category.
    pub fn entries_for_category(&self) -> Vec<(String, String, EntryKind)> {
        let category = self.current_category();
        if let Some(cat) = category {
            self.raw_entries
                .iter()
                .filter(|e| e.category == cat)
                .map(|e| (e.key.clone(), e.value.clone(), e.kind.clone()))
                .collect()
        } else {
            vec![]
        }
    }
}


impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
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



