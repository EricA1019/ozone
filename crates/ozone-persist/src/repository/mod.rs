use std::{collections::BTreeMap, fs, sync::Arc};

use ozone_core::{
    engine::{BranchId, BranchState, ConversationBranch, ConversationMessage, MessageId},
    session::{SessionId, UnixTimestamp},
};
use ozone_memory::{
    assess_artifact_staleness, storage_tier_for_age, ArtifactStaleness, MemoryArtifactId,
    PinnedMemoryView, Provenance, SearchSessionMetadata, StorageTier, StorageTierPolicy,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    schema::{ensure_global_schema, SESSION_MIGRATOR},
    PersistError, Result,
};

mod branch_ops;
mod character_ops;
mod export_ops;
mod fs_helpers;
mod generated_values;
mod lifecycle_ops;
mod memory_ops;
mod message_ops;
mod paths;
mod row_parsers;
mod search_helpers;
mod session_ops;
mod swipe_ops;

pub use character_ops::StoredCharacter;
use fs_helpers::{ensure_file_with_contents, open_connection, secure_path};
use generated_values::{current_timestamp_ms, generate_uuid_like};
use row_parsers::{
    parse_i64_as_u16, parse_i64_as_u64, parse_sqlite_text, read_branch_record,
    read_conversation_message, sqlite_text_parse_error,
};
use search_helpers::plain_text_fts_query;

pub use paths::PersistencePaths;
use paths::{DEFAULT_SESSION_CONFIG, DEFAULT_SESSION_DRAFT};

pub const STALE_LOCK_TIMEOUT_MS: UnixTimestamp = 60_000;

type ClockFn = Arc<dyn Fn() -> UnixTimestamp + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLock {
    pub instance_id: String,
    pub acquired_at: UnixTimestamp,
    pub heartbeat_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMessageRequest {
    pub parent_id: Option<String>,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub content: String,
}

impl CreateMessageRequest {
    pub fn new(author_kind: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            parent_id: None,
            author_kind: author_kind.into(),
            author_name: None,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRecord {
    pub message_id: String,
    pub session_id: SessionId,
    pub parent_id: Option<String>,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub content: String,
    pub created_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageSearchHit {
    pub message_id: String,
    pub author_kind: String,
    pub content: String,
    pub created_at: UnixTimestamp,
    pub bm25_score: f32,
}

#[derive(Debug, Clone)]
pub struct PinnedMemorySearchHit {
    pub memory: PinnedMemoryView,
    pub bm25_score: f32,
}

#[derive(Debug, Clone)]
pub struct CrossSessionPinnedMemorySearchHit {
    pub session: SearchSessionMetadata,
    pub memory: PinnedMemoryView,
    pub bm25_score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditMessageRequest {
    pub content: String,
    pub edited_at: Option<UnixTimestamp>,
}

impl EditMessageRequest {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            edited_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEditRecord {
    pub revision_id: i64,
    pub message_id: MessageId,
    pub previous_content: String,
    pub edited_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkRecord {
    pub bookmark_id: String,
    pub message_id: MessageId,
    pub note: Option<String>,
    pub created_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRecord {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
}

/// A persisted summary artifact (chunk summary or session synopsis).
#[derive(Debug, Clone)]
pub struct SummaryArtifactRecord {
    pub artifact_id: MemoryArtifactId,
    pub session_id: SessionId,
    pub kind: String,
    pub text: String,
    pub source_count: Option<usize>,
    pub message_count: Option<usize>,
    pub start_message_id: Option<String>,
    pub end_message_id: Option<String>,
    pub created_at: UnixTimestamp,
    pub snapshot_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivedArtifactKind {
    Embedding,
    ChunkSummary,
    SessionSynopsis,
}

impl DerivedArtifactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::ChunkSummary => "chunk_summary",
            Self::SessionSynopsis => "session_synopsis",
        }
    }

    fn from_storage_kind(value: &str) -> Result<Self> {
        match value {
            "embedding" => Ok(Self::Embedding),
            "chunk_summary" => Ok(Self::ChunkSummary),
            "session_synopsis" => Ok(Self::SessionSynopsis),
            other => Err(PersistError::InvalidData(format!(
                "unexpected derived artifact kind: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for DerivedArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedArtifactRecord {
    pub artifact_id: MemoryArtifactId,
    pub session_id: SessionId,
    pub kind: DerivedArtifactKind,
    pub provenance: Provenance,
    pub created_at: UnixTimestamp,
    pub snapshot_version: u64,
    pub source_start_message_id: Option<MessageId>,
    pub source_end_message_id: Option<MessageId>,
    pub source_exists: bool,
    pub text_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedArtifactLifecycleRecord {
    pub artifact_id: MemoryArtifactId,
    pub session_id: SessionId,
    pub kind: DerivedArtifactKind,
    pub provenance: Provenance,
    pub created_at: UnixTimestamp,
    pub snapshot_version: u64,
    pub source_start_message_id: Option<MessageId>,
    pub source_end_message_id: Option<MessageId>,
    pub source_exists: bool,
    pub text_preview: Option<String>,
    pub age_messages: u64,
    pub storage_tier: StorageTier,
    pub staleness: ArtifactStaleness,
}

impl DerivedArtifactLifecycleRecord {
    fn from_record(
        record: DerivedArtifactRecord,
        current_message_count: u64,
        storage_policy: &StorageTierPolicy,
        now_ms: UnixTimestamp,
        max_age_messages: usize,
        max_age_hours: u64,
    ) -> Self {
        let staleness = assess_artifact_staleness(
            record.snapshot_version,
            current_message_count,
            record.created_at,
            now_ms,
            max_age_messages,
            max_age_hours,
        );
        let storage_tier = storage_tier_for_age(staleness.age_messages, storage_policy);

        Self {
            artifact_id: record.artifact_id,
            session_id: record.session_id,
            kind: record.kind,
            provenance: record.provenance,
            created_at: record.created_at,
            snapshot_version: record.snapshot_version,
            source_start_message_id: record.source_start_message_id,
            source_end_message_id: record.source_end_message_id,
            source_exists: record.source_exists,
            text_preview: record.text_preview,
            age_messages: staleness.age_messages,
            storage_tier,
            staleness,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GarbageCollectionPolicy {
    pub max_active_embeddings: usize,
    pub purge_unreferenced_backlog: bool,
}

impl GarbageCollectionPolicy {
    pub const fn new(max_active_embeddings: usize, purge_unreferenced_backlog: bool) -> Self {
        Self {
            max_active_embeddings,
            purge_unreferenced_backlog,
        }
    }
}

impl Default for GarbageCollectionPolicy {
    fn default() -> Self {
        Self::new(usize::MAX, false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GarbageCollectionReason {
    OrphanedSource,
    MinimalTier,
    SupersededSynopsis,
    OverEmbeddingLimit,
}

impl GarbageCollectionReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrphanedSource => "orphaned_source",
            Self::MinimalTier => "minimal_tier",
            Self::SupersededSynopsis => "superseded_synopsis",
            Self::OverEmbeddingLimit => "over_embedding_limit",
        }
    }
}

impl std::fmt::Display for GarbageCollectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GarbageCollectionCandidate {
    pub artifact: DerivedArtifactLifecycleRecord,
    pub reasons: Vec<GarbageCollectionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GarbageCollectionPlan {
    pub inspected_count: usize,
    pub candidate_count: usize,
    pub reason_counts: BTreeMap<GarbageCollectionReason, usize>,
    pub candidates: Vec<GarbageCollectionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GarbageCollectionOutcome {
    pub deleted_count: usize,
    pub deleted_artifact_ids: BTreeMap<SessionId, Vec<MemoryArtifactId>>,
}

#[derive(Clone)]
pub struct SqliteRepository {
    paths: PersistencePaths,
    now_utc_ms: ClockFn,
}

impl std::fmt::Debug for SqliteRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteRepository")
            .field("paths", &self.paths)
            .finish()
    }
}

impl SqliteRepository {
    pub fn new(paths: PersistencePaths) -> Self {
        Self::with_clock(paths, Arc::new(current_timestamp_ms))
    }

    pub fn from_xdg() -> Result<Self> {
        Ok(Self::new(PersistencePaths::from_xdg()?))
    }

    pub fn with_clock(paths: PersistencePaths, now_utc_ms: ClockFn) -> Self {
        Self { paths, now_utc_ms }
    }

    pub fn paths(&self) -> &PersistencePaths {
        &self.paths
    }

    fn touch_session_summary(
        &self,
        session_id: &SessionId,
        touched_at: UnixTimestamp,
        message_delta: i64,
    ) -> Result<()> {
        let global_conn = self.ensure_global_connection()?;
        let rows = global_conn.execute(
            "UPDATE sessions
             SET message_count = message_count + ?3,
                 last_opened_at = MAX(last_opened_at, ?2),
                 db_size_bytes = ?4
             WHERE session_id = ?1",
            params![
                session_id.as_str(),
                touched_at,
                message_delta,
                self.session_db_size_i64(session_id),
            ],
        )?;

        if rows == 0 {
            return Err(PersistError::SessionNotFound(session_id.to_string()));
        }

        Ok(())
    }

    fn refresh_session_size(&self, session_id: &SessionId) -> Result<()> {
        let global_conn = self.ensure_global_connection()?;
        let rows = global_conn.execute(
            "UPDATE sessions
             SET db_size_bytes = ?2
             WHERE session_id = ?1",
            params![session_id.as_str(), self.session_db_size_i64(session_id)],
        )?;

        if rows == 0 {
            return Err(PersistError::SessionNotFound(session_id.to_string()));
        }

        Ok(())
    }

    fn ensure_global_connection(&self) -> Result<Connection> {
        self.ensure_root_directories()?;

        let global_db_path = self.paths.global_db_path();
        let (conn, _) = open_connection(&global_db_path)?;
        ensure_global_schema(&conn)?;
        secure_path(&global_db_path, 0o600)?;
        Ok(conn)
    }

    fn ensure_session_database(&self, session_id: &SessionId) -> Result<()> {
        self.ensure_root_directories()?;

        let session_dir = self.paths.session_dir(session_id);
        fs::create_dir_all(&session_dir)?;
        secure_path(&session_dir, 0o700)?;
        ensure_file_with_contents(
            &self.paths.session_config_path(session_id),
            DEFAULT_SESSION_CONFIG,
        )?;
        ensure_file_with_contents(
            &self.paths.session_draft_path(session_id),
            DEFAULT_SESSION_DRAFT,
        )?;

        let session_db_path = self.paths.session_db_path(session_id);
        let (mut conn, existed_before_open) = open_connection(&session_db_path)?;
        SESSION_MIGRATOR.migrate(&mut conn, &session_db_path, existed_before_open, self.now())?;
        secure_path(&session_db_path, 0o600)?;

        Ok(())
    }

    fn open_session_connection(&self, session_id: &SessionId) -> Result<Connection> {
        let session_db_path = self.paths.session_db_path(session_id);

        if !session_db_path.exists() {
            return Err(PersistError::SessionNotFound(session_id.to_string()));
        }

        let (mut conn, existed_before_open) = open_connection(&session_db_path)?;
        SESSION_MIGRATOR.migrate(&mut conn, &session_db_path, existed_before_open, self.now())?;
        secure_path(&session_db_path, 0o600)?;
        Ok(conn)
    }

    fn ensure_root_directories(&self) -> Result<()> {
        fs::create_dir_all(self.paths.data_dir())?;
        secure_path(self.paths.data_dir(), 0o700)?;

        let sessions_dir = self.paths.sessions_dir();
        fs::create_dir_all(&sessions_dir)?;
        secure_path(&sessions_dir, 0o700)?;
        Ok(())
    }

    fn generate_session_id(&self) -> Result<SessionId> {
        for _ in 0..8 {
            let session_id = SessionId::parse(generate_uuid_like())?;

            if !self.paths.session_dir(&session_id).exists() {
                return Ok(session_id);
            }
        }

        Err(PersistError::InvalidData(
            "failed to generate a unique session ID".to_owned(),
        ))
    }

    fn now(&self) -> UnixTimestamp {
        (self.now_utc_ms)()
    }

    fn current_message_count(&self, session_id: &SessionId) -> Result<u64> {
        self.get_session(session_id)?
            .map(|session| session.message_count)
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))
    }

    fn session_db_size(&self, session_id: &SessionId) -> Option<u64> {
        fs::metadata(self.paths.session_db_path(session_id))
            .ok()
            .map(|metadata| metadata.len())
    }

    fn session_db_size_i64(&self, session_id: &SessionId) -> Option<i64> {
        self.session_db_size(session_id)
            .and_then(|size| i64::try_from(size).ok())
    }
}

fn parse_tags_json(tags_json: Option<String>) -> Result<Vec<String>> {
    match tags_json {
        Some(tags_json) => serde_json::from_str(&tags_json).map_err(|error| {
            PersistError::InvalidData(format!("invalid session tags JSON: {error}"))
        }),
        None => Ok(Vec::new()),
    }
}

fn insert_conversation_message_in_tx(
    tx: &Transaction<'_>,
    message: &ConversationMessage,
) -> Result<()> {
    if let Some(parent_id) = &message.parent_id {
        ensure_message_exists_in_tx(tx, parent_id, &message.session_id)?;
        ensure_ancestry_in_tx(tx, parent_id, parent_id)?;
    }

    tx.execute(
        "INSERT INTO messages (
            message_id, session_id, parent_id, author_kind, author_name, content, created_at, edited_at, is_hidden
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            message.message_id.as_str(),
            message.session_id.as_str(),
            message.parent_id.as_ref().map(MessageId::as_str),
            message.author_kind.as_str(),
            message.author_name.as_deref(),
            message.content.as_str(),
            message.created_at,
            message.edited_at,
            if message.is_hidden { 1_i64 } else { 0_i64 },
        ],
    )?;
    tx.execute(
        "INSERT INTO message_ancestry (ancestor_id, descendant_id, depth) VALUES (?1, ?1, 0)",
        [message.message_id.as_str()],
    )?;

    if let Some(parent_id) = &message.parent_id {
        let inserted = tx.execute(
            "INSERT INTO message_ancestry (ancestor_id, descendant_id, depth)
             SELECT ancestor_id, ?1, depth + 1
             FROM message_ancestry
             WHERE descendant_id = ?2",
            params![message.message_id.as_str(), parent_id.as_str()],
        )?;

        if inserted == 0 {
            return Err(PersistError::ConsistencyError(format!(
                "message {} is missing closure rows",
                parent_id
            )));
        }
    }

    Ok(())
}

fn get_message_in_tx(
    tx: &Transaction<'_>,
    message_id: &MessageId,
) -> Result<Option<ConversationMessage>> {
    tx.query_row(
        "SELECT message_id, session_id, parent_id, author_kind, author_name, content, created_at, edited_at, is_hidden
         FROM messages
         WHERE message_id = ?1",
        [message_id.as_str()],
        read_conversation_message,
    )
    .optional()
    .map_err(PersistError::from)
}

fn get_message_in_conn(
    conn: &Connection,
    message_id: &MessageId,
) -> Result<Option<ConversationMessage>> {
    conn.query_row(
        "SELECT message_id, session_id, parent_id, author_kind, author_name, content, created_at, edited_at, is_hidden
         FROM messages
         WHERE message_id = ?1",
        [message_id.as_str()],
        read_conversation_message,
    )
    .optional()
    .map_err(PersistError::from)
}

fn ensure_message_exists_in_tx(
    tx: &Transaction<'_>,
    message_id: &MessageId,
    session_id: &SessionId,
) -> Result<ConversationMessage> {
    let message = get_message_in_tx(tx, message_id)?
        .ok_or_else(|| PersistError::MessageNotFound(message_id.to_string()))?;

    if message.session_id != *session_id {
        return Err(PersistError::ConsistencyError(format!(
            "message {} belongs to session {}, not {}",
            message_id, message.session_id, session_id
        )));
    }

    Ok(message)
}

fn ensure_message_exists_in_conn(
    conn: &Connection,
    message_id: &MessageId,
    session_id: &SessionId,
) -> Result<ConversationMessage> {
    let message = get_message_in_conn(conn, message_id)?
        .ok_or_else(|| PersistError::MessageNotFound(message_id.to_string()))?;

    if message.session_id != *session_id {
        return Err(PersistError::ConsistencyError(format!(
            "message {} belongs to session {}, not {}",
            message_id, message.session_id, session_id
        )));
    }

    Ok(message)
}

fn ensure_ancestry_in_tx(
    tx: &Transaction<'_>,
    ancestor_id: &MessageId,
    descendant_id: &MessageId,
) -> Result<()> {
    let exists = tx.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM message_ancestry
            WHERE ancestor_id = ?1 AND descendant_id = ?2
        )",
        params![ancestor_id.as_str(), descendant_id.as_str()],
        |row| row.get::<_, i64>(0),
    )? != 0;

    if !exists {
        return Err(PersistError::ConsistencyError(format!(
            "message {} is not an ancestor of {}",
            ancestor_id, descendant_id
        )));
    }

    Ok(())
}

fn get_branch_record_in_tx(
    tx: &Transaction<'_>,
    branch_id: &BranchId,
) -> Result<Option<BranchRecord>> {
    tx.query_row(
        "SELECT branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
         FROM branches
         WHERE branch_id = ?1",
        [branch_id.as_str()],
        read_branch_record,
    )
    .optional()
    .map_err(PersistError::from)
}

fn activate_branch_in_tx(
    tx: &Transaction<'_>,
    session_id: &SessionId,
    branch_id: &BranchId,
) -> Result<()> {
    let branch = get_branch_record_in_tx(tx, branch_id)?
        .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;

    if branch.branch.session_id != *session_id {
        return Err(PersistError::ConsistencyError(format!(
            "branch {} belongs to session {}, not {}",
            branch_id, branch.branch.session_id, session_id
        )));
    }

    match branch.branch.state {
        BranchState::Archived | BranchState::Deleted => {
            return Err(PersistError::ConsistencyError(format!(
                "branch {} cannot be activated from state {}",
                branch_id, branch.branch.state
            )));
        }
        BranchState::Active | BranchState::Inactive => {}
    }

    tx.execute(
        "UPDATE branches
         SET state = ?2
         WHERE session_id = ?1 AND state = ?3 AND branch_id != ?4",
        params![
            session_id.as_str(),
            BranchState::Inactive.as_str(),
            BranchState::Active.as_str(),
            branch_id.as_str(),
        ],
    )?;
    tx.execute(
        "UPDATE branches SET state = ?2 WHERE branch_id = ?1",
        params![branch_id.as_str(), BranchState::Active.as_str()],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests;
