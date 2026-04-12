mod error;
mod migration;
mod repository;
mod schema;

pub use error::{PersistError, Result};
pub use ozone_core::engine::{
    ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
    ConversationMessage, CreateBranchCommand, MessageId, RecordSwipeCandidateCommand,
    SwipeCandidate, SwipeCandidateState, SwipeGroup, SwipeGroupId,
};
pub use ozone_core::session::{
    CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UnixTimestamp,
};
pub use repository::{
    BranchRecord, CreateMessageRequest, EditMessageRequest, MessageEditRecord, MessageRecord,
    MessageSearchHit, PersistencePaths, SessionLock, SqliteRepository, STALE_LOCK_TIMEOUT_MS,
};
