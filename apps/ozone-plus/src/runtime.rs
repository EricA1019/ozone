use crate::{
    context_bridge::AppContextBridge,
    inference_adapter::{InferenceAdapter, InferenceAdapterInit},
};
use ozone_core::engine::{BranchId, GenerationState, SetGenerationStateCommand};
use ozone_engine::{
    ConversationBranchRecord, ConversationEngine, ConversationStore, EngineCommand,
    EngineCommandResult, SingleWriterConversationEngine, ThinkingDisplayMode,
};
use ozone_memory::{ImportanceScorer, KeywordExtractor};
use ozone_persist::{PersistError, SessionId, SqliteRepository};
use ozone_tui::{
    AppBootstrap as TuiBootstrap, GenerationPoll, RecallBrowser as TuiRecallBrowser,
    RuntimeCancellation as TuiRuntimeCancellation,
    RuntimeContextRefresh as TuiRuntimeContextRefresh,
    RuntimeSendReceipt as TuiRuntimeSendReceipt, RuntimeSessionLoad as TuiRuntimeSessionLoad,
    SessionContext as TuiSessionContext, SessionRuntime,
};
#[allow(unused_imports)]
use std::sync::mpsc;

mod bootstrap;
mod persisted_draft;
mod commands;
mod context;
mod generation;
mod management;
mod message_actions;
mod reroll;
mod shell_commands;
mod recall_helpers;
mod types;

#[allow(unused_imports)]
pub(crate) use generation::{
    WorkerEvent, PendingGeneration, PendingCompletion, PendingReroll, RerollBranchMode,
    RerollSource,
};

#[allow(unused_imports)]
pub(crate) use types::{
    SessionCommand, MemoryCommand, SearchCommand, ShellCommand, SummarizeShellCommand,
    ThinkingCommand, TierBCommand, HooksCommand, SafeModeCommand, RecentSearchSection,
};

use recall_helpers::{
    repository_template_dir, tui_branch_from_record, tui_context_dry_run_from_build,
    tui_context_preview_from_plan, tui_recall_browser_from_state,
    tui_transcript_item_from_message,
};
use std::sync::Arc;

// Generation-related types are defined in `generation.rs` and re-exported above.

// Command and small helper types are kept in `runtime/types.rs` and
// re-exported into the runtime module namespace for child modules.

// `PendingGeneration` and related types live in `generation.rs` and are re-exported above.

pub struct Phase1dRuntime {
    repo: SqliteRepository,
    engine: SingleWriterConversationEngine<crate::store::RepoConversationStore>,
    session_id: SessionId,
    lock_instance_id: String,
    inference: InferenceAdapter,
    context_bridge: AppContextBridge,
    pending_generation: Option<PendingGeneration>,
    recent_search: Option<RecentSearchSection>,
    thinking_display_mode: ThinkingDisplayMode,
    importance_scorer: ImportanceScorer,
    keyword_extractor: KeywordExtractor,
    custom_commands: Vec<crate::hooks::CustomCommand>,
    hooks_config: crate::hooks::HooksConfig,
    safe_mode: bool,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl Phase1dRuntime {
    pub fn open(repo: SqliteRepository, session_id: SessionId) -> Result<Self, String> {
        repo.get_session(&session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {session_id} was not found"))?;

        let instance_id = format!("ozone-plus-phase1d-{}", std::process::id());
        repo.acquire_session_lock(&session_id, &instance_id)
            .map_err(|error| match error {
                PersistError::SessionLocked {
                    instance_id,
                    acquired_at,
                } => format!(
                    "session {session_id} is locked by instance {instance_id} (since {})",
                    crate::format_timestamp(acquired_at)
                ),
                other => other.to_string(),
            })?;

        let inference = Self::load_inference_for_session(&repo, &session_id)?;

        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("failed to build tokio runtime: {error}"))?,
        );

        Ok(Self {
            engine: SingleWriterConversationEngine::new(crate::store::RepoConversationStore::new(
                repo.clone(),
            )),
            repo,
            session_id,
            lock_instance_id: instance_id,
            inference,
            context_bridge: AppContextBridge::default(),
            pending_generation: None,
            recent_search: None,
            thinking_display_mode: ThinkingDisplayMode::Hidden,
            importance_scorer: ImportanceScorer::default(),
            keyword_extractor: KeywordExtractor::new(),
            custom_commands: crate::hooks::discover_commands(),
            hooks_config: crate::hooks::HooksConfig::default(),
            safe_mode: false,
            runtime,
        })
    }

    /// Pre-flight health check — verifies the inference backend is reachable
    /// before committing a user message and spawning a generation task.
    pub fn check_backend_health(&self) -> Result<(), String> {
        self.inference
            .check_backend_health()
            .map_err(|e| e.to_string())
    }

    pub fn release_lock(&mut self) -> Result<(), String> {
        if !self
            .repo
            .release_session_lock(&self.session_id, &self.lock_instance_id)
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "session {} lock was acquired but could not be released cleanly",
                self.session_id
            ));
        }

        Ok(())
    }

    fn load_inference_for_session(
        repo: &SqliteRepository,
        session_id: &SessionId,
    ) -> Result<InferenceAdapter, String> {
        let session_config_path = repo.paths().session_config_path(session_id);
        let custom_template_dir = repository_template_dir();
        InferenceAdapter::load(InferenceAdapterInit {
            session_config_path: Some(session_config_path),
            custom_template_dir,
            ..Default::default()
        })
        .map_err(|error| format!("failed to initialize inference adapter: {error}"))
    }

    fn switch_to_session(&mut self, new_sid: SessionId) -> Result<(), String> {
        if new_sid != self.session_id {
            let _ = self.release_lock();

            let instance_id = format!("ozone-plus-phase1d-{}", std::process::id());
            self.repo
                .acquire_session_lock(&new_sid, &instance_id)
                .map_err(|error| format!("failed to lock session {new_sid}: {error}"))?;

            self.session_id = new_sid.clone();
            self.lock_instance_id = instance_id;
        }

        self.pending_generation = None;
        self.context_bridge = AppContextBridge::default();
        self.recent_search = None;
        self.engine = SingleWriterConversationEngine::new(crate::store::RepoConversationStore::new(
            self.repo.clone(),
        ));
        self.inference = Self::load_inference_for_session(&self.repo, &self.session_id)?;

        Ok(())
    }

    fn active_branch(&self, session_id: &SessionId) -> Result<ConversationBranchRecord, String> {
        self.engine
            .store()
            .get_active_branch(session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "session {session_id} has no active branch yet; send the first message to bootstrap the conversation"
                )
            })
    }

    fn is_tier_b_active(&self) -> bool {
        !self.safe_mode && self.inference.config().memory.tier_b.enabled
    }

    fn branch_by_id(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<ConversationBranchRecord, String> {
        self.engine
            .store()
            .list_branches(session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|record| &record.branch.branch_id == branch_id)
            .ok_or_else(|| format!("branch {branch_id} was not found for session {session_id}"))
    }

    fn set_generation_state(
        &mut self,
        branch_id: BranchId,
        state: GenerationState,
    ) -> Result<(), String> {
        match self
            .engine
            .process(EngineCommand::SetGenerationState(
                SetGenerationStateCommand { branch_id, state },
            ))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::GenerationStateUpdated { .. } => Ok(()),
            other => Err(format!(
                "unexpected engine result for generation state update: {other:?}"
            )),
        }
    }

    fn build_recall_browser(&self, session_id: &SessionId) -> Result<TuiRecallBrowser, String> {
        let memories = self
            .repo
            .list_saved_memories(session_id)
            .map_err(|error| error.to_string())?;
        Ok(tui_recall_browser_from_state(
            &memories,
            self.recent_search.as_ref(),
            &self.inference.config().memory,
        ))
    }

    fn build_recall_browser_refresh(
        &mut self,
        context: &TuiSessionContext,
        status_line: impl Into<String>,
    ) -> Result<TuiRuntimeContextRefresh, String> {
        Ok(TuiRuntimeContextRefresh {
            status_line: Some(status_line.into()),
            context_preview: self
                .context_bridge
                .latest_plan_preview()
                .map(tui_context_preview_from_plan),
            context_dry_run: self
                .context_bridge
                .latest_dry_run()
                .map(tui_context_dry_run_from_build),
            recall_browser: Some(self.build_recall_browser(&context.session_id)?),
            ..TuiRuntimeContextRefresh::default()
        })
    }

    fn status_only_refresh(status_line: impl Into<String>) -> TuiRuntimeContextRefresh {
        TuiRuntimeContextRefresh {
            status_line: Some(status_line.into()),
            ..TuiRuntimeContextRefresh::default()
        }
    }
}

impl Drop for Phase1dRuntime {
    fn drop(&mut self) {
        let _ = self
            .repo
            .release_session_lock(&self.session_id, &self.lock_instance_id);
    }
}

impl SessionRuntime for Phase1dRuntime {
    type Error = String;

    fn bootstrap(&mut self, context: &TuiSessionContext) -> Result<TuiBootstrap, Self::Error> {
        self.load_bootstrap(context)
    }

    fn send_draft(
        &mut self,
        context: &TuiSessionContext,
        prompt: &str,
    ) -> Result<Option<TuiRuntimeSendReceipt>, Self::Error> {
        self.send_draft_impl(context, prompt)
    }

    fn reroll_message(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeSendReceipt>, Self::Error> {
        self.reroll_message_impl(context, message_id)
    }

    fn poll_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<GenerationPoll>, Self::Error> {
        self.poll_generation_impl(context)
    }

    fn cancel_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeCancellation>, Self::Error> {
        self.cancel_generation_impl(context)
    }

    fn build_context_dry_run(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        self.build_dry_run_context_refresh(context).map(Some)
    }

    fn toggle_bookmark(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        self.toggle_bookmark_impl(context, message_id)
    }

    fn toggle_pinned_memory(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        self.toggle_pinned_memory_impl(context, message_id)
    }

    fn edit_message(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
        content: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        self.edit_message_impl(context, message_id, content)
    }

    fn run_command(
        &mut self,
        context: &TuiSessionContext,
        input: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        self.run_command_impl(context, input)
    }

    fn persist_draft(
        &mut self,
        context: &TuiSessionContext,
        draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.save_persisted_draft(&context.session_id, draft)
    }

    fn list_sessions(&mut self) -> Result<Vec<ozone_tui::SessionListEntry>, Self::Error> {
        self.list_sessions_impl()
    }

    fn get_settings(&mut self) -> Result<Vec<ozone_tui::SettingsEntry>, Self::Error> {
        self.get_settings_impl()
    }

    fn save_pref(&mut self, pref_key: &str, value: &str) -> Result<(), Self::Error> {
        self.save_pref_impl(pref_key, value)
    }

    fn set_session_folder(
        &mut self,
        session_id: &str,
        folder: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.set_session_folder_impl(session_id, folder)
    }

    fn list_characters(&mut self) -> Result<Vec<ozone_tui::CharacterEntry>, Self::Error> {
        self.list_characters_impl()
    }

    fn create_character(
        &mut self,
        detail: ozone_tui::CharacterDetail,
    ) -> Result<ozone_tui::CharacterEntry, Self::Error> {
        self.create_character_impl(detail)
    }

    fn update_character(
        &mut self,
        detail: ozone_tui::CharacterDetail,
    ) -> Result<ozone_tui::CharacterEntry, Self::Error> {
        self.update_character_impl(detail)
    }

    fn get_character(
        &mut self,
        card_id: &str,
    ) -> Result<Option<ozone_tui::CharacterDetail>, Self::Error> {
        self.get_character_impl(card_id)
    }

    fn import_character(&mut self, path: String) -> Result<ozone_tui::CharacterEntry, Self::Error> {
        self.import_character_impl(path)
    }

    fn create_session(
        &mut self,
        character_name: Option<&str>,
    ) -> Result<TuiRuntimeSessionLoad, Self::Error> {
        self.create_session_impl(character_name)
    }

    fn open_session(
        &mut self,
        session_id: &str,
    ) -> Result<Option<TuiRuntimeSessionLoad>, Self::Error> {
        self.open_session_impl(session_id)
    }
}

#[cfg(test)]
mod tests;
