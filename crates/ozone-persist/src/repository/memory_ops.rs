use ozone_core::{engine::MessageId, session::SessionId};
use ozone_memory::{
    CreateNoteMemoryRequest, CrossSessionSearchHit, MemoryArtifactId, MemoryContent,
    PinMessageMemoryRequest, PinnedMemoryContent, PinnedMemoryRecord, PinnedMemoryView, Provenance,
    SearchSessionMetadata,
};
use rusqlite::{params, Row, Transaction};

use crate::{PersistError, Result};

use super::*;

impl SqliteRepository {
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

    pub fn search_pinned_memories(
        &self,
        session_id: &SessionId,
        query: &str,
    ) -> Result<Vec<PinnedMemorySearchHit>> {
        let Some(query) = plain_text_fts_query(query) else {
            return Ok(Vec::new());
        };
        let current_message_count = self.current_message_count(session_id)?;
        let conn = self.open_session_connection(session_id)?;
        let mut stmt = conn.prepare(
            "SELECT ma.artifact_id, ma.session_id, ma.content_json, ma.source_start_message_id, ma.source_end_message_id, ma.provenance, ma.created_at, ma.snapshot_version, bm25(artifacts_fts)
             FROM artifacts_fts
             JOIN memory_artifacts ma ON ma.rowid = artifacts_fts.rowid
             WHERE ma.session_id = ?1 AND ma.kind = 'pinned_memory' AND artifacts_fts MATCH ?2
             ORDER BY bm25(artifacts_fts), ma.created_at DESC, ma.rowid ASC",
        )?;
        let rows = stmt.query_map(
            params![session_id.as_str(), query],
            read_stored_pinned_memory_search_hit,
        )?;
        let stored_hits = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        stored_hits
            .into_iter()
            .map(|hit| hit.into_search_hit(current_message_count))
            .collect()
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

    pub fn search_pinned_memories_across_sessions(
        &self,
        query: &str,
    ) -> Result<Vec<CrossSessionPinnedMemorySearchHit>> {
        if plain_text_fts_query(query).is_none() {
            return Ok(Vec::new());
        }
        let sessions = self.list_sessions()?;
        let mut hits = Vec::new();
        for session in sessions {
            let session_meta = SearchSessionMetadata {
                session_id: session.session_id.clone(),
                session_name: session.name.clone(),
                character_name: session.character_name.clone(),
                tags: session.tags.clone(),
            };
            for hit in self.search_pinned_memories(&session.session_id, query)? {
                hits.push(CrossSessionPinnedMemorySearchHit {
                    session: session_meta.clone(),
                    memory: hit.memory,
                    bm25_score: hit.bm25_score,
                });
            }
        }
        hits.sort_by(|left, right| {
            left.bm25_score
                .partial_cmp(&right.bm25_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    right
                        .memory
                        .record
                        .created_at
                        .cmp(&left.memory.record.created_at)
                })
                .then_with(|| {
                    left.memory
                        .record
                        .artifact_id
                        .as_str()
                        .cmp(right.memory.record.artifact_id.as_str())
                })
        });
        Ok(hits)
    }
}

fn insert_pinned_memory_artifact_in_tx(
    tx: &Transaction<'_>,
    session_id: &SessionId,
    content: PinnedMemoryContent,
    source_message_id: Option<&MessageId>,
    provenance: Provenance,
    created_at: ozone_core::session::UnixTimestamp,
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
struct StoredPinnedMemorySearchHit {
    artifact_id: String,
    session_id: String,
    content_json: String,
    source_start_message_id: Option<String>,
    source_end_message_id: Option<String>,
    provenance: String,
    created_at: i64,
    snapshot_version: i64,
    bm25_score: f32,
}

impl StoredPinnedMemorySearchHit {
    fn into_search_hit(self, current_message_count: u64) -> Result<PinnedMemorySearchHit> {
        let record = PinnedMemoryRecord::try_from(StoredPinnedMemoryArtifact {
            artifact_id: self.artifact_id,
            session_id: self.session_id,
            content_json: self.content_json,
            source_start_message_id: self.source_start_message_id,
            source_end_message_id: self.source_end_message_id,
            provenance: self.provenance,
            created_at: self.created_at,
            snapshot_version: self.snapshot_version,
        })?;
        Ok(PinnedMemorySearchHit {
            memory: record.into_view(current_message_count),
            bm25_score: self.bm25_score,
        })
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

fn read_stored_pinned_memory_search_hit(
    row: &Row<'_>,
) -> rusqlite::Result<StoredPinnedMemorySearchHit> {
    Ok(StoredPinnedMemorySearchHit {
        artifact_id: row.get(0)?,
        session_id: row.get(1)?,
        content_json: row.get(2)?,
        source_start_message_id: row.get(3)?,
        source_end_message_id: row.get(4)?,
        provenance: row.get(5)?,
        created_at: row.get(6)?,
        snapshot_version: row.get(7)?,
        bm25_score: row.get(8)?,
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
