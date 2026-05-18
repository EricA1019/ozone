use std::collections::HashSet;

use ozone_core::engine::{CommitMessageCommand, ConversationMessage};
use ozone_engine::{ConversationEngine, ConversationStore, EngineCommand};
use ozone_persist::{SessionId, SessionRecord};
use ozone_tui::{
    AppBootstrap as TuiBootstrap, BranchItem as TuiBranchItem,
    RuntimeSessionLoad as TuiRuntimeSessionLoad, ScreenState as TuiScreenState,
    SessionContext as TuiSessionContext, SessionMetadata as TuiSessionMetadata,
    SessionStats as TuiSessionStats, TranscriptItem as TuiTranscriptItem,
};

use super::{
    tui_branch_from_record, tui_context_dry_run_from_build, tui_context_preview_from_plan,
    tui_transcript_item_from_message, Phase1dRuntime,
};

pub(super) struct SessionSnapshot {
    pub(super) session_title: String,
    pub(super) transcript: Vec<TuiTranscriptItem>,
    pub(super) branches: Vec<TuiBranchItem>,
    pub(super) metadata: TuiSessionMetadata,
    pub(super) stats: TuiSessionStats,
}

impl Phase1dRuntime {
    pub(super) fn load_session_into_tui(
        &mut self,
        session_id: SessionId,
    ) -> Result<TuiRuntimeSessionLoad, String> {
        self.switch_to_session(session_id.clone())?;
        let session = self
            .repo
            .get_session(&session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {session_id} was not found"))?;
        let greeting = self.seed_greeting_if_present(&session)?;
        let ctx = TuiSessionContext::new(session_id.clone(), session.name.clone());
        let mut bootstrap = self.load_bootstrap(&ctx)?;
        if let Some(ref mut metadata) = bootstrap.session_metadata {
            metadata.greeting = greeting.clone();
        }
        Ok(TuiRuntimeSessionLoad {
            session_id: session_id.to_string(),
            session_name: session.name,
            bootstrap,
        })
    }

    pub(super) fn seed_greeting_if_present(
        &mut self,
        session: &SessionRecord,
    ) -> Result<Option<String>, String> {
        let character_name = match session.character_name.as_deref() {
            Some(name) if !name.is_empty() => name,
            _ => return Ok(None),
        };

        let character = self
            .repo
            .get_character_by_name(character_name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("character '{character_name}' not found"))?;

        let greeting_text = character.greeting.trim();
        if greeting_text.is_empty() {
            return Ok(None);
        }

        let messages = self
            .engine
            .store()
            .get_active_branch_transcript(&self.session_id)
            .map_err(|e| e.to_string())?;
        if !messages.is_empty() {
            return Ok(None);
        }

        let message_id = crate::generate_message_id()?;
        let now = crate::now_timestamp_ms();
        let mut greeting_message = ConversationMessage::new(
            self.session_id.clone(),
            message_id,
            "character",
            greeting_text,
            now,
        );
        greeting_message.author_name = Some(character_name.to_owned());

        let active_branch = self.active_branch(&self.session_id)?;
        self.engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id: active_branch.branch.branch_id.clone(),
                message: greeting_message,
            }))
            .map_err(|e| format!("failed to inject greeting: {e}"))?;

        Ok(Some(greeting_text.to_owned()))
    }

    pub(super) fn load_bootstrap(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<TuiBootstrap, String> {
        let snapshot = self.load_session_snapshot(context)?;

        let _ = self.dry_run_context_build(context);
        let context_preview = self
            .context_bridge
            .latest_plan_preview()
            .map(tui_context_preview_from_plan);
        let context_dry_run = self
            .context_bridge
            .latest_dry_run()
            .map(tui_context_dry_run_from_build);

        let active_launch_plan = std::env::var("OZONE__LAUNCH_PLAN")
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok());

        let mut bootstrap = TuiBootstrap {
            transcript: snapshot.transcript,
            branches: snapshot.branches,
            status_line: Some(format!(
                "{} backend ready ({}, template {}) · session locked by {} · b bookmark · Ctrl+K pin · Ctrl+D dry run · Ctrl+I inspector · :memories",
                self.inference.config().backend.r#type,
                self.inference.config().backend.url,
                self.inference.selected_template(),
                self.lock_instance_id
            )),
            draft: self.load_persisted_draft(&context.session_id)?,
            screen: Some(TuiScreenState::Conversation),
            session_metadata: Some(snapshot.metadata),
            session_stats: Some(snapshot.stats),
            context_preview: context_preview.clone(),
            context_dry_run: context_dry_run.clone(),
            recall_browser: None,
            active_launch_plan,
        };

        if let Some(status_line) = bootstrap.status_line.as_mut() {
            status_line.push_str(" · ");
            status_line.push_str(
                &context_preview
                    .as_ref()
                    .map(|preview| format!("context {}", preview.inline_status))
                    .unwrap_or_else(|| "context preview pending".to_string()),
            );
        }

        Ok(bootstrap)
    }

    pub(super) fn load_session_snapshot(
        &self,
        context: &TuiSessionContext,
    ) -> Result<SessionSnapshot, String> {
        let session = self
            .repo
            .get_session(&context.session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {} was not found", context.session_id))?;
        let branches = self
            .engine
            .store()
            .list_branches(&context.session_id)
            .map_err(|error| error.to_string())?;
        let bookmarks = self
            .repo
            .list_bookmarks(&context.session_id)
            .map_err(|error| error.to_string())?;
        let bookmarked_message_ids: HashSet<String> = bookmarks
            .iter()
            .map(|bookmark| bookmark.message_id.to_string())
            .collect();
        let transcript_messages = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        let message_count = transcript_messages.len();
        let branch_count = branches.len();
        let bookmark_count = bookmarked_message_ids.len();
        let transcript = transcript_messages
            .into_iter()
            .map(|message| {
                let is_bookmarked = bookmarked_message_ids.contains(message.message_id.as_str());
                tui_transcript_item_from_message(message, is_bookmarked)
            })
            .collect();

        Ok(SessionSnapshot {
            session_title: session.name,
            transcript,
            branches: branches.into_iter().map(tui_branch_from_record).collect(),
            metadata: TuiSessionMetadata {
                character_name: session.character_name,
                tags: session.tags,
                pinned_count: None,
                greeting: None,
                memory_metadata: None,
            },
            stats: TuiSessionStats {
                message_count,
                branch_count,
                bookmark_count,
            },
        })
    }

    // Persisted draft I/O moved to `persisted_draft.rs` as methods on
    // `Phase1dRuntime` to keep bootstrap responsibilities focused.
}