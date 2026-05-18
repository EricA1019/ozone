use super::model_types::*;
use super::helpers::*;
use crate::state::CommandEntry;
use crate::input::InputMode;
use crate::layout::{PaneLayout};
use crate::theme;
use ratatui::{
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use tui_textarea::TextArea;

pub fn render_composer(
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

pub fn render_composer_scrollbar(frame: &mut Frame, area: Rect, model: &ComposerPaneModel, text: &str) {
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

pub fn composer_scroll_offset(model: &ComposerPaneModel, text: &str, width: u16, height: u16) -> usize {
    composer_scroll_metrics(model, text, Rect::new(0, 0, width, height))
        .map(|(_, offset)| offset)
        .unwrap_or(0)
}

pub fn composer_scroll_metrics(
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

pub fn visual_line_count(text: &str, width: usize) -> usize {
    visual_cursor_position(text, width, text.chars().count()).0 as usize + 1
}

pub fn visual_cursor_position(text: &str, width: usize, cursor: usize) -> (u16, u16) {
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

pub fn render_slash_popup(frame: &mut Frame, composer_pane: &PaneLayout, model: &ComposerPaneModel) {
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

pub fn build_slash_suggestions(draft_text: &str) -> Vec<SlashSuggestion> {
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
    CommandEntry::slash_matches(&query)
        .into_iter()
        .map(|cmd| SlashSuggestion {
            name: format!("/{}", cmd.name),
            description: cmd.description,
        })
        .collect()
}

pub fn composer_hint(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => {
            "i insert · r reroll · / commands · a attach · b bookmark · Ctrl+K pin · Tab conversation · Ctrl+D dry-run · ? help"
        }
        InputMode::Insert => {
            "Enter send · Esc normal · ↑↓ history · Ctrl+D dry-run · F2 inspector"
        }
        InputMode::Command => "Enter send · Esc normal · Ctrl+C cancel · Ctrl+D dry-run",
        InputMode::Visual => "h/j/k/l move · y yank · d/x delete · c change · Esc exit",
    }
}

