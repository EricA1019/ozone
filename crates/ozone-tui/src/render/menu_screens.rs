use super::model_types::*;
use super::helpers::*;
use crate::layout::PaneLayout;
use crate::state::{EntryKind, FolderPickerState};
use crate::theme;
use ratatui::{
    layout::{Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

pub fn render_main_menu(frame: &mut Frame, pane: &PaneLayout, model: &MainMenuRenderModel) {
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

pub fn render_menu_placeholder(frame: &mut Frame, pane: &PaneLayout, title: &str) {
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

pub fn render_session_list(frame: &mut Frame, pane: &PaneLayout, model: &SessionListRenderModel) {
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

pub fn render_folder_picker(frame: &mut Frame, area: Rect, model: &FolderPickerRenderModel) {
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

pub fn render_character_list(frame: &mut Frame, pane: &PaneLayout, model: &CharacterListRenderModel) {
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
        if let Some(greeting) = &detail.greeting {
            detail_lines.push(Line::from(Span::styled(
                "  Greeting:",
                theme::muted_style(),
            )));
            for chunk in greeting.as_bytes().chunks(70) {
                if let Ok(s) = std::str::from_utf8(chunk) {
                    detail_lines.push(Line::from(Span::styled(
                        format!("  {s}"),
                        theme::text_style(),
                    )));
                }
            }
        }
        detail_lines.push(Line::from(Span::styled(
            format!("  {} session(s)", detail.session_count),
            theme::dim_style(),
        )));
        frame.render_widget(Paragraph::new(detail_lines), detail_area);
    }
}

pub fn render_character_form(frame: &mut Frame, pane: &PaneLayout, model: &CharacterFormRenderModel) {
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

pub fn render_settings(frame: &mut Frame, pane: &PaneLayout, model: &SettingsRenderModel) {
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

pub fn render_model_intelligence(
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

pub fn build_folder_picker_model(picker: &FolderPickerState) -> Option<FolderPickerRenderModel> {
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

