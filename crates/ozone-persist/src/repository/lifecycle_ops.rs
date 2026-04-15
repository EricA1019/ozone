use std::collections::{BTreeMap, BTreeSet};

use ozone_core::{
    engine::MessageId,
    session::SessionId,
};
use ozone_memory::{
    EmbeddingRecord,
    EmbeddingRecordMetadata, MemoryArtifactId, MemoryContent, Provenance, StorageTier,
    StorageTierPolicy,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::{PersistError, Result};

use super::*;

impl SqliteRepository {
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
}

const EMBEDDING_ARTIFACT_FORMAT: &str = "ozone-memory.embedding-artifact.v1";

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
