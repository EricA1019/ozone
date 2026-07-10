use crate::state::*;
use crate::input::{InputMode, KeyAction, dispatch_command_palette_key, dispatch_menu_key, dispatch_form_key, dispatch_edit_key, dispatch_key};
use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};


impl ShellState {
    pub(crate) fn handle_key_event_with_layout(
        &mut self,
        key: KeyEvent,
        layout: &crate::layout::LayoutModel,
    ) -> KeyAction {
        // Command palette takes priority when open
        if self.command_palette.open {
            if let Some(action) = dispatch_command_palette_key(key) {
                self.apply_action_with_layout(action, layout);
                return action;
            }
            return KeyAction::Noop;
        }

        if matches!(
            self.screen,
            ScreenState::MemoriesOverlay | ScreenState::CharacterOverlay(_)
        ) {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                self.screen = ScreenState::Conversation;
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
            }
            return KeyAction::Noop;
        }

        if self.screen == ScreenState::Help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                self.screen = ScreenState::Conversation;
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
            }
            return KeyAction::Noop;
        }

        // Slash-popup navigation: intercept arrow/Enter/Esc when popup is visible.
        if self.message_edit.is_none()
            && self.slash_selected.is_some()
            && matches!(
                self.screen,
                ScreenState::Conversation | ScreenState::Help | ScreenState::Quit
            )
        {
            let slash_action = match key.code {
                KeyCode::Up => Some(KeyAction::SlashUp),
                KeyCode::Down => Some(KeyAction::SlashDown),
                KeyCode::Enter => Some(KeyAction::SlashAccept),
                KeyCode::Esc => Some(KeyAction::SlashDismiss),
                _ => None,
            };
            if let Some(action) = slash_action {
                self.apply_action_with_layout(action, layout);
                return action;
            }
        }

        // Tab in Insert mode: slash tab-completion when draft starts with '/'.
        if self.message_edit.is_none()
            && self.input_mode == InputMode::Insert
            && !self.command_palette.open
            && key.code == KeyCode::Tab
            && key.modifiers.is_empty()
        {
            let slash = self.draft.text.trim_start();
            if slash.starts_with('/') && !slash.contains('\n') {
                self.slash_selected = Some(0);
                self.slash_dismissed = false;
                return KeyAction::Noop;
            }
        }

        // In Edit mode (message edit), dispatch to edit handler.
        if self.message_edit.is_some() {
            let action = dispatch_edit_key(key);
            self.apply_action_with_layout(action, layout);
            return action;
        }

        // Form inputs (character create/edit, etc.)
        if matches!(
            self.screen,
            ScreenState::CharacterCreate | ScreenState::CharacterEdit | ScreenState::Settings
        ) {
            let action = dispatch_form_key(key);
            self.apply_action_with_layout(action, layout);
            return action;
        }

        // Menu navigation (main menu)
        if self.menu.items.is_empty() {
            let action = dispatch_key(self.input_mode, key);
            if action != KeyAction::Noop {
                self.apply_action_with_layout(action, layout);
            }
            return action;
        }

        // Draft input mode
        if self.input_mode == InputMode::Insert {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('o') => {
                        let leave_action = KeyAction::LeaveInputMode;
                        self.apply_action_with_layout(leave_action, layout);
                        return leave_action;
                    }
                    KeyCode::Char('q') => {
                        let cancel_action = KeyAction::Cancel;
                        self.apply_action_with_layout(cancel_action, layout);
                        return cancel_action;
                    }
                    KeyCode::Char('c') => {
                        let command_palette_action = KeyAction::CommandPaletteOpen;
                        self.command_palette
                            .open(self.textarea.clone());
                        self.slash_selected = None;
                        return command_palette_action;
                    }
                    _ => {}
                }
            }

            // Slash-popup keybindings
            if self.slash_selected.is_some() {
                match key.code {
                    KeyCode::Up => {
                        let slash_up = KeyAction::SlashUp;
                        self.apply_action_with_layout(slash_up, layout);
                        return slash_up;
                    }
                    KeyCode::Down => {
                        let slash_down = KeyAction::SlashDown;
                        self.apply_action_with_layout(slash_down, layout);
                        return slash_down;
                    }
                    KeyCode::Enter => {
                        let slash_accept = KeyAction::SlashAccept;
                        self.apply_action_with_layout(slash_accept, layout);
                        return slash_accept;
                    }
                    KeyCode::Esc => {
                        let slash_dismiss = KeyAction::SlashDismiss;
                        self.apply_action_with_layout(slash_dismiss, layout);
                        return slash_dismiss;
                    }
                    KeyCode::Tab => {
                        // Tab accepts the currently highlighted slash suggestion.
                        let _ = self.slash_accept();
                        return KeyAction::Noop;
                    }
                    _ => {}
                }
            }

            // Regular draft input
            match key.code {
                KeyCode::Esc if key.modifiers.is_empty() => {
                    let leave_action = KeyAction::LeaveInputMode;
                    self.apply_action_with_layout(leave_action, layout);
                    return leave_action;
                }
                KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let leave_action = KeyAction::LeaveInputMode;
                    self.apply_action_with_layout(leave_action, layout);
                    return leave_action;
                }
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let cancel_action = KeyAction::Cancel;
                    self.apply_action_with_layout(cancel_action, layout);
                    return cancel_action;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.command_palette.open(self.textarea.clone());
                    self.slash_selected = None;
                    return KeyAction::CommandPaletteOpen;
                }
                _ => {}
            }

            match key.code {
                KeyCode::Enter if !key.modifiers.is_empty() => {
                    // Ctrl+Enter to submit regardless of slash popup
                    self.submit_draft();
                    return KeyAction::Noop;
                }
                _ => {}
            }
        }

        // Normal mode: context-sensitive dispatch
        match self.screen {
            ScreenState::Conversation | ScreenState::Help | ScreenState::Quit => {
                let action = dispatch_key(self.input_mode, key);
                if action != KeyAction::Noop {
                    self.apply_action_with_layout(action, layout);
                }
                action
            }
            ScreenState::SessionList => {
                let action = dispatch_key(self.input_mode, key);
                if action != KeyAction::Noop {
                    self.apply_action_with_layout(action, layout);
                }
                action
            }
            ScreenState::MainMenu => {
                let action = dispatch_menu_key(key, true);
                self.apply_action_with_layout(action, layout);
                action
            }
            ScreenState::CharacterManager
            | ScreenState::CharacterCreate
            | ScreenState::CharacterEdit
            | ScreenState::CharacterImport => {
                let action = dispatch_form_key(key);
                self.apply_action_with_layout(action, layout);
                action
            }
            ScreenState::Settings => {
                let action = dispatch_key(self.input_mode, key);
                self.apply_action_with_layout(action, layout);
                action
            }
            ScreenState::ModelIntelligence => dispatch_menu_key(key, false),
            ScreenState::MemoriesOverlay | ScreenState::CharacterOverlay(_) => KeyAction::Noop,
        }
    }

    /// Apply action with a pre-built layout (no rebuild needed).
    pub(crate) fn apply_action_with_layout(&mut self, action: KeyAction, layout: &crate::layout::LayoutModel) {
        match action {
            KeyAction::EnterConversation => {
                self.screen = ScreenState::Conversation;
                self.focus = FocusTarget::Draft;
                self.input_mode = InputMode::Insert;
            }
            KeyAction::LeaveInputMode => {
                self.input_mode = InputMode::Normal;
                self.textarea = crate::app::textareas::new_themed_textarea();
            }
            KeyAction::OpenMenu => {
                self.screen = ScreenState::MainMenu;
                self.menu = MenuState::new();
                self.focus = FocusTarget::Transcript;
            }
            KeyAction::OpenHelp => {
                self.screen = ScreenState::Help;
            }
            KeyAction::ToggleSettings => {
                if self.screen == ScreenState::Settings {
                    self.screen = ScreenState::Conversation;
                } else {
                    self.screen = ScreenState::Settings;
                }
            }
            KeyAction::OpenSessionList => {
                self.screen = ScreenState::SessionList;
                self.session_list = SessionListState::new();
            }
            KeyAction::OpenCharacterManager => {
                self.screen = ScreenState::CharacterManager;
            }
            KeyAction::SaveSession => {
                self.runtime_commands.push(RuntimeCommand::SaveSession);
            }
            KeyAction::DeleteSession => {
                self.runtime_commands.push(RuntimeCommand::DeleteSession);
            }
            KeyAction::Confirm => match self.screen {
                ScreenState::Conversation => self.submit_draft(),
                ScreenState::SessionList => {
                    if let Some(entry) = self.session_list.selected_entry() {
                        self.runtime_commands.push(RuntimeCommand::OpenSession {
                            session_id: entry.session_id.clone(),
                            session_name: entry.name.clone(),
                        });
                    }
                }
                _ => {}
            },
            KeyAction::Cancel => match self.screen {
                ScreenState::MainMenu => {
                    self.screen = ScreenState::SessionList;
                    self.menu.close();
                }
                ScreenState::Help => {
                    self.screen = ScreenState::Conversation;
                }
                ScreenState::SessionList => {
                    self.screen = ScreenState::Conversation;
                }
                _ => {}
            },
            KeyAction::ScrollUp => {
                self.scroll_conversation(layout, 1);
            }
            KeyAction::ScrollDown => {
                self.scroll_conversation(layout, -1);
            }
            KeyAction::SelectUp => {
                if self.menu.selected > 0 {
                    self.menu.selected -= 1;
                }
            }
            KeyAction::SelectDown => {
                if self.menu.selected < self.menu.items.len().saturating_sub(1) {
                    self.menu.selected += 1;
                }
            }
            KeyAction::SlashUp => {
                self.slash_move_up();
            }
            KeyAction::SlashDown => {
                self.slash_move_down();
            }
            KeyAction::SlashAccept => {
                let _ = self.slash_accept();
            }
            KeyAction::SlashDismiss => {
                self.slash_selected = None;
                self.slash_dismissed = true;
            }
            KeyAction::CommandPaletteTextAreaInput(key) => {
                self.command_palette.handle_textarea_input(key);
            }
            KeyAction::CommandPaletteUp => {
                if self.command_palette.selected > 0 {
                    self.command_palette.selected -= 1;
                }
            }
            KeyAction::CommandPaletteDown => {
                let max_index = self
                    .command_palette
                    .filtered_commands()
                    .len()
                    .saturating_sub(1);
                if self.command_palette.selected < max_index {
                    self.command_palette.selected += 1;
                }
            }
            KeyAction::CommandPaletteClose => {
                self.command_palette.close();
            }
            KeyAction::CommandPaletteSelect => {
                if let Some(command) = self.command_palette.selected_command() {
                    self.command_palette.close();
                    self.execute_command(&command.name);
                }
            }
            KeyAction::Noop
            | KeyAction::TextAreaInput(_)
            | KeyAction::CommandPaletteKey(_)
            | KeyAction::MenuKey(_)
            | KeyAction::FormKey(_)
            | KeyAction::EditKey(_)
            | KeyAction::DraftKey(_)
            | KeyAction::SettingsKey(_)
            | KeyAction::InspectorKey(_) => {
                // No action needed at this level - handled elsewhere
            }
            KeyAction::Quit => {
                self.should_quit = true;
            }
            KeyAction::ToggleHelp => {
                if self.screen == ScreenState::Help {
                    self.screen = ScreenState::Conversation;
                } else {
                    self.screen = ScreenState::Help;
                }
            }
            KeyAction::Clear => {
                self.draft.delete();
            }
            KeyAction::CommandPaletteOpen => {
                self.command_palette.open(self.textarea.clone());
            }
            KeyAction::DraftInsertChar(ch) => {
                self.draft.insert_char(ch);
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::DraftBackspace => {
                self.draft.backspace();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::DraftDelete => {
                self.draft.delete();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::MoveCursorLeft => {
                self.draft.move_cursor_left();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::MoveCursorRight => {
                self.draft.move_cursor_right();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::MoveCursorHome => {
                self.draft.move_cursor_home();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::MoveCursorEnd => {
                self.draft.move_cursor_end();
                self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
            }
            KeyAction::EnterInsert => {
                self.input_mode = InputMode::Insert;
                self.focus = FocusTarget::Draft;
            }
            KeyAction::HistoryPrevious => {
                if let Some(prev) = self.history.previous(&self.draft) {
                    self.draft = prev;
                    self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
                }
            }
            KeyAction::HistoryNext => {
                if let Some(next) = self.history.next_entry() {
                    self.draft = next;
                    self.sync_textarea_from_draft(&self.draft.text.clone(), self.draft.cursor);
                }
            }
            KeyAction::SubmitDraft => {
                self.submit_draft();
            }
            KeyAction::CancelGeneration => {
                self.cancel_generation();
            }
            KeyAction::ToggleInspector => {
                self.inspector.visible = !self.inspector.visible;
            }
            KeyAction::TriggerContextDryRun => {
                self.trigger_context_dry_run();
            }
            KeyAction::ToggleBookmark => {
                self.trigger_bookmark_toggle();
            }
            KeyAction::TogglePinnedMemory => {
                self.trigger_pinned_memory_toggle();
            }
            KeyAction::EditSelectedMessage => {
                self.begin_selected_message_edit();
            }
            KeyAction::RerollSelectedMessage => {
                self.trigger_reroll_selected_message();
            }
            KeyAction::ScrollConversationUp => {
                self.scroll_conversation(layout, 1);
            }
            KeyAction::ScrollConversationDown => {
                self.scroll_conversation(layout, -1);
            }
            KeyAction::ConfirmQuit => {
                self.should_quit = true;
            }
            KeyAction::OpenCommandPalette => {
                self.command_palette.open(self.textarea.clone());
            }
            _ => {}
        }

        self.sync_slash_state();
    }

    /// Execute a shell command by name.
    pub(crate) fn execute_command(&mut self, name: &str) {
        match name {
            "new" => {
                self.runtime_commands
                    .push(RuntimeCommand::CreateSession { character_name: None });
                self.status_line = Some("Creating session…".into());
            }
            "sessions" => {
                self.screen = ScreenState::SessionList;
                self.session_list = SessionListState::new();
            }
            "characters" => {
                self.screen = ScreenState::CharacterManager;
            }
            "settings" => {
                self.screen = ScreenState::Settings;
            }
            "help" => {
                self.screen = ScreenState::Help;
            }
            "quit" => {
                self.should_quit = true;
            }
            "menu" => {
                self.screen = ScreenState::MainMenu;
                self.focus = FocusTarget::Transcript;
                self.input_mode = InputMode::Normal;
            }
            "session show" | "session retitle" | "session reroll" | "memories"
            | "memory list" => {
                self.enqueue_shell_command(format!("/{name}"));
            }
            "session rename" | "session character" | "memory note" | "search session"
            | "search global" | "attach" => {
                self.prefill_shell_command(format!("/{name} "));
            }
            _ => {
                self.status_line = Some(format!("Unknown command: {name}"));
            }
        }
    }
}
