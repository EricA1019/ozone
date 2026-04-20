use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Noop,
    MoveSelectionUp,
    MoveSelectionDown,
    FocusTranscript,
    FocusDraft,
    EnterInsert,
    LeaveInputMode,
    SubmitDraft,
    CancelGeneration,
    ToggleInspector,
    TriggerContextDryRun,
    ToggleBookmark,
    TogglePinnedMemory,
    HistoryPrevious,
    HistoryNext,
    DraftInsertChar(char),
    DraftBackspace,
    DraftDelete,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorHome,
    MoveCursorEnd,
    /// Forward raw key event to the TextArea widget (Insert mode).
    TextAreaInput(KeyEvent),
    ToggleHelp,
    ConfirmQuit,
    MenuUp,
    MenuDown,
    MenuSelect,
    MenuBack,
    MenuShortcut(char),
    OpenCommandPalette,
    CommandPaletteInput(char),
    CommandPaletteBackspace,
    CommandPaletteSelect,
    CommandPaletteUp,
    CommandPaletteDown,
    CommandPaletteClose,
    // Character form actions
    CharacterCreate,
    CharacterImportPrompt,
    FormInsertChar(char),
    FormBackspace,
    FormMoveCursorLeft,
    FormMoveCursorRight,
    FormToggleField,
    FormSubmit,
    FormCancel,
    // Slash-popup navigation
    SlashUp,
    SlashDown,
    SlashAccept,
    SlashDismiss,
    SlashTabComplete,
    // Page navigation
    PageUp,
    PageDown,
}

pub fn dispatch_key(input_mode: InputMode, key: KeyEvent) -> KeyAction {
    if is_ctrl_c(key) {
        return KeyAction::CancelGeneration;
    }

    if is_ctrl_i(key) {
        return KeyAction::ToggleInspector;
    }

    if is_ctrl_d(key) {
        return KeyAction::TriggerContextDryRun;
    }

    if is_ctrl_k(key) {
        return KeyAction::TogglePinnedMemory;
    }

    match input_mode {
        InputMode::Normal => match key.code {
            KeyCode::Up | KeyCode::Char('k') => KeyAction::MoveSelectionUp,
            KeyCode::Down | KeyCode::Char('j') => KeyAction::MoveSelectionDown,
            KeyCode::Char('i') => KeyAction::EnterInsert,
            KeyCode::Tab => KeyAction::FocusDraft,
            KeyCode::Char('t') => KeyAction::FocusTranscript,
            KeyCode::Char('b') => KeyAction::ToggleBookmark,
            KeyCode::Char('?') => KeyAction::ToggleHelp,
            KeyCode::Char('/') | KeyCode::Char(':') => KeyAction::OpenCommandPalette,
            KeyCode::Esc | KeyCode::Char('q') => KeyAction::ConfirmQuit,
            _ => KeyAction::Noop,
        },
        InputMode::Insert => match key.code {
            KeyCode::Esc => KeyAction::LeaveInputMode,
            KeyCode::Enter => KeyAction::SubmitDraft,
            KeyCode::Tab => KeyAction::FocusTranscript,
            KeyCode::Up => KeyAction::HistoryPrevious,
            KeyCode::Down => KeyAction::HistoryNext,
            _ => KeyAction::TextAreaInput(key),
        },
        InputMode::Command => match key.code {
            KeyCode::Esc => KeyAction::LeaveInputMode,
            KeyCode::Enter => KeyAction::SubmitDraft,
            KeyCode::Char(ch) if allows_text_insertion(key.modifiers) => {
                KeyAction::DraftInsertChar(ch)
            }
            _ => KeyAction::Noop,
        },
    }
}

/// Dispatch keys when the TUI is on a menu screen (MainMenu, SessionList, etc.).
/// `is_root_menu` should be true only for the top-level MainMenu; on sub-screens
/// `q` navigates back instead of quitting the application.
pub fn dispatch_menu_key(key: KeyEvent, is_root_menu: bool) -> KeyAction {
    if is_ctrl_c(key) {
        return KeyAction::ConfirmQuit;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => KeyAction::MenuUp,
        KeyCode::Down | KeyCode::Char('j') => KeyAction::MenuDown,
        KeyCode::PageUp => KeyAction::PageUp,
        KeyCode::PageDown => KeyAction::PageDown,
        KeyCode::Enter => KeyAction::MenuSelect,
        KeyCode::Esc | KeyCode::Backspace => KeyAction::MenuBack,
        KeyCode::Char('q') if is_root_menu => KeyAction::ConfirmQuit,
        KeyCode::Char('q') => KeyAction::MenuBack,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char('/') | KeyCode::Char(':') => KeyAction::OpenCommandPalette,
        KeyCode::Char(ch) if ch.is_ascii_digit() => KeyAction::MenuShortcut(ch),
        _ => KeyAction::Noop,
    }
}

/// Dispatch keys when the command palette overlay is open.
pub fn dispatch_command_palette_key(key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => Some(KeyAction::CommandPaletteClose),
        KeyCode::Enter => Some(KeyAction::CommandPaletteSelect),
        KeyCode::Backspace => Some(KeyAction::CommandPaletteBackspace),
        KeyCode::Up => Some(KeyAction::CommandPaletteUp),
        KeyCode::Down => Some(KeyAction::CommandPaletteDown),
        KeyCode::Char(c) => Some(KeyAction::CommandPaletteInput(c)),
        _ => None,
    }
}

fn allows_text_insertion(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
}

/// Dispatch keys for character create/import form screens.
pub fn dispatch_form_key(key: KeyEvent) -> KeyAction {
    if is_ctrl_c(key) {
        return KeyAction::FormCancel;
    }

    match key.code {
        KeyCode::Esc => KeyAction::FormCancel,
        KeyCode::Enter => KeyAction::FormSubmit,
        KeyCode::Tab | KeyCode::BackTab => KeyAction::FormToggleField,
        KeyCode::Backspace => KeyAction::FormBackspace,
        KeyCode::Left => KeyAction::FormMoveCursorLeft,
        KeyCode::Right => KeyAction::FormMoveCursorRight,
        KeyCode::Char(ch) if allows_text_insertion(key.modifiers) => KeyAction::FormInsertChar(ch),
        _ => KeyAction::Noop,
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_ctrl_i(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            key.code,
            KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Tab
        )
}

fn is_ctrl_d(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_ctrl_k(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('k') | KeyCode::Char('K'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{dispatch_key, dispatch_menu_key, InputMode, KeyAction};

    #[test]
    fn normal_mode_maps_navigation_and_insert_keys() {
        assert_eq!(
            dispatch_key(
                InputMode::Normal,
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)
            ),
            KeyAction::EnterInsert
        );
        assert_eq!(
            dispatch_key(
                InputMode::Normal,
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
            ),
            KeyAction::MoveSelectionDown
        );
        assert_eq!(
            dispatch_key(
                InputMode::Normal,
                KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)
            ),
            KeyAction::ToggleBookmark
        );
    }

    #[test]
    fn insert_mode_maps_send_cancel_and_toggle_keys() {
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::SubmitDraft
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            KeyAction::CancelGeneration
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL)
            ),
            KeyAction::ToggleInspector
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)
            ),
            KeyAction::TriggerContextDryRun
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
            ),
            KeyAction::TogglePinnedMemory
        );
    }

    #[test]
    fn insert_mode_maps_editing_and_history_keys() {
        let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(
            dispatch_key(InputMode::Insert, key_x),
            KeyAction::TextAreaInput(key_x)
        );
        let key_bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(
            dispatch_key(InputMode::Insert, key_bs),
            KeyAction::TextAreaInput(key_bs)
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
            ),
            KeyAction::HistoryPrevious
        );
        let key_q = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        assert_eq!(
            dispatch_key(InputMode::Insert, key_q),
            KeyAction::TextAreaInput(key_q)
        );
    }

    #[test]
    fn command_mode_treats_question_mark_as_text() {
        assert_eq!(
            dispatch_key(
                InputMode::Command,
                KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)
            ),
            KeyAction::DraftInsertChar('?')
        );
    }

    #[test]
    fn menu_dispatch_maps_navigation_and_selection() {
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), true),
            KeyAction::MenuUp
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), true),
            KeyAction::MenuDown
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), true),
            KeyAction::MenuSelect
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), true),
            KeyAction::MenuBack
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), true),
            KeyAction::MenuDown
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), true),
            KeyAction::MenuUp
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE), true),
            KeyAction::MenuShortcut('1')
        );
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), true),
            KeyAction::ConfirmQuit
        );
    }

    #[test]
    fn menu_dispatch_q_goes_back_on_sub_screens() {
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), false),
            KeyAction::MenuBack
        );
        // Esc still goes back on sub-screens
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), false),
            KeyAction::MenuBack
        );
        // Navigation unchanged on sub-screens
        assert_eq!(
            dispatch_menu_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), false),
            KeyAction::MenuUp
        );
    }
}
