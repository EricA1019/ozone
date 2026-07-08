//! Settings and exit-confirmation screen rendering for the launcher.
//!
//! Extracted from `launcher.rs`.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::App;
use crate::theme::*;

// Re-use helpers from the parent launcher module.
use super::launcher::{chrome_block, chrome_block_with_hint, launcher_title};

pub fn render_settings(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(29),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(72),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // summary
            Constraint::Length(5), // backend block
            Constraint::Length(6), // model directory block
            Constraint::Length(3), // hint
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(Span::styled(
        " tune launcher defaults before model selection",
        style_muted(),
    )))
    .block(chrome_block_with_hint(
        launcher_title("Settings"),
        "↑↓ choose · Enter save · Esc back",
        style_lime(),
    ));
    f.render_widget(header, chunks[0]);

    let summary = Paragraph::new(Line::from(vec![
        Span::styled(" backend ", style_muted()),
        Span::styled("llama.cpp", style_cyan()),
    ]))
    .block(chrome_block(
        Line::from(Span::styled(" Active Defaults ", style_bold_cyan())),
        style_gray(),
    ));
    f.render_widget(summary, chunks[1]);

    // Backend block
    let backend_block = chrome_block(
        Line::from(Span::styled(
            " Backend ",
            style_panel_title(app.settings_section == 0),
        )),
        style_panel_border(app.settings_section == 0),
    );
    let backend_inner = backend_block.inner(chunks[2]);
    f.render_widget(backend_block, chunks[2]);

    let backend_options = ["llama.cpp"];
    let backend_items: Vec<ListItem> = backend_options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let selected = i == app.settings_backend_index;
            let focused = app.settings_section == 0;
            let marker = if selected && focused {
                HEX_CURSOR
            } else if selected {
                "●"
            } else {
                "○"
            };
            let style = if selected && focused {
                style_bold_lime()
            } else if selected {
                style_bold_cyan()
            } else {
                style_gray()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {marker} "), style),
                Span::styled(*label, style),
                Span::styled(
                    if selected { "  selected" } else { "" },
                    if focused {
                        style_hint_key()
                    } else {
                        style_muted()
                    },
                ),
            ]))
        })
        .collect();
    f.render_widget(List::new(backend_items), backend_inner);

    // Model directory block
    let model_dir_focused = app.settings_section == 1;
    let model_dir_block = chrome_block(
        Line::from(Span::styled(
            " Models Directory ",
            style_panel_title(model_dir_focused),
        )),
        style_panel_border(model_dir_focused),
    );
    let model_dir_inner = model_dir_block.inner(chunks[3]);
    f.render_widget(model_dir_block, chunks[3]);

    let current_dir = if app.settings_editing {
        &app.settings_input_buffer
    } else {
        app.prefs.models_dir.as_deref().unwrap_or("")
    };

    let dir_display = if app.settings_editing {
        format!("{current_dir}▌")
    } else {
        let default = ozone_core::paths::models_dir();
        let shown = if current_dir.is_empty() {
            default.to_string_lossy().to_string()
        } else {
            current_dir.to_string()
        };
        if model_dir_focused {
            format!("{shown}  ⬡ press Enter to edit")
        } else {
            shown
        }
    };

    let dir_paragraph = Paragraph::new(Line::from(Span::styled(
        &dir_display,
        if app.settings_editing {
            style_bold_cyan()
        } else {
            style_gray()
        },
    )));
    f.render_widget(dir_paragraph, model_dir_inner);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" choose  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" save  ", style_muted()),
        Span::styled("Esc", style_hint_key()),
        Span::styled(" back", style_muted()),
    ]))
    .block(chrome_block(
        Line::from(Span::styled(" Navigation ", style_bold_cyan())),
        style_gray(),
    ));
    f.render_widget(hint, chunks[4]);
}

pub fn render_exit_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(9),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(54),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let yes_style = if app.exit_confirm_index == 0 {
        style_bold_lime()
    } else {
        style_gray()
    };
    let no_style = if app.exit_confirm_index == 1 {
        style_bold_cyan()
    } else {
        style_gray()
    };
    let yes_marker = if app.exit_confirm_index == 0 {
        format!("{} Quit", HEX_CURSOR)
    } else {
        "  Quit".to_string()
    };
    let no_marker = if app.exit_confirm_index == 1 {
        format!("{} Stay", HEX_CURSOR)
    } else {
        "  Stay".to_string()
    };

    let lines = vec![
        Line::from(Span::styled("  Leave Ozone?", style_bold_lime())),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "  Esc from the launcher now asks before quitting.",
            style_gray(),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("  ", style_gray()),
            Span::styled(yes_marker, yes_style),
            Span::styled("     ", style_gray()),
            Span::styled(no_marker, no_style),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(
            format!(" {} Confirm Exit ", HEX_CURSOR),
            style_bold_lime(),
        ))
        .title_bottom(Line::from(Span::styled(
            "  ←→ choose · Enter confirm · Esc back",
            style_gray(),
        )))
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(Paragraph::new(lines).block(block), center_h);
}
