use super::model_types::*;
use super::helpers::*;
use super::labels::*;
use crate::app::ShellState;
use crate::state::{RecallBrowser, ScreenState};
use crate::input::InputMode;
use crate::layout::PaneLayout;
use crate::theme;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tui_textarea::TextArea;

pub fn render_overlay(frame: &mut Frame, pane: &PaneLayout, model: &OverlayRenderModel) {
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

pub fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let overlay = Block::default().style(theme::overlay_backdrop_style());
    frame.render_widget(Clear, area);
    frame.render_widget(overlay, area);

    let width = 60_u16.min(area.width.saturating_sub(4));
    let height = 22_u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let help_area = Rect::new(x, y, width, height);

    let help_text = vec![
        Line::from(Span::styled("⬡ Keybindings", theme::accent_style())),
        Line::from(""),
        Line::from(Span::styled("── Normal Mode ──", theme::overlay_section_style())),
        Line::from("  j/k      Scroll transcript"),
        Line::from("  ↑/↓      Move selected message"),
        Line::from("  i        Enter Insert mode"),
        Line::from("  I        Toggle Inspector"),
        Line::from("  r        Reroll selected assistant reply"),
        Line::from("  Ctrl+I   Edit selected message"),
        Line::from("  b        Toggle bookmark"),
        Line::from("  Ctrl+K   Pin to memory"),
        Line::from("  /        Command palette"),
        Line::from("  ?        Toggle help"),
        Line::from(""),
        Line::from(Span::styled("── Insert Mode ──", theme::overlay_section_style())),
        Line::from("  Enter    Send message"),
        Line::from("  Esc      Normal / cancel edit"),
        Line::from("  Tab      Autocomplete"),
        Line::from("  Ctrl+U   Undo"),
        Line::from("  Ctrl+Y   Redo"),
        Line::from("  F2       Toggle Inspector"),
        Line::from(""),
        Line::from(Span::styled("── Global ──", theme::overlay_section_style())),
        Line::from("  Ctrl+C   Cancel generation"),
        Line::from("  Ctrl+D   Context dry-run"),
        Line::from("  Ctrl+K   Pin to memory"),
    ];

    let help = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::focus_border_style())
            .title(" Help (Esc/q/? close) ")
            .title_style(theme::accent_style()),
    );

    frame.render_widget(help, help_area);
}

pub fn render_toast(frame: &mut Frame, area: Rect, message: &str) {
    let msg_width = (message.len() as u16 + 4).min(area.width);
    let x = area.x + area.width.saturating_sub(msg_width).saturating_sub(1);
    let y = area.y + area.height.saturating_sub(3);
    let toast_area = Rect::new(x, y, msg_width, 1);

    let toast = Paragraph::new(Line::from(Span::styled(
        format!(" {} ", message),
        theme::toast_style(),
    )));

    frame.render_widget(Clear, toast_area);
    frame.render_widget(toast, toast_area);
}

pub fn render_command_palette(
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

fn memories_overlay_model(recall_browser: Option<&RecallBrowser>) -> OverlayRenderModel {
    let title = recall_browser
        .map(|browser| browser.title.clone())
        .unwrap_or_else(|| "Memories".into());

    let mut lines = match recall_browser {
        Some(browser) => {
            let mut lines = vec![browser.summary.clone(), String::new()];
            if browser.lines.is_empty() {
                lines.push("No pinned or note memories are available yet.".into());
            } else {
                lines.extend(browser.lines.iter().cloned());
            }
            lines
        }
        None => vec![
            "Loading pinned memories…".into(),
            String::new(),
            "Run :memories or /memories from the composer to refresh this view.".into(),
        ],
    };

    lines.push(String::new());
    lines.push("Esc/q close".into());

    OverlayRenderModel { title, lines }
}

pub fn overlay_model(
    screen: &ScreenState,
    input_mode: InputMode,
    recall_browser: Option<&RecallBrowser>,
) -> Option<OverlayRenderModel> {
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
        ScreenState::MemoriesOverlay => Some(memories_overlay_model(recall_browser)),
        ScreenState::CharacterOverlay(_) => None,
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
                "  r              reroll the selected assistant reply".into(),
                "  b              toggle bookmark on selected message".into(),
                "  Ctrl+K         pin/unpin selected message to hard context".into(),
                "  Enter          send current draft".into(),
                "  Ctrl+C         cancel active generation".into(),
                "  Ctrl+D         build a context dry-run preview".into(),
                "  Ctrl+I         edit the selected persisted message".into(),
                "  I / F2         toggle the inspector pane".into(),
                "  Esc / q / ?    close help".into(),
                String::new(),
                "Slash Commands".into(),
                "  /session show              session metadata".into(),
                "  /session rename NAME       rename session".into(),
                "  /session retitle           generate session title".into(),
                "  /session reroll            reroll selected assistant reply".into(),
                "  /session character NAME     set character".into(),
                "  /session tags a,b          set tags".into(),
                "  /memories                  open recall browser".into(),
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

pub fn selected_message_line(state: &ShellState) -> String {
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

