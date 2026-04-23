use ratatui::{
    layout::{Alignment, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};
use tui_textarea::TextArea;

use crate::{
    app::{
        CommandEntry, ContextPreview, EntryKind, FocusTarget, FolderPickerState, InspectorFocus,
        RuntimePhase, ScreenState, ShellState, VisibleSessionItem,
    },
    input::InputMode,
    layout::{LayoutMode, LayoutModel, PaneId, PaneLayout},
    theme,
};

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
    pub message_count: usize,
    pub selected_index: Option<usize>,
    /// Compact-mode VRAM usage hint shown at right edge of the footer bar.
    pub vram_hint: Option<String>,
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
    pub active_field: crate::app::CharacterFormField,
    pub path_text: String,
    pub path_cursor: usize,
}

/// One editable field in the character form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterFieldRenderModel {
    pub field: crate::app::CharacterFormField,
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
}

fn build_folder_picker_model(picker: &FolderPickerState) -> Option<FolderPickerRenderModel> {
    if !picker.visible {
        return None;
    }
    Some(FolderPickerRenderModel {
        folders: picker.folders.clone(),
        selected: picker.selected,
        creating: picker.creating,
        new_folder_input: picker.new_folder_input.clone(),
        new_folder_index: picker.new_folder_index(),
    })
}

pub fn build_render_model(state: &ShellState, layout: &LayoutModel) -> RenderModel {
    let indicators = ShellIndicators {
        screen: screen_label(state.screen).into(),
        input_mode: input_mode_label(state.input_mode).into(),
        focus: focus_label(state.focus).into(),
        selection: selection_label(state),
        branch: branch_label(state),
    };

    let title = format!("{} ozone+ — {}", theme::HEX, state.session.context.title);
    let shell_label = match layout.mode {
        LayoutMode::Compact => "compact shell",
        LayoutMode::Wide => "wide shell",
    };
    let subtitle = format!(
        "{} · {} · {}",
        indicators.input_mode, indicators.focus, shell_label
    );

    let conv_entry_count = state.session.transcript.len();
    let conversation = ConversationPaneModel {
        title: if conv_entry_count > 0 {
            if let Some(sel) = state.session.selected_message {
                format!("Conversation [{}/{}]", sel + 1, conv_entry_count)
            } else {
                "Conversation".into()
            }
        } else {
            "Conversation".into()
        },
        subtitle: format!("{} · {}", indicators.selection, indicators.branch),
        entries: {
            let mut entries: Vec<ConversationEntryModel> = state
                .session
                .transcript
                .iter()
                .enumerate()
                .map(|(index, item)| ConversationEntryModel {
                    author: item.author.clone(),
                    content: item.content.clone(),
                    is_bookmarked: item.is_bookmarked,
                    selected: state.session.selected_message == Some(index),
                    is_streaming: false,
                    timestamp: item.timestamp.clone(),
                })
                .collect();
            // Show streamed partial content as a transient entry while generating.
            if let Some(partial) = state.session.runtime.partial_content() {
                entries.push(ConversationEntryModel {
                    author: "assistant".into(),
                    content: format!("{partial}▍"),
                    is_bookmarked: false,
                    selected: false,
                    is_streaming: true,
                    timestamp: None,
                });
            }
            entries
        },
        empty_state: "⬡ Start a conversation — press i to enter insert mode".into(),
        hint: if state.message_edit.is_some() {
            "Editing selected message · Enter save · Esc cancel · Ctrl+U undo · Ctrl+R redo · F2 inspector"
                .into()
        } else {
            "j/k scroll · ↑↓ select · Ctrl+I edit · b bookmark · Ctrl+K pin · / commands · Tab focus · i insert · I inspector · ? help"
                .into()
        },
        tick_count: state.tick_count,
        scroll_offset: state.conversation_scroll,
    };

    let composer = ComposerPaneModel {
        title: if state.message_edit.is_some() {
            "Edit Message".into()
        } else {
            "Composer".into()
        },
        mode: if state.message_edit.is_some() {
            format!("edit · {}", indicators.input_mode)
        } else {
            indicators.input_mode.clone()
        },
        lines: if state.draft.text.is_empty() {
            Vec::new()
        } else {
            state.draft.text.split('\n').map(str::to_owned).collect()
        },
        placeholder: if state.message_edit.is_some() {
            "Edit selected transcript message".into()
        } else {
            "Type a message · / or : for commands".into()
        },
        cursor: state.draft.cursor,
        dirty: state.draft.dirty,
        hint: if state.message_edit.is_some() {
            "Enter save edit · Esc cancel · Ctrl+U undo · Ctrl+R redo · arrows/tab stay in editor · F2 inspector"
                .into()
        } else {
            composer_hint(state.input_mode).into()
        },
        show_cursor: state.focus == FocusTarget::Draft && !state.command_palette.open,
        slash_suggestions: if state.message_edit.is_some() || state.slash_dismissed {
            Vec::new()
        } else {
            build_slash_suggestions(&state.draft.text)
        },
        slash_selected: state.slash_selected,
    };

    let mut notifications = vec![
        format!("screen {} · focus {}", indicators.screen, indicators.focus),
        format!("{} · {}", indicators.selection, indicators.branch),
        state
            .session_stats
            .as_ref()
            .map(|stats| {
                format!(
                    "{} messages · {} branches · {} bookmarks",
                    stats.message_count, stats.branch_count, stats.bookmark_count
                )
            })
            .unwrap_or_else(|| "session stats pending".into()),
        runtime_label(&state.session.runtime),
        inspector_visibility_label(layout, state),
        context_status_line(state),
    ];
    if let Some(browser) = state.recall_browser.as_ref() {
        notifications.push(format!("{} · {}", browser.title, browser.summary));
    }

    let mode_badge = if state.screen == ScreenState::Conversation {
        Some(input_mode_label(state.input_mode).to_uppercase())
    } else {
        None
    };

    let model_info = state
        .active_launch_plan
        .as_ref()
        .map(|plan| ModelInfoDisplay {
            estimated_vram_mb: plan.estimated_vram_mb,
            estimated_ram_mb: plan.estimated_ram_mb,
            gpu_layers: plan.gpu_layers_display(),
            cpu_layers: plan.cpu_layers,
            mode_label: plan.mode.display_label().to_string(),
            source_label: plan.layer_source_label.clone(),
        });

    let vram_hint = if matches!(layout.mode, LayoutMode::Compact) {
        state.active_launch_plan.as_ref().map(|plan| {
            let gb = plan.estimated_vram_mb as f64 / 1024.0;
            format!("{gb:.1}G VRAM")
        })
    } else {
        None
    };

    let status = StatusPaneModel {
        title: "Status".into(),
        summary: state
            .status_line
            .clone()
            .unwrap_or_else(|| runtime_label(&state.session.runtime)),
        notifications,
        hint: "⬡ ? help · q quit".into(),
        mode_badge,
        session_title: state.session.context.title.clone(),
        message_count: state.session.transcript.len(),
        selected_index: if state.session.transcript.is_empty() {
            None
        } else {
            state.session.selected_message
        },
        vram_hint,
    };

    let inspector = layout.inspector.map(|_| InspectorPaneModel {
        title: "Inspector".into(),
        lines: inspector_lines(state, &indicators),
        model_info: model_info.clone(),
    });

    let main_menu = if state.screen == ScreenState::MainMenu {
        Some(MainMenuRenderModel {
            header_lines: vec![
                format!("{}  {}  {}", theme::HEX, theme::HEX_FILLED, theme::HEX),
                "ozone+".into(),
                "local-LLM chat shell".into(),
            ],
            items: state
                .menu
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| MenuItemRenderModel {
                    label: item.label.to_string(),
                    description: item.description.to_string(),
                    shortcut: item.shortcut.map(|c| c.to_string()).unwrap_or_default(),
                    selected: i == state.menu.selected,
                })
                .collect(),
            hint: "j/k navigate · Enter select · 1-4/m quick-jump · q quit · ? help".into(),
            session_count: state.session_list.entries.len(),
        })
    } else {
        None
    };

    let session_list = if state.screen == ScreenState::SessionList {
        let grouped = state.session_list.grouped_visible_items();
        let has_headers = grouped
            .iter()
            .any(|i| matches!(i, VisibleSessionItem::FolderHeader { .. }));
        let items = grouped
            .into_iter()
            .map(|item| match item {
                VisibleSessionItem::FolderHeader { name } => {
                    SessionListItemRenderModel::Header { name }
                }
                VisibleSessionItem::Entry {
                    entry,
                    visual_index,
                } => SessionListItemRenderModel::Entry(SessionListEntryRenderModel {
                    name: entry.name.clone(),
                    character: entry
                        .character_name
                        .clone()
                        .unwrap_or_else(|| "\u{2014}".into()),
                    message_count: format!("{} msgs", entry.message_count),
                    last_active: entry
                        .last_active
                        .clone()
                        .unwrap_or_else(|| "\u{2014}".into()),
                    selected: visual_index == state.session_list.selected,
                    indented: has_headers,
                }),
            })
            .collect();
        Some(SessionListRenderModel {
            items,
            selected: state.session_list.selected,
            filter: state.session_list.filter.clone(),
            total_count: state.session_list.entries.len(),
            visible_count: state.session_list.visible_count(),
            hint: "j/k navigate \u{00b7} Enter open \u{00b7} n new session \u{00b7} f folder \u{00b7} F unfile \u{00b7} / filter \u{00b7} q/Esc back"
                .into(),
            loading: state.session_list.loading,
            folder_picker: build_folder_picker_model(&state.folder_picker),
        })
    } else {
        None
    };

    let character_list = if state.screen == ScreenState::CharacterManager {
        let entries = state
            .character_list
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| CharacterListEntryRenderModel {
                name: e.name.clone(),
                description: if e.description.chars().count() > 60 {
                    let truncated: String = e.description.chars().take(59).collect();
                    format!("{truncated}\u{2026}")
                } else {
                    e.description.clone()
                },
                session_count: format!("{} sessions", e.session_count),
                selected: i == state.character_list.selected,
            })
            .collect();
        let selected_detail =
            state
                .character_list
                .selected_entry()
                .map(|e| CharacterDetailRenderModel {
                    name: e.name.clone(),
                    description: e.description.clone(),
                    session_count: e.session_count,
                });
        Some(CharacterListRenderModel {
            total_count: state.character_list.entries.len(),
            entries,
            selected_detail,
        })
    } else {
        None
    };

    let settings = if state.screen == ScreenState::Settings {
        let categories: Vec<SettingsCategoryRenderItem> = state
            .settings
            .categories
            .iter()
            .enumerate()
            .map(|(i, cat)| SettingsCategoryRenderItem {
                label: cat.label().to_string(),
                selected: i == state.settings.selected_category,
            })
            .collect();

        let current_cat = state.settings.current_category().clone();
        let entries: Vec<SettingsEntryRenderItem> = state
            .settings
            .entries_for_category(&current_cat)
            .into_iter()
            .enumerate()
            .map(|(i, (label, value, kind))| SettingsEntryRenderItem {
                label,
                value,
                kind,
                selected: i == state.settings.selected_entry,
            })
            .collect();

        Some(SettingsRenderModel {
            drill_down: state.settings.drill_down,
            breadcrumb_category: if state.settings.drill_down {
                Some(current_cat.label().to_string())
            } else {
                None
            },
            categories,
            entries,
        })
    } else {
        None
    };

    let character_form = match state.screen {
        ScreenState::CharacterCreate | ScreenState::CharacterEdit => {
            let form_type = if state.screen == ScreenState::CharacterEdit {
                CharacterFormType::Edit
            } else {
                CharacterFormType::Create
            };
            let cs = &state.character_create;
            let fields = vec![
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::Name,
                    label: "Name",
                    text: cs.name.text.clone(),
                    cursor: cs.name.cursor,
                    placeholder: "(type character name)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::Description,
                    label: "Description",
                    text: cs.description.text.clone(),
                    cursor: cs.description.cursor,
                    placeholder: "(short tagline or description)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::SystemPrompt,
                    label: "System Prompt",
                    text: cs.system_prompt.text.clone(),
                    cursor: cs.system_prompt.cursor,
                    placeholder: "(instructions for the AI — personality, rules, context)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::Personality,
                    label: "Personality",
                    text: cs.personality.text.clone(),
                    cursor: cs.personality.cursor,
                    placeholder: "(personality traits — kind, sarcastic, stoic…)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::Scenario,
                    label: "Scenario",
                    text: cs.scenario.text.clone(),
                    cursor: cs.scenario.cursor,
                    placeholder: "(the setting or situation for conversations)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::Greeting,
                    label: "First Message",
                    text: cs.greeting.text.clone(),
                    cursor: cs.greeting.cursor,
                    placeholder: "(character's opening message)",
                },
                CharacterFieldRenderModel {
                    field: crate::app::CharacterFormField::ExampleDialogue,
                    label: "Example Dialogue",
                    text: cs.example_dialogue.text.clone(),
                    cursor: cs.example_dialogue.cursor,
                    placeholder: "(example conversation to guide style)",
                },
            ];
            Some(CharacterFormRenderModel {
                form_type,
                fields,
                active_field: cs.active_field,
                path_text: String::new(),
                path_cursor: 0,
            })
        }
        ScreenState::CharacterImport => Some(CharacterFormRenderModel {
            form_type: CharacterFormType::Import,
            fields: Vec::new(),
            active_field: crate::app::CharacterFormField::Name,
            path_text: state.character_import.path.text.clone(),
            path_cursor: state.character_import.path.cursor,
        }),
        _ => None,
    };

    let model_intelligence = match &state.active_launch_plan {
        Some(plan) => ModelIntelligenceRenderModel {
            has_plan: true,
            model_name: plan.model_name.clone(),
            mode_label: plan.mode.display_label().to_string(),
            gpu_layers: plan.gpu_layers_display(),
            total_layers: plan.total_layers,
            context_size: plan.context_size,
            estimated_vram_mb: plan.estimated_vram_mb,
            estimated_ram_mb: plan.estimated_ram_mb,
            source: plan.source.clone(),
            rationale: plan.rationale.clone(),
            estimated: plan.estimated,
            layer_source_label: plan.layer_source_label.clone(),
            layer_note: plan.layer_source_note.clone(),
            hint: "Esc → back to menu".into(),
        },
        None => ModelIntelligenceRenderModel::default(),
    };

    RenderModel {
        title,
        subtitle,
        conversation,
        composer,
        status,
        inspector,
        indicators,
        overlay: overlay_model(state.screen, state.input_mode),
        main_menu,
        session_list,
        character_list,
        character_form,
        settings,
        model_intelligence,
        hints: build_hints(state),
        breadcrumb: build_breadcrumb(state),
        command_palette: if state.command_palette.open {
            let filtered = state.command_palette.filtered_commands();
            Some(CommandPaletteRenderModel {
                input: state.command_palette.input_text(),
                entries: filtered
                    .iter()
                    .enumerate()
                    .map(|(i, cmd)| CommandPaletteEntry {
                        name: format!("/{}", cmd.name),
                        description: cmd.description.clone(),
                        selected: i == state.command_palette.selected,
                    })
                    .collect(),
                selected: state.command_palette.selected,
                hint: "Enter run/fill · Esc close · ↑↓ choose · Ctrl+U/Ctrl+R undo-redo".into(),
            })
        } else {
            None
        },
        toast_message: state.active_toast().map(str::to_owned),
    }
}

fn build_hints(state: &ShellState) -> Vec<HintItem> {
    match state.screen {
        ScreenState::MainMenu => vec![
            HintItem {
                key: "↑↓".into(),
                action: "Navigate".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "Select".into(),
            },
            HintItem {
                key: "1-4".into(),
                action: "Quick select".into(),
            },
            HintItem {
                key: "q".into(),
                action: "Quit".into(),
            },
            HintItem {
                key: "/".into(),
                action: "Commands".into(),
            },
        ],
        ScreenState::SessionList => vec![
            HintItem {
                key: "↑↓".into(),
                action: "Navigate".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "Open".into(),
            },
            HintItem {
                key: "f".into(),
                action: "Folder".into(),
            },
            HintItem {
                key: "F".into(),
                action: "Unfile".into(),
            },
            HintItem {
                key: "/".into(),
                action: "Commands".into(),
            },
            HintItem {
                key: "q".into(),
                action: "Back".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "Back".into(),
            },
        ],
        ScreenState::CharacterManager => vec![
            HintItem {
                key: "↑↓".into(),
                action: "Navigate".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "Chat".into(),
            },
            HintItem {
                key: "n".into(),
                action: "New character".into(),
            },
            HintItem {
                key: "e".into(),
                action: "Edit".into(),
            },
            HintItem {
                key: "i".into(),
                action: "Import JSON".into(),
            },
            HintItem {
                key: "/".into(),
                action: "Commands".into(),
            },
            HintItem {
                key: "q".into(),
                action: "Back".into(),
            },
        ],
        ScreenState::CharacterCreate | ScreenState::CharacterEdit => vec![
            HintItem {
                key: "Tab".into(),
                action: "Switch field".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "Save".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "Cancel".into(),
            },
        ],
        ScreenState::CharacterImport => vec![
            HintItem {
                key: "Enter".into(),
                action: "Import".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "Cancel".into(),
            },
        ],
        ScreenState::Settings => {
            if state.settings.drill_down {
                vec![
                    HintItem {
                        key: "↑↓".into(),
                        action: "Navigate".into(),
                    },
                    HintItem {
                        key: "Esc".into(),
                        action: "Categories".into(),
                    },
                ]
            } else {
                vec![
                    HintItem {
                        key: "↑↓".into(),
                        action: "Navigate".into(),
                    },
                    HintItem {
                        key: "Enter".into(),
                        action: "Open".into(),
                    },
                    HintItem {
                        key: "q/Esc".into(),
                        action: "Main menu".into(),
                    },
                ]
            }
        }
        ScreenState::Conversation => vec![
            HintItem {
                key: "i".into(),
                action: "Insert mode".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "Send".into(),
            },
            HintItem {
                key: "?".into(),
                action: "Help".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "Menu".into(),
            },
            HintItem {
                key: "/".into(),
                action: "Commands".into(),
            },
        ],
        ScreenState::Help => vec![
            HintItem {
                key: "Esc".into(),
                action: "Back".into(),
            },
            HintItem {
                key: "q".into(),
                action: "Quit".into(),
            },
        ],
        ScreenState::ModelIntelligence => vec![HintItem {
            key: "Esc".into(),
            action: "Main menu".into(),
        }],
        _ => vec![HintItem {
            key: "Esc".into(),
            action: "Back".into(),
        }],
    }
}

fn build_breadcrumb(state: &ShellState) -> String {
    match state.screen {
        ScreenState::MainMenu => "⬡ Ozone+".into(),
        ScreenState::SessionList => "⬡ Ozone+ › Sessions".into(),
        ScreenState::CharacterManager => "⬡ Ozone+ › Characters".into(),
        ScreenState::CharacterCreate => "⬡ Ozone+ › Characters › New".into(),
        ScreenState::CharacterEdit => "⬡ Ozone+ › Characters › Edit".into(),
        ScreenState::CharacterImport => "⬡ Ozone+ › Characters › Import".into(),
        ScreenState::Settings => {
            if state.settings.drill_down {
                format!(
                    "⬡ Ozone+ › Settings › {}",
                    state.settings.current_category().label()
                )
            } else {
                "⬡ Ozone+ › Settings".into()
            }
        }
        ScreenState::Conversation => format!("⬡ Ozone+ › {}", state.session.context.title),
        ScreenState::Help => "⬡ Ozone+ › Help".into(),
        ScreenState::Quit => "⬡ Ozone+".into(),
        ScreenState::ModelIntelligence => "⬡ Ozone+ › Model Intel".into(),
    }
}

pub fn render_shell(
    frame: &mut Frame,
    layout: &LayoutModel,
    model: &RenderModel,
    textarea: Option<&TextArea<'static>>,
    palette_textarea: Option<&TextArea<'static>>,
) {
    let full_area = frame.area();
    frame.render_widget(Clear, full_area);

    // Reserve bottom row for hints — skip when the 1-row status footer occupies that row,
    // so hints don't overwrite the footer content.
    let hint_candidate_y = full_area.y + full_area.height.saturating_sub(1);
    let status_occupies_bottom = layout.status.area.height > 0
        && layout.status.area.height <= 1
        && layout.status.area.y == hint_candidate_y;
    let hint_area = if full_area.height > 3 && !model.hints.is_empty() && !status_occupies_bottom {
        Rect::new(full_area.x, hint_candidate_y, full_area.width, 1)
    } else {
        Rect::default()
    };

    // Reserve top row for breadcrumb
    let breadcrumb_area = if full_area.height > 5 {
        Rect::new(
            full_area.x + 1,
            full_area.y,
            full_area.width.saturating_sub(2),
            1,
        )
    } else {
        Rect::default()
    };

    // Full-screen menu screens
    if let Some(menu_pane) = layout.menu_area.as_ref() {
        if let Some(menu_model) = model.main_menu.as_ref() {
            render_main_menu(frame, menu_pane, menu_model);
        } else if let Some(session_model) = model.session_list.as_ref() {
            render_session_list(frame, menu_pane, session_model);
        } else if let Some(char_model) = model.character_list.as_ref() {
            render_character_list(frame, menu_pane, char_model);
        } else if let Some(form_model) = model.character_form.as_ref() {
            render_character_form(frame, menu_pane, form_model);
        } else if let Some(settings_model) = model.settings.as_ref() {
            render_settings(frame, menu_pane, settings_model);
        } else if model.indicators.screen == "model intelligence" {
            render_model_intelligence(frame, menu_pane, &model.model_intelligence);
        } else {
            render_menu_placeholder(frame, menu_pane, &model.title);
        }

        // Render overlays on top of menu screens
        if breadcrumb_area.height > 0 {
            render_breadcrumb(frame, breadcrumb_area, &model.breadcrumb);
        }
        if hint_area.height > 0 {
            render_hints(frame, hint_area, &model.hints);
        }

        // Command palette overlay (on top of everything)
        if let Some(palette) = model.command_palette.as_ref() {
            render_command_palette(frame, palette, palette_textarea);
        }
        return;
    }

    render_conversation(
        frame,
        &layout.conversation,
        model,
        layout.focused == PaneId::Conversation,
    );
    render_composer(
        frame,
        &layout.composer,
        &model.composer,
        layout.focused == PaneId::Composer,
        textarea,
    );
    render_status(
        frame,
        &layout.status,
        &model.status,
        layout.focused == PaneId::Status,
    );

    if let (Some(pane), Some(model)) = (layout.inspector.as_ref(), model.inspector.as_ref()) {
        render_inspector(frame, pane, model, layout.focused == PaneId::Inspector);
    }

    if let (Some(pane), Some(overlay_model)) = (layout.overlay.as_ref(), model.overlay.as_ref()) {
        if pane.pane == PaneId::HelpOverlay {
            render_help_overlay(frame, pane.area);
        } else {
            render_overlay(frame, pane, overlay_model);
        }
    }

    // Toast notification (above conversation, below help overlay)
    if let Some(toast_msg) = model.toast_message.as_deref() {
        render_toast(frame, frame.area(), toast_msg);
    }

    // Render hints and breadcrumb last (on top)
    if breadcrumb_area.height > 0 {
        render_breadcrumb(frame, breadcrumb_area, &model.breadcrumb);
    }
    if hint_area.height > 0 {
        render_hints(frame, hint_area, &model.hints);
    }

    // Slash suggestion popup (floats above composer, below command palette)
    if model.command_palette.is_none() {
        render_slash_popup(frame, &layout.composer, &model.composer);
    }

    // Command palette overlay (on top of everything)
    if let Some(palette) = model.command_palette.as_ref() {
        render_command_palette(frame, palette, palette_textarea);
    }
}

fn render_hints(frame: &mut Frame, area: Rect, hints: &[HintItem]) {
    if hints.is_empty() || area.height == 0 {
        return;
    }
    let spans: Vec<Span> = hints
        .iter()
        .enumerate()
        .flat_map(|(i, h)| {
            let mut s = vec![
                Span::styled(format!(" {} ", h.key), theme::accent_style()),
                Span::styled(h.action.clone(), theme::dim_style()),
            ];
            if i < hints.len() - 1 {
                s.push(Span::styled("  │  ", theme::dim_style()));
            }
            s
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

fn render_command_palette(
    frame: &mut Frame,
    model: &CommandPaletteRenderModel,
    textarea: Option<&TextArea<'static>>,
) {
    let area = frame.area();
    let width = 60u16.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let max_entries = 8usize.min(model.entries.len());
    let list_rows = max_entries.max(1);
    let height = (list_rows as u16) + 5; // input + separator + entries + hint + border
    let palette_area = Rect::new(x, area.y + 2, width, height);

    frame.render_widget(Clear, palette_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(" Command Palette ", theme::accent_style()));

    let inner = block.inner(palette_area);
    frame.render_widget(block, palette_area);
    if inner.height == 0 {
        return;
    }

    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    if let Some(textarea) = textarea {
        frame.render_widget(textarea, input_area);
    } else {
        let input_line = Line::from(vec![
            Span::styled(" / ", theme::accent_style()),
            Span::styled(&model.input, theme::text_style()),
            Span::styled("▌", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(input_line), input_area);
    }

    if inner.height <= 1 {
        return;
    }

    let separator_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            theme::dim_style(),
        ))),
        separator_area,
    );

    let list_height = inner.height.saturating_sub(3);
    if list_height > 0 {
        let list_area = Rect::new(inner.x, inner.y + 2, inner.width, list_height);
        let mut lines = vec![];
        if model.entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No commands match the current input",
                theme::warning_style(),
            )));
        } else {
            for entry in model.entries.iter().take(max_entries) {
                let style = if entry.selected {
                    theme::highlight_style()
                } else {
                    theme::text_style()
                };
                let marker = if entry.selected { "▸ " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(&entry.name, style),
                    Span::styled("  ", Style::default()),
                    Span::styled(&entry.description, theme::dim_style()),
                ]));
            }
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), list_area);
    }

    let hint_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(&model.hint, theme::dim_style()))),
        hint_area,
    );
}

/// Floating autocomplete popup rendered ABOVE the composer pane.
/// Only shown when there are slash suggestions and the command palette is NOT open.
fn render_slash_popup(frame: &mut Frame, composer_pane: &PaneLayout, model: &ComposerPaneModel) {
    if model.slash_suggestions.is_empty() {
        return;
    }

    let max_items = 5usize.min(model.slash_suggestions.len());
    let popup_height = (max_items as u16) + 4; // items + spacer + hint + top/bottom border
    let popup_width = composer_pane.area.width;
    let popup_x = composer_pane.area.x;
    let popup_y = composer_pane.area.y.saturating_sub(popup_height);

    if popup_height < 3 || popup_width < 10 {
        return;
    }

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let mut lines: Vec<Line> = Vec::with_capacity(max_items);
    for (i, suggestion) in model.slash_suggestions.iter().take(max_items).enumerate() {
        let is_selected = model.slash_selected == Some(i);
        if is_selected {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("⬢ {}", suggestion.name),
                    Style::default()
                        .fg(theme::VIOLET_BRIGHT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    &suggestion.description,
                    Style::default().fg(theme::VIOLET_BRIGHT),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("⬡ {}", suggestion.name), theme::accent_style()),
                Span::styled("  ", Style::default()),
                Span::styled(&suggestion.description, theme::dim_style()),
            ]));
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Tab/Enter accept · Esc dismiss",
        theme::dim_style(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(" / Commands ", theme::accent_style()));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}

fn render_breadcrumb(frame: &mut Frame, area: Rect, breadcrumb: &str) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![Span::styled(breadcrumb, theme::accent_style())]);
    frame.render_widget(Paragraph::new(line), area);
}

struct ConversationContent {
    lines: Vec<Line<'static>>,
    total_visual_lines: usize,
    selected_range: Option<(usize, usize)>,
}

fn render_conversation(frame: &mut Frame, pane: &PaneLayout, model: &RenderModel, focused: bool) {
    let viewport = conversation_viewport(pane.area, &model.title, &model.conversation);
    let block = pane_block(&model.conversation.title, focused);
    let inner = block.inner(pane.area);
    let content_width = inner.width.saturating_sub(1).max(1);
    let content = build_conversation_content(&model.title, &model.conversation, content_width);
    let scroll_offset = model
        .conversation
        .scroll_offset
        .unwrap_or(viewport.default_scroll_offset)
        .min(viewport.max_scroll);

    frame.render_widget(
        Paragraph::new(content.lines)
            .block(block)
            .scroll((scroll_offset as u16, 0)),
        pane.area,
    );

    if content.total_visual_lines > viewport.visible_height && viewport.visible_height > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state =
            ScrollbarState::new(content.total_visual_lines).position(scroll_offset);
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

pub(crate) fn conversation_viewport(
    area: Rect,
    app_title: &str,
    model: &ConversationPaneModel,
) -> ConversationViewport {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let content_width = inner.width.saturating_sub(1).max(1);
    let content = build_conversation_content(app_title, model, content_width);
    let visible_height = inner.height as usize;
    let max_scroll = if visible_height == 0 {
        0
    } else {
        content.total_visual_lines.saturating_sub(visible_height)
    };
    let default_scroll_offset = if visible_height == 0 {
        0
    } else {
        auto_conversation_scroll_offset(content.selected_range, visible_height, max_scroll)
    };

    ConversationViewport {
        visible_height,
        max_scroll,
        default_scroll_offset,
    }
}

fn auto_conversation_scroll_offset(
    selected_range: Option<(usize, usize)>,
    visible_height: usize,
    max_scroll: usize,
) -> usize {
    let Some((selected_start, selected_end)) = selected_range else {
        return 0;
    };

    let mut scroll_offset = 0usize;
    if selected_end > visible_height {
        scroll_offset = selected_end.saturating_sub(visible_height);
    }
    if selected_start < scroll_offset {
        scroll_offset = selected_start;
    }
    scroll_offset.min(max_scroll)
}

fn build_conversation_content(
    app_title: &str,
    model: &ConversationPaneModel,
    content_width: u16,
) -> ConversationContent {
    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", theme::HEX), theme::brand_hex_style()),
            Span::styled(
                app_title.to_owned(),
                theme::text_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
    ];
    let mut total_visual_lines = rewrap_lines(&mut lines, content_width);
    let mut selected_range: Option<(usize, usize)> = None;

    if model.entries.is_empty() {
        let line = Line::from(Span::styled(model.empty_state.clone(), theme::dim_style()));
        total_visual_lines += push_wrapped_line(&mut lines, line, content_width);
    } else {
        let entry_count = model.entries.len();
        for (i, entry) in model.entries.iter().enumerate() {
            let marker = if entry.selected {
                format!("{} ", theme::HEX_FILLED)
            } else {
                "  ".into()
            };
            let marker_style = if entry.selected {
                theme::highlight_style()
            } else {
                theme::muted_style()
            };
            let author_style = if entry.selected {
                theme::author_selected_style()
            } else if entry.author == "user" {
                theme::author_user_style()
            } else {
                theme::author_style()
            };
            let bookmark_indicator = if entry.is_bookmarked {
                Span::styled("★ ", theme::bookmark_style())
            } else {
                Span::styled("  ", theme::muted_style())
            };

            // Colored left-border gutter per author role
            let gutter_color = if entry.author == "user" {
                theme::TEAL
            } else if entry.author == "system" {
                theme::TEXT_DIM
            } else {
                theme::VIOLET
            };
            let gutter = Span::styled("│ ", Style::default().fg(gutter_color));

            let author_display = if entry.is_streaming {
                let frame_str =
                    SPINNER_FRAMES[(model.tick_count / 3) as usize % SPINNER_FRAMES.len()];
                format!("{} {:<9}", frame_str, &entry.author)
            } else {
                format!("{:<10}", entry.author)
            };

            // Build the author line spans with optional dim timestamp
            let mut msg_spans = vec![
                Span::styled(marker, marker_style),
                bookmark_indicator,
                gutter,
                Span::styled(author_display, author_style),
            ];
            if let Some(ts) = &entry.timestamp {
                msg_spans.push(Span::styled(format!(" {ts}"), theme::dim_style()));
            }
            msg_spans.push(Span::raw(" "));
            msg_spans.push(Span::styled(entry.content.clone(), theme::text_style()));

            let line = Line::from(msg_spans);
            let line_height = push_wrapped_line(&mut lines, line, content_width);
            if entry.selected {
                selected_range = Some((total_visual_lines, total_visual_lines + line_height));
            }
            total_visual_lines += line_height;

            // Author-aware separator between messages
            if i + 1 < entry_count {
                let next_author = &model.entries[i + 1].author;
                let sep = if next_author != &entry.author {
                    format!("     │ ─── {} ───", next_author)
                } else {
                    "     │ · · ·".to_string()
                };
                let line = Line::from(Span::styled(
                    sep,
                    Style::default().fg(Color::Rgb(50, 50, 50)),
                ));
                total_visual_lines += push_wrapped_line(&mut lines, line, content_width);
            }
        }
    }

    let spacer = Line::default();
    total_visual_lines += push_wrapped_line(&mut lines, spacer, content_width);
    let hint_line = Line::from(Span::styled(model.hint.clone(), theme::dim_style()));
    total_visual_lines += push_wrapped_line(&mut lines, hint_line, content_width);

    ConversationContent {
        lines,
        total_visual_lines,
        selected_range,
    }
}

fn rewrap_lines(lines: &mut Vec<Line<'static>>, width: u16) -> usize {
    let original = std::mem::take(lines);
    for line in original {
        push_wrapped_line(lines, line, width);
    }
    lines.len()
}

fn push_wrapped_line(target: &mut Vec<Line<'static>>, line: Line<'static>, width: u16) -> usize {
    let wrapped = wrap_line(&line, width);
    let added = wrapped.len();
    target.extend(wrapped);
    added
}

fn wrap_line(line: &Line, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let mut wrapped = Vec::new();
    let mut current_line: Vec<Span<'static>> = Vec::new();
    let mut current_span_text = String::new();
    let mut current_span_style: Option<Style> = None;
    let mut current_width = 0usize;
    let mut ended_with_newline = false;

    let flush_span = |current_line: &mut Vec<Span<'static>>,
                      current_span_text: &mut String,
                      current_span_style: &mut Option<Style>| {
        if !current_span_text.is_empty() {
            current_line.push(Span::styled(
                std::mem::take(current_span_text),
                current_span_style.take().unwrap_or_default(),
            ));
        }
    };
    let flush_line = |wrapped: &mut Vec<Line<'static>>,
                      current_line: &mut Vec<Span<'static>>,
                      current_span_text: &mut String,
                      current_span_style: &mut Option<Style>,
                      current_width: &mut usize| {
        flush_span(current_line, current_span_text, current_span_style);
        wrapped.push(if current_line.is_empty() {
            Line::default()
        } else {
            Line::from(std::mem::take(current_line))
        });
        *current_width = 0;
    };

    for span in &line.spans {
        let style = span.style;
        for ch in span.content.chars() {
            if ch == '\n' {
                ended_with_newline = true;
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
                continue;
            }

            ended_with_newline = false;
            if current_width >= width {
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
            }

            if current_span_style != Some(style) && !current_span_text.is_empty() {
                flush_span(
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                );
            }
            current_span_style = Some(style);
            current_span_text.push(ch);
            current_width += 1;

            if current_width >= width {
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
            }
        }
    }

    flush_span(
        &mut current_line,
        &mut current_span_text,
        &mut current_span_style,
    );
    if !current_line.is_empty() || wrapped.is_empty() || ended_with_newline {
        wrapped.push(if current_line.is_empty() {
            Line::default()
        } else {
            Line::from(current_line)
        });
    }

    wrapped
}

fn render_composer(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &ComposerPaneModel,
    focused: bool,
    textarea: Option<&TextArea<'static>>,
) {
    let composer_text = model.lines.join("\n");

    // When a TextArea is available and focused, render it directly.
    if let Some(ta) = textarea {
        if model.show_cursor && focused {
            let block = pane_block(&model.title, focused);
            let inner = block.inner(pane.area);
            frame.render_widget(block, pane.area);

            // Render hint line below the textarea content
            let hint_height: u16 = 2; // blank line + hint
            let ta_height = inner.height.saturating_sub(hint_height);
            if ta_height > 0 {
                let ta_area = Rect::new(inner.x, inner.y, inner.width, ta_height);
                frame.render_widget(ta, ta_area);
                render_composer_scrollbar(frame, ta_area, model, &composer_text);

                // Hint line
                if inner.height > ta_height {
                    let hint_area =
                        Rect::new(inner.x, inner.y + ta_height, inner.width, hint_height);
                    let hint_lines = vec![
                        Line::default(),
                        Line::from(Span::styled(model.hint.clone(), theme::dim_style())),
                    ];
                    frame.render_widget(
                        Paragraph::new(hint_lines).wrap(Wrap { trim: false }),
                        hint_area,
                    );
                }
            } else {
                frame.render_widget(ta, inner);
                render_composer_scrollbar(frame, inner, model, &composer_text);
            }
            return;
        }
    }

    // Fallback: render with manual Paragraph + cursor (unfocused / no textarea).
    let mut lines: Vec<Line> = if model.lines.is_empty() {
        vec![Line::from(Span::styled(
            model.placeholder.clone(),
            theme::dim_style(),
        ))]
    } else {
        model
            .lines
            .iter()
            .cloned()
            .map(|text| Line::from(Span::styled(text, theme::text_style())))
            .collect()
    };

    let draft_state = if model.dirty { "dirty" } else { "clean" };
    let _ = draft_state; // retained for potential future use
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        model.hint.clone(),
        theme::dim_style(),
    )));

    let block = pane_block(&model.title, focused);
    let inner = block.inner(pane.area);
    let scroll_offset = composer_scroll_offset(model, &composer_text, inner.width, inner.height);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset as u16, 0)),
        pane.area,
    );
    render_composer_scrollbar(frame, inner, model, &composer_text);

    // Place the terminal cursor in the composer when in insert mode.
    // Block border offsets: +1 for left border, +1 for top border.
    if model.show_cursor && focused {
        let inner_x = pane.area.x + 1;
        let inner_y = pane.area.y + 1;
        let inner_width = pane.area.width.saturating_sub(2) as usize;

        if inner_width > 0 {
            let (row, col) = visual_cursor_position(&composer_text, inner_width, model.cursor);

            let cursor_x = inner_x + col;
            let cursor_y = inner_y + row.saturating_sub(scroll_offset as u16);

            // Only set cursor if it fits within the pane.
            if cursor_x < pane.area.x + pane.area.width && cursor_y < pane.area.y + pane.area.height
            {
                frame.set_cursor_position(Position::new(cursor_x, cursor_y));
            }
        }
    }
}

fn render_composer_scrollbar(frame: &mut Frame, area: Rect, model: &ComposerPaneModel, text: &str) {
    let Some((total_visual_lines, scroll_offset)) = composer_scroll_metrics(model, text, area)
    else {
        return;
    };

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"));
    let mut scrollbar_state = ScrollbarState::new(total_visual_lines).position(scroll_offset);
    frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

fn composer_scroll_offset(model: &ComposerPaneModel, text: &str, width: u16, height: u16) -> usize {
    composer_scroll_metrics(model, text, Rect::new(0, 0, width, height))
        .map(|(_, offset)| offset)
        .unwrap_or(0)
}

fn composer_scroll_metrics(
    model: &ComposerPaneModel,
    text: &str,
    area: Rect,
) -> Option<(usize, usize)> {
    if area.width == 0 || area.height == 0 || text.is_empty() {
        return None;
    }

    let visible_height = area.height as usize;
    let content_width = area.width as usize;
    let total_visual_lines = visual_line_count(text, content_width);
    if total_visual_lines <= visible_height {
        return None;
    }

    let cursor_row = visual_cursor_position(text, content_width, model.cursor).0 as usize;
    let scroll_offset = cursor_row
        .saturating_sub(visible_height.saturating_sub(1))
        .min(total_visual_lines.saturating_sub(visible_height));
    Some((total_visual_lines, scroll_offset))
}

fn visual_line_count(text: &str, width: usize) -> usize {
    visual_cursor_position(text, width, text.chars().count()).0 as usize + 1
}

fn visual_cursor_position(text: &str, width: usize, cursor: usize) -> (u16, u16) {
    if width == 0 {
        return (0, 0);
    }

    let mut row = 0usize;
    let mut col = 0usize;
    let target = cursor.min(text.chars().count());

    for (char_count, ch) in text.chars().enumerate() {
        if char_count == target {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
            if col >= width {
                row += 1;
                col = 0;
            }
        }
    }

    (row as u16, col as u16)
}

fn render_status(frame: &mut Frame, pane: &PaneLayout, model: &StatusPaneModel, _focused: bool) {
    if pane.area.height == 0 {
        return;
    }

    let (short_badge, badge_style) = match model.mode_badge.as_deref() {
        Some("INSERT") => (
            "INS",
            Style::default()
                .fg(Color::White)
                .bg(theme::MODE_INSERT_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Some("COMMAND") => (
            "CMD",
            Style::default()
                .fg(Color::White)
                .bg(theme::MODE_CMD_BG)
                .add_modifier(Modifier::BOLD),
        ),
        _ => (
            "NOR",
            Style::default()
                .fg(theme::TEAL)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let title = truncate_str(&model.session_title, 30);
    let msgs = if let Some(idx) = model.selected_index {
        if model.message_count > 1 {
            format!("{}/{} msgs", idx + 1, model.message_count)
        } else {
            format!("{} msgs", model.message_count)
        }
    } else {
        format!("{} msgs", model.message_count)
    };
    let sep = || Span::styled("  │  ", theme::muted_style());

    let mut spans = vec![
        Span::styled(format!(" {short_badge} "), badge_style),
        Span::styled(" ", Style::default()),
        Span::styled(title, theme::text_style()),
        sep(),
        Span::styled(msgs, theme::dim_style()),
    ];

    let runtime_text = status_short_runtime(&model.summary);
    if !runtime_text.is_empty() {
        spans.push(sep());
        spans.push(Span::styled(runtime_text, theme::dim_style()));
    }

    if let Some(vram) = model.vram_hint.as_deref() {
        let hint_width = vram.len() as u16 + 5; // "  ···  " (5) + vram text
        let current_width: u16 = spans.iter().map(|s| s.content.chars().count() as u16).sum();
        if current_width + hint_width < pane.area.width {
            spans.push(Span::styled("  ···  ", theme::muted_style()));
            spans.push(Span::styled(vram.to_string(), theme::highlight_style()));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), pane.area);
}

fn render_inspector(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &InspectorPaneModel,
    focused: bool,
) {
    let mut lines: Vec<Line> = model
        .lines
        .iter()
        .cloned()
        .map(|line| Line::from(Span::styled(line, theme::dim_style())))
        .collect();

    if let Some(info) = model.model_info.as_ref() {
        let pane_width = pane.area.width.saturating_sub(4) as usize;
        let divider = format!("─ Model Info {}", "─".repeat(pane_width.saturating_sub(13)));
        lines.push(Line::from(Span::styled(divider, theme::muted_style())));

        let vram_pct = if info.estimated_vram_mb > 0 {
            // Assume 8 GiB GPU as a display reference; show raw MB otherwise
            let pct = info.estimated_vram_mb as f64 / (8 * 1024) as f64 * 100.0;
            pct.min(999.0) as u32
        } else {
            0
        };
        let vram_color = if vram_pct > 95 {
            theme::RED
        } else if vram_pct > 80 {
            theme::AMBER
        } else {
            theme::GREEN
        };

        lines.push(Line::from(vec![
            Span::styled("  VRAM:  ", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!("{} MB", format_mb(info.estimated_vram_mb)),
                Style::default().fg(vram_color),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  RAM:   ", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!("{} MB", format_mb(info.estimated_ram_mb)),
                theme::text_style(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Layers:", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!(" {} GPU + {} CPU", info.gpu_layers, info.cpu_layers),
                theme::text_style(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Mode:  ", Style::default().fg(theme::TEAL)),
            Span::styled(info.mode_label.clone(), theme::text_style()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Source:", Style::default().fg(theme::TEAL)),
            Span::styled(format!(" {}", info.source_label), theme::dim_style()),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn format_mb(mb: u32) -> String {
    if mb >= 1000 {
        format!("{},{:03}", mb / 1000, mb % 1000)
    } else {
        mb.to_string()
    }
}

fn render_overlay(frame: &mut Frame, pane: &PaneLayout, model: &OverlayRenderModel) {
    let lines: Vec<Line> = model
        .lines
        .iter()
        .cloned()
        .map(|text| Line::from(Span::styled(text, theme::text_style())))
        .collect();

    frame.render_widget(Clear, pane.area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(overlay_block(&model.title))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let overlay = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(Clear, area);
    frame.render_widget(overlay, area);

    let width = 60_u16.min(area.width.saturating_sub(4));
    let height = 22_u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let help_area = Rect::new(x, y, width, height);

    let help_text = vec![
        Line::from(Span::styled(
            "⬡ Keybindings",
            Style::default()
                .fg(Color::Rgb(118, 183, 178))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "── Normal Mode ──",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from("  j/k      Scroll transcript"),
        Line::from("  ↑/↓      Move selected message"),
        Line::from("  i        Enter Insert mode"),
        Line::from("  I        Toggle Inspector"),
        Line::from("  Ctrl+I   Edit selected message"),
        Line::from("  b        Toggle bookmark"),
        Line::from("  Ctrl+K   Pin to memory"),
        Line::from("  /        Command palette"),
        Line::from("  ?        This help"),
        Line::from("  Esc      Back / Close"),
        Line::from(""),
        Line::from(Span::styled(
            "── Insert Mode ──",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from("  Enter    Send message"),
        Line::from("  Esc      Normal / cancel edit"),
        Line::from("  Tab      Autocomplete"),
        Line::from("  Ctrl+U   Undo"),
        Line::from("  Ctrl+R   Redo"),
        Line::from("  F2       Toggle Inspector"),
        Line::from(""),
        Line::from(Span::styled(
            "── Global ──",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from("  Ctrl+C   Quit"),
        Line::from("  Ctrl+D   Context dry-run"),
        Line::from("  Ctrl+K   Pin to memory"),
    ];

    let help = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(118, 183, 178)))
            .title(" Help (? to close) ")
            .title_style(
                Style::default()
                    .fg(Color::Rgb(118, 183, 178))
                    .add_modifier(Modifier::BOLD),
            ),
    );

    frame.render_widget(help, help_area);
}

fn render_toast(frame: &mut Frame, area: Rect, message: &str) {
    let msg_width = (message.len() as u16 + 4).min(area.width);
    let x = area.x + area.width.saturating_sub(msg_width).saturating_sub(1);
    let y = area.y + area.height.saturating_sub(3);
    let toast_area = Rect::new(x, y, msg_width, 1);

    let toast = Paragraph::new(Line::from(Span::styled(
        format!(" {} ", message),
        Style::default()
            .fg(Color::Rgb(141, 214, 209))
            .bg(Color::Rgb(30, 30, 30))
            .add_modifier(Modifier::BOLD),
    )));

    frame.render_widget(Clear, toast_area);
    frame.render_widget(toast, toast_area);
}

fn render_main_menu(frame: &mut Frame, pane: &PaneLayout, model: &MainMenuRenderModel) {
    let area = pane.area;

    // ── Branded header ──
    let mut lines: Vec<Line> = vec![
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled(
            "    ⬡  ⬢  ⬡  ⬢  ⬡",
            theme::brand_hex_style(),
        )]),
        Line::default(),
        Line::from(vec![
            Span::styled("    ", theme::text_style()),
            Span::styled(
                "ozone+",
                theme::title_focused_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("    ", theme::dim_style()),
            Span::styled(
                "local-LLM chat shell with persistent memory",
                theme::dim_style(),
            ),
        ]),
        Line::default(),
        Line::from(Span::styled(
            "    ─────────────────────────────────────────",
            theme::muted_style(),
        )),
        Line::default(),
    ];

    // ── Menu items ──
    for item in &model.items {
        let (marker, label_style, desc_style) = if item.selected {
            (
                format!("  {} ", theme::HEX_FILLED),
                theme::highlight_style(),
                theme::text_style(),
            )
        } else {
            (
                format!("  {} ", theme::HEX),
                theme::text_style(),
                theme::dim_style(),
            )
        };

        let shortcut_span = if !item.shortcut.is_empty() {
            Span::styled(format!("[{}] ", item.shortcut), theme::mode_badge_style())
        } else {
            Span::raw("")
        };

        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if item.selected {
                    theme::highlight_style()
                } else {
                    theme::muted_style()
                },
            ),
            shortcut_span,
            Span::styled(format!("{:<16}", item.label), label_style),
            Span::styled(item.description.clone(), desc_style),
        ]));

        lines.push(Line::default());
    }

    // ── Session count / Welcome panel ──
    if model.session_count == 0 {
        lines.push(Line::from(Span::styled(
            "    Getting Started",
            theme::text_style().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "    ───────────────",
            theme::muted_style(),
        )));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("1", theme::accent_style()),
            Span::styled(" → Start a new conversation", theme::dim_style()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("3", theme::accent_style()),
            Span::styled(" → Create your first character", theme::dim_style()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("/", theme::accent_style()),
            Span::styled(" → Open command palette", theme::dim_style()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("?", theme::accent_style()),
            Span::styled(" → Help anytime", theme::dim_style()),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "    {} session{} available",
                model.session_count,
                if model.session_count == 1 { "" } else { "s" }
            ),
            theme::dim_style(),
        )));
    }

    // ── Hint bar ──
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("    {}", model.hint),
        theme::dim_style(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" {} ozone+ ", theme::HEX),
            theme::title_focused_style(),
        ));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_menu_placeholder(frame: &mut Frame, pane: &PaneLayout, title: &str) {
    let lines = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  {} {}", theme::HEX, title),
            theme::highlight_style(),
        )),
        Line::default(),
        Line::from(Span::styled(
            "  No content to display — press Esc to return",
            theme::dim_style(),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" {} ozone+ ", theme::HEX),
            theme::title_focused_style(),
        ));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_session_list(frame: &mut Frame, pane: &PaneLayout, model: &SessionListRenderModel) {
    let area = pane.area;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" {} Sessions ", theme::HEX),
            theme::title_focused_style(),
        ));

    if model.loading {
        let mut lines = vec![
            Line::default(),
            Line::from(Span::styled(
                "  Loading sessions\u{2026}",
                theme::dim_style(),
            )),
        ];
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("  {}", model.hint),
            theme::dim_style(),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    if model.items.is_empty() {
        let empty_text = if model.filter.is_empty() {
            "  No sessions yet \u{2014} press n to create one"
        } else {
            "  No sessions match the current filter"
        };
        let lines = vec![
            Line::default(),
            Line::from(Span::styled(empty_text, theme::dim_style())),
            Line::default(),
            Line::from(Span::styled(
                format!("  {}", model.hint),
                theme::dim_style(),
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    // Build header lines as a prefix paragraph above the list
    let mut header_lines: Vec<Line> = Vec::new();
    header_lines.push(Line::from(vec![
        Span::styled(format!("  {} ", theme::HEX), theme::brand_hex_style()),
        Span::styled(
            "Sessions",
            theme::title_focused_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  ({} total{})",
                model.total_count,
                if model.visible_count != model.total_count {
                    format!(", {} matching", model.visible_count)
                } else {
                    String::new()
                }
            ),
            theme::dim_style(),
        ),
    ]));
    if !model.filter.is_empty() {
        header_lines.push(Line::from(vec![
            Span::styled("  filter: ", theme::dim_style()),
            Span::styled(model.filter.clone(), theme::mode_badge_style()),
        ]));
    }
    header_lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));
    header_lines.push(Line::from(vec![
        Span::styled("      ", theme::dim_style()),
        Span::styled(format!("{:<24}", "Name"), theme::dim_style()),
        Span::styled(format!("{:<16}", "Character"), theme::dim_style()),
        Span::styled(format!("{:<10}", "Messages"), theme::dim_style()),
        Span::styled(format!("{:<14}", "Last Active"), theme::dim_style()),
    ]));
    header_lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    // Split area: use full area with block, then split inner area for header + list
    let inner = block.inner(area);
    // header rows + hint row at bottom
    let header_height = header_lines.len() as u16;
    let hint_height = 2u16;
    let list_height = inner.height.saturating_sub(header_height + hint_height);

    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: header_height.min(inner.height),
    };
    let list_area = Rect {
        x: inner.x,
        y: inner.y + header_area.height,
        width: inner.width,
        height: list_height,
    };
    let hint_area = Rect {
        x: inner.x,
        y: inner.y + header_area.height + list_height,
        width: inner.width,
        height: hint_height.min(
            inner
                .height
                .saturating_sub(header_area.height + list_height),
        ),
    };

    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(header_lines), header_area);

    // Build List items — headers get a styled divider row; entries get the session row
    let mut sel_list_idx: Option<usize> = None;
    let list_items: Vec<ListItem> = model
        .items
        .iter()
        .enumerate()
        .map(|(list_idx, item)| match item {
            SessionListItemRenderModel::Header { name } => {
                let line = Line::from(vec![
                    Span::styled(" \u{25b8} ", theme::accent_style()),
                    Span::styled(
                        format!("{} ", name),
                        theme::accent_style().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("\u{2500}".repeat(40), theme::muted_style()),
                ]);
                ListItem::new(line)
            }
            SessionListItemRenderModel::Entry(entry) => {
                if entry.selected {
                    sel_list_idx = Some(list_idx);
                }
                let indent = if entry.indented { "  " } else { "" };
                let line = Line::from(vec![
                    Span::styled(
                        if entry.selected {
                            format!("{}{} ", indent, theme::HEX_FILLED)
                        } else {
                            format!("{}{} ", indent, theme::HEX)
                        },
                        if entry.selected {
                            theme::highlight_style()
                        } else {
                            theme::muted_style()
                        },
                    ),
                    Span::styled(
                        format!("{:<24}", truncate_str(&entry.name, 22)),
                        if entry.selected {
                            theme::highlight_style()
                        } else {
                            theme::text_style()
                        },
                    ),
                    Span::styled(
                        format!("{:<16}", truncate_str(&entry.character, 14)),
                        if entry.selected {
                            theme::text_style()
                        } else {
                            theme::dim_style()
                        },
                    ),
                    Span::styled(
                        format!("{:<10}", entry.message_count),
                        if entry.selected {
                            theme::text_style()
                        } else {
                            theme::dim_style()
                        },
                    ),
                    Span::styled(
                        format!("{:<14}", truncate_str(&entry.last_active, 12)),
                        if entry.selected {
                            theme::text_style()
                        } else {
                            theme::dim_style()
                        },
                    ),
                ]);
                ListItem::new(line)
            }
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(sel_list_idx);

    let list = List::new(list_items).highlight_style(theme::highlight_style());
    frame.render_stateful_widget(list, list_area, &mut list_state);

    // Scrollbar when there are more items than visible rows
    let total = model.items.len();
    if total > list_area.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut sb_state = ScrollbarState::new(total).position(sel_list_idx.unwrap_or(0));
        frame.render_stateful_widget(
            scrollbar,
            list_area.inner(Margin {
                vertical: 0,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }

    // Hint bar
    let hint_lines = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  {}", model.hint),
            theme::dim_style(),
        )),
    ];
    frame.render_widget(Paragraph::new(hint_lines), hint_area);

    // Folder picker overlay
    if let Some(picker) = &model.folder_picker {
        render_folder_picker(frame, area, picker);
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height);
    Rect::new(
        x,
        y.max(area.y),
        width.min(area.width),
        height.min(area.height),
    )
}

fn render_folder_picker(frame: &mut Frame, area: Rect, model: &FolderPickerRenderModel) {
    let popup_height = (model.folders.len() + 3).min(12) as u16;
    let popup_width = 36u16;
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Assign Folder ")
        .borders(Borders::ALL)
        .border_style(theme::accent_style());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines: Vec<Line> = model
        .folders
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let prefix = if i == model.selected && !model.creating {
                "● "
            } else {
                "  "
            };
            let style = if i == model.selected && !model.creating {
                theme::accent_style().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(vec![Span::styled(format!("{prefix}{name}"), style)])
        })
        .collect();

    let new_idx = model.new_folder_index;
    if model.creating {
        lines.push(Line::from(vec![
            Span::styled("  Name: ", theme::dim_style()),
            Span::styled(
                format!("{}▌", model.new_folder_input),
                theme::accent_style(),
            ),
        ]));
    } else {
        let prefix = if model.selected == new_idx {
            "● "
        } else {
            "  "
        };
        let sty = if model.selected == new_idx {
            theme::accent_style()
        } else {
            theme::dim_style()
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{prefix}[+ New folder]"),
            sty,
        )]));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

fn render_character_list(frame: &mut Frame, pane: &PaneLayout, model: &CharacterListRenderModel) {
    let area = pane.area;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(
            format!(" {} Characters ", theme::HEX),
            theme::accent_style(),
        ));

    if model.entries.is_empty() {
        let lines = vec![
            Line::from(vec![
                Span::styled(format!("  {} ", theme::HEX), theme::brand_hex_style()),
                Span::styled(
                    "Characters",
                    theme::title_focused_style().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({} total)", model.total_count),
                    theme::dim_style(),
                ),
            ]),
            Line::from(Span::styled(
                "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                theme::muted_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  No characters yet",
                theme::text_style().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press n to create your first character card,",
                theme::dim_style(),
            )),
            Line::from(Span::styled(
                "  or press i to import a JSON character card.",
                theme::dim_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Supports SillyTavern, TavernAI, and ozone-native formats.",
                theme::dim_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    // Build header
    let mut header_lines: Vec<Line> = Vec::new();
    header_lines.push(Line::from(vec![
        Span::styled(format!("  {} ", theme::HEX), theme::brand_hex_style()),
        Span::styled(
            "Characters",
            theme::title_focused_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  ({} total)", model.total_count),
            theme::dim_style(),
        ),
    ]));
    header_lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));
    header_lines.push(Line::from(vec![
        Span::styled("      Name                          ", theme::dim_style()),
        Span::styled("Sessions  ", theme::dim_style()),
        Span::styled("Description", theme::dim_style()),
    ]));
    header_lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    let inner = block.inner(area);
    let header_height = header_lines.len() as u16;

    // Reserve space for optional detail panel (3 lines separator + name + desc chunks + session count)
    let detail_height = if let Some(detail) = &model.selected_detail {
        let desc_chunks = (detail.description.len() / 70 + 1) as u16;
        2u16 + 1 + desc_chunks + 1
    } else {
        0
    };

    let list_height = inner.height.saturating_sub(header_height + detail_height);

    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: header_height.min(inner.height),
    };
    let list_area = Rect {
        x: inner.x,
        y: inner.y + header_area.height,
        width: inner.width,
        height: list_height,
    };
    let detail_area = Rect {
        x: inner.x,
        y: inner.y + header_area.height + list_height,
        width: inner.width,
        height: detail_height.min(
            inner
                .height
                .saturating_sub(header_area.height + list_height),
        ),
    };

    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(header_lines), header_area);

    // Build list items
    let items: Vec<ListItem> = model
        .entries
        .iter()
        .map(|entry| {
            let style = if entry.selected {
                theme::highlight_style()
            } else {
                theme::text_style()
            };
            let line = Line::from(vec![
                Span::styled(
                    if entry.selected {
                        format!("{} ", theme::HEX_FILLED)
                    } else {
                        format!("{} ", theme::HEX)
                    },
                    style,
                ),
                Span::styled(format!("{:<30}", truncate_str(&entry.name, 30)), style),
                Span::styled(format!("{:<10}", entry.session_count), theme::dim_style()),
                Span::styled(truncate_str(&entry.description, 40), theme::dim_style()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    if let Some(sel_idx) = model.entries.iter().position(|e| e.selected) {
        list_state.select(Some(sel_idx));
    }

    let list = List::new(items).highlight_style(theme::highlight_style());
    frame.render_stateful_widget(list, list_area, &mut list_state);

    // Scrollbar when needed
    let total = model.entries.len();
    if total > list_area.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut sb_state = ScrollbarState::new(total).position(list_state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            scrollbar,
            list_area.inner(Margin {
                vertical: 0,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }

    // Detail panel
    if let Some(detail) = &model.selected_detail {
        let mut detail_lines: Vec<Line> = Vec::new();
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            theme::muted_style(),
        )));
        detail_lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                &detail.name,
                theme::title_focused_style().add_modifier(Modifier::BOLD),
            ),
        ]));
        for chunk in detail.description.as_bytes().chunks(70) {
            if let Ok(s) = std::str::from_utf8(chunk) {
                detail_lines.push(Line::from(Span::styled(
                    format!("  {s}"),
                    theme::text_style(),
                )));
            }
        }
        detail_lines.push(Line::from(Span::styled(
            format!("  {} session(s)", detail.session_count),
            theme::dim_style(),
        )));
        frame.render_widget(Paragraph::new(detail_lines), detail_area);
    }
}

fn render_character_form(frame: &mut Frame, pane: &PaneLayout, model: &CharacterFormRenderModel) {
    let area = pane.area;
    let mut lines = vec![];

    let title = match model.form_type {
        CharacterFormType::Create => "New Character",
        CharacterFormType::Edit => "Edit Character",
        CharacterFormType::Import => "Import Character Card",
    };

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {} {title}", theme::HEX),
        theme::title_focused_style().add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));
    lines.push(Line::from(""));

    match model.form_type {
        CharacterFormType::Create | CharacterFormType::Edit => {
            for field_model in &model.fields {
                let is_active = field_model.field == model.active_field;
                let label_style = if is_active {
                    theme::accent_style().add_modifier(Modifier::BOLD)
                } else {
                    theme::dim_style()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}", field_model.label),
                    label_style,
                )));
                let indicator = if is_active { "\u{25b6} " } else { "  " };
                let display = if field_model.text.is_empty() {
                    field_model.placeholder.to_string()
                } else {
                    field_model.text.clone()
                };
                let style = if field_model.text.is_empty() && is_active {
                    theme::dim_style()
                } else if is_active {
                    theme::text_style().add_modifier(Modifier::UNDERLINED)
                } else {
                    theme::text_style()
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  {indicator}"), theme::accent_style()),
                    Span::styled(display, style),
                ]));
                lines.push(Line::from(""));
            }
        }
        CharacterFormType::Import => {
            lines.push(Line::from(Span::styled(
                "  File Path",
                theme::accent_style().add_modifier(Modifier::BOLD),
            )));
            let path_display = if model.path_text.is_empty() {
                "(type or paste path to .json character card)".to_string()
            } else {
                model.path_text.clone()
            };
            let path_style = if model.path_text.is_empty() {
                theme::dim_style()
            } else {
                theme::text_style().add_modifier(Modifier::UNDERLINED)
            };
            lines.push(Line::from(vec![
                Span::styled("  \u{25b6} ", theme::accent_style()),
                Span::styled(path_display, path_style),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Supports: SillyTavern V1/V2, TavernAI, Ozone native JSON",
                theme::dim_style(),
            )));
        }
    }

    lines.push(Line::from(""));
    let hint = match model.form_type {
        CharacterFormType::Create | CharacterFormType::Edit => {
            "  Tab switch field \u{00b7} Enter save \u{00b7} Esc cancel"
        }
        CharacterFormType::Import => "  Enter import \u{00b7} Esc cancel",
    };
    lines.push(Line::from(Span::styled(hint, theme::muted_style())));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(
            format!(" {} {title} ", theme::HEX),
            theme::accent_style(),
        ));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_settings(frame: &mut Frame, pane: &PaneLayout, model: &SettingsRenderModel) {
    let area = pane.area;
    let mut lines: Vec<Line> = vec![];

    if model.drill_down {
        // ── Entry list view ────────────────────────────────────────────────
        lines.push(Line::from(""));

        if model.entries.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  No settings available for this category.",
                theme::dim_style(),
            )));
        } else {
            for entry in &model.entries {
                let (label_style, marker) = if entry.selected {
                    (
                        theme::highlight_style(),
                        format!("  {} ", theme::HEX_FILLED),
                    )
                } else {
                    (theme::dim_style(), format!("  {} ", theme::HEX))
                };

                // Right-side indicator depends on entry kind
                let right_span = match &entry.kind {
                    EntryKind::ReadOnly => Span::styled(entry.value.clone(), theme::text_style()),
                    EntryKind::Toggle(v) => {
                        let indicator = if *v { "[✓]" } else { "[ ]" };
                        let style = if *v {
                            theme::accent_style()
                        } else {
                            theme::dim_style()
                        };
                        Span::styled(indicator, style)
                    }
                    EntryKind::Cycle { options, current } => {
                        let cur = options.get(*current).map(|s| s.as_str()).unwrap_or("—");
                        Span::styled(format!("< {cur} >"), theme::accent_style())
                    }
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        marker,
                        if entry.selected {
                            theme::highlight_style()
                        } else {
                            theme::muted_style()
                        },
                    ),
                    Span::styled(format!("{:<22}", entry.label), label_style),
                    right_span,
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ─────────────────────────────────────────────",
            theme::muted_style(),
        )));
        lines.push(Line::from(Span::styled(
            "  [Enter] toggle/cycle · [Esc] back to categories",
            theme::dim_style(),
        )));

        // Breadcrumb in block title: " Settings › Backend "
        let category_label = model.breadcrumb_category.as_deref().unwrap_or("Settings");
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme::focus_border_style())
            .title(vec![
                Span::styled(format!(" {} Settings ", theme::HEX), theme::dim_style()),
                Span::styled("\u{203a} ", theme::dim_style()),
                Span::styled(format!("{category_label} "), theme::accent_style()),
            ]);

        frame.render_widget(Paragraph::new(lines).block(block), area);
    } else {
        // ── Category list view ─────────────────────────────────────────────
        lines.push(Line::from(""));

        for cat in &model.categories {
            let (marker, style) = if cat.selected {
                (
                    format!("  {} ", theme::HEX_FILLED),
                    theme::highlight_style(),
                )
            } else {
                (format!("  {} ", theme::HEX), theme::text_style())
            };
            lines.push(Line::from(vec![
                Span::styled(
                    marker,
                    if cat.selected {
                        theme::highlight_style()
                    } else {
                        theme::muted_style()
                    },
                ),
                Span::styled(cat.label.clone(), style),
            ]));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "  ─────────────────────────────────────────────",
            theme::muted_style(),
        )));
        lines.push(Line::from(Span::styled(
            "  [Enter] open category \u{00b7} [Esc] main menu",
            theme::dim_style(),
        )));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme::focus_border_style())
            .title(Span::styled(
                format!(" {} Settings ", theme::HEX),
                theme::accent_style(),
            ));

        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}

fn render_model_intelligence(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &ModelIntelligenceRenderModel,
) {
    let area = pane.area;
    let mut lines: Vec<Line> = vec![
        Line::default(),
        Line::from(vec![
            Span::styled(format!("  {} ", theme::HEX), theme::brand_hex_style()),
            Span::styled(
                "Model Intelligence",
                theme::title_focused_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
    ];

    if !model.has_plan {
        lines.push(Line::from(Span::styled(
            "  No launch plan available.",
            theme::dim_style(),
        )));
        lines.push(Line::from(Span::styled(
            "  Launch a model from ozone to see intelligence data.",
            theme::dim_style(),
        )));
    } else {
        let row = |label: &str, value: String| -> Line {
            Line::from(vec![
                Span::styled(format!("  {:<16}", label), theme::dim_style()),
                Span::styled(value, theme::text_style()),
            ])
        };

        lines.push(row("Model", model.model_name.clone()));
        let mode_str = if model.estimated {
            format!("{} (estimated)", model.mode_label)
        } else {
            model.mode_label.clone()
        };
        lines.push(row("Mode", mode_str));
        lines.push(row(
            "GPU layers",
            format!("{}/{}", model.gpu_layers, model.total_layers),
        ));
        lines.push(row(
            "CPU layers",
            format!("{}", model.total_layers.saturating_sub(model.gpu_layers)),
        ));
        lines.push(row("Context", format!("{} tokens", model.context_size)));
        lines.push(row("Est. VRAM", format!("{} MiB", model.estimated_vram_mb)));
        lines.push(row("Est. RAM", format!("{} MiB", model.estimated_ram_mb)));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  ──────────────────────────────────────────",
            theme::muted_style(),
        )));
        lines.push(Line::default());
        lines.push(row("Source", model.source.clone()));
        lines.push(row("Layer source", model.layer_source_label.clone()));

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  Rationale",
            theme::dim_style().add_modifier(Modifier::BOLD),
        )));
        for line in textwrap_simple(&model.rationale, 60) {
            lines.push(Line::from(Span::styled(
                format!("    {}", line),
                theme::text_style(),
            )));
        }

        if let Some(note) = &model.layer_note {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("  Note: {}", note),
                theme::dim_style(),
            )));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("  {}", model.hint),
        theme::dim_style(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" {} Model Intel ", theme::HEX),
            theme::title_focused_style(),
        ));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

fn pane_block(title: &str, focused: bool) -> Block<'static> {
    let (title_style, border) = if focused {
        (theme::title_focused_style(), theme::focus_border_style())
    } else {
        (theme::title_style(), theme::border_style())
    };

    Block::default()
        .title(Span::styled(
            format!(" {} {} ", theme::HEX, title),
            title_style,
        ))
        .borders(Borders::ALL)
        .border_style(border)
}

fn overlay_block(title: &str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {} {} ", theme::HEX_FILLED, title),
            theme::warning_style().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme::warning_style())
}

fn input_mode_label(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Insert => "INSERT",
        InputMode::Command => "COMMAND",
    }
}

fn screen_label(screen: ScreenState) -> &'static str {
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
    }
}

fn focus_label(focus: FocusTarget) -> &'static str {
    match focus {
        FocusTarget::Transcript => "conversation",
        FocusTarget::Draft => "composer",
        FocusTarget::Status => "status",
    }
}

fn selection_label(state: &ShellState) -> String {
    match (
        state.session.selected_message,
        state.session.transcript.len(),
    ) {
        (_, 0) => "0 messages".into(),
        (Some(index), total) => format!("message {}/{}", index + 1, total),
        (None, total) => format!("{} messages", total),
    }
}

fn branch_label(state: &ShellState) -> String {
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

fn runtime_label(runtime: &RuntimePhase) -> String {
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

fn context_status_line(state: &ShellState) -> String {
    state
        .context_preview
        .as_ref()
        .map(|preview| format!("context {}", preview.inline_status))
        .unwrap_or_else(|| "context preview pending".into())
}

/// Returns a short human-readable runtime status string for the 1-line footer.
/// Returns an empty string when the runtime is idle or the summary is generic.
fn status_short_runtime(summary: &str) -> String {
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

fn inspector_lines(state: &ShellState, indicators: &ShellIndicators) -> Vec<String> {
    let mut lines = vec![
        format!("session {}", state.session.context.session_id),
        format!("name {}", state.session.context.title),
        indicators.branch.clone(),
        indicators.selection.clone(),
        format!("focus {}", inspector_focus_label(state.inspector.focus)),
        state
            .session_metadata
            .as_ref()
            .map(|metadata| {
                format!(
                    "character {}",
                    metadata
                        .character_name
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("—")
                )
            })
            .unwrap_or_else(|| "character —".into()),
        state
            .session_metadata
            .as_ref()
            .map(|metadata| format!("tags {}", format_tags(&metadata.tags)))
            .unwrap_or_else(|| "tags —".into()),
        state
            .session_stats
            .as_ref()
            .map(|stats| {
                format!(
                    "stats {} messages · {} branches · {} bookmarks",
                    stats.message_count, stats.branch_count, stats.bookmark_count
                )
            })
            .unwrap_or_else(|| "stats pending".into()),
        selected_message_line(state),
        runtime_label(&state.session.runtime),
    ];

    if let Some(browser) = state.recall_browser.as_ref() {
        lines.push(format!("{} · {}", browser.title, browser.summary));
        for line in &browser.lines {
            lines.push(format!("· {line}"));
        }
    } else {
        lines.push("recall browser idle (use :memories or /search …)".into());
    }

    if let Some(preview) = state.context_preview.as_ref() {
        lines.push(format!("context preview · {}", preview.summary));
        if let Some(selected_items) = preview.selected_items {
            lines.push(format!("included items {selected_items}"));
        }
        if let Some(omitted_items) = preview.omitted_items {
            lines.push(format!("omitted items {omitted_items}"));
        }
        if let Some(token_budget) = preview.token_budget.as_ref() {
            lines.push(format!(
                "token budget {} / {}",
                token_budget.used_tokens, token_budget.max_tokens
            ));
        }
        append_context_preview_lines(&mut lines, preview);
    } else {
        lines.push("context preview unavailable (send a prompt to build one)".into());
    }

    match state.context_dry_run.as_ref() {
        Some(dry_run) => lines.push(format!(
            "dry run captured at {} · {}",
            dry_run.built_at, dry_run.summary
        )),
        None => lines.push("dry run not captured yet".into()),
    }

    lines
}

fn append_context_preview_lines(lines: &mut Vec<String>, preview: &ContextPreview) {
    for line in &preview.lines {
        lines.push(format!("· {line}"));
    }
}

fn inspector_visibility_label(layout: &LayoutModel, state: &ShellState) -> String {
    match (
        layout.mode,
        layout.inspector.is_some(),
        state.inspector.visible,
    ) {
        (LayoutMode::Compact, _, true) => "compact shell · inspector hidden below width".into(),
        (LayoutMode::Compact, _, false) => "compact shell · inspector closed".into(),
        (LayoutMode::Wide, true, _) => "wide shell · inspector visible".into(),
        (LayoutMode::Wide, false, true) => "wide shell · inspector unavailable".into(),
        (LayoutMode::Wide, false, false) => "wide shell · inspector closed".into(),
    }
}

fn inspector_focus_label(focus: InspectorFocus) -> &'static str {
    match focus {
        InspectorFocus::Summary => "summary",
        InspectorFocus::Branches => "branches",
        InspectorFocus::Message => "message",
        InspectorFocus::Recall => "recall",
    }
}

fn composer_hint(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => {
            "i insert · / commands · b bookmark · Ctrl+K pin · Tab conversation · Ctrl+D dry-run · ? help"
        }
        InputMode::Insert => {
            "Enter send · Esc normal · Ctrl+U undo · Ctrl+R redo · Ctrl+C cancel · Ctrl+D dry-run · F2 inspector"
        }
        InputMode::Command => "Enter send · Esc normal · Ctrl+C cancel · Ctrl+D dry-run",
    }
}

/// Build inline slash-command suggestions when the draft starts with `/`.
fn build_slash_suggestions(draft_text: &str) -> Vec<SlashSuggestion> {
    if !draft_text.starts_with('/') {
        return Vec::new();
    }
    // Extract the command prefix after `/` (first word only).
    let query = draft_text
        .get(1..)
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    // Only show suggestions while typing the command name (no space yet).
    if draft_text.contains(' ') {
        return Vec::new();
    }
    CommandEntry::all()
        .into_iter()
        .filter(|cmd| {
            query.is_empty()
                || cmd.name.to_lowercase().starts_with(&query)
                || cmd.alias.iter().any(|a| a.starts_with(&query))
        })
        .map(|cmd| SlashSuggestion {
            name: format!("/{}", cmd.name),
            description: cmd.description,
        })
        .collect()
}

fn overlay_model(screen: ScreenState, input_mode: InputMode) -> Option<OverlayRenderModel> {
    match screen {
        ScreenState::MainMenu
        | ScreenState::SessionList
        | ScreenState::CharacterManager
        | ScreenState::CharacterCreate
        | ScreenState::CharacterEdit
        | ScreenState::CharacterImport
        | ScreenState::Settings
        | ScreenState::ModelIntelligence
        | ScreenState::Conversation => None,
        ScreenState::Help => Some(OverlayRenderModel {
            title: "Help".into(),
            lines: vec![
                format!(
                    "⬡ ozone+ TUI — current mode: {}",
                    input_mode_label(input_mode)
                ),
                String::new(),
                "Navigation".into(),
                "  j / k          scroll transcript".into(),
                "  ↑ / ↓          move selected message".into(),
                "  Tab            switch conversation ↔ composer focus".into(),
                "  i              enter insert mode".into(),
                "  Esc            return to normal mode".into(),
                String::new(),
                "Actions".into(),
                "  b              toggle bookmark on selected message".into(),
                "  Ctrl+K         pin/unpin selected message to hard context".into(),
                "  Enter          send current draft".into(),
                "  Ctrl+C         cancel active generation".into(),
                "  Ctrl+D         build a context dry-run preview".into(),
                "  Ctrl+I         edit the selected persisted message".into(),
                "  I / F2         toggle the inspector pane".into(),
                "  q              quit".into(),
                String::new(),
                "Slash Commands".into(),
                "  /session show              session metadata".into(),
                "  /session rename NAME       rename session".into(),
                "  /session retitle           generate session title".into(),
                "  /session character NAME     set character".into(),
                "  /session tags a,b          set tags".into(),
                "  /memory list               list pinned memories".into(),
                "  /memory note TEXT          create a note memory".into(),
                "  /memory unpin ID           unpin a memory".into(),
                "  /search session QUERY      search this session".into(),
                "  /search global QUERY       search all sessions".into(),
                "  :memories                  open recall browser".into(),
            ],
        }),
        ScreenState::Quit => Some(OverlayRenderModel {
            title: "Quit".into(),
            lines: vec![
                "⬡ Exiting ozone+".into(),
                String::new(),
                "Session state and draft have been saved.".into(),
                "Press any key or wait for cleanup to finish.".into(),
            ],
        }),
    }
}

fn selected_message_line(state: &ShellState) -> String {
    state
        .session
        .selected_message
        .and_then(|index| state.session.transcript.get(index))
        .map(|item| {
            format!(
                "selected {}{}",
                item.author,
                if item.is_bookmarked {
                    " · bookmarked"
                } else {
                    ""
                }
            )
        })
        .unwrap_or_else(|| "selected message unavailable".into())
}

fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".into()
    } else {
        tags.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::{
        build_conversation_content, build_render_model, pane_block, render_shell,
        SessionListItemRenderModel,
    };
    use crate::{
        app::{
            AppBootstrap, BranchItem, DraftState, ScreenState, SessionContext, ShellState,
            TranscriptItem,
        },
        input::InputMode,
        layout::build_layout_for_area,
    };

    fn seeded_state() -> ShellState {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let context = SessionContext::new(session_id, "Phase 1C");
        let mut state = ShellState::new(context);
        state.enter_conversation();
        state.hydrate(AppBootstrap {
            transcript: vec![
                TranscriptItem::persisted("msg-1", "user", "hello skeleton", false),
                TranscriptItem::persisted("msg-2", "assistant", "believable shell ready", true),
            ],
            branches: vec![
                BranchItem::new("main", "main", true),
                BranchItem::new("plan", "plan", false),
            ],
            status_line: Some("mock runtime ready".into()),
            draft: Some(DraftState::with_text("draft reply")),
            screen: None,
            session_metadata: Some(crate::app::SessionMetadata {
                character_name: Some("Beatrice".into()),
                tags: vec!["story".into()],
            }),
            session_stats: Some(crate::app::SessionStats {
                message_count: 2,
                branch_count: 2,
                bookmark_count: 1,
            }),
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            active_launch_plan: None,
        });
        state.session.selected_message = Some(1);
        state
    }

    #[test]
    fn render_model_tracks_compact_and_wide_shell_states() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;

        let compact = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let compact_model = build_render_model(&state, &compact);
        assert_eq!(compact_model.indicators.input_mode, "INSERT");
        assert!(compact_model.inspector.is_none());
        assert_eq!(compact_model.status.summary, "mock runtime ready");
        assert!(compact_model.conversation.entries[1].selected);

        state.inspector.visible = true;
        let wide = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let wide_model = build_render_model(&state, &wide);
        assert!(wide_model
            .inspector
            .as_ref()
            .expect("wide inspector should be present")
            .lines
            .iter()
            .any(|line| line.contains("123e4567-e89b-12d3-a456-426614174000")));
        assert!(wide_model.inspector.is_some());
        assert!(wide_model
            .status
            .notifications
            .iter()
            .any(|line| line.contains("wide shell")));
    }

    #[test]
    fn test_backend_renders_compact_shell_without_inspector_title() {
        let state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(
            rendered.contains("Ozone+"),
            "breadcrumb should be visible on top row"
        );
        assert!(rendered.contains("Composer"));
        assert!(
            rendered.contains("Phase 1C"),
            "session title should appear in footer"
        );
        assert!(rendered.contains("mock runtime ready"));
        assert!(!rendered.contains(" Inspector "));
    }

    #[test]
    fn test_backend_renders_wide_shell_with_inspector_placeholder() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;
        state.inspector.visible = true;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        let rendered = render_to_string(120, 40, &layout, &model);

        assert!(rendered.contains("Inspector"));
        assert!(rendered.contains("branch main"));
        assert!(
            rendered.contains(" INS "),
            "Insert mode badge should appear in footer"
        );
        assert!(rendered.contains("123e4567"));
        assert!(rendered.contains("context preview unavailable"));
    }

    #[test]
    fn render_model_surfaces_recall_browser_in_status_and_inspector() {
        let mut state = seeded_state();
        state.recall_browser = Some(crate::app::RecallBrowser {
            title: "Recall".into(),
            summary: "2 active · 1 recent hit".into(),
            lines: vec![
                "active pinned 2".into(),
                "session search \"nebula\" · 1 hit".into(),
            ],
        });
        state.inspector.visible = true;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model
            .status
            .notifications
            .iter()
            .any(|line| line.contains("Recall · 2 active · 1 recent hit")));
        assert!(model
            .inspector
            .as_ref()
            .expect("inspector should render")
            .lines
            .iter()
            .any(|line| line.contains("session search \"nebula\" · 1 hit")));
    }

    #[test]
    fn streaming_partial_content_appears_as_transient_entry_while_generating() {
        use crate::app::{RuntimeProgress, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-stream-1".into(),
            user_message: TranscriptItem::new("user", "stream test"),
            context_preview: None,
            context_dry_run: None,
        });
        state.apply_runtime_progress(RuntimeProgress {
            request_id: "req-stream-1".into(),
            partial_content: "streaming reply so far".into(),
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        // The partial content should appear as an extra entry with a cursor marker
        let partial_entry = model
            .conversation
            .entries
            .last()
            .expect("at least one entry");
        assert_eq!(partial_entry.author, "assistant");
        assert!(
            partial_entry.content.contains("streaming reply so far"),
            "partial entry should contain streamed text"
        );
        assert!(
            partial_entry.content.contains('▍'),
            "partial entry should have cursor marker"
        );
    }

    #[test]
    fn failed_runtime_label_includes_message() {
        use crate::app::{RuntimeFailure, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-fail-1".into(),
            user_message: TranscriptItem::new("user", "fail test"),
            context_preview: None,
            context_dry_run: None,
        });
        state.apply_runtime_failure(RuntimeFailure {
            request_id: "req-fail-1".into(),
            message: "context window exceeded".into(),
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        assert!(
            model
                .status
                .notifications
                .iter()
                .any(|n| n.contains("runtime failed") && n.contains("req-fail-1")),
            "status notifications should mention runtime failed"
        );
    }

    fn render_to_string(
        width: u16,
        height: u16,
        layout: &crate::layout::LayoutModel,
        model: &crate::render::RenderModel,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_shell(frame, layout, model, None, None))
            .unwrap();

        buffer_to_string(terminal.backend().buffer(), width, height)
    }

    fn buffer_to_string(buffer: &Buffer, width: u16, height: u16) -> String {
        (0..height)
            .map(|y| {
                let mut line = String::new();
                for x in 0..width {
                    line.push_str(buffer[(x, y)].symbol());
                }
                line
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn render_shell_clears_stale_conversation_rows_between_draws() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        terminal
            .draw(|frame| render_shell(frame, &layout, &model, None, None))
            .unwrap();

        let mut cleared_state = state.clone();
        cleared_state.session.transcript.clear();
        cleared_state.session.selected_message = None;
        let cleared_layout = build_layout_for_area(&cleared_state, Rect::new(0, 0, 80, 24));
        let cleared_model = build_render_model(&cleared_state, &cleared_layout);
        terminal
            .draw(|frame| render_shell(frame, &cleared_layout, &cleared_model, None, None))
            .unwrap();

        let rendered = buffer_to_string(terminal.backend().buffer(), 80, 24);
        assert!(rendered.contains("Start a conversation"));
        assert!(!rendered.contains("hello skeleton"));
        assert!(!rendered.contains("believable shell ready"));
        assert!(!rendered.contains("assistant ───"));
    }

    #[test]
    fn render_conversation_shows_scrollbar_for_long_transcripts() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.session.transcript = (0..24)
            .map(|index| {
                crate::app::TranscriptItem::persisted(
                    format!("msg-{index}"),
                    "assistant",
                    format!("line {index}"),
                    false,
                )
            })
            .collect();
        state.session.selected_message = Some(20);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("↑") || rendered.contains("↓"));
    }

    #[test]
    fn render_conversation_respects_manual_scroll_offset() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.session.transcript = vec![crate::app::TranscriptItem::persisted(
            "msg-0",
            "assistant",
            "AAA0 AAA1 AAA2 AAA3 AAA4 AAA5 AAA6 AAA7 AAA8 AAA9 AAA10 AAA11 AAA12 AAA13 AAA14 AAA15",
            false,
        )];
        state.session.selected_message = Some(0);
        state.conversation_scroll = Some(4);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        let model = build_render_model(&state, &layout);
        let block = pane_block(
            &model.conversation.title,
            layout.focused == crate::layout::PaneId::Conversation,
        );
        let inner = block.inner(layout.conversation.area);
        let content = build_conversation_content(
            &model.title,
            &model.conversation,
            inner.width.saturating_sub(1).max(1),
        );
        let wrapped_rows: Vec<String> = content
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|line: &String| line.contains("AAA"))
            .collect();
        let rendered = render_to_string(40, 12, &layout, &model);

        assert!(
            wrapped_rows.len() >= 3,
            "expected wrapped transcript rows, got {wrapped_rows:?}"
        );
        assert!(rendered.contains(&wrapped_rows[2]));
        assert!(!rendered.contains(&wrapped_rows[0]));
    }

    #[test]
    fn main_menu_screen_produces_menu_render_model() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.main_menu.is_some());
        assert!(model.session_list.is_none());

        let menu = model.main_menu.unwrap();
        assert_eq!(menu.items.len(), 6);
        assert!(menu.items[0].selected); // first item selected by default
        assert!(!menu.items[1].selected);
        assert_eq!(menu.items[0].label, "New Chat");
        assert_eq!(menu.items[1].label, "Sessions");
        assert_eq!(menu.items[5].label, "Quit");
    }

    #[test]
    fn session_list_screen_produces_session_list_render_model() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;
        state.session_list.entries = vec![
            crate::app::SessionListEntry {
                session_id: "test-1".into(),
                name: "My First Chat".into(),
                character_name: Some("Aster".into()),
                message_count: 42,
                last_active: Some("2 hours ago".into()),
                folder: None,
            },
            crate::app::SessionListEntry {
                session_id: "test-2".into(),
                name: "World Building".into(),
                character_name: None,
                message_count: 7,
                last_active: Some("yesterday".into()),
                folder: None,
            },
        ];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.session_list.is_some());
        assert!(model.main_menu.is_none());

        let list = model.session_list.unwrap();
        // Both entries have no folder, so items = [Entry, Entry] (no headers)
        assert_eq!(list.items.len(), 2);
        let entry0 = match &list.items[0] {
            SessionListItemRenderModel::Entry(e) => e,
            _ => panic!("expected Entry"),
        };
        let entry1 = match &list.items[1] {
            SessionListItemRenderModel::Entry(e) => e,
            _ => panic!("expected Entry"),
        };
        assert!(entry0.selected);
        assert!(!entry1.selected);
        assert_eq!(entry0.name, "My First Chat");
        assert_eq!(entry0.character, "Aster");
        assert_eq!(entry1.character, "—");
        assert_eq!(list.total_count, 2);
        assert_eq!(list.visible_count, 2);
    }

    #[test]
    fn conversation_screen_has_no_menu_models() {
        let mut state = seeded_state();
        state.screen = ScreenState::Conversation;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.main_menu.is_none());
        assert!(model.session_list.is_none());
    }

    #[test]
    fn main_menu_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn session_list_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;
        state.session_list.entries = vec![crate::app::SessionListEntry {
            session_id: "test-1".into(),
            name: "Test Session".into(),
            character_name: None,
            message_count: 10,
            last_active: None,
            folder: None,
        }];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn empty_session_list_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn command_palette_render_shows_empty_match_copy() {
        let mut state = seeded_state();
        state.command_palette.open();
        state.command_palette.restore_input_text("zzzzz", 5);
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("No commands match the current input"));
        assert!(rendered.contains("Enter run"));
    }

    #[test]
    fn slash_popup_render_shows_accept_hint() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.input_mode = InputMode::Insert;
        state.draft.text = "/he".into();
        state.draft.cursor = 3;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("Tab/Enter accept"));
        assert!(rendered.contains("/help"));
    }

    #[test]
    fn slash_popup_not_shown_when_command_palette_open() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;
        // Put a `/` prefix in the draft so slash suggestions would normally appear.
        state.draft.text = "/he".into();
        state.draft.cursor = 3;
        // Open the command palette — this should suppress the slash popup.
        state.command_palette.open();

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        // The composer model still has suggestions (derived from draft text)…
        assert!(
            !model.composer.slash_suggestions.is_empty(),
            "slash_suggestions should be populated from the draft"
        );
        // …but the render model's command_palette is Some, so render_shell
        // skips render_slash_popup (guarded by `model.command_palette.is_none()`).
        assert!(
            model.command_palette.is_some(),
            "command palette should be present when open"
        );
    }

    #[test]
    fn message_edit_hides_slash_suggestions_and_updates_hint_copy() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut state = seeded_state();
        state.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL));
        state.draft.text = "/he".into();
        state.draft.cursor = 3;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        assert!(model.composer.slash_suggestions.is_empty());
        assert!(model.composer.hint.contains("arrows/tab stay in editor"));
        assert!(model.conversation.hint.contains("Enter save"));
    }

    #[test]
    fn render_composer_shows_scrollbar_for_long_edit_buffers() {
        use crate::app::DraftCheckpoint;

        let mut state = seeded_state();
        state.handle_key_event(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        state.draft = DraftState::restore(DraftCheckpoint::new(
            (0..18)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            0,
        ));
        state.focus = crate::app::FocusTarget::Draft;
        state.input_mode = InputMode::Insert;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("↑") || rendered.contains("↓"));
    }

    #[test]
    fn slash_suggestions_clear_on_space_after_slash() {
        use crate::input::KeyAction;

        let mut state = seeded_state();
        // Clear the hydrated draft so we start fresh.
        state.draft.text.clear();
        state.draft.cursor = 0;
        state.apply_action(KeyAction::EnterInsert);

        // Type `/he` — suggestions should be populated.
        state.apply_action(KeyAction::DraftInsertChar('/'));
        state.apply_action(KeyAction::DraftInsertChar('h'));
        state.apply_action(KeyAction::DraftInsertChar('e'));

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        assert!(
            !model.composer.slash_suggestions.is_empty(),
            "suggestions should appear for `/he`"
        );

        // Insert a space — draft becomes `/he `, suggestions should clear.
        state.apply_action(KeyAction::DraftInsertChar(' '));

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        assert!(
            model.composer.slash_suggestions.is_empty(),
            "suggestions should be empty after a space (command name complete)"
        );
    }
}
