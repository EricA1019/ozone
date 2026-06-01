use ozone_core::engine::MessageId;
use ozone_engine::{ConversationEngine, EngineCommand, EngineCommandResult};
use ozone_persist::{AuthorId, PinMessageMemoryRequest, Provenance};

use super::{OzonePlusRuntime, TuiRuntimeContextRefresh, TuiSessionContext};

impl OzonePlusRuntime {
    pub(super) fn toggle_bookmark_impl(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, String> {
        let message_id = match MessageId::parse(message_id) {
            Ok(message_id) => message_id,
            Err(_) => {
                return Ok(Some(Self::status_only_refresh(
                    "Selected message has an invalid ID and could not be bookmarked",
                )))
            }
        };
        let is_bookmarked = self
            .repo
            .list_bookmarks(&context.session_id)
            .map_err(|error| error.to_string())?
            .iter()
            .any(|bookmark| bookmark.message_id == message_id);
        let now_bookmarked = !is_bookmarked;
        self.repo
            .set_message_bookmark(&context.session_id, &message_id, now_bookmarked, None)
            .map_err(|error| error.to_string())?;

        self.build_session_refresh(
            context,
            if now_bookmarked {
                "Bookmark added to selected message"
            } else {
                "Bookmark removed from selected message"
            },
        )
        .map(Some)
    }

    pub(super) fn toggle_pinned_memory_impl(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, String> {
        let message_id = match MessageId::parse(message_id) {
            Ok(message_id) => message_id,
            Err(_) => {
                return Ok(Some(Self::status_only_refresh(
                    "Selected message has an invalid ID and could not be pinned",
                )))
            }
        };

        let existing = self
            .repo
            .list_pinned_memories(&context.session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|memory| memory.record.source_message_id.as_ref() == Some(&message_id))
            .map(|memory| memory.record.artifact_id)
            .collect::<Vec<_>>();

        let status_line = if existing.is_empty() {
            self.repo
                .pin_message_memory(
                    &context.session_id,
                    &message_id,
                    PinMessageMemoryRequest {
                        pinned_by: AuthorId::User,
                        expires_after_turns: None,
                        provenance: Provenance::UserAuthored,
                    },
                )
                .map_err(|error| error.to_string())?;
            "Pinned selected message into hard context".to_owned()
        } else {
            for artifact_id in &existing {
                self.repo
                    .remove_pinned_memory(&context.session_id, artifact_id)
                    .map_err(|error| error.to_string())?;
            }
            if existing.len() == 1 {
                "Removed pinned memory from selected message".to_owned()
            } else {
                format!(
                    "Removed {} pinned memories from selected message",
                    existing.len()
                )
            }
        };

        self.refresh_context_cache(context);
        self.build_recall_browser_refresh(context, status_line)
            .map(Some)
    }

    pub(super) fn edit_message_impl(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
        content: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, String> {
        if content.trim().is_empty() {
            return Ok(Some(Self::status_only_refresh(
                "Edited message cannot be empty",
            )));
        }

        let message_id = match MessageId::parse(message_id) {
            Ok(message_id) => message_id,
            Err(_) => {
                return Ok(Some(Self::status_only_refresh(
                    "Selected message has an invalid ID and could not be edited",
                )))
            }
        };

        match self
            .engine
            .process(EngineCommand::EditMessage(
                ozone_engine::EditMessageCommand {
                    session_id: context.session_id.clone(),
                    message_id,
                    content: content.to_owned(),
                    edited_at: Some(crate::now_timestamp_ms()),
                },
            ))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageEdited(_) => self
                .build_session_refresh(context, "Updated selected message")
                .map(Some),
            other => Err(format!("unexpected engine result for edit: {other:?}")),
        }
    }
}