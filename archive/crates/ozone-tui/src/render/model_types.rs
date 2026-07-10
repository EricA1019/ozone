//! Render model types — pure data structures for the TUI rendering pipeline.
//!
//! This module contains all the intermediate representation types between
//! the application state (`ShellState`) and the final rendered output.

use crate::state::EntryKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEntryModel {
    pub author: String,
    pub content: String,
    pub is_bookmarked: bool,
    pub selected: bool,
    pub is_streaming: bool,
    /// Pre-formatted display timestamp, e.g. "2:15 PM".
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPaneModel {
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<ConversationEntryModel>,
    pub empty_state: String,
    pub hint: String,
    pub tick_count: u64,
    pub scroll_offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConversationViewport {
    pub visible_height: usize,
    pub max_scroll: usize,
    pub default_scroll_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerPaneModel {
    pub title: String,
    pub mode: String,
    pub lines: Vec<String>,
    pub placeholder: String,
    pub cursor: usize,
    pub dirty: bool,
    pub hint: String,
    pub show_cursor: bool,
    /// Inline slash-command suggestions shown when draft starts with `/`.
    pub slash_suggestions: Vec<SlashSuggestion>,
    /// Index of the currently highlighted suggestion (None = no selection).
    pub slash_selected: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashSuggestion {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPaneModel {
    pub title: String,
    pub summary: String,
    pub notifications: Vec<String>,
    pub hint: String,
    pub mode_badge: Option<String>,
    pub session_title: String,
    pub character_label: Option<String>,
    pub message_count: usize,
    pub selected_index: Option<usize>,
    /// Compact-mode VRAM usage hint shown at right edge of the footer bar.
    pub vram_hint: Option<String>,
    /// Context token usage bar: used / max, rendered as a string like "[████░░░░ 50%]".
    /// None if no budget data is available yet.
    pub context_bar: Option<String>,
    /// Raw token budget for programmatic checks (e.g. warning color when > 80%).
    pub token_budget: Option<(u32, u32)>, // (used, max)
    /// Compact-mode high-value notice that would otherwise be hidden in the inspector.
    pub compact_notice: Option<String>,
}

/// Structured model info for the inspector pane's Model Info section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfoDisplay {
    pub estimated_vram_mb: u32,
    pub estimated_ram_mb: u32,
    pub gpu_layers: u32,
    pub cpu_layers: u32,
    pub mode_label: String,
    pub source_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorPaneModel {
    pub title: String,
    pub lines: Vec<String>,
    /// Model info section, rendered with brand colors in wide mode.
    pub model_info: Option<ModelInfoDisplay>,
    /// Context token usage bar: used / max, rendered as a string like "[████░░░░ 50%]".
    /// None if no budget data is available yet.
    pub context_bar: Option<String>,
    /// Formatted token budget string like "context 12,450 / 128,000 tokens (94%)".
    pub token_budget: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellIndicators {
    pub screen: String,
    pub input_mode: String,
    pub focus: String,
    pub selection: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayRenderModel {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintItem {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteRenderModel {
    pub input: String,
    pub entries: Vec<CommandPaletteEntry>,
    pub selected: usize,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteEntry {
    pub name: String,
    pub description: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainMenuRenderModel {
    pub header_lines: Vec<String>,
    pub items: Vec<MenuItemRenderModel>,
    pub hint: String,
    pub session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemRenderModel {
    pub label: String,
    pub description: String,
    pub shortcut: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListRenderModel {
    pub items: Vec<SessionListItemRenderModel>,
    pub selected: usize,
    pub filter: String,
    pub total_count: usize,
    pub visible_count: usize,
    pub hint: String,
    pub loading: bool,
    pub folder_picker: Option<FolderPickerRenderModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPickerRenderModel {
    pub folders: Vec<String>,
    pub selected: usize,
    pub creating: bool,
    pub new_folder_input: String,
    pub new_folder_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionListItemRenderModel {
    Header { name: String },
    Entry(SessionListEntryRenderModel),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntryRenderModel {
    pub name: String,
    pub character: String,
    pub message_count: String,
    pub last_active: String,
    pub last_message_preview: String,
    pub selected: bool,
    /// True when entries should be indented under a folder header.
    pub indented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterListRenderModel {
    pub entries: Vec<CharacterListEntryRenderModel>,
    pub selected_detail: Option<CharacterDetailRenderModel>,
    pub total_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterListEntryRenderModel {
    pub name: String,
    pub description: String,
    pub session_count: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterDetailRenderModel {
    pub name: String,
    pub description: String,
    pub greeting: Option<String>,
    pub session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsCategoryRenderItem {
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEntryRenderItem {
    pub label: String,
    pub value: String,
    pub kind: EntryKind,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRenderModel {
    /// `false` = showing category list; `true` = inside a category.
    pub drill_down: bool,
    /// Category label shown in breadcrumb when drilled in (e.g. "Backend").
    pub breadcrumb_category: Option<String>,
    pub categories: Vec<SettingsCategoryRenderItem>,
    /// Entries for the currently selected category (populated regardless of drill_down).
    pub entries: Vec<SettingsEntryRenderItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterFormRenderModel {
    pub form_type: CharacterFormType,
    /// All 7 editable text fields in display order.
    pub fields: Vec<CharacterFieldRenderModel>,
    pub active_field: crate::state::CharacterFormField,
    pub path_text: String,
    pub path_cursor: usize,
}

/// One editable field in the character form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterFieldRenderModel {
    pub field: crate::state::CharacterFormField,
    pub label: &'static str,
    pub text: String,
    pub cursor: usize,
    pub placeholder: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterFormType {
    Create,
    Edit,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIntelligenceRenderModel {
    pub has_plan: bool,
    pub model_name: String,
    pub mode_label: String,
    pub gpu_layers: u32,
    pub total_layers: u32,
    pub context_size: u32,
    pub estimated_vram_mb: u32,
    pub estimated_ram_mb: u32,
    pub source: String,
    pub rationale: String,
    pub estimated: bool,
    pub layer_source_label: String,
    pub layer_note: Option<String>,
    pub hint: String,
}

impl Default for ModelIntelligenceRenderModel {
    fn default() -> Self {
        Self {
            has_plan: false,
            model_name: String::new(),
            mode_label: String::new(),
            gpu_layers: 0,
            total_layers: 0,
            context_size: 0,
            estimated_vram_mb: 0,
            estimated_ram_mb: 0,
            source: String::new(),
            rationale: String::new(),
            estimated: false,
            layer_source_label: String::new(),
            layer_note: None,
            hint: "Esc → back".into(),
        }
    }
}

/// The complete render model — pure data that the TUI rendering pipeline
/// needs to paint a single frame. Produced by `build_render_model` from
/// `ShellState` + `LayoutModel`, consumed by `render_shell` and all
/// sub-renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderModel {
    pub title: String,
    pub subtitle: String,
    pub conversation: ConversationPaneModel,
    pub composer: ComposerPaneModel,
    pub status: StatusPaneModel,
    pub inspector: Option<InspectorPaneModel>,
    pub indicators: ShellIndicators,
    pub overlay: Option<OverlayRenderModel>,
    pub main_menu: Option<MainMenuRenderModel>,
    pub session_list: Option<SessionListRenderModel>,
    pub character_list: Option<CharacterListRenderModel>,
    pub character_form: Option<CharacterFormRenderModel>,
    pub settings: Option<SettingsRenderModel>,
    pub model_intelligence: ModelIntelligenceRenderModel,
    pub hints: Vec<HintItem>,
    pub breadcrumb: String,
    pub command_palette: Option<CommandPaletteRenderModel>,
    pub toast_message: Option<String>,
    /// Memory metadata for the memories overlay (populated at session load)
    pub memory_metadata: Option<crate::state::TuiSessionMemoryMetadata>,
}
