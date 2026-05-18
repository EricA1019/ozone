use crate::state::*;
use crate::input::InputMode;


impl ShellState {
    pub fn slash_completion_names(&self) -> Vec<String> {
        if self.message_edit.is_some()
            || !self.draft.text.starts_with('/')
            || self.draft.text.contains(' ')
        {
            return Vec::new();
        }
        let query = self
            .draft
            .text
            .get(1..)
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        CommandEntry::slash_matches(&query)
            .into_iter()
            .map(|cmd| format!("/{}", cmd.name))
            .collect()
    }

    /// True when the slash popup should be visible to the user.
    pub fn slash_popup_active(&self) -> bool {
        self.message_edit.is_none()
            && self.input_mode == InputMode::Insert
            && !self.command_palette.open
            && !self.slash_dismissed
            && !self.slash_completion_names().is_empty()
    }

    /// Move popup highlight up (wraps from top to bottom).
    pub fn slash_move_up(&mut self) {
        let len = self.slash_completion_names().len();
        if len == 0 {
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(i) if i > 0 => i - 1,
            _ => len - 1,
        });
    }

    /// Move popup highlight down (wraps from bottom to top).
    pub fn slash_move_down(&mut self) {
        let len = self.slash_completion_names().len();
        if len == 0 {
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(i) if i + 1 < len => i + 1,
            _ => 0,
        });
    }

    /// Fill the draft with the currently highlighted suggestion.
    /// Returns `true` if a suggestion was accepted, `false` if nothing was selected.
    pub fn slash_accept(&mut self) -> bool {
        if let Some(idx) = self.slash_selected {
            let names = self.slash_completion_names();
            if let Some(name) = names.get(idx) {
                let filled = name.clone() + " ";
                self.replace_draft(DraftState::with_text(filled));
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
                self.slash_selected = None;
                self.slash_dismissed = false;
                return true;
            }
        }
        false
    }

    /// Keep `slash_selected` / `slash_dismissed` consistent with the current draft.
    /// Called automatically at the end of every `apply_action`.
    pub(crate) fn sync_slash_state(&mut self) {
        let names = self.slash_completion_names();
        let has_suggestions = !names.is_empty() && !self.command_palette.open;

        if !has_suggestions {
            // No applicable suggestions — reset everything.
            self.slash_selected = None;
            self.slash_dismissed = false;
        } else if self.slash_dismissed {
            // Popup was dismissed; keep selected = None but don't reopen.
            self.slash_selected = None;
        } else {
            // Suggestions exist and popup is not dismissed.
            if self.slash_selected.is_none() {
                // Auto-highlight the first item when popup first appears.
                self.slash_selected = Some(0);
            } else {
                // Clamp in case the list shrank (e.g., user typed more).
                let len = names.len();
                if let Some(idx) = self.slash_selected {
                    if idx >= len {
                        self.slash_selected = Some(0);
                    }
                }
            }
        }
    }


}
