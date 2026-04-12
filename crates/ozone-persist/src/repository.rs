use std::{
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
    session::{CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UnixTimestamp},
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::{
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageSearchHit {
    pub message_id: String,
    pub author_kind: String,
    pub content: String,
    pub created_at: UnixTimestamp,
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
pub struct BranchRecord {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
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
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT m.message_id, m.author_kind, m.content, m.created_at
             FROM messages_fts
             JOIN messages m ON m.rowid = messages_fts.rowid
             WHERE m.session_id = ?1 AND messages_fts MATCH ?2
             ORDER BY m.created_at ASC, m.rowid ASC",
        )?;
        let rows = stmt.query_map(params![session_id.as_str(), query], |row| {
            Ok(MessageSearchHit {
                message_id: row.get(0)?,
                author_kind: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(PersistError::from)
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
        let tags = match value.tags_json {
            Some(tags_json) => serde_json::from_str(&tags_json).map_err(|error| {
                PersistError::InvalidData(format!("invalid session tags JSON: {error}"))
            })?,
            None => Vec::new(),
        };
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
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicI64, AtomicU64, Ordering},
            Arc,
        },
    };

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
}
