use thiserror::Error;

pub type Result<T> = std::result::Result<T, PersistError>;

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("failed to resolve ozone data directory")]
    DataDirUnavailable,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration failed: {version} -> {target}: {reason}")]
    MigrationFailed {
        version: u32,
        target: u32,
        reason: String,
    },
    #[error("session locked by instance {instance_id} (since {acquired_at})")]
    SessionLocked {
        instance_id: String,
        acquired_at: i64,
    },
    #[error("session {0} was not found")]
    SessionNotFound(String),
    #[error("message {0} was not found")]
    MessageNotFound(String),
    #[error("branch {0} was not found")]
    BranchNotFound(String),
    #[error("swipe group {0} was not found")]
    SwipeGroupNotFound(String),
    #[error("swipe candidate {ordinal} was not found in group {swipe_group_id}")]
    SwipeCandidateNotFound {
        swipe_group_id: String,
        ordinal: u16,
    },
    #[error("unsupported schema version {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("invalid persisted data: {0}")]
    InvalidData(String),
    #[error("persistence consistency error: {0}")]
    ConsistencyError(String),
}

impl From<ozone_core::session::SessionIdError> for PersistError {
    fn from(_: ozone_core::session::SessionIdError) -> Self {
        Self::InvalidData("invalid session identifier".to_owned())
    }
}

impl From<ozone_core::engine::EngineIdError> for PersistError {
    fn from(error: ozone_core::engine::EngineIdError) -> Self {
        Self::InvalidData(error.to_string())
    }
}

impl From<ozone_core::engine::DomainParseError> for PersistError {
    fn from(error: ozone_core::engine::DomainParseError) -> Self {
        Self::InvalidData(error.to_string())
    }
}

impl From<ozone_memory::MemoryIdError> for PersistError {
    fn from(error: ozone_memory::MemoryIdError) -> Self {
        Self::InvalidData(error.to_string())
    }
}

impl From<ozone_memory::MemoryParseError> for PersistError {
    fn from(error: ozone_memory::MemoryParseError) -> Self {
        Self::InvalidData(error.to_string())
    }
}
