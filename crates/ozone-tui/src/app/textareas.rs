use super::TextAreaSurface;
use crate::app::shell_state::utils::{textarea_cursor_position, textarea_lines};
use ratatui::{
    style::{Color, Modifier, Style},
};
use tui_textarea::TextArea;


/// Create a fresh TextArea with ozone+ theme styling.
pub(crate) fn new_themed_textarea() -> TextArea<'static> {
    new_themed_textarea_for(TextAreaSurface::Composer)
}

pub(crate) fn new_themed_textarea_for(surface: TextAreaSurface) -> TextArea<'static> {
    let mut textarea = TextArea::default();
    configure_themed_textarea(&mut textarea, surface);
    textarea
}

fn configure_themed_textarea(textarea: &mut TextArea<'static>, surface: TextAreaSurface) {
    textarea.set_block(ratatui::widgets::Block::default());
    textarea.set_style(Style::default().fg(crate::theme::cyan(crate::theme::active_preset())));
    textarea.set_cursor_style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::REVERSED),
    );
    textarea.set_selection_style(crate::theme::textarea_selection_style());
    textarea.set_placeholder_style(crate::theme::textarea_placeholder_style());

    match surface {
        TextAreaSurface::Composer => {
            textarea.set_cursor_line_style(Style::default());
            textarea.remove_line_number();
            textarea.set_placeholder_text("Type a message · / or : for commands");
            textarea.set_tab_length(4);
            textarea.set_max_histories(256);
        }
        TextAreaSurface::MessageEdit => {
            textarea.set_cursor_line_style(crate::theme::textarea_cursor_line_style());
            textarea.set_placeholder_text("Edit selected transcript message");
            textarea.set_tab_length(4);
            textarea.set_max_histories(256);
            if textarea.lines().len() > 1 {
                textarea.set_line_number_style(crate::theme::textarea_line_number_style());
            } else {
                textarea.remove_line_number();
            }
        }
        TextAreaSurface::CommandPalette => {
            textarea.set_cursor_line_style(Style::default());
            textarea.remove_line_number();
            textarea.set_placeholder_text("Type a command");
            textarea.set_tab_length(0);
            textarea.set_max_histories(64);
        }
    }
}

pub(crate) fn themed_textarea_from_text(
    surface: TextAreaSurface,
    text: &str,
    cursor: usize,
) -> TextArea<'static> {
    let lines = textarea_lines(text);
    let mut textarea = TextArea::new(lines.clone());
    configure_themed_textarea(&mut textarea, surface);
    let (row, col) = textarea_cursor_position(&lines, cursor);
    textarea.move_cursor(tui_textarea::CursorMove::Jump(row, col));
    textarea
}
