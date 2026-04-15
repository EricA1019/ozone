use ozone_core::{
    engine::{
        BranchId, BranchState, ConversationMessage, CreateBranchCommand,
        MessageId,
    },
    session::SessionId,
};
use rusqlite::{params, OptionalExtension};

use crate::{PersistError, Result};

use super::*;

impl SqliteRepository {
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
}
