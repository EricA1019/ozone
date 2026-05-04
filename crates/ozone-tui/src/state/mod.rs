//! State module - re-exports all state types from app module.
//!
//! This module establishes the architectural boundary for state types.
//! The actual type definitions remain in app.rs to preserve existing
//! functionality and avoid dependency issues. Over time, types may be
//! moved here directly as the module structure stabilizes.

pub use crate::app::{
    CharacterCreateState, CharacterDetail, CharacterEntry, CharacterFormField,
    CharacterImportState, CharacterListState, CommandEntry, CommandPaletteState,
    DraftCheckpoint, DraftState, EntryKind, FocusTarget, FolderPickerState,
    InspectorFocus, InspectorState, InputHistoryState, MenuItem, MenuState,
    RuntimePhase, ScreenState, SessionContext, SessionListEntry, SessionListState,
    SessionMetadata, SessionStats, SettingsCategory, SettingsEntry, SettingsState,
    TranscriptItem, VisibleSessionItem,
};
