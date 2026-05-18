use super::model_types::*;
use super::labels::*;
use super::conversation::*;
use super::composer::*;
use super::status_inspector::*;
use super::menu_screens::*;
use super::overlays::*;
use crate::app::ShellState;
use crate::state::{FocusTarget, RuntimePhase, ScreenState, VisibleSessionItem};
use crate::input::InputMode;
use crate::layout::{LayoutMode, LayoutModel, PaneId};
use crate::theme;
use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use tui_textarea::TextArea;

fn compact_status_notice(state: &ShellState) -> Option<String> {
    if let Some(browser) = state.recall_browser.as_ref() {
        return Some(format!("{} · {}", browser.title, browser.summary));
    }

    if let Some(preview) = state.context_preview.as_ref() {
        return Some(format!("context · {}", preview.summary));
    }

    state
        .context_dry_run
        .as_ref()
        .map(|dry_run| format!("dry run · {}", dry_run.summary))
}

pub fn build_render_model(state: &ShellState, layout: &LayoutModel) -> RenderModel {
    let indicators = ShellIndicators {
        screen: screen_label(&state.screen).into(),
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
            match &state.session.runtime {
                RuntimePhase::Generating {
                    partial_content: Some(partial),
                    ..
                } => {
                    entries.push(ConversationEntryModel {
                        author: "assistant".into(),
                        content: format!("{partial}▍"),
                        is_bookmarked: false,
                        selected: false,
                        is_streaming: true,
                        timestamp: None,
                    });
                }
                RuntimePhase::Generating {
                    partial_content: None,
                    ..
                } => {
                    entries.push(ConversationEntryModel {
                        author: "assistant".into(),
                        content: "Thinking…▍".into(),
                        is_bookmarked: false,
                        selected: false,
                        is_streaming: true,
                        timestamp: None,
                    });
                }
                _ => {}
            }
            entries
        },
        empty_state: {
            // Show character greeting if transcript is empty and greeting is set
            if state.session.transcript.is_empty() {
                if let Some(greeting) = state
                    .session_metadata
                    .as_ref()
                    .and_then(|m| m.greeting.as_ref())
                {
                    let name_prefix = state
                        .session_metadata
                        .as_ref()
                        .and_then(|m| m.character_name.as_ref())
                        .filter(|n| !n.is_empty());
                    match name_prefix {
                        Some(name) => format!("⬡ {} says: {}", name, greeting),
                        None => format!("⬡ {}", greeting),
                    }
                } else {
                    "⬡ Start a conversation — press i to enter insert mode".into()
                }
            } else {
                "⬡ Start a conversation — press i to enter insert mode".into()
            }
        },
        hint: if state.message_edit.is_some() {
            "Editing selected message · Enter save · Esc cancel · Ctrl+U undo · Ctrl+Y redo · F2 inspector"
                .into()
        } else {
            "j/k scroll · ↑↓ select · i insert · r reroll · / commands · ? help"
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
            "Enter save edit · Esc cancel · Ctrl+U undo · Ctrl+Y redo · arrows/tab stay in editor · F2 inspector"
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
                let pinned_suffix = state
                    .session_metadata
                    .as_ref()
                    .and_then(|m| m.pinned_count)
                    .map(|c| format!(" · {} pinned", c))
                    .unwrap_or_default();
                format!(
                    "{} messages · {} branches · {} bookmarks{}",
                    stats.message_count, stats.branch_count, stats.bookmark_count, pinned_suffix
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

    let token_budget = state
        .context_preview
        .as_ref()
        .and_then(|preview| preview.token_budget.as_ref())
        .map(|b| (b.used_tokens, b.max_tokens));

    let context_bar = token_budget.map(|(used, max)| {
        if max == 0 {
            return String::new();
        }
        let pct = (used as f64 / max as f64 * 100.0).min(100.0) as u8;
        let filled = (pct as usize * 20) / 100; // 20-block bar
        let empty = 20 - filled;
        let bar: String = "█".repeat(filled) + &"░".repeat(empty);
        let color = if pct >= 90 { "!" } else if pct >= 75 { "+" } else { "" };
        format!("[{bar}] {pct:3}%{color}", pct = pct)
    });

    // Formatted token budget string for inspector pane, e.g. "context 12,450 / 128,000 tokens (94%)".
    let token_budget_display = token_budget.map(|(used, max)| {
        if max == 0 {
            return String::new();
        }
        let pct = (used as f64 / max as f64 * 100.0).min(100.0) as u8;
        let used_str = format!("{}", used);
        let max_str = format!("{}", max);
        // Add commas manually since format! doesn't support {:,} for integer separation
        let format_with_commas = |s: &str| {
            let chars: Vec<char> = s.chars().collect();
            let mut result = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i > 0 && (chars.len() - i).is_multiple_of(3) {
                    result.push(',');
                }
                result.push(*c);
            }
            result
        };
        format!(
            "context {} / {} tokens ({pct}%)",
            format_with_commas(&used_str),
            format_with_commas(&max_str)
        )
    });

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
        character_label: state
            .session_metadata
            .as_ref()
            .and_then(|metadata| metadata.character_name.as_ref())
            .filter(|name| !name.is_empty())
            .map(|name| format!("char {name}")),
        message_count: state.session.transcript.len(),
        selected_index: if state.session.transcript.is_empty() {
            None
        } else {
            state.session.selected_message
        },
        vram_hint,
        context_bar: context_bar.clone(),
        token_budget,
        compact_notice: if matches!(layout.mode, LayoutMode::Compact) {
            compact_status_notice(state)
        } else {
            None
        },
    };

    let inspector = layout.inspector.map(|_| InspectorPaneModel {
        title: "Inspector".into(),
        lines: inspector_lines(state, &indicators),
        model_info: model_info.clone(),
        context_bar: context_bar.clone(),
        token_budget: token_budget_display.clone(),
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
                    last_message_preview: entry
                        .last_message_preview
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
                    greeting: if e.greeting.is_empty() { None } else { Some(e.greeting.clone()) },
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

        let current_cat = state.settings.current_category();
        let entries: Vec<SettingsEntryRenderItem> = state
            .settings
            .entries_for_category()
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
                current_cat.map(|c| c.to_string())
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
                    field: crate::state::CharacterFormField::Name,
                    label: "Name",
                    text: cs.name.text.clone(),
                    cursor: cs.name.cursor,
                    placeholder: "(type character name)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::Description,
                    label: "Description",
                    text: cs.description.text.clone(),
                    cursor: cs.description.cursor,
                    placeholder: "(short tagline or description)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::SystemPrompt,
                    label: "System Prompt",
                    text: cs.system_prompt.text.clone(),
                    cursor: cs.system_prompt.cursor,
                    placeholder: "(instructions for the AI — personality, rules, context)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::Personality,
                    label: "Personality",
                    text: cs.personality.text.clone(),
                    cursor: cs.personality.cursor,
                    placeholder: "(personality traits — kind, sarcastic, stoic…)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::Scenario,
                    label: "Scenario",
                    text: cs.scenario.text.clone(),
                    cursor: cs.scenario.cursor,
                    placeholder: "(the setting or situation for conversations)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::Greeting,
                    label: "First Message",
                    text: cs.greeting.text.clone(),
                    cursor: cs.greeting.cursor,
                    placeholder: "(character's opening message)",
                },
                CharacterFieldRenderModel {
                    field: crate::state::CharacterFormField::ExampleDialogue,
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
            active_field: crate::state::CharacterFormField::Name,
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
        overlay: overlay_model(&state.screen, state.input_mode, state.recall_browser.as_ref()),
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
                hint: "Enter run/fill · Esc close · ↑↓ choose".into(),
            })
        } else {
            None
        },
        toast_message: state.active_toast().map(|(msg, _)| msg.clone()),
        memory_metadata: state.session_metadata.as_ref().and_then(|m| m.memory_metadata.clone()),
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

pub fn build_hints(state: &ShellState) -> Vec<HintItem> {
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
        ScreenState::Conversation => match state.input_mode {
            InputMode::Normal => vec![
                HintItem {
                    key: "j/k".into(),
                    action: "scroll".into(),
                },
                HintItem {
                    key: "r".into(),
                    action: "reroll".into(),
                },
                HintItem {
                    key: "i".into(),
                    action: "insert".into(),
                },
                HintItem {
                    key: "?".into(),
                    action: "help".into(),
                },
                HintItem {
                    key: "Esc".into(),
                    action: "menu".into(),
                },
                HintItem {
                    key: "/".into(),
                    action: "commands".into(),
                },
            ],
            InputMode::Insert => vec![
                HintItem {
                    key: "Esc".into(),
                    action: "exit".into(),
                },
                HintItem {
                    key: "Enter".into(),
                    action: "send".into(),
                },
                HintItem {
                    key: "Tab".into(),
                    action: "focus".into(),
                },
                HintItem {
                    key: "↑↓".into(),
                    action: "history".into(),
                },
                HintItem {
                    key: "Ctrl+D".into(),
                    action: "dry run".into(),
                },
            ],
            _ => vec![],
        },
        ScreenState::Help => vec![
            HintItem {
                key: "Esc/q/?".into(),
                action: "close".into(),
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

pub fn build_breadcrumb(state: &ShellState) -> String {
    match &state.screen {
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
                    state.settings.current_category().unwrap_or("")
                )
            } else {
                "⬡ Ozone+ › Settings".into()
            }
        }
        ScreenState::Conversation => format!("⬡ Ozone+ › {}", state.session.context.title),
        ScreenState::Help => "⬡ Ozone+ › Help".into(),
        ScreenState::Quit => "⬡ Ozone+".into(),
        ScreenState::ModelIntelligence => "⬡ Ozone+ › Model Intel".into(),
        ScreenState::MemoriesOverlay => {
            let title = state
                .recall_browser
                .as_ref()
                .map(|browser| browser.title.as_str())
                .unwrap_or("Memories");
            format!("⬡ Ozone+ › {title}")
        }
        ScreenState::CharacterOverlay(detail) => {
            format!("⬡ Ozone+ › Character › {}", detail.name)
        }
    }
}

pub fn render_hints(frame: &mut Frame, area: Rect, hints: &[HintItem]) {
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

pub fn render_breadcrumb(frame: &mut Frame, area: Rect, breadcrumb: &str) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![Span::styled(breadcrumb, theme::accent_style())]);
    frame.render_widget(Paragraph::new(line), area);
}

pub fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".into()
    } else {
        tags.join(", ")
    }
}

