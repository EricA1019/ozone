use ozone_core::{
    engine::{BranchId, ConversationMessage, SwipeCandidate, SwipeGroup},
    session::{SessionId, SessionSummary},
};
use rusqlite::params;

use crate::{
    import_export::{
        SessionExport, SessionExportBookmark, SessionExportBranch, SessionExportMessage,
        SessionExportSummary, SessionExportSwipeCandidate, SessionExportSwipeGroup,
        StoredCharacterCard, TranscriptExport, TranscriptExportBranch, TranscriptExportSession,
        SESSION_EXPORT_FORMAT, TRANSCRIPT_EXPORT_FORMAT,
    },
    PersistError, Result,
};

use super::*;

impl SqliteRepository {
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

    pub(crate) fn store_character_card(
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
