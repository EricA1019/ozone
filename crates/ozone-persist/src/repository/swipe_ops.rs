use ozone_core::{
    engine::{
        ActivateSwipeCommand, MessageId, RecordSwipeCandidateCommand, SwipeCandidate,
        SwipeCandidateState, SwipeGroup, SwipeGroupId,
    },
    session::SessionId,
};
use rusqlite::{params, OptionalExtension, Row, Transaction};

use crate::{PersistError, Result};

use super::*;

impl SqliteRepository {
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
