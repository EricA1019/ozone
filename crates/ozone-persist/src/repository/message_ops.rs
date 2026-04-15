use ozone_core::{
    engine::{CommitMessageCommand, ConversationMessage, MessageId},
    session::SessionId,
};
use rusqlite::{params, Row};

use crate::{PersistError, Result};

use super::*;

impl SqliteRepository {
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
            ozone_core::engine::BranchState::Archived
            | ozone_core::engine::BranchState::Deleted => {
                return Err(PersistError::ConsistencyError(format!(
                    "branch {} cannot accept new messages while in state {}",
                    branch_id, branch.branch.state
                )));
            }
            ozone_core::engine::BranchState::Active
            | ozone_core::engine::BranchState::Inactive => {}
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
