use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
        ConversationMessage, CreateBranchCommand, MessageId, RecordSwipeCandidateCommand,
        SwipeCandidate, SwipeCandidateState, SwipeGroup, SwipeGroupId,
    },
    paths as core_paths,
    session::{
        CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UnixTimestamp,
        UpdateSessionRequest,
    },
};
use ozone_memory::{
    assess_artifact_staleness, storage_tier_for_age, ArtifactStaleness, CreateNoteMemoryRequest,
    CrossSessionSearchHit, EmbeddingRecord, EmbeddingRecordMetadata, MemoryArtifactId,
    MemoryContent, PinMessageMemoryRequest, PinnedMemoryContent, PinnedMemoryRecord,
    PinnedMemoryView, Provenance, SearchSessionMetadata, StorageTier, StorageTierPolicy,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::{
    import_export::{
        ImportCharacterCardRequest, ImportedCharacterCard, SessionExport, SessionExportBookmark,
        SessionExportBranch, SessionExportMessage, SessionExportSummary,
        SessionExportSwipeCandidate, SessionExportSwipeGroup, StoredCharacterCard,
        TranscriptExport, TranscriptExportBranch, TranscriptExportSession, SESSION_EXPORT_FORMAT,
        TRANSCRIPT_EXPORT_FORMAT,
    },
    schema::{ensure_global_schema, SESSION_MIGRATOR},
    PersistError, Result,
};

pub const STALE_LOCK_TIMEOUT_MS: UnixTimestamp = 60_000;

type ClockFn = Arc<dyn Fn() -> UnixTimestamp + Send + Sync + 'static>;

const DEFAULT_SESSION_CONFIG: &str = "[meta]\nconfig_version = 1\n";
const DEFAULT_SESSION_DRAFT: &str = "";

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistencePaths {
    data_dir: PathBuf,
}

impl PersistencePaths {
    pub fn from_data_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: path.into(),
        }
    }

    pub fn from_xdg() -> Result<Self> {
        let data_dir = core_paths::data_dir().ok_or(PersistError::DataDirUnavailable)?;
        Ok(Self::from_data_dir(data_dir))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn global_db_path(&self) -> PathBuf {
        self.data_dir.join("global.db")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    pub fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        self.sessions_dir().join(session_id.as_str())
    }

    pub fn session_db_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("session.db")
    }

    pub fn session_config_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("config.toml")
    }

    pub fn session_draft_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("draft.txt")
    }
}

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

    pub fn create_session(&self, request: CreateSessionRequest) -> Result<SessionRecord> {
        let session_id = self.generate_session_id()?;
        let created_at = self.now();

        self.ensure_session_database(&session_id)?;

        let summary = SessionSummary {
            session_id: session_id.clone(),
            name: request.name,
            character_name: request.character_name,
            created_at,
            last_opened_at: created_at,
            message_count: 0,
            db_size_bytes: self.session_db_size(&session_id),
            tags: request.tags,
        };

        let global_conn = self.ensure_global_connection()?;
        upsert_session_summary(&global_conn, &summary)?;

        Ok(summary)
    }

    pub fn import_character_card(
        &self,
        request: ImportCharacterCardRequest,
    ) -> Result<ImportedCharacterCard> {
        let session_name = request
            .session_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| request.card.name.clone());
        let mut create_request = CreateSessionRequest::new(session_name);
        create_request.character_name = Some(request.card.name.clone());
        create_request.tags = merge_tags(&request.card.tags, &request.tags);

        let session = self.create_session(create_request)?;
        let imported_at = self.now();
        let stored_card = StoredCharacterCard {
            imported_at,
            provenance: request.provenance,
            card: request.card,
        };
        self.store_character_card(&session.session_id, &stored_card)?;

        let (seeded_branch_id, seeded_message_id) =
            if let Some(greeting) = stored_card.card.greeting.clone() {
                let message = self.insert_message(
                    &session.session_id,
                    CreateMessageRequest {
                        parent_id: None,
                        author_kind: "assistant".to_owned(),
                        author_name: Some(stored_card.card.name.clone()),
                        content: greeting,
                    },
                )?;
                let message_id = MessageId::parse(message.message_id)?;
                let mut branch = ConversationBranch::new(
                    BranchId::parse(generate_uuid_like())?,
                    session.session_id.clone(),
                    "main",
                    message_id.clone(),
                    message.created_at,
                );
                branch.state = BranchState::Active;
                let branch = self.create_branch(CreateBranchCommand {
                    branch,
                    forked_from: message_id.clone(),
                })?;
                (Some(branch.branch.branch_id), Some(message_id))
            } else {
                (None, None)
            };

        let session = self
            .get_session(&session.session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session.session_id.to_string()))?;

        Ok(ImportedCharacterCard {
            session,
            seeded_branch_id,
            seeded_message_id,
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let conn = self.ensure_global_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, name, character_name, created_at, last_opened_at, message_count, db_size_bytes, tags
             FROM sessions
             ORDER BY last_opened_at DESC, created_at DESC, session_id ASC",
        )?;
        let rows = stmt.query_map([], read_stored_session_summary)?;

        rows.map(|row| {
            row.map_err(PersistError::from)
                .and_then(SessionSummary::try_from)
        })
        .collect()
    }

    pub fn get_session(&self, session_id: &SessionId) -> Result<Option<SessionRecord>> {
        let conn = self.ensure_global_connection()?;
        let stored = conn
            .query_row(
                "SELECT session_id, name, character_name, created_at, last_opened_at, message_count, db_size_bytes, tags
                 FROM sessions
                 WHERE session_id = ?1",
                [session_id.as_str()],
                read_stored_session_summary,
            )
            .optional()?;

        stored.map(SessionSummary::try_from).transpose()
    }

    pub fn update_session_metadata(
        &self,
        session_id: &SessionId,
        request: UpdateSessionRequest,
    ) -> Result<SessionRecord> {
        let mut summary = self
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))?;
        let touched_at = self.now();

        if let Some(name) = request.name {
            summary.name = name;
        }
        if let Some(character_name) = request.character_name {
            summary.character_name = character_name;
        }
        if let Some(tags) = request.tags {
            summary.tags = tags;
        }
        summary.last_opened_at = summary.last_opened_at.max(touched_at);
        summary.db_size_bytes = self.session_db_size(session_id);

        let conn = self.ensure_global_connection()?;
        upsert_session_summary(&conn, &summary)?;
        Ok(summary)
    }

    pub fn acquire_session_lock(
        &self,
        session_id: &SessionId,
        instance_id: impl Into<String>,
    ) -> Result<SessionLock> {
        let instance_id = instance_id.into();
        let now = self.now();
        let conn = self.open_session_connection(session_id)?;
        let existing_lock = read_session_lock(&conn)?;

        match existing_lock {
            None => {
                conn.execute(
                    "INSERT INTO session_lock (id, instance_id, acquired_at, heartbeat_at) VALUES (1, ?1, ?2, ?2)",
                    params![instance_id, now],
                )?;

                Ok(SessionLock {
                    instance_id,
                    acquired_at: now,
                    heartbeat_at: now,
                })
            }
            Some(lock) if lock.instance_id == instance_id => {
                conn.execute(
                    "UPDATE session_lock SET heartbeat_at = ?2 WHERE id = 1 AND instance_id = ?1",
                    params![instance_id, now],
                )?;

                Ok(SessionLock {
                    instance_id,
                    acquired_at: lock.acquired_at,
                    heartbeat_at: now,
                })
            }
            Some(lock) if now.saturating_sub(lock.heartbeat_at) > STALE_LOCK_TIMEOUT_MS => {
                conn.execute(
                    "UPDATE session_lock SET instance_id = ?1, acquired_at = ?2, heartbeat_at = ?2 WHERE id = 1",
                    params![instance_id, now],
                )?;

                Ok(SessionLock {
                    instance_id,
                    acquired_at: now,
                    heartbeat_at: now,
                })
            }
            Some(lock) => Err(PersistError::SessionLocked {
                instance_id: lock.instance_id,
                acquired_at: lock.acquired_at,
            }),
        }
    }

    pub fn heartbeat_session_lock(
        &self,
        session_id: &SessionId,
        instance_id: &str,
    ) -> Result<bool> {
        let conn = self.open_session_connection(session_id)?;
        let rows = conn.execute(
            "UPDATE session_lock SET heartbeat_at = ?2 WHERE id = 1 AND instance_id = ?1",
            params![instance_id, self.now()],
        )?;

        Ok(rows != 0)
    }

    pub fn release_session_lock(&self, session_id: &SessionId, instance_id: &str) -> Result<bool> {
        let conn = self.open_session_connection(session_id)?;
        let rows = conn.execute(
            "DELETE FROM session_lock WHERE id = 1 AND instance_id = ?1",
            [instance_id],
        )?;

        Ok(rows != 0)
    }

    pub fn commit_message(&self, command: CommitMessageCommand) -> Result<ConversationMessage> {
        let branch_id = command.branch_id;
        let message = command.message;
        let session_id = message.session_id.clone();
        let mut session_conn = self.open_session_connection(&session_id)?;
        let tx = session_conn.transaction()?;
        let branch = get_branch_record_in_tx(&tx, &branch_id)?
            .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;

        if branch.branch.session_id != session_id {
            return Err(PersistError::ConsistencyError(format!(
                "branch {} belongs to session {}, not {}",
                branch_id, branch.branch.session_id, session_id
            )));
        }

        match branch.branch.state {
            BranchState::Archived | BranchState::Deleted => {
                return Err(PersistError::ConsistencyError(format!(
                    "branch {} cannot accept new messages while in state {}",
                    branch_id, branch.branch.state
                )));
            }
            BranchState::Active | BranchState::Inactive => {}
        }

        if message.parent_id.as_ref() != Some(&branch.branch.tip_message_id) {
            let actual_parent = message
                .parent_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "<none>".to_owned());
            return Err(PersistError::ConsistencyError(format!(
                "message {} must append to branch {} tip {} (got parent {})",
                message.message_id, branch_id, branch.branch.tip_message_id, actual_parent
            )));
        }

        insert_conversation_message_in_tx(&tx, &message)?;
        tx.execute(
            "UPDATE branches SET tip_message_id = ?2 WHERE branch_id = ?1",
            params![branch_id.as_str(), message.message_id.as_str()],
        )?;
        tx.commit()?;

        self.sync_message_search_entry(&message)?;
        self.touch_session_summary(&session_id, message.created_at, 1)?;
        Ok(message)
    }

    pub fn edit_message(
        &self,
        session_id: &SessionId,
        message_id: &MessageId,
        request: EditMessageRequest,
    ) -> Result<ConversationMessage> {
        let edited_at = request.edited_at.unwrap_or_else(|| self.now());
        let new_content = request.content;
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        let mut message = get_message_in_tx(&tx, message_id)?
            .ok_or_else(|| PersistError::MessageNotFound(message_id.to_string()))?;

        if &message.session_id != session_id {
            return Err(PersistError::ConsistencyError(format!(
                "message {} belongs to session {}, not {}",
                message_id, message.session_id, session_id
            )));
        }

        if message.content == new_content {
            return Ok(message);
        }

        tx.execute(
            "INSERT INTO message_edits (message_id, previous_content, edited_at) VALUES (?1, ?2, ?3)",
            params![message_id.as_str(), message.content.as_str(), edited_at],
        )?;
        tx.execute(
            "UPDATE messages SET content = ?2, edited_at = ?3 WHERE message_id = ?1",
            params![message_id.as_str(), new_content.as_str(), edited_at],
        )?;
        tx.commit()?;

        message.content = new_content;
        message.edited_at = Some(edited_at);
        self.sync_message_search_entry(&message)?;
        self.touch_session_summary(session_id, edited_at, 0)?;
        Ok(message)
    }

    pub fn list_message_edits(
        &self,
        session_id: &SessionId,
        message_id: &MessageId,
    ) -> Result<Vec<MessageEditRecord>> {
        let conn = self.open_session_connection(session_id)?;
        ensure_message_exists_in_conn(&conn, message_id, session_id)?;
        let mut stmt = conn.prepare(
            "SELECT revision_id, message_id, previous_content, edited_at
             FROM message_edits
             WHERE message_id = ?1
             ORDER BY revision_id ASC",
        )?;
        let rows = stmt.query_map([message_id.as_str()], read_message_edit_record)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn list_bookmarks(&self, session_id: &SessionId) -> Result<Vec<BookmarkRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT b.bookmark_id, b.message_id, b.note, b.created_at
             FROM bookmarks b
             JOIN messages m ON m.message_id = b.message_id
             WHERE m.session_id = ?1
             ORDER BY b.created_at ASC, b.bookmark_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_bookmark_record)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn set_message_bookmark(
        &self,
        session_id: &SessionId,
        message_id: &MessageId,
        bookmarked: bool,
        note: Option<String>,
    ) -> Result<Option<BookmarkRecord>> {
        let touched_at = self.now();
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        ensure_message_exists_in_tx(&tx, message_id, session_id)?;

        tx.execute(
            "DELETE FROM bookmarks WHERE message_id = ?1",
            [message_id.as_str()],
        )?;

        let bookmark = if bookmarked {
            let bookmark = BookmarkRecord {
                bookmark_id: generate_uuid_like(),
                message_id: message_id.clone(),
                note,
                created_at: touched_at,
            };
            tx.execute(
                "INSERT INTO bookmarks (bookmark_id, message_id, note, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    bookmark.bookmark_id.as_str(),
                    bookmark.message_id.as_str(),
                    bookmark.note.as_deref(),
                    bookmark.created_at,
                ],
            )?;
            tx.execute(
                "UPDATE messages
                 SET bookmarked = 1,
                     bookmark_note = ?2
                 WHERE message_id = ?1",
                params![message_id.as_str(), bookmark.note.as_deref()],
            )?;
            Some(bookmark)
        } else {
            tx.execute(
                "UPDATE messages
                 SET bookmarked = 0,
                     bookmark_note = NULL
                 WHERE message_id = ?1",
                [message_id.as_str()],
            )?;
            None
        };

        tx.commit()?;
        self.touch_session_summary(session_id, touched_at, 0)?;
        Ok(bookmark)
    }

    pub fn create_branch(&self, command: CreateBranchCommand) -> Result<BranchRecord> {
        let mut branch = command.branch;
        let forked_from = command.forked_from;
        let session_id = branch.session_id.clone();
        let requested_state = branch.state;
        let mut session_conn = self.open_session_connection(&session_id)?;
        let tx = session_conn.transaction()?;

        ensure_message_exists_in_tx(&tx, &branch.tip_message_id, &session_id)?;
        ensure_message_exists_in_tx(&tx, &forked_from, &session_id)?;
        ensure_ancestry_in_tx(&tx, &forked_from, &branch.tip_message_id)?;

        if requested_state == BranchState::Active {
            branch.state = BranchState::Inactive;
        }

        tx.execute(
            "INSERT INTO branches (
                branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                branch.branch_id.as_str(),
                session_id.as_str(),
                branch.name.as_str(),
                branch.tip_message_id.as_str(),
                branch.created_at,
                branch.state.as_str(),
                branch.description.as_deref(),
                forked_from.as_str(),
            ],
        )?;

        if requested_state == BranchState::Active {
            activate_branch_in_tx(&tx, &session_id, &branch.branch_id)?;
            branch.state = BranchState::Active;
        }

        tx.commit()?;
        self.touch_session_summary(&session_id, branch.created_at, 0)?;
        Ok(BranchRecord {
            branch,
            forked_from,
        })
    }

    pub fn list_branches(&self, session_id: &SessionId) -> Result<Vec<BranchRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
             FROM branches
             ORDER BY
                CASE state
                    WHEN 'active' THEN 0
                    WHEN 'inactive' THEN 1
                    WHEN 'archived' THEN 2
                    ELSE 3
                END,
                created_at ASC,
                branch_id ASC",
        )?;
        let rows = stmt.query_map([], read_branch_record)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn get_branch(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<Option<BranchRecord>> {
        let conn = self.open_session_connection(session_id)?;
        conn.query_row(
            "SELECT branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
             FROM branches
             WHERE branch_id = ?1",
            [branch_id.as_str()],
            read_branch_record,
        )
        .optional()
        .map_err(PersistError::from)
    }

    pub fn get_active_branch(&self, session_id: &SessionId) -> Result<Option<BranchRecord>> {
        let conn = self.open_session_connection(session_id)?;
        conn.query_row(
            "SELECT branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
             FROM branches
             WHERE state = ?1
             ORDER BY created_at DESC, branch_id ASC
             LIMIT 1",
            [BranchState::Active.as_str()],
            read_branch_record,
        )
        .optional()
        .map_err(PersistError::from)
    }

    pub fn activate_branch(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<BranchRecord> {
        let touched_at = self.now();
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        activate_branch_in_tx(&tx, session_id, branch_id)?;
        let branch = get_branch_record_in_tx(&tx, branch_id)?
            .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;
        tx.commit()?;
        self.touch_session_summary(session_id, touched_at, 0)?;
        Ok(branch)
    }

    pub fn set_branch_tip(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        tip_message_id: &MessageId,
    ) -> Result<BranchRecord> {
        let touched_at = self.now();
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        let branch = get_branch_record_in_tx(&tx, branch_id)?
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
                    "branch {} cannot move tip while in state {}",
                    branch_id, branch.branch.state
                )));
            }
            BranchState::Active | BranchState::Inactive => {}
        }

        ensure_message_exists_in_tx(&tx, tip_message_id, session_id)?;
        ensure_ancestry_in_tx(&tx, &branch.forked_from, tip_message_id)?;

        tx.execute(
            "UPDATE branches SET tip_message_id = ?2 WHERE branch_id = ?1",
            params![branch_id.as_str(), tip_message_id.as_str()],
        )?;
        let updated_branch = get_branch_record_in_tx(&tx, branch_id)?
            .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;
        tx.commit()?;

        self.touch_session_summary(session_id, touched_at, 0)?;
        Ok(updated_branch)
    }

    pub fn record_swipe_candidate(
        &self,
        session_id: &SessionId,
        command: RecordSwipeCandidateCommand,
    ) -> Result<SwipeCandidate> {
        let group = command.group;
        let candidate = command.candidate;
        let touched_at = self.now();
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;

        ensure_message_exists_in_tx(&tx, &group.parent_message_id, session_id)?;
        if let Some(parent_context_message_id) = &group.parent_context_message_id {
            ensure_message_exists_in_tx(&tx, parent_context_message_id, session_id)?;
        }
        ensure_message_exists_in_tx(&tx, &candidate.message_id, session_id)?;

        match get_swipe_group_in_tx(&tx, &group.swipe_group_id)? {
            Some(existing) => {
                if existing.parent_message_id != group.parent_message_id
                    || existing.parent_context_message_id != group.parent_context_message_id
                {
                    return Err(PersistError::ConsistencyError(format!(
                        "swipe group {} parent references do not match existing persisted group",
                        group.swipe_group_id
                    )));
                }
            }
            None => {
                tx.execute(
                    "INSERT INTO swipe_groups (
                        swipe_group_id, parent_message_id, parent_context_message_id, active_ordinal
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        group.swipe_group_id.as_str(),
                        group.parent_message_id.as_str(),
                        group
                            .parent_context_message_id
                            .as_ref()
                            .map(MessageId::as_str),
                        i64::from(group.active_ordinal),
                    ],
                )?;
            }
        }

        tx.execute(
            "INSERT INTO swipe_candidates (
                swipe_group_id, ordinal, message_id, state, partial_content, tokens_generated
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                candidate.swipe_group_id.as_str(),
                i64::from(candidate.ordinal),
                candidate.message_id.as_str(),
                candidate.state.as_str(),
                candidate.partial_content.as_deref(),
                candidate
                    .tokens_generated
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| {
                        PersistError::InvalidData(
                            "tokens_generated exceeds SQLite INTEGER".to_owned(),
                        )
                    })?,
            ],
        )?;
        tx.commit()?;

        self.touch_session_summary(session_id, touched_at, 0)?;
        Ok(candidate)
    }

    pub fn list_swipe_groups(&self, session_id: &SessionId) -> Result<Vec<SwipeGroup>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT sg.swipe_group_id, sg.parent_message_id, sg.parent_context_message_id, sg.active_ordinal
             FROM swipe_groups sg
             JOIN messages parent ON parent.message_id = sg.parent_message_id
             WHERE parent.session_id = ?1
             ORDER BY parent.created_at ASC, sg.swipe_group_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_swipe_group)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn get_swipe_group(
        &self,
        session_id: &SessionId,
        swipe_group_id: &SwipeGroupId,
    ) -> Result<Option<SwipeGroup>> {
        let conn = self.open_session_connection(session_id)?;
        conn.query_row(
            "SELECT sg.swipe_group_id, sg.parent_message_id, sg.parent_context_message_id, sg.active_ordinal
             FROM swipe_groups sg
             JOIN messages parent ON parent.message_id = sg.parent_message_id
             WHERE parent.session_id = ?1 AND sg.swipe_group_id = ?2",
            params![session_id.as_str(), swipe_group_id.as_str()],
            read_swipe_group,
        )
        .optional()
        .map_err(PersistError::from)
    }

    pub fn list_swipe_candidates(
        &self,
        session_id: &SessionId,
        swipe_group_id: &SwipeGroupId,
    ) -> Result<Vec<SwipeCandidate>> {
        let conn = self.open_session_connection(session_id)?;
        if self.get_swipe_group(session_id, swipe_group_id)?.is_none() {
            return Err(PersistError::SwipeGroupNotFound(swipe_group_id.to_string()));
        }

        let mut stmt = conn.prepare(
            "SELECT sc.swipe_group_id, sc.ordinal, sc.message_id, sc.state, sc.partial_content, sc.tokens_generated
             FROM swipe_candidates sc
             JOIN swipe_groups sg ON sg.swipe_group_id = sc.swipe_group_id
             JOIN messages parent ON parent.message_id = sg.parent_message_id
             WHERE parent.session_id = ?1 AND sc.swipe_group_id = ?2
             ORDER BY sc.ordinal ASC, sc.message_id ASC",
        )?;
        let rows = stmt.query_map(
            params![session_id.as_str(), swipe_group_id.as_str()],
            read_swipe_candidate,
        )?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn activate_swipe_candidate(
        &self,
        session_id: &SessionId,
        command: ActivateSwipeCommand,
    ) -> Result<SwipeGroup> {
        let touched_at = self.now();
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        let group = get_swipe_group_in_tx(&tx, &command.swipe_group_id)?
            .ok_or_else(|| PersistError::SwipeGroupNotFound(command.swipe_group_id.to_string()))?;

        ensure_swipe_group_belongs_to_session_in_tx(&tx, &group, session_id)?;

        let candidate_exists: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM swipe_candidates
                WHERE swipe_group_id = ?1 AND ordinal = ?2
            )",
            params![command.swipe_group_id.as_str(), i64::from(command.ordinal)],
            |row| row.get::<_, i64>(0),
        )? != 0;

        if !candidate_exists {
            return Err(PersistError::SwipeCandidateNotFound {
                swipe_group_id: command.swipe_group_id.to_string(),
                ordinal: command.ordinal,
            });
        }

        tx.execute(
            "UPDATE swipe_groups SET active_ordinal = ?2 WHERE swipe_group_id = ?1",
            params![command.swipe_group_id.as_str(), i64::from(command.ordinal)],
        )?;
        let mut updated_group = group;
        updated_group.active_ordinal = command.ordinal;
        tx.commit()?;

        self.touch_session_summary(session_id, touched_at, 0)?;
        Ok(updated_group)
    }

    pub fn list_branch_messages(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<Vec<ConversationMessage>> {
        let conn = self.open_session_connection(session_id)?;
        let branch = conn
            .query_row(
                "SELECT branch_id, session_id, name, tip_message_id, created_at, state, description, forked_from_message_id
                 FROM branches
                 WHERE branch_id = ?1",
                [branch_id.as_str()],
                read_branch_record,
            )
            .optional()?
            .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT m.message_id, m.session_id, m.parent_id, m.author_kind, m.author_name, m.content, m.created_at, m.edited_at, m.is_hidden
             FROM message_ancestry ancestry
             JOIN messages m ON m.message_id = ancestry.ancestor_id
             WHERE ancestry.descendant_id = ?1
             ORDER BY ancestry.depth DESC, m.created_at ASC, m.message_id ASC",
        )?;
        let rows = stmt.query_map(
            [branch.branch.tip_message_id.as_str()],
            read_conversation_message,
        )?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn get_active_branch_transcript(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationMessage>> {
        match self.get_active_branch(session_id)? {
            Some(branch) => self.list_branch_messages(session_id, &branch.branch.branch_id),
            None => Ok(Vec::new()),
        }
    }

    pub fn list_session_messages(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationMessage>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT message_id, session_id, parent_id, author_kind, author_name, content, created_at, edited_at, is_hidden
             FROM messages
             WHERE session_id = ?1
             ORDER BY created_at ASC, message_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_conversation_message)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn get_character_card(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<StoredCharacterCard>> {
        let conn = self.open_session_connection(session_id)?;
        conn.query_row(
            "SELECT content_json
             FROM memory_artifacts
             WHERE session_id = ?1 AND kind = 'character_card'
             ORDER BY created_at DESC, artifact_id DESC
             LIMIT 1",
            [session_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|content_json| {
            serde_json::from_str::<StoredCharacterCard>(&content_json).map_err(|error| {
                PersistError::InvalidData(format!(
                    "invalid stored character card JSON for session {session_id}: {error}"
                ))
            })
        })
        .transpose()
    }

    pub fn export_session(&self, session_id: &SessionId) -> Result<SessionExport> {
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))?;
        let branches = self.list_branches(session_id)?;
        let messages = self.list_session_messages(session_id)?;
        let bookmarks = self.list_bookmarks(session_id)?;
        let swipe_groups = self
            .list_swipe_groups(session_id)?
            .into_iter()
            .map(|group| {
                let candidates = self
                    .list_swipe_candidates(session_id, &group.swipe_group_id)?
                    .into_iter()
                    .map(export_swipe_candidate)
                    .collect();
                Ok(export_swipe_group(group, candidates))
            })
            .collect::<Result<Vec<_>>>()?;
        let branches = branches
            .into_iter()
            .map(|record| {
                let transcript_message_ids = self
                    .list_branch_messages(session_id, &record.branch.branch_id)?
                    .into_iter()
                    .map(|message| message.message_id.to_string())
                    .collect();
                Ok(export_branch(record, transcript_message_ids))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(SessionExport {
            format: SESSION_EXPORT_FORMAT.to_owned(),
            exported_at: self.now(),
            session: export_session_summary(&session),
            active_branch_id: self
                .get_active_branch(session_id)?
                .map(|record| record.branch.branch_id.to_string()),
            character_card: self.get_character_card(session_id)?,
            branches,
            messages: messages.into_iter().map(export_message).collect(),
            bookmarks: bookmarks.into_iter().map(export_bookmark).collect(),
            swipe_groups,
        })
    }

    pub fn export_transcript(
        &self,
        session_id: &SessionId,
        branch_id: Option<&BranchId>,
    ) -> Result<TranscriptExport> {
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))?;
        let branch = match branch_id {
            Some(branch_id) => {
                let record = self
                    .get_branch(session_id, branch_id)?
                    .ok_or_else(|| PersistError::BranchNotFound(branch_id.to_string()))?;
                if record.branch.session_id != *session_id {
                    return Err(PersistError::BranchNotFound(branch_id.to_string()));
                }
                Some(record)
            }
            None => self.get_active_branch(session_id)?,
        };
        let messages = match branch.as_ref() {
            Some(record) => self.list_branch_messages(session_id, &record.branch.branch_id)?,
            None => Vec::new(),
        };

        Ok(TranscriptExport {
            format: TRANSCRIPT_EXPORT_FORMAT.to_owned(),
            exported_at: self.now(),
            session: TranscriptExportSession {
                session_id: session.session_id.to_string(),
                name: session.name,
                character_name: session.character_name,
            },
            branch: branch.map(export_transcript_branch),
            messages: messages.into_iter().map(export_message).collect(),
        })
    }

    pub fn insert_message(
        &self,
        session_id: &SessionId,
        request: CreateMessageRequest,
    ) -> Result<MessageRecord> {
        let message_id = MessageId::parse(generate_uuid_like())?;
        let created_at = self.now();
        let parent_id = request
            .parent_id
            .as_deref()
            .map(MessageId::parse)
            .transpose()?;
        let mut message = ConversationMessage::new(
            session_id.clone(),
            message_id.clone(),
            request.author_kind,
            request.content,
            created_at,
        );
        message.parent_id = parent_id.clone();
        message.author_name = request.author_name;

        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        insert_conversation_message_in_tx(&tx, &message)?;
        tx.commit()?;

        self.sync_message_search_entry(&message)?;
        self.touch_session_summary(session_id, created_at, 1)?;

        Ok(MessageRecord {
            message_id: message_id.to_string(),
            session_id: session_id.clone(),
            parent_id: parent_id.map(|value| value.to_string()),
            author_kind: message.author_kind,
            author_name: message.author_name,
            content: message.content,
            created_at,
        })
    }

    pub fn search_messages(
        &self,
        session_id: &SessionId,
        query: &str,
    ) -> Result<Vec<MessageSearchHit>> {
        let Some(query) = plain_text_fts_query(query) else {
            return Ok(Vec::new());
        };
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT m.message_id, m.author_kind, m.content, m.created_at, bm25(messages_fts)
             FROM messages_fts
             JOIN messages m ON m.rowid = messages_fts.rowid
             WHERE m.session_id = ?1 AND messages_fts MATCH ?2
             ORDER BY bm25(messages_fts), m.created_at DESC, m.rowid ASC",
        )?;
        let rows = stmt.query_map(params![session_id.as_str(), query], |row| {
            Ok(MessageSearchHit {
                message_id: row.get(0)?,
                author_kind: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                bm25_score: row.get(4)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
    }

    pub fn pin_message_memory(
        &self,
        session_id: &SessionId,
        message_id: &MessageId,
        request: PinMessageMemoryRequest,
    ) -> Result<PinnedMemoryRecord> {
        let created_at = self.now();
        let snapshot_version = self.current_message_count(session_id)?;
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        let source_message = ensure_message_exists_in_tx(&tx, message_id, session_id)?;
        let content = PinnedMemoryContent {
            text: source_message.content,
            pinned_by: request.pinned_by,
            expires_after_turns: request.expires_after_turns,
        };
        let record = insert_pinned_memory_artifact_in_tx(
            &tx,
            session_id,
            content,
            Some(message_id),
            request.provenance,
            created_at,
            snapshot_version,
        )?;
        tx.commit()?;

        self.touch_session_summary(session_id, created_at, 0)?;
        Ok(record)
    }

    pub fn create_note_memory(
        &self,
        session_id: &SessionId,
        request: CreateNoteMemoryRequest,
    ) -> Result<PinnedMemoryRecord> {
        let created_at = self.now();
        let snapshot_version = self.current_message_count(session_id)?;
        let mut session_conn = self.open_session_connection(session_id)?;
        let tx = session_conn.transaction()?;
        let record = insert_pinned_memory_artifact_in_tx(
            &tx,
            session_id,
            request.content,
            None,
            request.provenance,
            created_at,
            snapshot_version,
        )?;
        tx.commit()?;

        self.touch_session_summary(session_id, created_at, 0)?;
        Ok(record)
    }

    pub fn list_pinned_memories(&self, session_id: &SessionId) -> Result<Vec<PinnedMemoryView>> {
        let current_message_count = self.current_message_count(session_id)?;
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT artifact_id, session_id, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
             FROM memory_artifacts
             WHERE session_id = ?1 AND kind = 'pinned_memory'
             ORDER BY created_at ASC, artifact_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_stored_pinned_memory_artifact)?;
        let records = rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(PinnedMemoryRecord::try_from)
            .collect::<Result<Vec<_>>>()?;

        Ok(records
            .into_iter()
            .map(|record| record.into_view(current_message_count))
            .collect())
    }

    pub fn remove_pinned_memory(
        &self,
        session_id: &SessionId,
        artifact_id: &MemoryArtifactId,
    ) -> Result<bool> {
        let touched_at = self.now();
        let conn = self.open_session_connection(session_id)?;
        let deleted = conn.execute(
            "DELETE FROM memory_artifacts
             WHERE session_id = ?1 AND artifact_id = ?2 AND kind = 'pinned_memory'",
            params![session_id.as_str(), artifact_id.as_str()],
        )? > 0;

        if deleted {
            self.touch_session_summary(session_id, touched_at, 0)?;
        }

        Ok(deleted)
    }

    pub fn store_chunk_summary(
        &self,
        session_id: &SessionId,
        summary_text: &str,
        source_count: usize,
        start_message_id: &MessageId,
        end_message_id: &MessageId,
        snapshot_version: u64,
    ) -> Result<SummaryArtifactRecord> {
        let artifact_id = MemoryArtifactId::parse(generate_uuid_like())?;
        let content = MemoryContent::chunk_summary(summary_text, source_count);
        let content_json = serde_json::to_string(&content).map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize chunk summary for session {session_id}: {error}"
            ))
        })?;
        let created_at = self.now();
        let snapshot_version_i64 = i64::try_from(snapshot_version).map_err(|_| {
            PersistError::InvalidData("snapshot_version exceeds SQLite INTEGER".to_owned())
        })?;

        let conn = self.open_session_connection(session_id)?;
        conn.execute(
            "INSERT INTO memory_artifacts (artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version)
             VALUES (?1, ?2, 'chunk_summary', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                artifact_id.as_str(),
                session_id.as_str(),
                content_json,
                start_message_id.as_str(),
                end_message_id.as_str(),
                Provenance::SystemGenerated.as_str(),
                created_at,
                snapshot_version_i64,
            ],
        )?;

        Ok(SummaryArtifactRecord {
            artifact_id,
            session_id: session_id.clone(),
            kind: "chunk_summary".to_owned(),
            text: summary_text.to_owned(),
            source_count: Some(source_count),
            message_count: None,
            start_message_id: Some(start_message_id.to_string()),
            end_message_id: Some(end_message_id.to_string()),
            created_at,
            snapshot_version,
        })
    }

    pub fn store_session_synopsis(
        &self,
        session_id: &SessionId,
        synopsis_text: &str,
        message_count: usize,
        snapshot_version: u64,
    ) -> Result<SummaryArtifactRecord> {
        let artifact_id = MemoryArtifactId::parse(generate_uuid_like())?;
        let content = MemoryContent::session_synopsis(synopsis_text, message_count);
        let content_json = serde_json::to_string(&content).map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize session synopsis for session {session_id}: {error}"
            ))
        })?;
        let created_at = self.now();
        let snapshot_version_i64 = i64::try_from(snapshot_version).map_err(|_| {
            PersistError::InvalidData("snapshot_version exceeds SQLite INTEGER".to_owned())
        })?;

        let conn = self.open_session_connection(session_id)?;
        conn.execute(
            "INSERT INTO memory_artifacts (artifact_id, session_id, kind, content_json, provenance, created_at, snapshot_version)
             VALUES (?1, ?2, 'session_synopsis', ?3, ?4, ?5, ?6)",
            params![
                artifact_id.as_str(),
                session_id.as_str(),
                content_json,
                Provenance::SystemGenerated.as_str(),
                created_at,
                snapshot_version_i64,
            ],
        )?;

        Ok(SummaryArtifactRecord {
            artifact_id,
            session_id: session_id.clone(),
            kind: "session_synopsis".to_owned(),
            text: synopsis_text.to_owned(),
            source_count: None,
            message_count: Some(message_count),
            start_message_id: None,
            end_message_id: None,
            created_at,
            snapshot_version,
        })
    }

    pub fn list_chunk_summaries(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SummaryArtifactRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, created_at, snapshot_version
             FROM memory_artifacts
             WHERE session_id = ?1 AND kind = 'chunk_summary'
             ORDER BY created_at DESC, artifact_id DESC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], |row| {
            Ok(StoredSummaryArtifact {
                artifact_id: row.get(0)?,
                session_id: row.get(1)?,
                kind: row.get(2)?,
                content_json: row.get(3)?,
                start_message_id: row.get(4)?,
                end_message_id: row.get(5)?,
                created_at: row.get(6)?,
                snapshot_version: row.get(7)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(SummaryArtifactRecord::try_from)
            .collect()
    }

    pub fn get_latest_session_synopsis(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SummaryArtifactRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let result = conn
            .query_row(
                "SELECT artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, created_at, snapshot_version
                 FROM memory_artifacts
                 WHERE session_id = ?1 AND kind = 'session_synopsis'
                 ORDER BY created_at DESC, artifact_id DESC
                 LIMIT 1",
                [session_id.as_str()],
                |row| {
                    Ok(StoredSummaryArtifact {
                        artifact_id: row.get(0)?,
                        session_id: row.get(1)?,
                        kind: row.get(2)?,
                        content_json: row.get(3)?,
                        start_message_id: row.get(4)?,
                        end_message_id: row.get(5)?,
                        created_at: row.get(6)?,
                        snapshot_version: row.get(7)?,
                    })
                },
            )
            .optional()?;

        match result {
            Some(stored) => Ok(Some(SummaryArtifactRecord::try_from(stored)?)),
            None => Ok(None),
        }
    }

    pub fn delete_summary_artifact(
        &self,
        session_id: &SessionId,
        artifact_id: &MemoryArtifactId,
    ) -> Result<bool> {
        let conn = self.open_session_connection(session_id)?;
        let deleted = conn.execute(
            "DELETE FROM memory_artifacts
             WHERE artifact_id = ?1 AND session_id = ?2
             AND kind IN ('chunk_summary', 'session_synopsis')",
            params![artifact_id.as_str(), session_id.as_str()],
        )? > 0;
        Ok(deleted)
    }

    pub fn upsert_embedding_artifacts(&self, records: &[EmbeddingRecord]) -> Result<usize> {
        let grouped = group_embedding_records(records)?;
        let mut inserted = 0;

        for (session_id, session_records) in grouped {
            let mut session_conn = self.open_session_connection(&session_id)?;
            let tx = session_conn.transaction()?;
            for record in session_records {
                upsert_embedding_artifact_in_tx(&tx, record)?;
                inserted += 1;
            }
            tx.commit()?;
            self.refresh_session_size(&session_id)?;
        }

        Ok(inserted)
    }

    pub fn list_embedding_artifacts(
        &self,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<EmbeddingRecord>> {
        let mut records = match session_id {
            Some(session_id) => self.list_session_embedding_artifacts(session_id)?,
            None => {
                let mut records = Vec::new();
                for session in self.list_sessions()? {
                    records.extend(self.list_session_embedding_artifacts(&session.session_id)?);
                }
                records
            }
        };

        records.sort_by(|left, right| {
            left.session_id
                .as_str()
                .cmp(right.session_id.as_str())
                .then_with(|| {
                    left.source_message_id
                        .as_ref()
                        .map(|message_id| message_id.as_str())
                        .cmp(
                            &right
                                .source_message_id
                                .as_ref()
                                .map(|message_id| message_id.as_str()),
                        )
                })
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.artifact_id.as_str().cmp(right.artifact_id.as_str()))
        });
        Ok(records)
    }

    pub fn list_derived_artifacts(
        &self,
        session_id: Option<&SessionId>,
    ) -> Result<Vec<DerivedArtifactRecord>> {
        let mut records = Vec::new();

        for session_id in target_artifact_sessions(self, session_id)? {
            records.extend(self.list_session_derived_artifacts(&session_id)?);
        }

        records.sort_by(|left, right| {
            left.session_id
                .as_str()
                .cmp(right.session_id.as_str())
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.artifact_id.as_str().cmp(right.artifact_id.as_str()))
        });
        Ok(records)
    }

    pub fn inspect_derived_artifacts(
        &self,
        session_id: Option<&SessionId>,
        storage_policy: &StorageTierPolicy,
        max_age_messages: usize,
        max_age_hours: u64,
    ) -> Result<Vec<DerivedArtifactLifecycleRecord>> {
        let now_ms = self.now();
        let current_message_counts = session_message_counts(self, session_id)?;

        self.list_derived_artifacts(session_id)?
            .into_iter()
            .map(|record| {
                let current_message_count = current_message_counts
                    .get(&record.session_id)
                    .copied()
                    .ok_or_else(|| PersistError::SessionNotFound(record.session_id.to_string()))?;
                Ok(DerivedArtifactLifecycleRecord::from_record(
                    record,
                    current_message_count,
                    storage_policy,
                    now_ms,
                    max_age_messages,
                    max_age_hours,
                ))
            })
            .collect()
    }

    pub fn plan_garbage_collection(
        &self,
        session_id: Option<&SessionId>,
        storage_policy: &StorageTierPolicy,
        max_age_messages: usize,
        max_age_hours: u64,
        policy: &GarbageCollectionPolicy,
    ) -> Result<GarbageCollectionPlan> {
        let inspected = self.inspect_derived_artifacts(
            session_id,
            storage_policy,
            max_age_messages,
            max_age_hours,
        )?;
        let inspected_count = inspected.len();
        let mut candidate_reasons =
            BTreeMap::<(SessionId, MemoryArtifactId), Vec<GarbageCollectionReason>>::new();

        if policy.purge_unreferenced_backlog {
            for artifact in &inspected {
                if !artifact.source_exists {
                    mark_gc_reason(
                        &mut candidate_reasons,
                        artifact,
                        GarbageCollectionReason::OrphanedSource,
                    );
                }
            }
        }

        for artifact in &inspected {
            if artifact.storage_tier == StorageTier::Minimal
                && matches!(
                    artifact.kind,
                    DerivedArtifactKind::Embedding | DerivedArtifactKind::ChunkSummary
                )
            {
                mark_gc_reason(
                    &mut candidate_reasons,
                    artifact,
                    GarbageCollectionReason::MinimalTier,
                );
            }
        }

        let mut newest_synopses = BTreeMap::<SessionId, MemoryArtifactId>::new();
        for artifact in inspected
            .iter()
            .filter(|artifact| artifact.kind == DerivedArtifactKind::SessionSynopsis)
        {
            newest_synopses.insert(artifact.session_id.clone(), artifact.artifact_id.clone());
        }
        for artifact in inspected
            .iter()
            .filter(|artifact| artifact.kind == DerivedArtifactKind::SessionSynopsis)
        {
            if newest_synopses
                .get(&artifact.session_id)
                .is_some_and(|latest| latest != &artifact.artifact_id)
            {
                mark_gc_reason(
                    &mut candidate_reasons,
                    artifact,
                    GarbageCollectionReason::SupersededSynopsis,
                );
            }
        }

        let mut remaining_embeddings = inspected
            .iter()
            .filter(|artifact| artifact.kind == DerivedArtifactKind::Embedding)
            .filter(|artifact| {
                !candidate_reasons
                    .contains_key(&(artifact.session_id.clone(), artifact.artifact_id.clone()))
            })
            .collect::<Vec<_>>();
        remaining_embeddings.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.session_id.as_str().cmp(right.session_id.as_str()))
                .then_with(|| left.artifact_id.as_str().cmp(right.artifact_id.as_str()))
        });

        let over_limit = remaining_embeddings
            .len()
            .saturating_sub(policy.max_active_embeddings);
        for artifact in remaining_embeddings.into_iter().take(over_limit) {
            mark_gc_reason(
                &mut candidate_reasons,
                artifact,
                GarbageCollectionReason::OverEmbeddingLimit,
            );
        }

        let candidates = inspected
            .into_iter()
            .filter_map(|artifact| {
                candidate_reasons
                    .remove(&(artifact.session_id.clone(), artifact.artifact_id.clone()))
                    .map(|reasons| GarbageCollectionCandidate { artifact, reasons })
            })
            .collect::<Vec<_>>();
        let mut reason_counts = BTreeMap::new();
        for candidate in &candidates {
            for reason in &candidate.reasons {
                *reason_counts.entry(*reason).or_insert(0) += 1;
            }
        }

        Ok(GarbageCollectionPlan {
            inspected_count,
            candidate_count: candidates.len(),
            reason_counts,
            candidates,
        })
    }

    pub fn apply_garbage_collection_plan(
        &self,
        plan: &GarbageCollectionPlan,
    ) -> Result<GarbageCollectionOutcome> {
        let mut planned = BTreeMap::<SessionId, BTreeSet<MemoryArtifactId>>::new();
        for candidate in &plan.candidates {
            planned
                .entry(candidate.artifact.session_id.clone())
                .or_default()
                .insert(candidate.artifact.artifact_id.clone());
        }

        let mut deleted_artifact_ids = BTreeMap::<SessionId, Vec<MemoryArtifactId>>::new();

        for (session_id, artifact_ids) in planned {
            let mut session_conn = self.open_session_connection(&session_id)?;
            let tx = session_conn.transaction()?;
            let mut deleted_in_session = Vec::new();

            for artifact_id in artifact_ids {
                if tx.execute(
                    "DELETE FROM memory_artifacts
                     WHERE session_id = ?1 AND artifact_id = ?2
                     AND kind IN ('embedding', 'chunk_summary', 'session_synopsis')",
                    params![session_id.as_str(), artifact_id.as_str()],
                )? > 0
                {
                    deleted_in_session.push(artifact_id);
                }
            }

            tx.commit()?;
            if !deleted_in_session.is_empty() {
                self.refresh_session_size(&session_id)?;
                deleted_artifact_ids.insert(session_id, deleted_in_session);
            }
        }

        let deleted_count = deleted_artifact_ids.values().map(Vec::len).sum();
        Ok(GarbageCollectionOutcome {
            deleted_count,
            deleted_artifact_ids,
        })
    }

    pub fn remove_embedding_artifacts(&self, session_id: Option<&SessionId>) -> Result<usize> {
        let target_sessions = match session_id {
            Some(session_id) => vec![session_id.clone()],
            None => self
                .list_sessions()?
                .into_iter()
                .map(|session| session.session_id)
                .collect(),
        };
        let mut removed = 0;

        for session_id in target_sessions {
            let conn = self.open_session_connection(&session_id)?;
            let session_removed = conn.execute(
                "DELETE FROM memory_artifacts
                 WHERE session_id = ?1 AND kind = 'embedding'",
                [session_id.as_str()],
            )?;
            if session_removed != 0 {
                self.refresh_session_size(&session_id)?;
            }
            removed += session_removed;
        }

        Ok(removed)
    }

    pub fn compact_events(
        &self,
        session_id: Option<&SessionId>,
        older_than_ms: u64,
    ) -> Result<usize> {
        let target_sessions = match session_id {
            Some(session_id) => vec![session_id.clone()],
            None => self
                .list_sessions()?
                .into_iter()
                .map(|session| session.session_id)
                .collect(),
        };
        let mut removed = 0;

        for session_id in target_sessions {
            let conn = self.open_session_connection(&session_id)?;
            let session_removed = conn.execute(
                "DELETE FROM events WHERE created_at < ?1",
                [older_than_ms as i64],
            )?;
            if session_removed != 0 {
                self.refresh_session_size(&session_id)?;
            }
            removed += session_removed;
        }

        Ok(removed)
    }

    pub fn replace_embedding_artifacts(
        &self,
        session_id: Option<&SessionId>,
        records: &[EmbeddingRecord],
    ) -> Result<usize> {
        let grouped = group_embedding_records(records)?;
        let target_sessions = target_embedding_sessions(self, session_id, &grouped)?;
        let mut inserted = 0;

        for session_id in target_sessions {
            let session_records = grouped.get(&session_id).cloned().unwrap_or_else(Vec::new);
            let mut session_conn = self.open_session_connection(&session_id)?;
            let tx = session_conn.transaction()?;
            tx.execute(
                "DELETE FROM memory_artifacts
                 WHERE session_id = ?1 AND kind = 'embedding'",
                [session_id.as_str()],
            )?;
            for record in session_records {
                upsert_embedding_artifact_in_tx(&tx, record)?;
                inserted += 1;
            }
            tx.commit()?;
            self.refresh_session_size(&session_id)?;
        }

        Ok(inserted)
    }

    pub fn search_across_sessions(&self, query: &str) -> Result<Vec<CrossSessionSearchHit>> {
        let Some(query) = plain_text_fts_query(query) else {
            return Ok(Vec::new());
        };
        let conn = self.ensure_global_connection()?;
        let mut stmt = conn.prepare(
            "SELECT ss.session_id, s.name, s.character_name, s.tags, ss.message_id, ss.author_kind, ss.content, ss.created_at, bm25(session_search_fts)
             FROM session_search_fts
             JOIN session_search ss ON ss.rowid = session_search_fts.rowid
             JOIN sessions s ON s.session_id = ss.session_id
             WHERE session_search_fts MATCH ?1
             ORDER BY bm25(session_search_fts), ss.created_at DESC, ss.rowid ASC",
        )?;
        let rows = stmt.query_map([query], read_stored_cross_session_search_hit)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(CrossSessionSearchHit::try_from)
            .collect()
    }

    fn store_character_card(
        &self,
        session_id: &SessionId,
        card: &StoredCharacterCard,
    ) -> Result<()> {
        let conn = self.open_session_connection(session_id)?;
        let content_json = serde_json::to_string(card).map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize character card artifact for session {session_id}: {error}"
            ))
        })?;
        conn.execute(
            "INSERT INTO memory_artifacts (
                artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
             ) VALUES (?1, ?2, 'character_card', ?3, NULL, NULL, ?4, ?5, ?6)",
            params![
                generate_uuid_like(),
                session_id.as_str(),
                content_json,
                card.provenance.as_str(),
                card.imported_at,
                1_i64,
            ],
        )?;

        Ok(())
    }

    fn sync_message_search_entry(&self, message: &ConversationMessage) -> Result<()> {
        let global_conn = self.ensure_global_connection()?;
        global_conn.execute(
            "INSERT INTO session_search (session_id, message_id, content, author_kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id, message_id) DO UPDATE SET
                 content = excluded.content,
                 author_kind = excluded.author_kind,
                 created_at = excluded.created_at",
            params![
                message.session_id.as_str(),
                message.message_id.as_str(),
                message.content.as_str(),
                message.author_kind.as_str(),
                message.created_at,
            ],
        )?;

        Ok(())
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

    fn list_session_derived_artifacts(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<DerivedArtifactRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let message_ids = session_message_id_set(&conn, session_id)?;
        let mut stmt = conn.prepare(
            "SELECT artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
             FROM memory_artifacts
             WHERE session_id = ?1
             AND kind IN ('embedding', 'chunk_summary', 'session_synopsis')
             ORDER BY created_at ASC, artifact_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_stored_derived_artifact)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|stored| stored.into_record(&message_ids))
            .collect()
    }

    fn list_session_embedding_artifacts(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<EmbeddingRecord>> {
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT artifact_id, session_id, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
             FROM memory_artifacts
             WHERE session_id = ?1 AND kind = 'embedding'
             ORDER BY created_at ASC, artifact_id ASC",
        )?;
        let rows = stmt.query_map([session_id.as_str()], read_stored_embedding_artifact)?;

        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(EmbeddingRecord::try_from)
            .collect()
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

fn merge_tags(primary: &[String], secondary: &[String]) -> Vec<String> {
    let mut tags = Vec::new();
    for candidate in primary.iter().chain(secondary.iter()) {
        let trimmed = candidate.trim();
        if trimmed.is_empty() || tags.iter().any(|existing| existing == trimmed) {
            continue;
        }
        tags.push(trimmed.to_owned());
    }
    tags
}

fn export_session_summary(session: &SessionSummary) -> SessionExportSummary {
    SessionExportSummary {
        session_id: session.session_id.to_string(),
        name: session.name.clone(),
        character_name: session.character_name.clone(),
        created_at: session.created_at,
        last_opened_at: session.last_opened_at,
        message_count: session.message_count,
        db_size_bytes: session.db_size_bytes,
        tags: session.tags.clone(),
    }
}

fn export_branch(record: BranchRecord, transcript_message_ids: Vec<String>) -> SessionExportBranch {
    SessionExportBranch {
        branch_id: record.branch.branch_id.to_string(),
        name: record.branch.name,
        state: record.branch.state.as_str().to_owned(),
        tip_message_id: record.branch.tip_message_id.to_string(),
        forked_from_message_id: record.forked_from.to_string(),
        created_at: record.branch.created_at,
        description: record.branch.description,
        transcript_message_ids,
    }
}

fn export_transcript_branch(record: BranchRecord) -> TranscriptExportBranch {
    TranscriptExportBranch {
        branch_id: record.branch.branch_id.to_string(),
        name: record.branch.name,
        state: record.branch.state.as_str().to_owned(),
        tip_message_id: record.branch.tip_message_id.to_string(),
        forked_from_message_id: record.forked_from.to_string(),
        created_at: record.branch.created_at,
        description: record.branch.description,
    }
}

fn export_message(message: ConversationMessage) -> SessionExportMessage {
    SessionExportMessage {
        message_id: message.message_id.to_string(),
        session_id: message.session_id.to_string(),
        parent_id: message.parent_id.map(|parent_id| parent_id.to_string()),
        author_kind: message.author_kind,
        author_name: message.author_name,
        content: message.content,
        created_at: message.created_at,
        edited_at: message.edited_at,
        is_hidden: message.is_hidden,
    }
}

fn export_bookmark(bookmark: BookmarkRecord) -> SessionExportBookmark {
    SessionExportBookmark {
        bookmark_id: bookmark.bookmark_id,
        message_id: bookmark.message_id.to_string(),
        note: bookmark.note,
        created_at: bookmark.created_at,
    }
}

fn export_swipe_group(
    group: SwipeGroup,
    candidates: Vec<SessionExportSwipeCandidate>,
) -> SessionExportSwipeGroup {
    SessionExportSwipeGroup {
        swipe_group_id: group.swipe_group_id.to_string(),
        parent_message_id: group.parent_message_id.to_string(),
        parent_context_message_id: group
            .parent_context_message_id
            .map(|message_id| message_id.to_string()),
        active_ordinal: group.active_ordinal,
        candidates,
    }
}

fn export_swipe_candidate(candidate: SwipeCandidate) -> SessionExportSwipeCandidate {
    SessionExportSwipeCandidate {
        ordinal: candidate.ordinal,
        message_id: candidate.message_id.to_string(),
        state: candidate.state.as_str().to_owned(),
        partial_content: candidate.partial_content,
        tokens_generated: candidate.tokens_generated,
    }
}

fn insert_pinned_memory_artifact_in_tx(
    tx: &Transaction<'_>,
    session_id: &SessionId,
    content: PinnedMemoryContent,
    source_message_id: Option<&MessageId>,
    provenance: Provenance,
    created_at: UnixTimestamp,
    snapshot_version: u64,
) -> Result<PinnedMemoryRecord> {
    let artifact_id = MemoryArtifactId::parse(generate_uuid_like())?;
    let content_json =
        serde_json::to_string(&MemoryContent::from(content.clone())).map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize pinned memory artifact for session {session_id}: {error}"
            ))
        })?;
    let snapshot_version_i64 = i64::try_from(snapshot_version).map_err(|_| {
        PersistError::InvalidData("snapshot_version exceeds SQLite INTEGER".to_owned())
    })?;
    let source_message_id_str = source_message_id.map(MessageId::as_str);

    tx.execute(
        "INSERT INTO memory_artifacts (
            artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
         ) VALUES (?1, ?2, 'pinned_memory', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            artifact_id.as_str(),
            session_id.as_str(),
            content_json,
            source_message_id_str,
            source_message_id_str,
            provenance.as_str(),
            created_at,
            snapshot_version_i64,
        ],
    )?;

    Ok(PinnedMemoryRecord {
        artifact_id,
        session_id: session_id.clone(),
        content,
        source_message_id: source_message_id.cloned(),
        provenance,
        created_at,
        snapshot_version,
    })
}

#[derive(Debug)]
struct StoredDerivedArtifact {
    artifact_id: String,
    session_id: String,
    kind: String,
    content_json: String,
    source_start_message_id: Option<String>,
    source_end_message_id: Option<String>,
    provenance: String,
    created_at: i64,
    snapshot_version: i64,
}

impl StoredDerivedArtifact {
    fn into_record(self, message_ids: &BTreeSet<String>) -> Result<DerivedArtifactRecord> {
        let kind = DerivedArtifactKind::from_storage_kind(&self.kind)?;

        match kind {
            DerivedArtifactKind::Embedding => {
                let record = EmbeddingRecord::try_from(StoredEmbeddingArtifact {
                    artifact_id: self.artifact_id,
                    session_id: self.session_id,
                    content_json: self.content_json,
                    source_start_message_id: self.source_start_message_id,
                    source_end_message_id: self.source_end_message_id,
                    provenance: self.provenance,
                    created_at: self.created_at,
                    snapshot_version: self.snapshot_version,
                })?;
                let source_start_message_id = record.source_message_id.clone();
                let source_end_message_id = record.source_message_id;

                Ok(DerivedArtifactRecord {
                    artifact_id: record.artifact_id,
                    session_id: record.session_id,
                    kind,
                    provenance: record.provenance,
                    created_at: record.created_at,
                    snapshot_version: record.snapshot_version,
                    source_exists: source_exists(
                        source_start_message_id.as_ref(),
                        source_end_message_id.as_ref(),
                        message_ids,
                    ),
                    source_start_message_id,
                    source_end_message_id,
                    text_preview: None,
                })
            }
            DerivedArtifactKind::ChunkSummary | DerivedArtifactKind::SessionSynopsis => {
                let artifact_id = MemoryArtifactId::parse(self.artifact_id)?;
                let session_id = SessionId::parse(self.session_id.clone())?;
                let provenance = self.provenance.parse()?;
                let snapshot_version = u64::try_from(self.snapshot_version).map_err(|_| {
                    PersistError::InvalidData(format!(
                        "snapshot_version {} is not a valid unsigned integer",
                        self.snapshot_version
                    ))
                })?;
                let source_start_message_id = self
                    .source_start_message_id
                    .map(MessageId::parse)
                    .transpose()?;
                let source_end_message_id = self
                    .source_end_message_id
                    .map(MessageId::parse)
                    .transpose()?;
                let memory_content = serde_json::from_str::<MemoryContent>(&self.content_json)
                    .map_err(|error| {
                        PersistError::InvalidData(format!(
                            "invalid summary artifact JSON for session {}: {error}",
                            self.session_id
                        ))
                    })?;
                let text_preview = match kind {
                    DerivedArtifactKind::ChunkSummary => Some(
                        memory_content
                            .into_chunk_summary()
                            .ok_or_else(|| {
                                PersistError::InvalidData(format!(
                                    "artifact {artifact_id} did not contain chunk summary content"
                                ))
                            })?
                            .text,
                    ),
                    DerivedArtifactKind::SessionSynopsis => Some(
                        memory_content
                            .into_session_synopsis()
                            .ok_or_else(|| {
                                PersistError::InvalidData(format!(
                                    "artifact {artifact_id} did not contain session synopsis content"
                                ))
                            })?
                            .text,
                    ),
                    DerivedArtifactKind::Embedding => unreachable!(),
                };

                Ok(DerivedArtifactRecord {
                    artifact_id,
                    session_id,
                    kind,
                    provenance,
                    created_at: self.created_at,
                    snapshot_version,
                    source_exists: source_exists(
                        source_start_message_id.as_ref(),
                        source_end_message_id.as_ref(),
                        message_ids,
                    ),
                    source_start_message_id,
                    source_end_message_id,
                    text_preview,
                })
            }
        }
    }
}

#[derive(Debug)]
struct StoredPinnedMemoryArtifact {
    artifact_id: String,
    session_id: String,
    content_json: String,
    source_start_message_id: Option<String>,
    source_end_message_id: Option<String>,
    provenance: String,
    created_at: i64,
    snapshot_version: i64,
}

impl TryFrom<StoredPinnedMemoryArtifact> for PinnedMemoryRecord {
    type Error = PersistError;

    fn try_from(value: StoredPinnedMemoryArtifact) -> Result<Self> {
        let artifact_id = MemoryArtifactId::parse(value.artifact_id)?;
        let session_id = SessionId::parse(value.session_id.clone())?;
        let memory_content =
            serde_json::from_str::<MemoryContent>(&value.content_json).map_err(|error| {
                PersistError::InvalidData(format!(
                    "invalid pinned memory JSON for session {}: {error}",
                    value.session_id
                ))
            })?;
        let content = memory_content.into_pinned().ok_or_else(|| {
            PersistError::InvalidData(format!(
                "memory artifact {artifact_id} did not contain pinned memory content"
            ))
        })?;
        let snapshot_version = u64::try_from(value.snapshot_version).map_err(|_| {
            PersistError::InvalidData(format!(
                "snapshot_version {} is not a valid unsigned integer",
                value.snapshot_version
            ))
        })?;
        let source_message_id = match (value.source_start_message_id, value.source_end_message_id) {
            (None, None) => None,
            (Some(start), None) => Some(MessageId::parse(start)?),
            (Some(start), Some(end)) if start == end => Some(MessageId::parse(start)?),
            (Some(start), Some(end)) => {
                return Err(PersistError::InvalidData(format!(
                "pinned memory artifact {artifact_id} has mismatched source range {start}..{end}"
            )))
            }
            (None, Some(end)) => {
                return Err(PersistError::InvalidData(format!(
                    "pinned memory artifact {artifact_id} has dangling source_end_message_id {end}"
                )))
            }
        };

        Ok(PinnedMemoryRecord {
            artifact_id,
            session_id,
            content,
            source_message_id,
            provenance: value.provenance.parse()?,
            created_at: value.created_at,
            snapshot_version,
        })
    }
}

#[derive(Debug)]
struct StoredSummaryArtifact {
    artifact_id: String,
    session_id: String,
    kind: String,
    content_json: String,
    start_message_id: Option<String>,
    end_message_id: Option<String>,
    created_at: i64,
    snapshot_version: i64,
}

impl TryFrom<StoredSummaryArtifact> for SummaryArtifactRecord {
    type Error = PersistError;

    fn try_from(value: StoredSummaryArtifact) -> Result<Self> {
        let artifact_id = MemoryArtifactId::parse(value.artifact_id)?;
        let session_id = SessionId::parse(value.session_id.clone())?;
        let memory_content =
            serde_json::from_str::<MemoryContent>(&value.content_json).map_err(|error| {
                PersistError::InvalidData(format!(
                    "invalid summary artifact JSON for session {}: {error}",
                    value.session_id
                ))
            })?;
        let snapshot_version = u64::try_from(value.snapshot_version).map_err(|_| {
            PersistError::InvalidData(format!(
                "snapshot_version {} is not a valid unsigned integer",
                value.snapshot_version
            ))
        })?;

        let (text, source_count, message_count) = match &value.kind[..] {
            "chunk_summary" => {
                let cs = memory_content.into_chunk_summary().ok_or_else(|| {
                    PersistError::InvalidData(format!(
                        "artifact {artifact_id} did not contain chunk summary content"
                    ))
                })?;
                (cs.text, Some(cs.source_count), None)
            }
            "session_synopsis" => {
                let ss = memory_content.into_session_synopsis().ok_or_else(|| {
                    PersistError::InvalidData(format!(
                        "artifact {artifact_id} did not contain session synopsis content"
                    ))
                })?;
                (ss.text, None, Some(ss.message_count))
            }
            other => {
                return Err(PersistError::InvalidData(format!(
                    "unexpected summary artifact kind: {other}"
                )))
            }
        };

        Ok(SummaryArtifactRecord {
            artifact_id,
            session_id,
            kind: value.kind,
            text,
            source_count,
            message_count,
            start_message_id: value.start_message_id,
            end_message_id: value.end_message_id,
            created_at: value.created_at,
            snapshot_version,
        })
    }
}

const EMBEDDING_ARTIFACT_FORMAT: &str = "ozone-memory.embedding-artifact.v1";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredEmbeddingArtifactContent {
    format: String,
    content: MemoryContent,
    metadata: EmbeddingRecordMetadata,
}

impl StoredEmbeddingArtifactContent {
    fn from_record(record: &EmbeddingRecord) -> Self {
        Self {
            format: EMBEDDING_ARTIFACT_FORMAT.to_owned(),
            content: MemoryContent::from(record.content.clone()),
            metadata: record.metadata.clone(),
        }
    }
}

#[derive(Debug)]
struct StoredEmbeddingArtifact {
    artifact_id: String,
    session_id: String,
    content_json: String,
    source_start_message_id: Option<String>,
    source_end_message_id: Option<String>,
    provenance: String,
    created_at: i64,
    snapshot_version: i64,
}

impl TryFrom<StoredEmbeddingArtifact> for EmbeddingRecord {
    type Error = PersistError;

    fn try_from(value: StoredEmbeddingArtifact) -> Result<Self> {
        let artifact_id = MemoryArtifactId::parse(value.artifact_id)?;
        let session_id = SessionId::parse(value.session_id.clone())?;
        let stored = serde_json::from_str::<StoredEmbeddingArtifactContent>(&value.content_json)
            .map_err(|error| {
                PersistError::InvalidData(format!(
                    "invalid embedding artifact JSON for session {}: {error}",
                    value.session_id
                ))
            })?;
        if stored.format != EMBEDDING_ARTIFACT_FORMAT {
            return Err(PersistError::InvalidData(format!(
                "embedding artifact {artifact_id} used unsupported format `{}`",
                stored.format
            )));
        }
        let content = stored.content.into_embedding().ok_or_else(|| {
            PersistError::InvalidData(format!(
                "memory artifact {artifact_id} did not contain embedding content"
            ))
        })?;
        let snapshot_version = u64::try_from(value.snapshot_version).map_err(|_| {
            PersistError::InvalidData(format!(
                "snapshot_version {} is not a valid unsigned integer",
                value.snapshot_version
            ))
        })?;
        if stored.metadata.dimensions == 0 {
            return Err(PersistError::InvalidData(format!(
                "embedding artifact {artifact_id} stored zero dimensions"
            )));
        }
        if content.vector.len() != stored.metadata.dimensions {
            return Err(PersistError::InvalidData(format!(
                "embedding artifact {artifact_id} dimensions {} did not match vector length {}",
                stored.metadata.dimensions,
                content.vector.len()
            )));
        }
        let source_message_id = match (value.source_start_message_id, value.source_end_message_id) {
            (None, None) => None,
            (Some(start), None) => Some(MessageId::parse(start)?),
            (Some(start), Some(end)) if start == end => Some(MessageId::parse(start)?),
            (Some(start), Some(end)) => {
                return Err(PersistError::InvalidData(format!(
                    "embedding artifact {artifact_id} has mismatched source range {start}..{end}"
                )))
            }
            (None, Some(end)) => {
                return Err(PersistError::InvalidData(format!(
                    "embedding artifact {artifact_id} has dangling source_end_message_id {end}"
                )))
            }
        };

        Ok(EmbeddingRecord {
            artifact_id,
            session_id,
            content,
            source_message_id,
            provenance: value.provenance.parse()?,
            created_at: value.created_at,
            snapshot_version,
            metadata: stored.metadata,
        })
    }
}

fn upsert_embedding_artifact_in_tx(tx: &Transaction<'_>, record: &EmbeddingRecord) -> Result<()> {
    if let Some(source_message_id) = record.source_message_id.as_ref() {
        ensure_message_exists_in_tx(tx, source_message_id, &record.session_id)?;
    }
    let content_json = serde_json::to_string(&StoredEmbeddingArtifactContent::from_record(record))
        .map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize embedding artifact for session {}: {error}",
                record.session_id
            ))
        })?;
    let snapshot_version_i64 = i64::try_from(record.snapshot_version).map_err(|_| {
        PersistError::InvalidData("snapshot_version exceeds SQLite INTEGER".to_owned())
    })?;
    let source_message_id = record.source_message_id.as_ref().map(MessageId::as_str);

    tx.execute(
        "INSERT INTO memory_artifacts (
            artifact_id, session_id, kind, content_json, source_start_message_id, source_end_message_id, provenance, created_at, snapshot_version
         ) VALUES (?1, ?2, 'embedding', ?3, ?4, ?4, ?5, ?6, ?7)
         ON CONFLICT(artifact_id) DO UPDATE SET
            session_id = excluded.session_id,
            kind = excluded.kind,
            content_json = excluded.content_json,
            source_start_message_id = excluded.source_start_message_id,
            source_end_message_id = excluded.source_end_message_id,
            provenance = excluded.provenance,
            created_at = excluded.created_at,
            snapshot_version = excluded.snapshot_version",
        params![
            record.artifact_id.as_str(),
            record.session_id.as_str(),
            content_json,
            source_message_id,
            record.provenance.as_str(),
            record.created_at,
            snapshot_version_i64,
        ],
    )?;

    Ok(())
}

fn group_embedding_records(
    records: &[EmbeddingRecord],
) -> Result<BTreeMap<SessionId, Vec<&EmbeddingRecord>>> {
    let mut grouped = BTreeMap::<SessionId, Vec<&EmbeddingRecord>>::new();
    for record in records {
        grouped
            .entry(record.session_id.clone())
            .or_default()
            .push(record);
    }

    for session_records in grouped.values_mut() {
        session_records.sort_by(|left, right| {
            left.artifact_id
                .as_str()
                .cmp(right.artifact_id.as_str())
                .then_with(|| left.created_at.cmp(&right.created_at))
        });
    }

    Ok(grouped)
}

fn target_embedding_sessions(
    repo: &SqliteRepository,
    session_id: Option<&SessionId>,
    grouped: &BTreeMap<SessionId, Vec<&EmbeddingRecord>>,
) -> Result<Vec<SessionId>> {
    match session_id {
        Some(session_id) => {
            if let Some((unexpected_session_id, _)) = grouped
                .iter()
                .find(|(record_session_id, _)| *record_session_id != session_id)
            {
                return Err(PersistError::ConsistencyError(format!(
                    "embedding artifact {} did not belong to session {}",
                    unexpected_session_id, session_id
                )));
            }
            Ok(vec![session_id.clone()])
        }
        None => {
            let mut sessions = repo
                .list_sessions()?
                .into_iter()
                .map(|session| session.session_id)
                .collect::<Vec<_>>();
            for session_id in grouped.keys() {
                if sessions.iter().all(|existing| existing != session_id) {
                    sessions.push(session_id.clone());
                }
            }
            sessions.sort();
            sessions.dedup();
            Ok(sessions)
        }
    }
}

fn target_artifact_sessions(
    repo: &SqliteRepository,
    session_id: Option<&SessionId>,
) -> Result<Vec<SessionId>> {
    match session_id {
        Some(session_id) => Ok(vec![session_id.clone()]),
        None => {
            let mut sessions = repo
                .list_sessions()?
                .into_iter()
                .map(|session| session.session_id)
                .collect::<Vec<_>>();
            sessions.sort();
            Ok(sessions)
        }
    }
}

fn session_message_counts(
    repo: &SqliteRepository,
    session_id: Option<&SessionId>,
) -> Result<BTreeMap<SessionId, u64>> {
    match session_id {
        Some(session_id) => Ok(BTreeMap::from([(
            session_id.clone(),
            repo.current_message_count(session_id)?,
        )])),
        None => Ok(repo
            .list_sessions()?
            .into_iter()
            .map(|session| (session.session_id, session.message_count))
            .collect()),
    }
}

fn session_message_id_set(conn: &Connection, session_id: &SessionId) -> Result<BTreeSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT message_id
         FROM messages
         WHERE session_id = ?1",
    )?;
    let rows = stmt.query_map([session_id.as_str()], |row| row.get::<_, String>(0))?;

    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect())
}

fn source_exists(
    start_message_id: Option<&MessageId>,
    end_message_id: Option<&MessageId>,
    message_ids: &BTreeSet<String>,
) -> bool {
    start_message_id
        .into_iter()
        .chain(end_message_id)
        .all(|message_id| message_ids.contains(message_id.as_str()))
}

fn mark_gc_reason(
    candidate_reasons: &mut BTreeMap<(SessionId, MemoryArtifactId), Vec<GarbageCollectionReason>>,
    artifact: &DerivedArtifactLifecycleRecord,
    reason: GarbageCollectionReason,
) {
    let reasons = candidate_reasons
        .entry((artifact.session_id.clone(), artifact.artifact_id.clone()))
        .or_default();
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

#[derive(Debug)]
struct StoredCrossSessionSearchHit {
    session_id: String,
    session_name: String,
    character_name: Option<String>,
    tags_json: Option<String>,
    message_id: String,
    author_kind: String,
    content: String,
    created_at: i64,
    bm25_score: f32,
}

impl TryFrom<StoredCrossSessionSearchHit> for CrossSessionSearchHit {
    type Error = PersistError;

    fn try_from(value: StoredCrossSessionSearchHit) -> Result<Self> {
        Ok(CrossSessionSearchHit {
            session: SearchSessionMetadata {
                session_id: SessionId::parse(value.session_id)?,
                session_name: value.session_name,
                character_name: value.character_name,
                tags: parse_tags_json(value.tags_json)?,
            },
            message_id: MessageId::parse(value.message_id)?,
            author_kind: value.author_kind,
            content: value.content,
            created_at: value.created_at,
            bm25_score: value.bm25_score,
        })
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

#[derive(Debug)]
struct StoredSessionSummary {
    session_id: String,
    name: String,
    character_name: Option<String>,
    created_at: i64,
    last_opened_at: i64,
    message_count: i64,
    db_size_bytes: Option<i64>,
    tags_json: Option<String>,
}

impl TryFrom<StoredSessionSummary> for SessionSummary {
    type Error = PersistError;

    fn try_from(value: StoredSessionSummary) -> Result<Self> {
        let session_id = SessionId::parse(value.session_id)?;
        let tags = parse_tags_json(value.tags_json)?;
        let message_count = u64::try_from(value.message_count).map_err(|_| {
            PersistError::InvalidData(format!(
                "message_count {} is not a valid unsigned integer",
                value.message_count
            ))
        })?;
        let db_size_bytes = match value.db_size_bytes {
            Some(size) => Some(u64::try_from(size).map_err(|_| {
                PersistError::InvalidData(format!(
                    "db_size_bytes {size} is not a valid unsigned integer"
                ))
            })?),
            None => None,
        };

        Ok(SessionSummary {
            session_id,
            name: value.name,
            character_name: value.character_name,
            created_at: value.created_at,
            last_opened_at: value.last_opened_at,
            message_count,
            db_size_bytes,
            tags,
        })
    }
}

fn read_stored_session_summary(row: &Row<'_>) -> rusqlite::Result<StoredSessionSummary> {
    Ok(StoredSessionSummary {
        session_id: row.get(0)?,
        name: row.get(1)?,
        character_name: row.get(2)?,
        created_at: row.get(3)?,
        last_opened_at: row.get(4)?,
        message_count: row.get(5)?,
        db_size_bytes: row.get(6)?,
        tags_json: row.get(7)?,
    })
}

fn read_stored_derived_artifact(row: &Row<'_>) -> rusqlite::Result<StoredDerivedArtifact> {
    Ok(StoredDerivedArtifact {
        artifact_id: row.get(0)?,
        session_id: row.get(1)?,
        kind: row.get(2)?,
        content_json: row.get(3)?,
        source_start_message_id: row.get(4)?,
        source_end_message_id: row.get(5)?,
        provenance: row.get(6)?,
        created_at: row.get(7)?,
        snapshot_version: row.get(8)?,
    })
}

fn read_stored_pinned_memory_artifact(
    row: &Row<'_>,
) -> rusqlite::Result<StoredPinnedMemoryArtifact> {
    Ok(StoredPinnedMemoryArtifact {
        artifact_id: row.get(0)?,
        session_id: row.get(1)?,
        content_json: row.get(2)?,
        source_start_message_id: row.get(3)?,
        source_end_message_id: row.get(4)?,
        provenance: row.get(5)?,
        created_at: row.get(6)?,
        snapshot_version: row.get(7)?,
    })
}

fn read_stored_embedding_artifact(row: &Row<'_>) -> rusqlite::Result<StoredEmbeddingArtifact> {
    Ok(StoredEmbeddingArtifact {
        artifact_id: row.get(0)?,
        session_id: row.get(1)?,
        content_json: row.get(2)?,
        source_start_message_id: row.get(3)?,
        source_end_message_id: row.get(4)?,
        provenance: row.get(5)?,
        created_at: row.get(6)?,
        snapshot_version: row.get(7)?,
    })
}

fn read_stored_cross_session_search_hit(
    row: &Row<'_>,
) -> rusqlite::Result<StoredCrossSessionSearchHit> {
    Ok(StoredCrossSessionSearchHit {
        session_id: row.get(0)?,
        session_name: row.get(1)?,
        character_name: row.get(2)?,
        tags_json: row.get(3)?,
        message_id: row.get(4)?,
        author_kind: row.get(5)?,
        content: row.get(6)?,
        created_at: row.get(7)?,
        bm25_score: row.get(8)?,
    })
}

fn read_session_lock(conn: &Connection) -> Result<Option<SessionLock>> {
    conn.query_row(
        "SELECT instance_id, acquired_at, heartbeat_at FROM session_lock WHERE id = 1",
        [],
        |row| {
            Ok(SessionLock {
                instance_id: row.get(0)?,
                acquired_at: row.get(1)?,
                heartbeat_at: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(PersistError::from)
}

fn upsert_session_summary(conn: &Connection, summary: &SessionSummary) -> Result<()> {
    let tags_json = serde_json::to_string(&summary.tags)
        .map_err(|error| PersistError::InvalidData(format!("failed to serialize tags: {error}")))?;

    conn.execute(
        "INSERT INTO sessions (
            session_id, name, character_name, created_at, last_opened_at, message_count, db_size_bytes, tags
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(session_id) DO UPDATE SET
            name = excluded.name,
            character_name = excluded.character_name,
            created_at = excluded.created_at,
            last_opened_at = excluded.last_opened_at,
            message_count = excluded.message_count,
            db_size_bytes = excluded.db_size_bytes,
            tags = excluded.tags",
        params![
            summary.session_id.as_str(),
            summary.name,
            summary.character_name,
            summary.created_at,
            summary.last_opened_at,
            i64::try_from(summary.message_count).map_err(|_| {
                PersistError::InvalidData("message_count exceeds SQLite INTEGER".to_owned())
            })?,
            summary.db_size_bytes.and_then(|value| i64::try_from(value).ok()),
            tags_json
        ],
    )?;

    Ok(())
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

fn get_swipe_group_in_tx(
    tx: &Transaction<'_>,
    swipe_group_id: &SwipeGroupId,
) -> Result<Option<SwipeGroup>> {
    tx.query_row(
        "SELECT swipe_group_id, parent_message_id, parent_context_message_id, active_ordinal
         FROM swipe_groups
         WHERE swipe_group_id = ?1",
        [swipe_group_id.as_str()],
        read_swipe_group,
    )
    .optional()
    .map_err(PersistError::from)
}

fn ensure_swipe_group_belongs_to_session_in_tx(
    tx: &Transaction<'_>,
    group: &SwipeGroup,
    session_id: &SessionId,
) -> Result<()> {
    ensure_message_exists_in_tx(tx, &group.parent_message_id, session_id)?;
    if let Some(parent_context_message_id) = &group.parent_context_message_id {
        ensure_message_exists_in_tx(tx, parent_context_message_id, session_id)?;
    }
    Ok(())
}

fn read_conversation_message(row: &Row<'_>) -> rusqlite::Result<ConversationMessage> {
    let message_id = parse_sqlite_text::<MessageId>(row.get(0)?, 0)?;
    let session_id = SessionId::parse(row.get::<_, String>(1)?)
        .map_err(|error| sqlite_text_parse_error(1, error))?;
    let parent_id = row
        .get::<_, Option<String>>(2)?
        .map(|value| parse_sqlite_text::<MessageId>(value, 2))
        .transpose()?;

    Ok(ConversationMessage {
        message_id,
        session_id,
        parent_id,
        author_kind: row.get(3)?,
        author_name: row.get(4)?,
        content: row.get(5)?,
        created_at: row.get(6)?,
        edited_at: row.get(7)?,
        is_hidden: row.get::<_, i64>(8)? != 0,
    })
}

fn read_message_edit_record(row: &Row<'_>) -> rusqlite::Result<MessageEditRecord> {
    Ok(MessageEditRecord {
        revision_id: row.get(0)?,
        message_id: parse_sqlite_text::<MessageId>(row.get(1)?, 1)?,
        previous_content: row.get(2)?,
        edited_at: row.get(3)?,
    })
}

fn read_bookmark_record(row: &Row<'_>) -> rusqlite::Result<BookmarkRecord> {
    Ok(BookmarkRecord {
        bookmark_id: row.get(0)?,
        message_id: parse_sqlite_text::<MessageId>(row.get(1)?, 1)?,
        note: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn read_branch_record(row: &Row<'_>) -> rusqlite::Result<BranchRecord> {
    let branch_id = parse_sqlite_text::<BranchId>(row.get(0)?, 0)?;
    let session_id = SessionId::parse(row.get::<_, String>(1)?)
        .map_err(|error| sqlite_text_parse_error(1, error))?;
    let tip_message_id = parse_sqlite_text::<MessageId>(row.get(3)?, 3)?;
    let state = row
        .get::<_, String>(5)?
        .parse::<BranchState>()
        .map_err(|error| sqlite_text_parse_error(5, error))?;
    let forked_from = row
        .get::<_, Option<String>>(7)?
        .ok_or_else(|| {
            sqlite_text_parse_error(
                7,
                PersistError::InvalidData("branch is missing forked_from_message_id".to_owned()),
            )
        })
        .and_then(|value| parse_sqlite_text::<MessageId>(value, 7))?;

    Ok(BranchRecord {
        branch: ConversationBranch {
            branch_id,
            session_id,
            name: row.get(2)?,
            tip_message_id,
            created_at: row.get(4)?,
            state,
            description: row.get(6)?,
        },
        forked_from,
    })
}

fn read_swipe_group(row: &Row<'_>) -> rusqlite::Result<SwipeGroup> {
    Ok(SwipeGroup {
        swipe_group_id: parse_sqlite_text::<SwipeGroupId>(row.get(0)?, 0)?,
        parent_message_id: parse_sqlite_text::<MessageId>(row.get(1)?, 1)?,
        parent_context_message_id: row
            .get::<_, Option<String>>(2)?
            .map(|value| parse_sqlite_text::<MessageId>(value, 2))
            .transpose()?,
        active_ordinal: parse_i64_as_u16(row.get(3)?, 3, "active_ordinal")?,
    })
}

fn read_swipe_candidate(row: &Row<'_>) -> rusqlite::Result<SwipeCandidate> {
    let state = row
        .get::<_, String>(3)?
        .parse::<SwipeCandidateState>()
        .map_err(|error| sqlite_text_parse_error(3, error))?;

    Ok(SwipeCandidate {
        swipe_group_id: parse_sqlite_text::<SwipeGroupId>(row.get(0)?, 0)?,
        ordinal: parse_i64_as_u16(row.get(1)?, 1, "ordinal")?,
        message_id: parse_sqlite_text::<MessageId>(row.get(2)?, 2)?,
        state,
        partial_content: row.get(4)?,
        tokens_generated: row
            .get::<_, Option<i64>>(5)?
            .map(|value| parse_i64_as_u64(value, 5, "tokens_generated"))
            .transpose()?,
    })
}

fn parse_sqlite_text<T>(value: String, column_index: usize) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse::<T>()
        .map_err(|error| sqlite_text_parse_error(column_index, error))
}

fn parse_i64_as_u16(value: i64, column_index: usize, field: &'static str) -> rusqlite::Result<u16> {
    u16::try_from(value).map_err(|_| {
        sqlite_integer_parse_error(
            column_index,
            PersistError::InvalidData(format!("{field} {value} is out of range for u16")),
        )
    })
}

fn parse_i64_as_u64(value: i64, column_index: usize, field: &'static str) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| {
        sqlite_integer_parse_error(
            column_index,
            PersistError::InvalidData(format!("{field} {value} is out of range for u64")),
        )
    })
}

fn sqlite_text_parse_error(
    column_index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column_index,
        rusqlite::types::Type::Text,
        Box::new(error),
    )
}

fn sqlite_integer_parse_error(
    column_index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column_index,
        rusqlite::types::Type::Integer,
        Box::new(error),
    )
}

fn open_connection(path: &Path) -> Result<(Connection, bool)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        secure_path(parent, 0o700)?;
    }

    let existed_before_open = path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", 1_i64)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    secure_path(path, 0o600)?;
    Ok((conn, existed_before_open))
}

fn ensure_file_with_contents(path: &Path, contents: &str) -> Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())?;
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }

    secure_path(path, 0o600)?;
    Ok(())
}

fn current_timestamp_ms() -> UnixTimestamp {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));

    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn plain_text_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn generate_uuid_like() -> String {
    let counter = u128::from(ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let pid = u128::from(std::process::id());
    let mut bytes = (nanos ^ (counter << 64) ^ (pid << 32)).to_be_bytes();

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

#[cfg(unix)]
fn secure_path(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn secure_path(path: &Path, _mode: u32) -> Result<()> {
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicI64, AtomicU64, Ordering},
            Arc,
        },
    };

    use ozone_memory::{source_text_hash, AuthorId, EmbeddingProviderKind};
    use rusqlite::Connection;

    use super::*;
    use crate::{migration::backup_path, schema::SESSION_SCHEMA_VERSION};

    static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn schema_creation_builds_session_and_global_databases() {
        let sandbox = TestSandbox::new("schema-creation");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_123);
        let mut request = CreateSessionRequest::new("Schema Session");
        request.character_name = Some("Alice".to_owned());
        request.tags = vec!["alpha".to_owned(), "beta".to_owned()];

        let summary = repo.create_session(request).unwrap();
        let session_db_path = repo.paths().session_db_path(&summary.session_id);
        let global_db_path = repo.paths().global_db_path();

        assert!(session_db_path.exists());
        assert!(repo
            .paths()
            .session_config_path(&summary.session_id)
            .exists());
        assert!(repo
            .paths()
            .session_draft_path(&summary.session_id)
            .exists());
        assert!(global_db_path.exists());
        assert_eq!(summary.created_at, 1_725_647_200_123);
        assert_eq!(summary.last_opened_at, 1_725_647_200_123);

        let session_conn = Connection::open(&session_db_path).unwrap();
        for name in [
            "schema_version",
            "session_lock",
            "messages",
            "message_edits",
            "branches",
            "message_ancestry",
            "swipe_groups",
            "swipe_candidates",
            "memory_artifacts",
            "bookmarks",
            "context_plans",
            "events",
            "messages_fts",
            "artifacts_fts",
        ] {
            assert_sqlite_object(&session_conn, "table", name);
        }
        for name in [
            "messages_fts_insert",
            "messages_fts_update",
            "messages_fts_delete",
            "artifacts_fts_insert",
            "artifacts_fts_update",
            "artifacts_fts_delete",
        ] {
            assert_sqlite_object(&session_conn, "trigger", name);
        }
        let version: i64 = session_conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, i64::from(SESSION_SCHEMA_VERSION));

        let global_conn = Connection::open(&global_db_path).unwrap();
        for name in ["sessions", "session_search", "session_search_fts"] {
            assert_sqlite_object(&global_conn, "table", name);
        }
        for name in [
            "session_search_fts_insert",
            "session_search_fts_update",
            "session_search_fts_delete",
        ] {
            assert_sqlite_object(&global_conn, "trigger", name);
        }
    }

    #[test]
    fn create_list_and_get_session_flow_uses_global_index() {
        let sandbox = TestSandbox::new("create-list-get");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);

        let first = repo
            .create_session(CreateSessionRequest::new("First Session"))
            .unwrap();
        clock.store(1_725_647_260_000, Ordering::SeqCst);
        let mut second_request = CreateSessionRequest::new("Second Session");
        second_request.character_name = Some("Beatrice".to_owned());
        second_request.tags = vec!["story".to_owned()];
        let second = repo.create_session(second_request).unwrap();

        let sessions = repo.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, second.session_id);
        assert_eq!(sessions[1].session_id, first.session_id);
        assert_eq!(sessions[0].character_name.as_deref(), Some("Beatrice"));
        assert_eq!(sessions[0].tags, vec!["story".to_owned()]);
        assert_eq!(sessions[0].created_at, 1_725_647_260_000);

        let fetched = repo.get_session(&second.session_id).unwrap().unwrap();
        assert_eq!(fetched, second);
        assert!(repo
            .get_session(&SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap())
            .unwrap()
            .is_none());
    }

    #[test]
    fn update_session_metadata_rewrites_global_summary_fields() {
        let sandbox = TestSandbox::new("update-session-metadata");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Original Session"))
            .unwrap();

        clock.store(1_725_647_260_000, Ordering::SeqCst);
        let updated = repo
            .update_session_metadata(
                &session.session_id,
                UpdateSessionRequest {
                    name: Some("Renamed Session".to_owned()),
                    character_name: Some(Some("Beatrice".to_owned())),
                    tags: Some(vec!["story".to_owned(), "phase1f".to_owned()]),
                },
            )
            .unwrap();

        assert_eq!(updated.name, "Renamed Session");
        assert_eq!(updated.character_name.as_deref(), Some("Beatrice"));
        assert_eq!(updated.tags, vec!["story".to_owned(), "phase1f".to_owned()]);
        assert_eq!(updated.last_opened_at, 1_725_647_260_000);

        let fetched = repo.get_session(&session.session_id).unwrap().unwrap();
        assert_eq!(fetched, updated);
    }

    #[test]
    fn message_fts_triggers_sync_on_insert() {
        let sandbox = TestSandbox::new("message-fts");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("FTS Session"))
            .unwrap();

        let record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("The nebula fox remembers the hidden orchard"),
            )
            .unwrap();

        let hits = repo.search_messages(&session.session_id, "nebula").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_id, record.message_id);
        assert!(hits[0].content.contains("hidden orchard"));

        let global_conn = Connection::open(repo.paths().global_db_path()).unwrap();
        let count: i64 = global_conn
            .query_row(
                "SELECT COUNT(*)
                 FROM session_search_fts
                 JOIN session_search ON session_search.rowid = session_search_fts.rowid
                 WHERE session_search.session_id = ?1 AND session_search_fts MATCH 'nebula'",
                [session.session_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn pinning_and_unpinning_message_memories_round_trip() {
        let sandbox = TestSandbox::new("pinned-memory-lifecycle");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Pinned Memory Session"))
            .unwrap();
        let message = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Remember the observatory override phrase."),
            )
            .unwrap();
        let message_id = MessageId::parse(message.message_id).unwrap();

        let pinned = repo
            .pin_message_memory(
                &session.session_id,
                &message_id,
                PinMessageMemoryRequest {
                    pinned_by: ozone_memory::AuthorId::User,
                    expires_after_turns: Some(3),
                    provenance: Provenance::UserAuthored,
                },
            )
            .unwrap();

        assert_eq!(pinned.source_message_id, Some(message_id));
        assert_eq!(
            pinned.content.text,
            "Remember the observatory override phrase."
        );
        assert_eq!(pinned.content.expires_after_turns, Some(3));
        assert_eq!(pinned.snapshot_version, 1);

        let listed = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].record.artifact_id, pinned.artifact_id);
        assert_eq!(listed[0].remaining_turns, Some(3));
        assert!(listed[0].is_active);

        let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM memory_artifacts
                 WHERE session_id = ?1 AND kind = 'pinned_memory'",
                [session.session_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        assert!(repo
            .remove_pinned_memory(&session.session_id, &pinned.artifact_id)
            .unwrap());
        assert!(repo
            .list_pinned_memories(&session.session_id)
            .unwrap()
            .is_empty());
        assert!(!repo
            .remove_pinned_memory(&session.session_id, &pinned.artifact_id)
            .unwrap());
    }

    #[test]
    fn freeform_note_memories_persist_without_source_messages() {
        let sandbox = TestSandbox::new("pinned-note-memory");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Pinned Notes"))
            .unwrap();

        let mut request = CreateNoteMemoryRequest::new(
            "Pack the brass lantern before leaving camp.",
            ozone_memory::AuthorId::User,
            Provenance::UserAuthored,
        );
        request.content.expires_after_turns = Some(4);

        let note = repo
            .create_note_memory(&session.session_id, request)
            .unwrap();
        assert_eq!(note.source_message_id, None);
        assert_eq!(note.snapshot_version, 0);

        let listed = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].record.content.text,
            "Pack the brass lantern before leaving camp."
        );
        assert_eq!(listed[0].remaining_turns, Some(4));

        let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
        let stored: (String, String, Option<String>, Option<String>, String, i64) = conn
            .query_row(
                "SELECT kind, content_json, source_start_message_id, source_end_message_id, provenance, snapshot_version
                 FROM memory_artifacts
                 WHERE artifact_id = ?1",
                [note.artifact_id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(stored.0, "pinned_memory");
        assert!(stored.1.contains("\"kind\":\"pinned_memory\""));
        assert!(stored.1.contains("Pack the brass lantern"));
        assert_eq!(stored.2, None);
        assert_eq!(stored.3, None);
        assert_eq!(stored.4, "user_authored");
        assert_eq!(stored.5, 0);
    }

    #[test]
    fn pinned_memory_expiry_tracks_message_count() {
        let sandbox = TestSandbox::new("pinned-memory-expiry");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Expiry Session"))
            .unwrap();
        let seed = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("The comet marker is blue."),
            )
            .unwrap();
        let seed_id = MessageId::parse(seed.message_id).unwrap();

        let pinned = repo
            .pin_message_memory(
                &session.session_id,
                &seed_id,
                PinMessageMemoryRequest {
                    pinned_by: ozone_memory::AuthorId::User,
                    expires_after_turns: Some(2),
                    provenance: Provenance::UserAuthored,
                },
            )
            .unwrap();
        assert_eq!(pinned.snapshot_version, 1);

        let initial = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(initial[0].turns_elapsed, 0);
        assert_eq!(initial[0].remaining_turns, Some(2));
        assert!(initial[0].is_active);

        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("A fresh turn advances the countdown."),
        )
        .unwrap();
        let after_one_turn = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(after_one_turn[0].turns_elapsed, 1);
        assert_eq!(after_one_turn[0].remaining_turns, Some(1));
        assert!(after_one_turn[0].is_active);

        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("The countdown should now expire."),
        )
        .unwrap();
        let expired = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(expired[0].turns_elapsed, 2);
        assert_eq!(expired[0].remaining_turns, Some(0));
        assert!(!expired[0].is_active);
        assert!(expired[0].is_expired());
    }

    #[test]
    fn embedding_artifacts_round_trip_and_replace_per_session() {
        let sandbox = TestSandbox::new("embedding-artifacts");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let first = repo
            .create_session(CreateSessionRequest::new("Embedding Session One"))
            .unwrap();
        let second = repo
            .create_session(CreateSessionRequest::new("Embedding Session Two"))
            .unwrap();
        let first_message = repo
            .insert_message(
                &first.session_id,
                CreateMessageRequest::user("The observatory key is under the lamp."),
            )
            .unwrap();
        let second_message = repo
            .insert_message(
                &second.session_id,
                CreateMessageRequest::user("The gate opens at dusk."),
            )
            .unwrap();

        let first_record = embedding_record(
            "923e4567-e89b-12d3-a456-426614174000",
            &first.session_id,
            Some(&MessageId::parse(first_message.message_id.clone()).unwrap()),
            vec![0.1, 0.2, 0.3],
            "The observatory key is under the lamp.",
            1_725_647_200_000,
            1,
        );
        let second_record = embedding_record(
            "a23e4567-e89b-12d3-a456-426614174000",
            &second.session_id,
            Some(&MessageId::parse(second_message.message_id.clone()).unwrap()),
            vec![0.4, 0.5, 0.6],
            "The gate opens at dusk.",
            1_725_647_200_100,
            1,
        );

        assert_eq!(
            repo.upsert_embedding_artifacts(&[first_record.clone(), second_record.clone()])
                .unwrap(),
            2
        );
        assert_eq!(
            repo.list_embedding_artifacts(Some(&first.session_id))
                .unwrap(),
            vec![first_record.clone()]
        );
        let global = repo.list_embedding_artifacts(None).unwrap();
        assert_eq!(global.len(), 2);
        assert!(global.contains(&first_record));
        assert!(global.contains(&second_record));

        let updated_first = embedding_record(
            "923e4567-e89b-12d3-a456-426614174000",
            &first.session_id,
            Some(&MessageId::parse(first_message.message_id.clone()).unwrap()),
            vec![0.9, 0.0, 0.1],
            "The observatory key moved behind the painting.",
            1_725_647_200_200,
            2,
        );
        repo.upsert_embedding_artifacts(std::slice::from_ref(&updated_first))
            .unwrap();
        assert_eq!(
            repo.list_embedding_artifacts(Some(&first.session_id))
                .unwrap(),
            vec![updated_first.clone()]
        );

        let replacement = embedding_record(
            "b23e4567-e89b-12d3-a456-426614174000",
            &first.session_id,
            None,
            vec![0.0, 1.0, 0.0],
            "Pack the brass lantern before leaving camp.",
            1_725_647_200_300,
            0,
        );
        assert_eq!(
            repo.replace_embedding_artifacts(
                Some(&first.session_id),
                std::slice::from_ref(&replacement),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            repo.list_embedding_artifacts(Some(&first.session_id))
                .unwrap(),
            vec![replacement.clone()]
        );
        let global = repo.list_embedding_artifacts(None).unwrap();
        assert_eq!(global.len(), 2);
        assert!(global.contains(&replacement));
        assert!(global.contains(&second_record));
    }

    #[test]
    fn cross_session_search_returns_session_metadata_and_local_search_still_scopes() {
        let sandbox = TestSandbox::new("cross-session-search");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

        let mut first_request = CreateSessionRequest::new("Observatory Log");
        first_request.character_name = Some("Aster".to_owned());
        first_request.tags = vec!["stellar".to_owned()];
        let first = repo.create_session(first_request).unwrap();

        let mut second_request = CreateSessionRequest::new("Village Log");
        second_request.character_name = Some("Mira".to_owned());
        second_request.tags = vec!["grounded".to_owned(), "phase2a".to_owned()];
        let second = repo.create_session(second_request).unwrap();

        let first_nebula = repo
            .insert_message(
                &first.session_id,
                CreateMessageRequest::user("The nebula gate opens only at dusk."),
            )
            .unwrap();
        repo.insert_message(
            &first.session_id,
            CreateMessageRequest::user("The orchard trail stays quiet tonight."),
        )
        .unwrap();
        let second_nebula = repo
            .insert_message(
                &second.session_id,
                CreateMessageRequest::new("assistant", "Nebula charts point east of the river."),
            )
            .unwrap();

        let local = repo.search_messages(&first.session_id, "nebula").unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].message_id, first_nebula.message_id);

        let hits = repo.search_across_sessions("nebula").unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .all(|hit| hit.content.to_lowercase().contains("nebula")));

        let first_hit = hits
            .iter()
            .find(|hit| hit.message_id.as_str() == first_nebula.message_id)
            .unwrap();
        assert_eq!(first_hit.session.session_id, first.session_id);
        assert_eq!(first_hit.session.session_name, "Observatory Log");
        assert_eq!(first_hit.session.character_name.as_deref(), Some("Aster"));
        assert_eq!(first_hit.session.tags, vec!["stellar".to_owned()]);

        let second_hit = hits
            .iter()
            .find(|hit| hit.message_id.as_str() == second_nebula.message_id)
            .unwrap();
        assert_eq!(second_hit.session.session_id, second.session_id);
        assert_eq!(second_hit.session.session_name, "Village Log");
        assert_eq!(second_hit.session.character_name.as_deref(), Some("Mira"));
        assert_eq!(
            second_hit.session.tags,
            vec!["grounded".to_owned(), "phase2a".to_owned()]
        );
    }

    #[test]
    fn plain_text_search_treats_hyphenated_terms_as_literals() {
        let sandbox = TestSandbox::new("hyphenated-search");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

        let first = repo
            .create_session(CreateSessionRequest::new("Hyphen Search A"))
            .unwrap();
        let second = repo
            .create_session(CreateSessionRequest::new("Hyphen Search B"))
            .unwrap();

        let keyword = "observatory-phase2a-validate";
        let first_message = repo
            .insert_message(
                &first.session_id,
                CreateMessageRequest::user(format!("The keyword is {keyword} in session A.")),
            )
            .unwrap();
        let second_message = repo
            .insert_message(
                &second.session_id,
                CreateMessageRequest::user(format!("Session B also stores {keyword}.")),
            )
            .unwrap();

        let local = repo.search_messages(&first.session_id, keyword).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].message_id, first_message.message_id);

        let global = repo.search_across_sessions(keyword).unwrap();
        assert_eq!(global.len(), 2);
        assert!(global
            .iter()
            .any(|hit| hit.message_id.as_str() == first_message.message_id));
        assert!(global
            .iter()
            .any(|hit| hit.message_id.as_str() == second_message.message_id));
    }

    #[test]
    fn set_message_bookmark_round_trips_and_clears() {
        let sandbox = TestSandbox::new("message-bookmarks");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Bookmark Session"))
            .unwrap();
        let record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("remember this line"),
            )
            .unwrap();
        let message_id = MessageId::parse(&record.message_id).unwrap();

        let bookmark = repo
            .set_message_bookmark(
                &session.session_id,
                &message_id,
                true,
                Some("favorite".to_owned()),
            )
            .unwrap()
            .expect("bookmark should be created");
        assert_eq!(bookmark.note.as_deref(), Some("favorite"));

        let bookmarks = repo.list_bookmarks(&session.session_id).unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].message_id, message_id);
        assert_eq!(bookmarks[0].note.as_deref(), Some("favorite"));

        let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
        let flagged: i64 = conn
            .query_row(
                "SELECT bookmarked FROM messages WHERE message_id = ?1",
                [record.message_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(flagged, 1);

        let removed = repo
            .set_message_bookmark(&session.session_id, &message_id, false, None)
            .unwrap();
        assert_eq!(removed, None);
        assert!(repo.list_bookmarks(&session.session_id).unwrap().is_empty());
    }

    #[test]
    fn advisory_session_lock_acquire_release_and_stale_takeover_work() {
        let sandbox = TestSandbox::new("session-locks");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Lock Session"))
            .unwrap();

        let first_lock = repo
            .acquire_session_lock(&session.session_id, "instance-a")
            .unwrap();
        assert_eq!(first_lock.instance_id, "instance-a");
        assert!(!repo
            .heartbeat_session_lock(&session.session_id, "instance-b")
            .unwrap());

        let error = repo
            .acquire_session_lock(&session.session_id, "instance-b")
            .unwrap_err();
        match error {
            PersistError::SessionLocked {
                instance_id,
                acquired_at,
            } => {
                assert_eq!(instance_id, "instance-a");
                assert_eq!(acquired_at, first_lock.acquired_at);
            }
            other => panic!("expected SessionLocked error, got {other:?}"),
        }

        clock.store(
            first_lock.heartbeat_at + STALE_LOCK_TIMEOUT_MS + 1,
            Ordering::SeqCst,
        );

        let second_lock = repo
            .acquire_session_lock(&session.session_id, "instance-b")
            .unwrap();
        assert_eq!(second_lock.instance_id, "instance-b");
        assert!(!repo
            .release_session_lock(&session.session_id, "instance-a")
            .unwrap());
        assert!(repo
            .release_session_lock(&session.session_id, "instance-b")
            .unwrap());

        let session_conn =
            Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
        let lock_rows: i64 = session_conn
            .query_row("SELECT COUNT(*) FROM session_lock", [], |row| row.get(0))
            .unwrap();
        assert_eq!(lock_rows, 0);
    }

    #[test]
    fn migrating_existing_session_db_creates_backup_before_schema_upgrade() {
        let sandbox = TestSandbox::new("session-backup");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let session_dir = repo.paths().session_dir(&session_id);
        fs::create_dir_all(&session_dir).unwrap();
        let session_db_path = repo.paths().session_db_path(&session_id);
        let legacy_conn = Connection::open(&session_db_path).unwrap();
        legacy_conn
            .execute("CREATE TABLE legacy_notes (value TEXT NOT NULL)", [])
            .unwrap();
        drop(legacy_conn);

        repo.ensure_session_database(&session_id).unwrap();

        assert!(backup_path(&session_db_path, 0).exists());

        let session_conn = Connection::open(&session_db_path).unwrap();
        let version: i64 = session_conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, i64::from(SESSION_SCHEMA_VERSION));
    }

    #[test]
    fn branch_transcripts_follow_closure_paths() {
        let sandbox = TestSandbox::new("branch-transcripts");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Branch Session"))
            .unwrap();

        let root = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Root prompt"),
            )
            .unwrap();
        let root_id = MessageId::parse(root.message_id).unwrap();
        let main_branch_id = branch_id("323e4567-e89b-12d3-a456-426614174000");
        let alt_branch_id = branch_id("423e4567-e89b-12d3-a456-426614174000");

        let mut main_branch = ConversationBranch::new(
            main_branch_id.clone(),
            session.session_id.clone(),
            "main",
            root_id.clone(),
            1_725_647_200_100,
        );
        main_branch.state = BranchState::Active;
        let created_main = repo
            .create_branch(CreateBranchCommand {
                branch: main_branch.clone(),
                forked_from: root_id.clone(),
            })
            .unwrap();
        assert_eq!(created_main.forked_from, root_id);
        assert_eq!(created_main.branch.state, BranchState::Active);

        let assistant_id = message_id("523e4567-e89b-12d3-a456-426614174000");
        let mut assistant = ConversationMessage::new(
            session.session_id.clone(),
            assistant_id.clone(),
            "assistant",
            "Assistant reply",
            1_725_647_200_200,
        );
        assistant.parent_id = Some(root_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: main_branch_id.clone(),
            message: assistant.clone(),
        })
        .unwrap();

        let user_follow_up_id = message_id("623e4567-e89b-12d3-a456-426614174000");
        let mut user_follow_up = ConversationMessage::new(
            session.session_id.clone(),
            user_follow_up_id.clone(),
            "user",
            "User follow-up",
            1_725_647_200_300,
        );
        user_follow_up.parent_id = Some(assistant_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: main_branch_id.clone(),
            message: user_follow_up.clone(),
        })
        .unwrap();

        let alt_branch = ConversationBranch::new(
            alt_branch_id.clone(),
            session.session_id.clone(),
            "alternate",
            assistant_id.clone(),
            1_725_647_200_350,
        );
        let created_alt = repo
            .create_branch(CreateBranchCommand {
                branch: alt_branch,
                forked_from: assistant_id.clone(),
            })
            .unwrap();
        assert_eq!(created_alt.forked_from, assistant_id);
        assert_eq!(created_alt.branch.state, BranchState::Inactive);

        let alt_assistant_id = message_id("723e4567-e89b-12d3-a456-426614174000");
        let mut alt_assistant = ConversationMessage::new(
            session.session_id.clone(),
            alt_assistant_id.clone(),
            "assistant",
            "Alternate assistant reply",
            1_725_647_200_400,
        );
        alt_assistant.parent_id = Some(assistant_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: alt_branch_id.clone(),
            message: alt_assistant.clone(),
        })
        .unwrap();

        let main_ids = transcript_ids(
            &repo
                .list_branch_messages(&session.session_id, &main_branch_id)
                .unwrap(),
        );
        assert_eq!(
            main_ids,
            vec![
                root_id.clone(),
                assistant_id.clone(),
                user_follow_up_id.clone()
            ]
        );

        let alt_ids = transcript_ids(
            &repo
                .list_branch_messages(&session.session_id, &alt_branch_id)
                .unwrap(),
        );
        assert_eq!(
            alt_ids,
            vec![
                root_id.clone(),
                assistant_id.clone(),
                alt_assistant_id.clone()
            ]
        );

        let active_ids = transcript_ids(
            &repo
                .get_active_branch_transcript(&session.session_id)
                .unwrap(),
        );
        assert_eq!(active_ids, main_ids);

        let session_conn =
            Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
        assert_ancestry_depth(&session_conn, &root_id, &root_id, 0);
        assert_ancestry_depth(&session_conn, &assistant_id, &assistant_id, 0);
        assert_ancestry_depth(&session_conn, &root_id, &assistant_id, 1);
        assert_ancestry_depth(&session_conn, &root_id, &user_follow_up_id, 2);
        assert_ancestry_depth(&session_conn, &assistant_id, &user_follow_up_id, 1);
        assert_ancestry_depth(&session_conn, &assistant_id, &alt_assistant_id, 1);
    }

    #[test]
    fn editing_messages_records_history_and_refreshes_search() {
        let sandbox = TestSandbox::new("message-edits");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Edit Session"))
            .unwrap();

        let record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("The comet is red"),
            )
            .unwrap();
        let message_id = MessageId::parse(record.message_id).unwrap();
        clock.store(1_725_647_260_000, Ordering::SeqCst);

        let edited = repo
            .edit_message(
                &session.session_id,
                &message_id,
                EditMessageRequest::new("The comet is blue"),
            )
            .unwrap();
        assert_eq!(edited.edited_at, Some(1_725_647_260_000));
        assert_eq!(edited.content, "The comet is blue");

        let history = repo
            .list_message_edits(&session.session_id, &message_id)
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].message_id, message_id);
        assert_eq!(history[0].previous_content, "The comet is red");
        assert_eq!(history[0].edited_at, 1_725_647_260_000);

        let new_hits = repo.search_messages(&session.session_id, "blue").unwrap();
        assert_eq!(new_hits.len(), 1);
        assert_eq!(new_hits[0].message_id, message_id.to_string());
        assert!(repo
            .search_messages(&session.session_id, "red")
            .unwrap()
            .is_empty());

        let global_conn = Connection::open(repo.paths().global_db_path()).unwrap();
        let stored_content: String = global_conn
            .query_row(
                "SELECT content FROM session_search WHERE session_id = ?1 AND message_id = ?2",
                params![session.session_id.as_str(), message_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_content, "The comet is blue");
    }

    #[test]
    fn activating_a_branch_keeps_one_active_branch_per_session() {
        let sandbox = TestSandbox::new("branch-activation");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Activation Session"))
            .unwrap();
        let root = repo
            .insert_message(&session.session_id, CreateMessageRequest::user("Root"))
            .unwrap();
        let root_id = MessageId::parse(root.message_id).unwrap();

        let first_branch_id = branch_id("823e4567-e89b-12d3-a456-426614174000");
        let second_branch_id = branch_id("923e4567-e89b-12d3-a456-426614174000");

        let mut first_branch = ConversationBranch::new(
            first_branch_id.clone(),
            session.session_id.clone(),
            "first",
            root_id.clone(),
            1_725_647_200_100,
        );
        first_branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch: first_branch,
            forked_from: root_id.clone(),
        })
        .unwrap();

        let second_branch = ConversationBranch::new(
            second_branch_id.clone(),
            session.session_id.clone(),
            "second",
            root_id.clone(),
            1_725_647_200_200,
        );
        repo.create_branch(CreateBranchCommand {
            branch: second_branch,
            forked_from: root_id.clone(),
        })
        .unwrap();

        assert_eq!(
            repo.get_active_branch(&session.session_id)
                .unwrap()
                .unwrap()
                .branch
                .branch_id,
            first_branch_id
        );

        let activated = repo
            .activate_branch(&session.session_id, &second_branch_id)
            .unwrap();
        assert_eq!(activated.branch.branch_id, second_branch_id);
        assert_eq!(activated.branch.state, BranchState::Active);

        let branches = repo.list_branches(&session.session_id).unwrap();
        let active_ids: Vec<_> = branches
            .iter()
            .filter(|branch| branch.branch.state == BranchState::Active)
            .map(|branch| branch.branch.branch_id.clone())
            .collect();
        assert_eq!(active_ids, vec![second_branch_id.clone()]);

        let first = branches
            .iter()
            .find(|branch| branch.branch.branch_id == first_branch_id)
            .unwrap();
        assert_eq!(first.branch.state, BranchState::Inactive);
    }

    #[test]
    fn swipe_candidates_preserve_order_and_activation_state() {
        let sandbox = TestSandbox::new("swipe-state");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Swipe Session"))
            .unwrap();
        let root = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Tell me a story"),
            )
            .unwrap();
        let root_id = MessageId::parse(root.message_id).unwrap();
        let branch_id = branch_id("a23e4567-e89b-12d3-a456-426614174000");

        let mut branch = ConversationBranch::new(
            branch_id.clone(),
            session.session_id.clone(),
            "main",
            root_id.clone(),
            1_725_647_200_050,
        );
        branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch,
            forked_from: root_id.clone(),
        })
        .unwrap();

        let visible_candidate_id = message_id("b23e4567-e89b-12d3-a456-426614174000");
        let mut visible_candidate = ConversationMessage::new(
            session.session_id.clone(),
            visible_candidate_id.clone(),
            "assistant",
            "Version one",
            1_725_647_200_100,
        );
        visible_candidate.parent_id = Some(root_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: branch_id.clone(),
            message: visible_candidate.clone(),
        })
        .unwrap();

        let swipe_group_id = swipe_group_id("c23e4567-e89b-12d3-a456-426614174000");
        let mut group = SwipeGroup::new(swipe_group_id.clone(), visible_candidate_id.clone());
        group.parent_context_message_id = Some(root_id.clone());
        group.active_ordinal = 0;
        repo.record_swipe_candidate(
            &session.session_id,
            RecordSwipeCandidateCommand {
                group: group.clone(),
                candidate: SwipeCandidate::new(swipe_group_id.clone(), 0, visible_candidate_id),
            },
        )
        .unwrap();

        let alternate_record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest {
                    parent_id: Some(root_id.to_string()),
                    author_kind: "assistant".to_owned(),
                    author_name: None,
                    content: "Version two".to_owned(),
                },
            )
            .unwrap();
        let alternate_id = MessageId::parse(alternate_record.message_id).unwrap();
        repo.record_swipe_candidate(
            &session.session_id,
            RecordSwipeCandidateCommand {
                group: group.clone(),
                candidate: SwipeCandidate::new(swipe_group_id.clone(), 1, alternate_id.clone()),
            },
        )
        .unwrap();

        let partial_record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest {
                    parent_id: Some(root_id.to_string()),
                    author_kind: "assistant".to_owned(),
                    author_name: None,
                    content: "Version three (partial)".to_owned(),
                },
            )
            .unwrap();
        let partial_id = MessageId::parse(partial_record.message_id).unwrap();
        let mut partial_candidate = SwipeCandidate::new(swipe_group_id.clone(), 2, partial_id);
        partial_candidate.state = SwipeCandidateState::FailedMidStream;
        partial_candidate.partial_content = Some("Version three".to_owned());
        partial_candidate.tokens_generated = Some(17);
        repo.record_swipe_candidate(
            &session.session_id,
            RecordSwipeCandidateCommand {
                group: group.clone(),
                candidate: partial_candidate.clone(),
            },
        )
        .unwrap();

        let groups = repo.list_swipe_groups(&session.session_id).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].swipe_group_id, swipe_group_id);
        assert_eq!(groups[0].parent_context_message_id, Some(root_id.clone()));
        assert_eq!(groups[0].active_ordinal, 0);

        let candidates = repo
            .list_swipe_candidates(&session.session_id, &swipe_group_id)
            .unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(candidates[1].message_id, alternate_id);
        assert_eq!(candidates[2].state, SwipeCandidateState::FailedMidStream);
        assert_eq!(
            candidates[2].partial_content.as_deref(),
            Some("Version three")
        );
        assert_eq!(candidates[2].tokens_generated, Some(17));

        let activated = repo
            .activate_swipe_candidate(
                &session.session_id,
                ActivateSwipeCommand {
                    swipe_group_id: swipe_group_id.clone(),
                    ordinal: 1,
                },
            )
            .unwrap();
        assert_eq!(activated.active_ordinal, 1);
        assert_eq!(
            repo.get_swipe_group(&session.session_id, &swipe_group_id)
                .unwrap()
                .unwrap()
                .active_ordinal,
            1
        );
        assert_eq!(
            repo.list_swipe_candidates(&session.session_id, &swipe_group_id)
                .unwrap()
                .iter()
                .map(|candidate| candidate.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn importing_character_card_stores_artifact_and_seeds_greeting() {
        let sandbox = TestSandbox::new("character-card-import");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

        let imported = repo
            .import_character_card(crate::ImportCharacterCardRequest {
                card: crate::CharacterCard::from_json_str(
                    r#"{
                        "name": "Aster",
                        "description": "A patient observatory guide.",
                        "first_mes": "Welcome back to the observatory.",
                        "tags": ["stellar"]
                    }"#,
                )
                .unwrap(),
                session_name: Some("Aster Intake".to_owned()),
                tags: vec!["phase1f".to_owned()],
                provenance: "tests/cards/aster.json".to_owned(),
            })
            .unwrap();

        assert_eq!(imported.session.name, "Aster Intake");
        assert_eq!(imported.session.character_name.as_deref(), Some("Aster"));
        assert_eq!(
            imported.session.tags,
            vec!["stellar".to_owned(), "phase1f".to_owned()]
        );
        assert!(imported.seeded_branch_id.is_some());
        assert!(imported.seeded_message_id.is_some());

        let stored = repo
            .get_character_card(&imported.session.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.provenance, "tests/cards/aster.json");
        assert_eq!(stored.card.name, "Aster");

        let transcript = repo
            .get_active_branch_transcript(&imported.session.session_id)
            .unwrap();
        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript[0].author_kind, "assistant");
        assert_eq!(transcript[0].author_name.as_deref(), Some("Aster"));
        assert_eq!(transcript[0].content, "Welcome back to the observatory.");
    }

    #[test]
    fn session_export_includes_character_cards_bookmarks_and_swipes() {
        let sandbox = TestSandbox::new("session-export");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

        let imported = repo
            .import_character_card(crate::ImportCharacterCardRequest {
                card: crate::CharacterCard::from_json_str(
                    r#"{
                        "name": "Aster",
                        "description": "A patient observatory guide.",
                        "first_mes": "Welcome back to the observatory."
                    }"#,
                )
                .unwrap(),
                session_name: None,
                tags: vec!["phase1f".to_owned()],
                provenance: "tests/cards/aster.json".to_owned(),
            })
            .unwrap();
        let session_id = imported.session.session_id.clone();
        let greeting_id = imported.seeded_message_id.clone().unwrap();

        repo.set_message_bookmark(&session_id, &greeting_id, true, Some("opening".to_owned()))
            .unwrap();

        let swipe_group_id = swipe_group_id("d23e4567-e89b-12d3-a456-426614174000");
        repo.record_swipe_candidate(
            &session_id,
            RecordSwipeCandidateCommand {
                group: SwipeGroup::new(swipe_group_id.clone(), greeting_id.clone()),
                candidate: SwipeCandidate::new(swipe_group_id.clone(), 0, greeting_id.clone()),
            },
        )
        .unwrap();
        let alternate = repo
            .insert_message(
                &session_id,
                CreateMessageRequest {
                    parent_id: None,
                    author_kind: "assistant".to_owned(),
                    author_name: Some("Aster".to_owned()),
                    content: "The stars have shifted since your last visit.".to_owned(),
                },
            )
            .unwrap();
        repo.record_swipe_candidate(
            &session_id,
            RecordSwipeCandidateCommand {
                group: SwipeGroup::new(swipe_group_id.clone(), greeting_id.clone()),
                candidate: SwipeCandidate::new(
                    swipe_group_id.clone(),
                    1,
                    MessageId::parse(alternate.message_id).unwrap(),
                ),
            },
        )
        .unwrap();

        let export = repo.export_session(&session_id).unwrap();
        let json = export.to_pretty_json().unwrap();

        assert_eq!(export.format, SESSION_EXPORT_FORMAT);
        assert_eq!(export.session.session_id, session_id.to_string());
        assert_eq!(export.character_card.as_ref().unwrap().card.name, "Aster");
        assert_eq!(export.branches.len(), 1);
        assert_eq!(
            export.branches[0].transcript_message_ids,
            vec![greeting_id.to_string()]
        );
        assert_eq!(export.messages.len(), 2);
        assert_eq!(export.bookmarks.len(), 1);
        assert_eq!(export.swipe_groups.len(), 1);
        assert_eq!(export.swipe_groups[0].candidates.len(), 2);
        assert!(json.contains("\"format\": \"ozone-plus.session-export.v1\""));
    }

    #[test]
    fn transcript_export_preserves_branch_message_order() {
        let sandbox = TestSandbox::new("transcript-export");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Transcript Export Session"))
            .unwrap();
        let root = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Root prompt"),
            )
            .unwrap();
        let root_id = MessageId::parse(root.message_id).unwrap();
        let branch_id = branch_id("e23e4567-e89b-12d3-a456-426614174000");
        let mut branch = ConversationBranch::new(
            branch_id.clone(),
            session.session_id.clone(),
            "main",
            root_id.clone(),
            1_725_647_200_050,
        );
        branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch,
            forked_from: root_id.clone(),
        })
        .unwrap();

        let assistant_id = message_id("f23e4567-e89b-12d3-a456-426614174000");
        let mut assistant = ConversationMessage::new(
            session.session_id.clone(),
            assistant_id.clone(),
            "assistant",
            "Assistant reply",
            1_725_647_200_100,
        );
        assistant.parent_id = Some(root_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: branch_id.clone(),
            message: assistant,
        })
        .unwrap();

        let user_follow_up_id = message_id("123e4567-e89b-42d3-a456-426614174000");
        let mut user_follow_up = ConversationMessage::new(
            session.session_id.clone(),
            user_follow_up_id.clone(),
            "user",
            "User follow-up",
            1_725_647_200_150,
        );
        user_follow_up.parent_id = Some(assistant_id.clone());
        repo.commit_message(CommitMessageCommand {
            branch_id: branch_id.clone(),
            message: user_follow_up,
        })
        .unwrap();

        let export = repo
            .export_transcript(&session.session_id, Some(&branch_id))
            .unwrap();
        let ids = export
            .messages
            .iter()
            .map(|message| message.message_id.clone())
            .collect::<Vec<_>>();

        assert_eq!(export.format, TRANSCRIPT_EXPORT_FORMAT);
        assert_eq!(
            export.branch.as_ref().unwrap().branch_id,
            branch_id.to_string()
        );
        assert_eq!(
            ids,
            vec![
                root_id.to_string(),
                assistant_id.to_string(),
                user_follow_up_id.to_string()
            ]
        );
    }

    fn assert_sqlite_object(conn: &Connection, kind: &str, name: &str) {
        let exists: i64 = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM sqlite_master
                    WHERE type = ?1 AND name = ?2
                )",
                params![kind, name],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "missing {kind} {name}");
    }

    fn assert_ancestry_depth(
        conn: &Connection,
        ancestor_id: &MessageId,
        descendant_id: &MessageId,
        expected_depth: i64,
    ) {
        let depth: i64 = conn
            .query_row(
                "SELECT depth
                 FROM message_ancestry
                 WHERE ancestor_id = ?1 AND descendant_id = ?2",
                params![ancestor_id.as_str(), descendant_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(depth, expected_depth);
    }

    fn transcript_ids(messages: &[ConversationMessage]) -> Vec<MessageId> {
        messages
            .iter()
            .map(|message| message.message_id.clone())
            .collect()
    }

    fn message_id(value: &str) -> MessageId {
        MessageId::parse(value).unwrap()
    }

    fn branch_id(value: &str) -> BranchId {
        BranchId::parse(value).unwrap()
    }

    fn swipe_group_id(value: &str) -> SwipeGroupId {
        SwipeGroupId::parse(value).unwrap()
    }

    fn embedding_record(
        artifact_id: &str,
        session_id: &SessionId,
        source_message_id: Option<&MessageId>,
        vector: Vec<f32>,
        text: &str,
        created_at: i64,
        snapshot_version: u64,
    ) -> EmbeddingRecord {
        EmbeddingRecord {
            artifact_id: MemoryArtifactId::parse(artifact_id).unwrap(),
            session_id: session_id.clone(),
            content: ozone_memory::EmbeddingContent::new(vector, source_text_hash(text)),
            source_message_id: source_message_id.cloned(),
            provenance: Provenance::UserAuthored,
            created_at,
            snapshot_version,
            metadata: EmbeddingRecordMetadata {
                provider: EmbeddingProviderKind::Mock,
                model: "mock/stable".to_owned(),
                dimensions: 3,
            },
        }
    }

    fn test_repo(sandbox: &TestSandbox, initial_time: i64) -> (SqliteRepository, Arc<AtomicI64>) {
        let clock = Arc::new(AtomicI64::new(initial_time));
        let clock_for_repo = Arc::clone(&clock);
        let repo = SqliteRepository::with_clock(
            PersistencePaths::from_data_dir(sandbox.data_dir()),
            Arc::new(move || clock_for_repo.load(Ordering::SeqCst)),
        );
        (repo, clock)
    }

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ozone-persist-tests")
                .join(format!(
                    "{prefix}-{}-{}",
                    std::process::id(),
                    SANDBOX_COUNTER.fetch_add(1, Ordering::Relaxed)
                ));

            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }

            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn data_dir(&self) -> PathBuf {
            self.root.join("data")
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn store_and_list_chunk_summaries() {
        let sandbox = TestSandbox::new("chunk-summaries");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Summary Session"))
            .unwrap();

        let msg1 = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("First message"),
            )
            .unwrap();
        let msg2 = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Second message"),
            )
            .unwrap();
        let msg3 = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Third message"),
            )
            .unwrap();

        let start1 = message_id(&msg1.message_id);
        let end1 = message_id(&msg2.message_id);
        let start2 = message_id(&msg2.message_id);
        let end2 = message_id(&msg3.message_id);

        let first = repo
            .store_chunk_summary(
                &session.session_id,
                "Alice greeted Bob.",
                2,
                &start1,
                &end1,
                1,
            )
            .unwrap();
        assert_eq!(first.kind, "chunk_summary");
        assert_eq!(first.text, "Alice greeted Bob.");
        assert_eq!(first.source_count, Some(2));
        assert_eq!(first.message_count, None);

        clock.store(1_725_647_201_000, Ordering::SeqCst);
        let second = repo
            .store_chunk_summary(
                &session.session_id,
                "They discussed the plan.",
                3,
                &start2,
                &end2,
                3,
            )
            .unwrap();

        let summaries = repo.list_chunk_summaries(&session.session_id).unwrap();
        assert_eq!(summaries.len(), 2);
        // Ordered by created_at DESC — second is first
        assert_eq!(summaries[0].artifact_id, second.artifact_id);
        assert_eq!(summaries[0].text, "They discussed the plan.");
        assert_eq!(summaries[0].source_count, Some(3));
        assert_eq!(
            summaries[0].start_message_id.as_deref(),
            Some(start2.as_str())
        );
        assert_eq!(summaries[0].end_message_id.as_deref(), Some(end2.as_str()));

        assert_eq!(summaries[1].artifact_id, first.artifact_id);
        assert_eq!(summaries[1].text, "Alice greeted Bob.");
        assert_eq!(summaries[1].source_count, Some(2));
    }

    #[test]
    fn store_and_get_session_synopsis() {
        let sandbox = TestSandbox::new("session-synopsis");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Synopsis Session"))
            .unwrap();

        assert!(repo
            .get_latest_session_synopsis(&session.session_id)
            .unwrap()
            .is_none());

        let first = repo
            .store_session_synopsis(
                &session.session_id,
                "A roleplay about forest exploration.",
                10,
                5,
            )
            .unwrap();
        assert_eq!(first.kind, "session_synopsis");
        assert_eq!(first.text, "A roleplay about forest exploration.");
        assert_eq!(first.message_count, Some(10));
        assert_eq!(first.source_count, None);

        clock.store(1_725_647_201_000, Ordering::SeqCst);
        let second = repo
            .store_session_synopsis(
                &session.session_id,
                "Updated synopsis with more detail.",
                20,
                10,
            )
            .unwrap();

        let latest = repo
            .get_latest_session_synopsis(&session.session_id)
            .unwrap()
            .expect("should find a synopsis");
        assert_eq!(latest.artifact_id, second.artifact_id);
        assert_eq!(latest.text, "Updated synopsis with more detail.");
        assert_eq!(latest.message_count, Some(20));
        assert_eq!(latest.snapshot_version, 10);
    }

    #[test]
    fn delete_summary_artifact_removes_only_summaries() {
        let sandbox = TestSandbox::new("delete-summary");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Delete Summary"))
            .unwrap();

        let msg = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Some message"),
            )
            .unwrap();
        let msg_id = message_id(&msg.message_id);

        let chunk = repo
            .store_chunk_summary(&session.session_id, "A chunk.", 1, &msg_id, &msg_id, 1)
            .unwrap();
        let synopsis = repo
            .store_session_synopsis(&session.session_id, "A synopsis.", 5, 2)
            .unwrap();

        assert!(repo
            .delete_summary_artifact(&session.session_id, &chunk.artifact_id)
            .unwrap());
        // Deleting again returns false
        assert!(!repo
            .delete_summary_artifact(&session.session_id, &chunk.artifact_id)
            .unwrap());

        // Chunk is gone
        let remaining = repo.list_chunk_summaries(&session.session_id).unwrap();
        assert!(remaining.is_empty());

        // Synopsis still there
        let latest = repo
            .get_latest_session_synopsis(&session.session_id)
            .unwrap();
        assert!(latest.is_some());

        // Delete synopsis
        assert!(repo
            .delete_summary_artifact(&session.session_id, &synopsis.artifact_id)
            .unwrap());
        assert!(repo
            .get_latest_session_synopsis(&session.session_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn derived_artifact_inventory_reports_source_existence_and_previews() {
        let sandbox = TestSandbox::new("derived-artifact-inventory");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Derived Inventory"))
            .unwrap();

        let first = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("The lantern stays by the observatory door."),
            )
            .unwrap();
        let second = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Bring the spare key before dusk."),
            )
            .unwrap();
        let first_message_id = message_id(&first.message_id);
        let second_message_id = message_id(&second.message_id);
        let missing_message_id = message_id("c23e4567-e89b-12d3-a456-426614174000");

        let embedding = embedding_record(
            "c33e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            Some(&first_message_id),
            vec![0.1, 0.2, 0.3],
            "The lantern stays by the observatory door.",
            1_725_647_200_000,
            1,
        );
        repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
            .unwrap();

        clock.store(1_725_647_200_010, Ordering::SeqCst);
        let chunk = repo
            .store_chunk_summary(
                &session.session_id,
                "Lantern location confirmed.",
                2,
                &first_message_id,
                &second_message_id,
                2,
            )
            .unwrap();

        clock.store(1_725_647_200_020, Ordering::SeqCst);
        let synopsis = repo
            .store_session_synopsis(
                &session.session_id,
                "A short exchange about lantern placement.",
                2,
                2,
            )
            .unwrap();

        let conn = repo.open_session_connection(&session.session_id).unwrap();
        conn.execute(
            "UPDATE memory_artifacts
             SET source_end_message_id = ?2
             WHERE artifact_id = ?1",
            params![chunk.artifact_id.as_str(), missing_message_id.as_str()],
        )
        .unwrap();

        let artifacts = repo
            .list_derived_artifacts(Some(&session.session_id))
            .unwrap();
        assert_eq!(artifacts.len(), 3);

        assert_eq!(artifacts[0].artifact_id, embedding.artifact_id);
        assert_eq!(artifacts[0].kind, DerivedArtifactKind::Embedding);
        assert_eq!(
            artifacts[0].source_start_message_id.as_ref(),
            Some(&first_message_id)
        );
        assert_eq!(
            artifacts[0].source_end_message_id.as_ref(),
            Some(&first_message_id)
        );
        assert!(artifacts[0].source_exists);
        assert_eq!(artifacts[0].text_preview, None);

        assert_eq!(artifacts[1].artifact_id, chunk.artifact_id);
        assert_eq!(artifacts[1].kind, DerivedArtifactKind::ChunkSummary);
        assert_eq!(
            artifacts[1].text_preview.as_deref(),
            Some("Lantern location confirmed.")
        );
        assert_eq!(
            artifacts[1].source_start_message_id.as_ref(),
            Some(&first_message_id)
        );
        assert_eq!(
            artifacts[1].source_end_message_id.as_ref(),
            Some(&missing_message_id)
        );
        assert!(!artifacts[1].source_exists);

        assert_eq!(artifacts[2].artifact_id, synopsis.artifact_id);
        assert_eq!(artifacts[2].kind, DerivedArtifactKind::SessionSynopsis);
        assert_eq!(
            artifacts[2].text_preview.as_deref(),
            Some("A short exchange about lantern placement.")
        );
        assert!(artifacts[2].source_exists);
        assert!(artifacts[2].source_start_message_id.is_none());
        assert!(artifacts[2].source_end_message_id.is_none());
    }

    #[test]
    fn minimal_tier_plan_marks_embeddings_chunks_and_superseded_synopses() {
        let sandbox = TestSandbox::new("gc-minimal-tier");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Minimal Tier Session"))
            .unwrap();

        let messages = (0..5)
            .map(|index| {
                repo.insert_message(
                    &session.session_id,
                    CreateMessageRequest::user(format!("Message #{index}")),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let first_message_id = message_id(&messages[0].message_id);
        let second_message_id = message_id(&messages[1].message_id);

        let embedding = embedding_record(
            "d33e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            Some(&first_message_id),
            vec![0.2, 0.3, 0.4],
            "Message #0",
            1_725_647_200_000,
            1,
        );
        repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
            .unwrap();

        clock.store(1_725_647_200_010, Ordering::SeqCst);
        let chunk = repo
            .store_chunk_summary(
                &session.session_id,
                "Messages summarized.",
                2,
                &first_message_id,
                &second_message_id,
                1,
            )
            .unwrap();

        clock.store(1_725_647_200_020, Ordering::SeqCst);
        let old_synopsis = repo
            .store_session_synopsis(&session.session_id, "Older synopsis.", 2, 1)
            .unwrap();

        clock.store(1_725_647_200_030, Ordering::SeqCst);
        let new_synopsis = repo
            .store_session_synopsis(&session.session_id, "Newest synopsis.", 5, 4)
            .unwrap();

        let plan = repo
            .plan_garbage_collection(
                Some(&session.session_id),
                &StorageTierPolicy::new(1, 2),
                500,
                168,
                &GarbageCollectionPolicy::new(10, false),
            )
            .unwrap();
        let candidates = candidate_reasons_by_artifact(&plan);

        assert_eq!(plan.inspected_count, 4);
        assert_eq!(plan.candidate_count, 3);
        assert_eq!(
            plan.reason_counts
                .get(&GarbageCollectionReason::MinimalTier),
            Some(&2)
        );
        assert_eq!(
            plan.reason_counts
                .get(&GarbageCollectionReason::SupersededSynopsis),
            Some(&1)
        );
        assert_eq!(
            candidates.get(&(session.session_id.clone(), embedding.artifact_id.clone())),
            Some(&vec![GarbageCollectionReason::MinimalTier])
        );
        assert_eq!(
            candidates.get(&(session.session_id.clone(), chunk.artifact_id.clone())),
            Some(&vec![GarbageCollectionReason::MinimalTier])
        );
        assert_eq!(
            candidates.get(&(session.session_id.clone(), old_synopsis.artifact_id.clone())),
            Some(&vec![GarbageCollectionReason::SupersededSynopsis])
        );
        assert!(!candidates.contains_key(&(session.session_id, new_synopsis.artifact_id)));
    }

    #[test]
    fn orphaned_source_cleanup_marks_candidates_when_enabled() {
        let sandbox = TestSandbox::new("gc-orphaned-source");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Orphan Cleanup"))
            .unwrap();
        let message = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("Archive the observatory route."),
            )
            .unwrap();
        let source_message_id = message_id(&message.message_id);
        let missing_message_id = message_id("e33e4567-e89b-12d3-a456-426614174000");

        let embedding = embedding_record(
            "f33e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            Some(&source_message_id),
            vec![0.3, 0.2, 0.1],
            "Archive the observatory route.",
            1_725_647_200_000,
            1,
        );
        repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
            .unwrap();

        let conn = repo.open_session_connection(&session.session_id).unwrap();
        conn.execute(
            "UPDATE memory_artifacts
             SET source_start_message_id = ?2, source_end_message_id = ?3
             WHERE artifact_id = ?1",
            params![
                embedding.artifact_id.as_str(),
                missing_message_id.as_str(),
                missing_message_id.as_str()
            ],
        )
        .unwrap();

        let plan = repo
            .plan_garbage_collection(
                Some(&session.session_id),
                &StorageTierPolicy::new(100, 1_000),
                500,
                168,
                &GarbageCollectionPolicy::new(10, true),
            )
            .unwrap();

        assert_eq!(plan.candidate_count, 1);
        assert_eq!(
            plan.reason_counts
                .get(&GarbageCollectionReason::OrphanedSource),
            Some(&1)
        );
        assert_eq!(
            plan.candidates[0].artifact.artifact_id,
            embedding.artifact_id
        );
        assert_eq!(
            plan.candidates[0].reasons,
            vec![GarbageCollectionReason::OrphanedSource]
        );
        assert!(!plan.candidates[0].artifact.source_exists);
    }

    #[test]
    fn embedding_cap_marks_oldest_remaining_embeddings() {
        let sandbox = TestSandbox::new("gc-embedding-cap");
        let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Embedding Cap"))
            .unwrap();

        let first = embedding_record(
            "133e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            None,
            vec![0.1, 0.0, 0.0],
            "First embedding",
            1_725_647_200_000,
            0,
        );
        let second = embedding_record(
            "233e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            None,
            vec![0.0, 0.1, 0.0],
            "Second embedding",
            1_725_647_200_010,
            0,
        );
        let third = embedding_record(
            "333e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            None,
            vec![0.0, 0.0, 0.1],
            "Third embedding",
            1_725_647_200_020,
            0,
        );
        repo.upsert_embedding_artifacts(&[first.clone(), second.clone(), third.clone()])
            .unwrap();

        let plan = repo
            .plan_garbage_collection(
                Some(&session.session_id),
                &StorageTierPolicy::new(100, 1_000),
                500,
                168,
                &GarbageCollectionPolicy::new(2, false),
            )
            .unwrap();

        assert_eq!(plan.candidate_count, 1);
        assert_eq!(
            plan.reason_counts
                .get(&GarbageCollectionReason::OverEmbeddingLimit),
            Some(&1)
        );
        assert_eq!(plan.candidates[0].artifact.artifact_id, first.artifact_id);
        assert_eq!(
            plan.candidates[0].reasons,
            vec![GarbageCollectionReason::OverEmbeddingLimit]
        );
    }

    #[test]
    fn applying_gc_plan_deletes_only_planned_derived_artifacts() {
        let sandbox = TestSandbox::new("gc-apply");
        let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
        let session = repo
            .create_session(CreateSessionRequest::new("Apply GC"))
            .unwrap();

        let messages = (0..4)
            .map(|index| {
                repo.insert_message(
                    &session.session_id,
                    CreateMessageRequest::user(format!("Apply message #{index}")),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let first_message_id = message_id(&messages[0].message_id);
        let second_message_id = message_id(&messages[1].message_id);

        let pinned = repo
            .pin_message_memory(
                &session.session_id,
                &first_message_id,
                PinMessageMemoryRequest::new(AuthorId::User, Provenance::UserAuthored),
            )
            .unwrap();

        let embedding = embedding_record(
            "433e4567-e89b-12d3-a456-426614174000",
            &session.session_id,
            Some(&first_message_id),
            vec![0.9, 0.1, 0.2],
            "Apply message #0",
            1_725_647_200_000,
            1,
        );
        repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
            .unwrap();

        clock.store(1_725_647_200_010, Ordering::SeqCst);
        let chunk = repo
            .store_chunk_summary(
                &session.session_id,
                "Chunk ready for cleanup.",
                2,
                &first_message_id,
                &second_message_id,
                1,
            )
            .unwrap();

        clock.store(1_725_647_200_020, Ordering::SeqCst);
        let old_synopsis = repo
            .store_session_synopsis(&session.session_id, "Old synopsis.", 2, 1)
            .unwrap();

        clock.store(1_725_647_200_030, Ordering::SeqCst);
        let new_synopsis = repo
            .store_session_synopsis(&session.session_id, "Current synopsis.", 4, 4)
            .unwrap();

        let plan = repo
            .plan_garbage_collection(
                Some(&session.session_id),
                &StorageTierPolicy::new(1, 2),
                500,
                168,
                &GarbageCollectionPolicy::new(10, false),
            )
            .unwrap();
        assert_eq!(plan.candidate_count, 3);

        let outcome = repo.apply_garbage_collection_plan(&plan).unwrap();
        assert_eq!(outcome.deleted_count, 3);
        assert_eq!(
            outcome
                .deleted_artifact_ids
                .get(&session.session_id)
                .map(Vec::len),
            Some(3)
        );

        assert!(repo
            .list_embedding_artifacts(Some(&session.session_id))
            .unwrap()
            .is_empty());
        assert!(repo
            .list_chunk_summaries(&session.session_id)
            .unwrap()
            .is_empty());
        assert_eq!(
            repo.get_latest_session_synopsis(&session.session_id)
                .unwrap()
                .map(|artifact| artifact.artifact_id),
            Some(new_synopsis.artifact_id.clone())
        );
        let pinned_memories = repo.list_pinned_memories(&session.session_id).unwrap();
        assert_eq!(pinned_memories.len(), 1);
        assert_eq!(pinned_memories[0].record.artifact_id, pinned.artifact_id);

        let remaining_derived = repo
            .list_derived_artifacts(Some(&session.session_id))
            .unwrap();
        assert_eq!(remaining_derived.len(), 1);
        assert_eq!(remaining_derived[0].artifact_id, new_synopsis.artifact_id);
        assert_eq!(
            remaining_derived[0].kind,
            DerivedArtifactKind::SessionSynopsis
        );

        let deleted_ids = outcome
            .deleted_artifact_ids
            .get(&session.session_id)
            .unwrap();
        assert!(deleted_ids.contains(&embedding.artifact_id));
        assert!(deleted_ids.contains(&chunk.artifact_id));
        assert!(deleted_ids.contains(&old_synopsis.artifact_id));
    }

    fn candidate_reasons_by_artifact(
        plan: &GarbageCollectionPlan,
    ) -> BTreeMap<(SessionId, MemoryArtifactId), Vec<GarbageCollectionReason>> {
        plan.candidates
            .iter()
            .map(|candidate| {
                (
                    (
                        candidate.artifact.session_id.clone(),
                        candidate.artifact.artifact_id.clone(),
                    ),
                    candidate.reasons.clone(),
                )
            })
            .collect()
    }
}
