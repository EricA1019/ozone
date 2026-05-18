#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAreaSurface {
    Composer,
    MessageEdit,
    CommandPalette,
}

// ShellState is defined in enums_runtime.rs
pub use crate::state::enums_runtime::ShellState;

// Re-export types needed by test modules
pub use crate::state::{
    AppBootstrap, BranchItem, CharacterCreateState, CharacterEntry, CharacterImportState,
    CharacterListState, DraftCheckpoint, DraftState, EntryKind, FolderPickerState,
    RecallBrowser, RuntimeCancellation, RuntimeFailure, RuntimePhase, RuntimeProgress,
    RuntimeSendReceipt, ScreenState, SessionContext, SessionMetadata, SessionStats,
    SettingsCategory, SettingsEntry, SettingsState, TranscriptItem, VisibleSessionItem,
};

pub mod textareas;
pub mod shell_state;
