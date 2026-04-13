mod error;
mod import_export;
mod migration;
mod repository;
mod schema;

pub use error::{PersistError, Result};
pub use import_export::{
    CharacterCard, ImportCharacterCardRequest, ImportedCharacterCard, SessionExport,
    SessionExportBookmark, SessionExportBranch, SessionExportMessage, SessionExportSummary,
    SessionExportSwipeCandidate, SessionExportSwipeGroup, StoredCharacterCard, TranscriptExport,
    TranscriptExportBranch, TranscriptExportSession, CHARACTER_CARD_FORMAT, SESSION_EXPORT_FORMAT,
    TRANSCRIPT_EXPORT_FORMAT,
};
pub use ozone_core::engine::{
    ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
    ConversationMessage, CreateBranchCommand, MessageId, RecordSwipeCandidateCommand,
    SwipeCandidate, SwipeCandidateState, SwipeGroup, SwipeGroupId,
};
pub use ozone_core::session::{
    CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UnixTimestamp,
    UpdateSessionRequest,
};
pub use ozone_memory::{
    AuthorId, CreateNoteMemoryRequest, CrossSessionSearchHit, EmbeddingContent, EmbeddingRecord,
    EmbeddingRecordMetadata, MemoryArtifactId, MemoryContent, PinMessageMemoryRequest,
    PinnedMemoryContent, PinnedMemoryRecord, PinnedMemoryView, Provenance, RecallHit,
    SearchSessionMetadata,
};
pub use repository::{
    BookmarkRecord, BranchRecord, CreateMessageRequest, EditMessageRequest, MessageEditRecord,
    MessageRecord, MessageSearchHit, PersistencePaths, SessionLock, SqliteRepository,
    SummaryArtifactRecord, STALE_LOCK_TIMEOUT_MS,
};
