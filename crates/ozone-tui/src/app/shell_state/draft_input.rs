use crate::app::shell_state::utils::textarea_cursor_offset;
use crate::state::*;
use crate::app::TextAreaSurface;
use crate::app::textareas::themed_textarea_from_text;
use crate::input::{InputMode, KeyAction};


impl ShellState {
    fn active_textarea_surface(&self) -> TextAreaSurface {
        if self.message_edit.is_some() {
            TextAreaSurface::MessageEdit
        } else {
            TextAreaSurface::Composer
        }
    }

    fn sync_draft_from_textarea(&mut self) {
        let lines = self.textarea.lines();
        let cursor = self.textarea.cursor();
        self.draft.text = lines.join("\n");
        self.draft.cursor = textarea_cursor_offset(lines, cursor.0, cursor.1);
        self.draft.sync_dirty();
    }
}
