use ratatui::{
    layout::{Alignment, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{CommandEntry, ContextPreview, FocusTarget, InspectorFocus, RuntimePhase, ScreenState, ShellState},
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPaneModel {
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<ConversationEntryModel>,
    pub empty_state: String,
    pub hint: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorPaneModel {
    pub title: String,
    pub lines: Vec<String>,
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
    pub entries: Vec<SessionListEntryRenderModel>,
    pub selected: usize,
    pub filter: String,
    pub total_count: usize,
    pub visible_count: usize,
    pub hint: String,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntryRenderModel {
    pub name: String,
    pub character: String,
    pub message_count: String,
    pub last_active: String,
    pub selected: bool,
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
pub struct SettingsRenderModel {
    pub entries: Vec<SettingsRenderEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterFormRenderModel {
    pub form_type: CharacterFormType,
    pub name_text: String,
    pub name_cursor: usize,
    pub system_prompt_text: String,
    pub system_prompt_cursor: usize,
    pub active_field: crate::app::CharacterFormField,
    pub path_text: String,
    pub path_cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterFormType {
    Create,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRenderEntry {
    pub category: String,
    pub key: String,
    pub value: String,
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
    pub hints: Vec<HintItem>,
    pub breadcrumb: String,
    pub command_palette: Option<CommandPaletteRenderModel>,
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

    let conversation = ConversationPaneModel {
        title: "Conversation".into(),
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
                })
                .collect();
            // Show streamed partial content as a transient entry while generating.
            if let Some(partial) = state.session.runtime.partial_content() {
                entries.push(ConversationEntryModel {
                    author: "assistant".into(),
                    content: format!("{partial}▍"),
                    is_bookmarked: false,
                    selected: false,
                });
            }
            entries
        },
        empty_state: "⬡ Start a conversation — press i to enter insert mode".into(),
        hint:
            "j/k navigate · b bookmark · Ctrl+K pin · Tab focus · i insert · ? help"
                .into(),
    };

    let composer = ComposerPaneModel {
        title: "Composer".into(),
        mode: indicators.input_mode.clone(),
        lines: if state.draft.text.is_empty() {
            Vec::new()
        } else {
            state.draft.text.lines().map(str::to_owned).collect()
        },
        placeholder: "Write a reply or type / for commands…".into(),
        cursor: state.draft.cursor,
        dirty: state.draft.dirty,
        hint: composer_hint(state.input_mode).into(),
        show_cursor: state.input_mode == InputMode::Insert
            && state.focus == FocusTarget::Draft
            && !state.command_palette.open,
        slash_suggestions: build_slash_suggestions(&state.draft.text),
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

    let status = StatusPaneModel {
        title: "Status".into(),
        summary: state
            .status_line
            .clone()
            .unwrap_or_else(|| runtime_label(&state.session.runtime)),
        notifications,
        hint: "⬡ ? help · q quit".into(),
        mode_badge,
    };

    let inspector = layout.inspector.map(|_| InspectorPaneModel {
        title: "Inspector".into(),
        lines: inspector_lines(state, &indicators),
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
            hint: "j/k navigate · Enter select · 1-4 quick-jump · q quit · ? help".into(),
            session_count: state.session_list.entries.len(),
        })
    } else {
        None
    };

    let session_list = if state.screen == ScreenState::SessionList {
        let visible = state.session_list.visible_entries();
        Some(SessionListRenderModel {
            entries: visible
                .iter()
                .enumerate()
                .map(|(i, entry)| SessionListEntryRenderModel {
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
                    selected: i == state.session_list.selected,
                })
                .collect(),
            selected: state.session_list.selected,
            filter: state.session_list.filter.clone(),
            total_count: state.session_list.entries.len(),
            visible_count: state.session_list.visible_count(),
            hint: "j/k navigate \u{00b7} Enter open \u{00b7} n new session \u{00b7} / filter \u{00b7} q/Esc back"
                .into(),
            loading: state.session_list.loading,
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
        let selected_detail = state.character_list.selected_entry().map(|e| {
            CharacterDetailRenderModel {
                name: e.name.clone(),
                description: e.description.clone(),
                session_count: e.session_count,
            }
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
        Some(SettingsRenderModel {
            entries: state.settings.entries.iter().map(|e| {
                SettingsRenderEntry {
                    category: e.category.clone(),
                    key: e.key.clone(),
                    value: e.value.clone(),
                }
            }).collect(),
        })
    } else {
        None
    };

    let character_form = match state.screen {
        ScreenState::CharacterCreate => Some(CharacterFormRenderModel {
            form_type: CharacterFormType::Create,
            name_text: state.character_create.name.text.clone(),
            name_cursor: state.character_create.name.cursor,
            system_prompt_text: state.character_create.system_prompt.text.clone(),
            system_prompt_cursor: state.character_create.system_prompt.cursor,
            active_field: state.character_create.active_field,
            path_text: String::new(),
            path_cursor: 0,
        }),
        ScreenState::CharacterImport => Some(CharacterFormRenderModel {
            form_type: CharacterFormType::Import,
            name_text: String::new(),
            name_cursor: 0,
            system_prompt_text: String::new(),
            system_prompt_cursor: 0,
            active_field: crate::app::CharacterFormField::Name,
            path_text: state.character_import.path.text.clone(),
            path_cursor: state.character_import.path.cursor,
        }),
        _ => None,
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
        hints: build_hints(state),
        breadcrumb: build_breadcrumb(state),
        command_palette: if state.command_palette.open {
            let filtered = state.command_palette.filtered_commands();
            Some(CommandPaletteRenderModel {
                input: state.command_palette.input.clone(),
                entries: filtered.iter().enumerate().map(|(i, cmd)| CommandPaletteEntry {
                    name: format!("/{}", cmd.name),
                    description: cmd.description.clone(),
                    selected: i == state.command_palette.selected,
                }).collect(),
                selected: state.command_palette.selected,
            })
        } else {
            None
        },
    }
}

fn build_hints(state: &ShellState) -> Vec<HintItem> {
    match state.screen {
        ScreenState::MainMenu => vec![
            HintItem { key: "↑↓".into(), action: "Navigate".into() },
            HintItem { key: "Enter".into(), action: "Select".into() },
            HintItem { key: "1-4".into(), action: "Quick select".into() },
            HintItem { key: "q".into(), action: "Quit".into() },
            HintItem { key: "/".into(), action: "Commands".into() },
        ],
        ScreenState::SessionList => vec![
            HintItem { key: "↑↓".into(), action: "Navigate".into() },
            HintItem { key: "Enter".into(), action: "Open".into() },
            HintItem { key: "/".into(), action: "Commands".into() },
            HintItem { key: "q".into(), action: "Back".into() },
            HintItem { key: "Esc".into(), action: "Back".into() },
        ],
        ScreenState::CharacterManager => vec![
            HintItem { key: "↑↓".into(), action: "Navigate".into() },
            HintItem { key: "n".into(), action: "New character".into() },
            HintItem { key: "i".into(), action: "Import JSON".into() },
            HintItem { key: "/".into(), action: "Commands".into() },
            HintItem { key: "q".into(), action: "Back".into() },
        ],
        ScreenState::CharacterCreate => vec![
            HintItem { key: "Tab".into(), action: "Switch field".into() },
            HintItem { key: "Enter".into(), action: "Save".into() },
            HintItem { key: "Esc".into(), action: "Cancel".into() },
        ],
        ScreenState::CharacterImport => vec![
            HintItem { key: "Enter".into(), action: "Import".into() },
            HintItem { key: "Esc".into(), action: "Cancel".into() },
        ],
        ScreenState::Settings => vec![
            HintItem { key: "↑↓".into(), action: "Navigate".into() },
            HintItem { key: "/".into(), action: "Commands".into() },
            HintItem { key: "q".into(), action: "Back".into() },
            HintItem { key: "Esc".into(), action: "Back".into() },
        ],
        ScreenState::Conversation => vec![
            HintItem { key: "i".into(), action: "Insert mode".into() },
            HintItem { key: "Enter".into(), action: "Send".into() },
            HintItem { key: "?".into(), action: "Help".into() },
            HintItem { key: "Esc".into(), action: "Menu".into() },
            HintItem { key: "/".into(), action: "Commands".into() },
        ],
        ScreenState::Help => vec![
            HintItem { key: "Esc".into(), action: "Back".into() },
            HintItem { key: "q".into(), action: "Quit".into() },
        ],
        _ => vec![
            HintItem { key: "Esc".into(), action: "Back".into() },
        ],
    }
}

fn build_breadcrumb(state: &ShellState) -> String {
    match state.screen {
        ScreenState::MainMenu => "⬡ Ozone+".into(),
        ScreenState::SessionList => "⬡ Ozone+ › Sessions".into(),
        ScreenState::CharacterManager => "⬡ Ozone+ › Characters".into(),
        ScreenState::CharacterCreate => "⬡ Ozone+ › Characters › New".into(),
        ScreenState::CharacterImport => "⬡ Ozone+ › Characters › Import".into(),
        ScreenState::Settings => "⬡ Ozone+ › Settings".into(),
        ScreenState::Conversation => format!("⬡ Ozone+ › {}", state.session.context.title),
        ScreenState::Help => "⬡ Ozone+ › Help".into(),
        ScreenState::Quit => "⬡ Ozone+".into(),
    }
}

pub fn render_shell(frame: &mut Frame, layout: &LayoutModel, model: &RenderModel) {
    let full_area = frame.area();

    // Reserve bottom row for hints
    let hint_area = if full_area.height > 3 && !model.hints.is_empty() {
        Rect::new(full_area.x, full_area.y + full_area.height - 1, full_area.width, 1)
    } else {
        Rect::default()
    };

    // Reserve top row for breadcrumb
    let breadcrumb_area = if full_area.height > 5 {
        Rect::new(full_area.x + 1, full_area.y, full_area.width.saturating_sub(2), 1)
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
            render_command_palette(frame, palette);
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

    if let (Some(pane), Some(model)) = (layout.overlay.as_ref(), model.overlay.as_ref()) {
        render_overlay(frame, pane, model);
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
        render_command_palette(frame, palette);
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

fn render_command_palette(frame: &mut Frame, model: &CommandPaletteRenderModel) {
    let area = frame.area();
    let width = 60u16.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let max_entries = 8usize.min(model.entries.len());
    let height = (max_entries as u16) + 3; // input + separator + entries + border
    let palette_area = Rect::new(x, area.y + 2, width, height);

    frame.render_widget(Clear, palette_area);

    let mut lines = vec![];

    let input_line = Line::from(vec![
        Span::styled(" / ", theme::accent_style()),
        Span::styled(&model.input, theme::text_style()),
        Span::styled("▌", theme::dim_style()),
    ]);
    lines.push(input_line);

    lines.push(Line::from(Span::styled(
        "─".repeat(width.saturating_sub(2) as usize),
        theme::dim_style(),
    )));

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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(" Command Palette ", theme::accent_style()));

    frame.render_widget(
        Paragraph::new(lines).block(block),
        palette_area,
    );
}

/// Floating autocomplete popup rendered ABOVE the composer pane.
/// Only shown when there are slash suggestions and the command palette is NOT open.
fn render_slash_popup(frame: &mut Frame, composer_pane: &PaneLayout, model: &ComposerPaneModel) {
    if model.slash_suggestions.is_empty() {
        return;
    }

    let max_items = 5usize.min(model.slash_suggestions.len());
    let popup_height = (max_items as u16) + 2; // items + top/bottom border
    let popup_width = composer_pane.area.width;
    let popup_x = composer_pane.area.x;
    let popup_y = composer_pane.area.y.saturating_sub(popup_height);

    if popup_height < 3 || popup_width < 10 {
        return;
    }

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let mut lines: Vec<Line> = Vec::with_capacity(max_items);
    for suggestion in model.slash_suggestions.iter().take(max_items) {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", suggestion.name), theme::accent_style()),
            Span::styled("  ", Style::default()),
            Span::styled(&suggestion.description, theme::dim_style()),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(" / Commands ", theme::accent_style()));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(lines).block(block),
        popup_area,
    );
}

fn render_breadcrumb(frame: &mut Frame, area: Rect, breadcrumb: &str) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![
        Span::styled(breadcrumb, theme::accent_style()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_conversation(frame: &mut Frame, pane: &PaneLayout, model: &RenderModel, focused: bool) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", theme::HEX), theme::brand_hex_style()),
            Span::styled(
                model.title.clone(),
                theme::text_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!("{} · {}", model.subtitle, model.conversation.subtitle),
            theme::dim_style(),
        )),
        Line::default(),
    ];

    if model.conversation.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            model.conversation.empty_state.clone(),
            theme::dim_style(),
        )));
    } else {
        for entry in &model.conversation.entries {
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

            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                bookmark_indicator,
                Span::styled(format!("{:<10}", entry.author), author_style),
                Span::raw(" "),
                Span::styled(entry.content.clone(), theme::text_style()),
            ]));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        model.conversation.hint.clone(),
        theme::dim_style(),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.conversation.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_composer(frame: &mut Frame, pane: &PaneLayout, model: &ComposerPaneModel, focused: bool) {
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

    let draft_state = if model.dirty {
        "dirty"
    } else {
        "clean"
    };
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("mode ", theme::dim_style()),
        Span::styled(model.mode.clone(), theme::mode_badge_style()),
        Span::styled(
            format!(" · cursor {} · {}", model.cursor, draft_state),
            theme::dim_style(),
        ),
    ]));
    lines.push(Line::from(Span::styled(model.hint.clone(), theme::dim_style())));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );

    // Place the terminal cursor in the composer when in insert mode.
    // Block border offsets: +1 for left border, +1 for top border.
    if model.show_cursor && focused {
        let inner_x = pane.area.x + 1;
        let inner_y = pane.area.y + 1;
        let inner_width = pane.area.width.saturating_sub(2) as usize;

        if inner_width > 0 {
            // Walk through draft text to find cursor row/col with wrapping.
            let text = model.lines.join("\n");
            let mut row: u16 = 0;
            let mut col: u16 = 0;

            for (char_count, ch) in text.chars().enumerate() {
                if char_count == model.cursor {
                    break;
                }
                if ch == '\n' {
                    row += 1;
                    col = 0;
                } else {
                    col += 1;
                    if col as usize >= inner_width {
                        row += 1;
                        col = 0;
                    }
                }
            }

            let cursor_x = inner_x + col;
            let cursor_y = inner_y + row;

            // Only set cursor if it fits within the pane.
            if cursor_x < pane.area.x + pane.area.width
                && cursor_y < pane.area.y + pane.area.height
            {
                frame.set_cursor_position(Position::new(cursor_x, cursor_y));
            }
        }
    }
}

fn render_status(frame: &mut Frame, pane: &PaneLayout, model: &StatusPaneModel, focused: bool) {
    let mut lines = Vec::new();

    // Summary line with optional mode badge
    if let Some(badge) = &model.mode_badge {
        let badge_style = match badge.as_str() {
            "INSERT" => theme::accent_style().add_modifier(Modifier::BOLD),
            "COMMAND" => theme::title_focused_style().add_modifier(Modifier::BOLD),
            _ => theme::dim_style().add_modifier(Modifier::BOLD),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("[{badge}] "), badge_style),
            Span::styled(model.summary.clone(), theme::text_style().add_modifier(Modifier::BOLD)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            model.summary.clone(),
            theme::text_style().add_modifier(Modifier::BOLD),
        )));
    }

    lines.extend(
        model
            .notifications
            .iter()
            .cloned()
            .map(|line| Line::from(Span::styled(line, theme::dim_style()))),
    );
    lines.push(Line::from(Span::styled(model.hint.clone(), theme::dim_style())));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_inspector(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &InspectorPaneModel,
    focused: bool,
) {
    let lines: Vec<Line> = model
        .lines
        .iter()
        .cloned()
        .map(|line| Line::from(Span::styled(line, theme::dim_style())))
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
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
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_session_list(frame: &mut Frame, pane: &PaneLayout, model: &SessionListRenderModel) {
    let area = pane.area;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
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

    // Filter bar (if active)
    if !model.filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  filter: ", theme::dim_style()),
            Span::styled(model.filter.clone(), theme::mode_badge_style()),
        ]));
    }

    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    // Column headers
    lines.push(Line::from(vec![
        Span::styled("      ", theme::dim_style()),
        Span::styled(format!("{:<30}", "Name"), theme::dim_style()),
        Span::styled(format!("{:<16}", "Character"), theme::dim_style()),
        Span::styled(format!("{:<10}", "Messages"), theme::dim_style()),
        Span::styled("Last Active", theme::dim_style()),
    ]));

    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    if model.loading {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  Loading sessions\u{2026}",
            theme::dim_style(),
        )));
    } else if model.entries.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            if model.filter.is_empty() {
                "  No sessions yet \u{2014} press n to create one"
            } else {
                "  No sessions match the current filter"
            },
            theme::dim_style(),
        )));
    } else {
        for entry in &model.entries {
            let (marker, name_style, detail_style) = if entry.selected {
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

            lines.push(Line::from(vec![
                Span::styled(
                    marker,
                    if entry.selected {
                        theme::highlight_style()
                    } else {
                        theme::muted_style()
                    },
                ),
                Span::styled(
                    format!("{:<30}", truncate_str(&entry.name, 28)),
                    name_style,
                ),
                Span::styled(
                    format!("{:<16}", truncate_str(&entry.character, 14)),
                    detail_style,
                ),
                Span::styled(format!("{:<10}", entry.message_count), detail_style),
                Span::styled(entry.last_active.clone(), detail_style),
            ]));
        }
    }

    // Hint bar at bottom
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("  {}", model.hint),
        theme::dim_style(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            format!(" {} Sessions ", theme::HEX),
            theme::title_focused_style(),
        ));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_character_list(frame: &mut Frame, pane: &PaneLayout, model: &CharacterListRenderModel) {
    let area = pane.area;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
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

    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    if model.entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No characters yet",
            theme::text_style().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press n to create your first character card,",
            theme::dim_style(),
        )));
        lines.push(Line::from(Span::styled(
            "  or press i to import a JSON character card.",
            theme::dim_style(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Supports SillyTavern, TavernAI, and ozone-native formats.",
            theme::dim_style(),
        )));
    } else {
        // Column headers
        lines.push(Line::from(vec![
            Span::styled("      Name                          ", theme::dim_style()),
            Span::styled("Sessions  ", theme::dim_style()),
            Span::styled("Description", theme::dim_style()),
        ]));
        lines.push(Line::from(Span::styled(
            "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            theme::muted_style(),
        )));

        for entry in &model.entries {
            let marker = if entry.selected {
                format!("  {} ", theme::HEX_FILLED)
            } else {
                format!("  {} ", theme::HEX)
            };
            let style = if entry.selected {
                theme::highlight_style()
            } else {
                theme::text_style()
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{:<30}", truncate_str(&entry.name, 30)), style),
                Span::styled(format!("{:<10}", entry.session_count), theme::dim_style()),
                Span::styled(truncate_str(&entry.description, 40), theme::dim_style()),
            ]));
        }
    }

    // Selected detail panel
    if let Some(detail) = &model.selected_detail {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            theme::muted_style(),
        )));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                &detail.name,
                theme::title_focused_style().add_modifier(Modifier::BOLD),
            ),
        ]));
        for chunk in detail.description.as_bytes().chunks(70) {
            if let Ok(s) = std::str::from_utf8(chunk) {
                lines.push(Line::from(Span::styled(
                    format!("  {s}"),
                    theme::text_style(),
                )));
            }
        }
        lines.push(Line::from(Span::styled(
            format!("  {} session(s)", detail.session_count),
            theme::dim_style(),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(
            format!(" {} Characters ", theme::HEX),
            theme::accent_style(),
        ));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_character_form(frame: &mut Frame, pane: &PaneLayout, model: &CharacterFormRenderModel) {
    let area = pane.area;
    let mut lines = vec![];

    let title = match model.form_type {
        CharacterFormType::Create => "New Character",
        CharacterFormType::Import => "Import Character Card",
    };

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} {title}", theme::HEX),
            theme::title_focused_style().add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));
    lines.push(Line::from(""));

    match model.form_type {
        CharacterFormType::Create => {
            // Name field
            let name_active = model.active_field == crate::app::CharacterFormField::Name;
            let name_label_style = if name_active {
                theme::accent_style().add_modifier(Modifier::BOLD)
            } else {
                theme::dim_style()
            };
            lines.push(Line::from(Span::styled("  Name", name_label_style)));
            let name_indicator = if name_active { "\u{25b6} " } else { "  " };
            let name_display = if model.name_text.is_empty() {
                "(type character name)".to_string()
            } else {
                model.name_text.clone()
            };
            let name_style = if model.name_text.is_empty() && name_active {
                theme::dim_style()
            } else if name_active {
                theme::text_style().add_modifier(Modifier::UNDERLINED)
            } else {
                theme::text_style()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {name_indicator}"), theme::accent_style()),
                Span::styled(name_display, name_style),
            ]));
            lines.push(Line::from(""));

            // System Prompt field
            let prompt_active = model.active_field == crate::app::CharacterFormField::SystemPrompt;
            let prompt_label_style = if prompt_active {
                theme::accent_style().add_modifier(Modifier::BOLD)
            } else {
                theme::dim_style()
            };
            lines.push(Line::from(Span::styled("  System Prompt", prompt_label_style)));
            let prompt_indicator = if prompt_active { "\u{25b6} " } else { "  " };
            let prompt_display = if model.system_prompt_text.is_empty() {
                "(optional: describe this character's personality)".to_string()
            } else {
                model.system_prompt_text.clone()
            };
            let prompt_style = if model.system_prompt_text.is_empty() && prompt_active {
                theme::dim_style()
            } else if prompt_active {
                theme::text_style().add_modifier(Modifier::UNDERLINED)
            } else {
                theme::text_style()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {prompt_indicator}"), theme::accent_style()),
                Span::styled(prompt_display, prompt_style),
            ]));
        }
        CharacterFormType::Import => {
            lines.push(Line::from(Span::styled("  File Path", theme::accent_style().add_modifier(Modifier::BOLD))));
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
        CharacterFormType::Create => "  Tab switch field \u{00b7} Enter save \u{00b7} Esc cancel",
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
    let mut lines = vec![];

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} Settings", theme::HEX),
            theme::title_focused_style().add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));

    if model.entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No configuration loaded.",
            theme::dim_style(),
        )));
        lines.push(Line::from(Span::styled(
            "  Session settings are stored in config.toml.",
            theme::dim_style(),
        )));
    } else {
        let mut current_category = String::new();
        for entry in &model.entries {
            if entry.category != current_category {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} {}", theme::HEX_FILLED, entry.category),
                        theme::title_focused_style(),
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                    theme::muted_style(),
                )));
                current_category = entry.category.clone();
            }
            lines.push(Line::from(vec![
                Span::styled(format!("    {:<24}", entry.key), theme::text_style()),
                Span::styled(&entry.value, theme::highlight_style()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        theme::muted_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  Edit config.toml directly to change settings.",
        theme::dim_style(),
    )));
    lines.push(Line::from(Span::styled(
        "  Changes take effect on next session open.",
        theme::dim_style(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focus_border_style())
        .title(Span::styled(
            format!(" {} Settings ", theme::HEX),
            theme::accent_style(),
        ));

    frame.render_widget(
        Paragraph::new(lines).block(block),
        area,
    );
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
        ScreenState::CharacterImport => "character import",
        ScreenState::Settings => "settings",
        ScreenState::Conversation => "conversation",
        ScreenState::Help => "help",
        ScreenState::Quit => "quit",
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
            "i insert · b bookmark · Ctrl+K pin · Tab conversation · Ctrl+D dry-run · ? help"
        }
        InputMode::Insert => {
            "Enter send · Esc normal · Ctrl+C cancel · Ctrl+D dry-run · Ctrl+I inspector"
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
        | ScreenState::CharacterImport
        | ScreenState::Settings
        | ScreenState::Conversation => None,
        ScreenState::Help => Some(OverlayRenderModel {
            title: "Help".into(),
            lines: vec![
                format!("⬡ ozone+ TUI — current mode: {}", input_mode_label(input_mode)),
                String::new(),
                "Navigation".into(),
                "  j / k          move selection up/down".into(),
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
                "  Ctrl+I         toggle the inspector pane".into(),
                "  q              quit".into(),
                String::new(),
                "Slash Commands".into(),
                "  /session show              session metadata".into(),
                "  /session rename NAME       rename session".into(),
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

    use super::{build_render_model, render_shell};
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

        assert!(rendered.contains("Ozone+"), "breadcrumb should be visible on top row");
        assert!(rendered.contains("Composer"));
        assert!(rendered.contains("Status"));
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
        assert!(rendered.contains("INSERT"));
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
            .draw(|frame| render_shell(frame, layout, model))
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
    fn main_menu_screen_produces_menu_render_model() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.main_menu.is_some());
        assert!(model.session_list.is_none());

        let menu = model.main_menu.unwrap();
        assert_eq!(menu.items.len(), 5);
        assert!(menu.items[0].selected); // first item selected by default
        assert!(!menu.items[1].selected);
        assert_eq!(menu.items[0].label, "New Chat");
        assert_eq!(menu.items[1].label, "Sessions");
        assert_eq!(menu.items[4].label, "Quit");
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
            },
            crate::app::SessionListEntry {
                session_id: "test-2".into(),
                name: "World Building".into(),
                character_name: None,
                message_count: 7,
                last_active: Some("yesterday".into()),
            },
        ];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.session_list.is_some());
        assert!(model.main_menu.is_none());

        let list = model.session_list.unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(list.entries[0].selected);
        assert!(!list.entries[1].selected);
        assert_eq!(list.entries[0].name, "My First Chat");
        assert_eq!(list.entries[0].character, "Aster");
        assert_eq!(list.entries[1].character, "—");
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
                render_shell(frame, &layout, &model);
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
        }];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model);
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
                render_shell(frame, &layout, &model);
            })
            .unwrap();
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
