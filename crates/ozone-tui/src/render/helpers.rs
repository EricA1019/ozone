use crate::theme;
use ratatui::{
    style::Modifier,
    text::Span,
    widgets::{Block, Borders},
};

pub fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
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

pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

pub fn pane_block(title: &str, focused: bool) -> Block<'static> {
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

pub fn overlay_block(title: &str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {} {} ", theme::HEX_FILLED, title),
            theme::warning_style().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme::warning_style())
}

