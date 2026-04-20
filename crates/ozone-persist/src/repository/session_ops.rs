use ozone_core::{
    engine::{BranchId, BranchState, ConversationBranch, CreateBranchCommand, MessageId},
    session::{
        CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UpdateSessionRequest,
    },
};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    import_export::{ImportCharacterCardRequest, ImportedCharacterCard, StoredCharacterCard},
    PersistError, Result,
};

use super::*;

impl SqliteRepository {
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

    /// Assign or remove the folder for a session.
    /// Folder membership is stored as a `folder:<name>` tag on the session.
    /// Pass `None` to remove from any folder.
    pub fn set_session_folder(
        &self,
        session_id: &SessionId,
        folder: Option<&str>,
    ) -> Result<SessionRecord> {
        let mut summary = self
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))?;
        summary.set_folder(folder);
        let request = UpdateSessionRequest {
            name: None,
            character_name: None,
            tags: Some(summary.tags),
        };
        self.update_session_metadata(session_id, request)
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

    /// Unconditionally clear any session lock regardless of instance_id.
    /// Used by `--force` to override stale locks.
    pub fn force_clear_session_lock(&self, session_id: &SessionId) -> Result<bool> {
        let conn = self.open_session_connection(session_id)?;
        let rows = conn.execute("DELETE FROM session_lock WHERE id = 1", [])?;
        Ok(rows != 0)
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

pub(crate) fn upsert_session_summary(conn: &Connection, summary: &SessionSummary) -> Result<()> {
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

pub(super) fn read_stored_session_summary(row: &Row<'_>) -> rusqlite::Result<StoredSessionSummary> {
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

#[derive(Debug)]
pub(super) struct StoredSessionSummary {
    pub(super) session_id: String,
    pub(super) name: String,
    pub(super) character_name: Option<String>,
    pub(super) created_at: i64,
    pub(super) last_opened_at: i64,
    pub(super) message_count: i64,
    pub(super) db_size_bytes: Option<i64>,
    pub(super) tags_json: Option<String>,
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
