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
    ToggleHelp,
    ConfirmQuit,
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
            KeyCode::Esc | KeyCode::Char('q') => KeyAction::ConfirmQuit,
            _ => KeyAction::Noop,
        },
        InputMode::Insert => match key.code {
            KeyCode::Esc => KeyAction::LeaveInputMode,
            KeyCode::Enter => KeyAction::SubmitDraft,
            KeyCode::Tab => KeyAction::FocusTranscript,
            KeyCode::Up => KeyAction::HistoryPrevious,
            KeyCode::Down => KeyAction::HistoryNext,
            KeyCode::Backspace => KeyAction::DraftBackspace,
            KeyCode::Delete => KeyAction::DraftDelete,
            KeyCode::Left => KeyAction::MoveCursorLeft,
            KeyCode::Right => KeyAction::MoveCursorRight,
            KeyCode::Home => KeyAction::MoveCursorHome,
            KeyCode::End => KeyAction::MoveCursorEnd,
            KeyCode::Char('?') => KeyAction::ToggleHelp,
            KeyCode::Char(ch) if allows_text_insertion(key.modifiers) => {
                KeyAction::DraftInsertChar(ch)
            }
            _ => KeyAction::Noop,
        },
        InputMode::Command => match key.code {
            KeyCode::Esc => KeyAction::LeaveInputMode,
            KeyCode::Enter => KeyAction::SubmitDraft,
            KeyCode::Char('?') => KeyAction::ToggleHelp,
            _ => KeyAction::Noop,
        },
    }
}

fn allows_text_insertion(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
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

    use super::{dispatch_key, InputMode, KeyAction};

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
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)
            ),
            KeyAction::DraftInsertChar('x')
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
            ),
            KeyAction::DraftBackspace
        );
        assert_eq!(
            dispatch_key(
                InputMode::Insert,
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
            ),
            KeyAction::HistoryPrevious
        );
    }
}
