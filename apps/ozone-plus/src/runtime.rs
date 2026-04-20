use crate::{
    context_bridge::{
        AppContextBridge, ContextBuildResult, ContextPlanPreview, DryRunContextBuild,
    },
    hybrid_search::HybridSearchService,
    inference_adapter::{InferenceAdapter, InferenceAdapterInit},
};
use ozone_core::engine::{
    BranchId, BranchState, CancelReason, CommitMessageCommand, ConversationMessage,
    GenerationState, RequestId, SetGenerationStateCommand,
};
use ozone_engine::{
    ConversationBranchRecord, ConversationEngine, ConversationStore, EngineCommand,
    EngineCommandResult, SingleWriterConversationEngine, ThinkingBlockDecoder, ThinkingDisplayMode,
    ThinkingOutput,
};
use ozone_inference::{InferenceError, MemoryConfig, StreamChunk};
use ozone_memory::{ImportanceScorer, KeywordExtractor};
use ozone_persist::{
    AuthorId, CreateNoteMemoryRequest, MemoryArtifactId, PersistError, PinMessageMemoryRequest,
    PinnedMemoryView, Provenance, SessionId, SqliteRepository, UpdateSessionRequest,
};
use ozone_tui::{
    AppBootstrap as TuiBootstrap, BranchItem as TuiBranchItem,
    ContextDryRunPreview as TuiContextDryRunPreview, ContextPreview as TuiContextPreview,
    ContextTokenBudget as TuiContextTokenBudget, DraftState as TuiDraftState, EntryKind,
    GenerationPoll, RecallBrowser as TuiRecallBrowser,
    RuntimeCancellation as TuiRuntimeCancellation, RuntimeCompletion as TuiRuntimeCompletion,
    RuntimeContextRefresh as TuiRuntimeContextRefresh, RuntimeFailure as TuiRuntimeFailure,
    RuntimeProgress as TuiRuntimeProgress, RuntimeSendReceipt as TuiRuntimeSendReceipt,
    SessionContext as TuiSessionContext, SessionMetadata as TuiSessionMetadata, SessionRuntime,
    SessionStats as TuiSessionStats, TranscriptItem as TuiTranscriptItem,
};
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

#[derive(Debug)]
enum WorkerEvent {
    Token(String),
    Finished,
    Failed(String),
    Cancelled,
}

struct PendingGeneration {
    branch_id: BranchId,
    request_id: RequestId,
    started_at: Instant,
    partial_content: String,
    thinking_content: String,
    thinking_decoder: ThinkingBlockDecoder,
    tokens_generated: u64,
    receiver: Receiver<WorkerEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

struct SessionSnapshot {
    session_title: String,
    transcript: Vec<TuiTranscriptItem>,
    branches: Vec<TuiBranchItem>,
    metadata: TuiSessionMetadata,
    stats: TuiSessionStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionCommand {
    Show,
    Rename(String),
    Character(Option<String>),
    Tags(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MemoryCommand {
    List,
    Note(String),
    Unpin(MemoryArtifactId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchCommand {
    Session(String),
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShellCommand {
    Session(SessionCommand),
    Memory(MemoryCommand),
    Search(SearchCommand),
    Summarize(SummarizeShellCommand),
    Thinking(ThinkingCommand),
    TierB(TierBCommand),
    Hooks(HooksCommand),
    SafeMode(SafeModeCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SummarizeShellCommand {
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThinkingCommand {
    Status,
    SetMode(ThinkingDisplayMode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TierBCommand {
    Status,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HooksCommand {
    Status,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SafeModeCommand {
    Status,
    On,
    Off,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecentSearchSection {
    summary: String,
    hit_count: usize,
    lines: Vec<String>,
}

impl PendingGeneration {
    fn failed(branch_id: BranchId, request_id: RequestId, error: String) -> Self {
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(WorkerEvent::Failed(error));
        Self {
            branch_id,
            request_id,
            started_at: Instant::now(),
            partial_content: String::new(),
            thinking_content: String::new(),
            thinking_decoder: ThinkingBlockDecoder::new(ThinkingDisplayMode::Hidden),
            tokens_generated: 0,
            receiver,
            cancel_tx: None,
        }
    }
}

pub(crate) struct Phase1dRuntime {
    repo: SqliteRepository,
    engine: SingleWriterConversationEngine<crate::RepoConversationStore>,
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
}

impl Phase1dRuntime {
    pub(crate) fn open(repo: SqliteRepository, session_id: SessionId) -> Result<Self, String> {
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

        let session_config_path = repo.paths().session_config_path(&session_id);
        let custom_template_dir = repository_template_dir();
        let inference = InferenceAdapter::load(InferenceAdapterInit {
            session_config_path: Some(session_config_path),
            custom_template_dir,
            ..Default::default()
        })
        .map_err(|error| format!("failed to initialize inference adapter: {error}"))?;

        Ok(Self {
            engine: SingleWriterConversationEngine::new(crate::RepoConversationStore::new(
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
        })
    }

    /// Pre-flight health check — verifies the inference backend is reachable
    /// before committing a user message and spawning a generation task.
    pub(crate) fn check_backend_health(&self) -> Result<(), String> {
        self.inference
            .check_backend_health()
            .map_err(|e| e.to_string())
    }

    pub(crate) fn release_lock(&mut self) -> Result<(), String> {
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

    fn load_bootstrap(&mut self, context: &TuiSessionContext) -> Result<TuiBootstrap, String> {
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
            screen: None,
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

    fn load_session_snapshot(
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
            },
            stats: TuiSessionStats {
                message_count,
                branch_count,
                bookmark_count,
            },
        })
    }

    fn load_persisted_draft(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<TuiDraftState>, String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        match fs::read_to_string(&draft_path) {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(TuiDraftState::restore(
                ozone_tui::app::DraftCheckpoint::new(text.clone(), text.chars().count()),
            ))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!(
                "failed to read persisted draft at {}: {error}",
                draft_path.display()
            )),
        }
    }

    fn save_persisted_draft(
        &self,
        session_id: &SessionId,
        draft: Option<&str>,
    ) -> Result<(), String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        let parent = draft_path.parent().ok_or_else(|| {
            format!(
                "draft path {} has no parent directory",
                draft_path.display()
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create draft directory {}: {error}",
                parent.display()
            )
        })?;

        match draft.filter(|text| !text.is_empty()) {
            Some(text) => fs::write(&draft_path, text.as_bytes()).map_err(|error| {
                format!(
                    "failed to write persisted draft {}: {error}",
                    draft_path.display()
                )
            })?,
            None => match fs::remove_file(&draft_path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "failed to remove persisted draft {}: {error}",
                        draft_path.display()
                    ))
                }
            },
        }

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

    fn start_generation_task(
        &self,
        branch_id: BranchId,
        request_id: RequestId,
        prompt: String,
        thinking_mode: ThinkingDisplayMode,
    ) -> Result<PendingGeneration, String> {
        let gateway = self.inference.gateway().clone();
        let request = self.inference.build_request(prompt);
        let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>();
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

        thread::Builder::new()
            .name(format!("ozone-plus-gen-{request_id}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = event_tx.send(WorkerEvent::Failed(format!(
                            "failed to build tokio runtime: {error}"
                        )));
                        return;
                    }
                };

                runtime.block_on(async move {
                    // Probe backend max context length and warn if our prompt is large.
                    if let Some(max_ctx) = gateway.probe_max_context_length().await {
                        let prompt_chars = request.prompt.len();
                        // Rough heuristic: ~3.5 chars per token for English text.
                        let estimated_prompt_tokens = prompt_chars * 10 / 35;
                        if estimated_prompt_tokens > max_ctx {
                            let _ = event_tx.send(WorkerEvent::Token(format!(
                                "\n⚠ prompt (~{estimated_prompt_tokens} est. tokens) may exceed backend context ({max_ctx})\n"
                            )));
                        }
                    }

                    let (stream_tx, mut stream_rx) = tokio_mpsc::channel::<StreamChunk>(128);
                    let stream_gateway = gateway.clone();
                    let stream_task = tokio::spawn(async move {
                        stream_gateway
                            .stream_with_retry(request, stream_tx, cancel_rx, 0)
                            .await
                    });

                    let mut saw_done = false;
                    while let Some(chunk) = stream_rx.recv().await {
                        match chunk {
                            StreamChunk::Token(token) => {
                                if event_tx.send(WorkerEvent::Token(token)).is_err() {
                                    return;
                                }
                            }
                            StreamChunk::FinishReason(_) => {}
                            StreamChunk::Done => {
                                saw_done = true;
                                let _ = event_tx.send(WorkerEvent::Finished);
                            }
                        }
                    }

                    // Only consult the task result if the stream channel
                    // didn't already deliver a Done/Finished event.
                    if !saw_done {
                        match stream_task.await {
                            Ok(Ok(_)) => {
                                let _ = event_tx.send(WorkerEvent::Finished);
                            }
                            Ok(Err(error)) => {
                                if error
                                    .downcast_ref::<InferenceError>()
                                    .is_some_and(|inner| {
                                        matches!(inner, InferenceError::Cancelled)
                                    })
                                {
                                    let _ = event_tx.send(WorkerEvent::Cancelled);
                                } else {
                                    let _ =
                                        event_tx.send(WorkerEvent::Failed(error.to_string()));
                                }
                            }
                            Err(error) => {
                                let _ = event_tx.send(WorkerEvent::Failed(format!(
                                    "generation task join failure: {error}"
                                )));
                            }
                        }
                    }
                });
            })
            .map_err(|error| format!("failed to spawn generation worker: {error}"))?;

        Ok(PendingGeneration {
            branch_id,
            request_id,
            started_at: Instant::now(),
            partial_content: String::new(),
            thinking_content: String::new(),
            thinking_decoder: ThinkingBlockDecoder::new(thinking_mode),
            tokens_generated: 0,
            receiver: event_rx,
            cancel_tx: Some(cancel_tx),
        })
    }

    fn build_context_for_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<ContextBuildResult, String> {
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        let pinned_memories = self
            .repo
            .list_pinned_memories(&context.session_id)
            .map_err(|error| error.to_string())?;
        let retrieved_memories =
            HybridSearchService::new(&self.repo, &self.inference.config().memory)
                .context_retrieval(&context.session_id, &transcript, &pinned_memories, 3)?;
        self.context_bridge.build_from_transcript(
            &transcript,
            &pinned_memories,
            retrieved_memories.as_ref(),
            None,
            &self.inference,
        )
    }

    /// Reserved for context inspection integration.
    #[allow(dead_code)]
    pub fn latest_context_plan_preview(&self) -> Option<&ContextPlanPreview> {
        self.context_bridge.latest_plan_preview()
    }

    /// Reserved for context inspection integration.
    #[allow(dead_code)]
    pub fn latest_context_dry_run(&self) -> Option<&DryRunContextBuild> {
        self.context_bridge.latest_dry_run()
    }

    /// Reserved for context inspection integration.
    #[allow(dead_code)]
    pub fn status_line_context_preview_text(&self) -> String {
        self.context_bridge.status_line_preview_text()
    }

    /// Reserved for context inspection integration.
    #[allow(dead_code)]
    pub fn dry_run_context_build(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<DryRunContextBuild, String> {
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        let pinned_memories = self
            .repo
            .list_pinned_memories(&context.session_id)
            .map_err(|error| error.to_string())?;
        let retrieved_memories =
            HybridSearchService::new(&self.repo, &self.inference.config().memory)
                .context_retrieval(&context.session_id, &transcript, &pinned_memories, 3)?;
        self.context_bridge.dry_run_from_transcript(
            &transcript,
            &pinned_memories,
            retrieved_memories.as_ref(),
            None,
            &self.inference,
        )
    }

    fn build_dry_run_context_refresh(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<TuiRuntimeContextRefresh, String> {
        let dry_run = self.dry_run_context_build(context)?;
        Ok(TuiRuntimeContextRefresh {
            status_line: Some(format!(
                "Context dry run captured · {}",
                dry_run.result.preview.summary
            )),
            context_preview: self
                .context_bridge
                .latest_plan_preview()
                .map(tui_context_preview_from_plan),
            context_dry_run: self
                .context_bridge
                .latest_dry_run()
                .map(tui_context_dry_run_from_build),
            ..TuiRuntimeContextRefresh::default()
        })
    }

    fn build_session_refresh(
        &mut self,
        context: &TuiSessionContext,
        status_line: impl Into<String>,
    ) -> Result<TuiRuntimeContextRefresh, String> {
        let snapshot = self.load_session_snapshot(context)?;
        Ok(TuiRuntimeContextRefresh {
            status_line: Some(status_line.into()),
            session_title: Some(snapshot.session_title),
            transcript: Some(snapshot.transcript),
            session_metadata: Some(snapshot.metadata),
            session_stats: Some(snapshot.stats),
            context_preview: self
                .context_bridge
                .latest_plan_preview()
                .map(tui_context_preview_from_plan),
            context_dry_run: self
                .context_bridge
                .latest_dry_run()
                .map(tui_context_dry_run_from_build),
            ..TuiRuntimeContextRefresh::default()
        })
    }

    fn refresh_context_cache(&mut self, context: &TuiSessionContext) {
        let _ = self.dry_run_context_build(context);
    }

    fn build_recall_browser(&self, session_id: &SessionId) -> Result<TuiRecallBrowser, String> {
        let pinned_memories = self
            .repo
            .list_pinned_memories(session_id)
            .map_err(|error| error.to_string())?;
        Ok(tui_recall_browser_from_state(
            &pinned_memories,
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

    fn complete_generation(
        &mut self,
        context: &TuiSessionContext,
        pending: PendingGeneration,
    ) -> Result<TuiRuntimeCompletion, String> {
        let branch = self.branch_by_id(&context.session_id, &pending.branch_id)?;
        let thinking_content = pending.thinking_content.clone();
        let session_id_str = context.session_id.to_string();
        let mut assistant_message = ConversationMessage::new(
            context.session_id.clone(),
            crate::generate_message_id()?,
            "assistant",
            pending.partial_content,
            crate::now_timestamp_ms(),
        );
        assistant_message.author_name = Some(format!(
            "{} backend",
            self.inference.config().backend.r#type
        ));
        assistant_message.parent_id = Some(branch.branch.tip_message_id.clone());

        let committed = match self
            .engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id: pending.branch_id.clone(),
                message: assistant_message,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageCommitted(message) => message,
            other => {
                return Err(format!(
                    "unexpected engine result for assistant completion: {other:?}"
                ))
            }
        };

        let duration_ms =
            u64::try_from(pending.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let tokens_generated = pending.tokens_generated;
        self.set_generation_state(
            pending.branch_id,
            GenerationState::Completed {
                request_id: pending.request_id.clone(),
                message_id: committed.message_id.clone(),
                tokens_generated,
                duration_ms,
            },
        )?;

        // Tier B: post-generation assistive artifacts
        if self.is_tier_b_active() {
            let tier_b = self.inference.config().memory.tier_b.clone();

            if tier_b.importance_proposals {
                if let Some(_proposal) =
                    self.importance_scorer.propose(&committed.content, false, 0)
                {
                    // Proposal computed — available for future display/storage on user request
                }
            }

            if tier_b.retrieval_keys {
                let _retrieval_key = self.keyword_extractor.to_retrieval_key(&committed.content);
                // Retrieval key computed — available for future indexing
            }

            if tier_b.thinking_summaries && !thinking_content.is_empty() {
                let preview = &thinking_content[..thinking_content.len().min(480)];
                let _ = self.repo.create_note_memory(
                    &context.session_id,
                    CreateNoteMemoryRequest::new(
                        format!("[thinking] {preview}"),
                        AuthorId::System,
                        Provenance::SystemGenerated,
                    ),
                );
            }
        }

        // Post-generation hook
        let _ = self
            .hooks_config
            .run_post_generation(&session_id_str, &committed.content);

        Ok(TuiRuntimeCompletion {
            request_id: pending.request_id.to_string(),
            assistant_message: tui_transcript_item_from_message(committed, false),
        })
    }

    fn mark_generation_failure(
        &mut self,
        pending: PendingGeneration,
        error: String,
    ) -> Result<TuiRuntimeFailure, String> {
        let state = if pending.partial_content.is_empty() {
            GenerationState::Failed {
                request_id: pending.request_id.clone(),
                error: error.clone(),
            }
        } else {
            GenerationState::FailedMidStream {
                request_id: pending.request_id.clone(),
                partial_content: pending.partial_content.clone(),
                tokens_generated: pending.tokens_generated,
                error: error.clone(),
            }
        };

        self.set_generation_state(pending.branch_id, state)?;
        Ok(TuiRuntimeFailure {
            request_id: pending.request_id.to_string(),
            message: error,
        })
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
        if prompt.trim().is_empty() {
            return Ok(None);
        }

        let active_branch = self
            .engine
            .store()
            .get_active_branch(&context.session_id)
            .map_err(|error| error.to_string())?;
        let branch_id = active_branch
            .as_ref()
            .map(|record| record.branch.branch_id.clone())
            .unwrap_or(crate::generate_branch_id()?);
        let mut message = ConversationMessage::new(
            context.session_id.clone(),
            crate::generate_message_id()?,
            "user",
            prompt.to_owned(),
            crate::now_timestamp_ms(),
        );
        message.parent_id = active_branch
            .as_ref()
            .map(|record| record.branch.tip_message_id.clone());

        let committed = match self
            .engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id,
                message,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageCommitted(message) => message,
            other => return Err(format!("unexpected engine result for send: {other:?}")),
        };

        let active_branch = self.active_branch(&context.session_id)?;
        let request_id = crate::generate_request_id()?;
        self.set_generation_state(
            active_branch.branch.branch_id.clone(),
            GenerationState::Queued {
                request_id: request_id.clone(),
            },
        )?;

        let receipt = TuiRuntimeSendReceipt {
            request_id: request_id.to_string(),
            user_message: tui_transcript_item_from_message(committed, false),
            context_preview: None,
            context_dry_run: None,
        };

        let context_build = match self.build_context_for_generation(context) {
            Ok(context_build) => context_build,
            Err(error) => {
                self.pending_generation = Some(PendingGeneration::failed(
                    active_branch.branch.branch_id.clone(),
                    request_id,
                    error,
                ));
                return Ok(Some(TuiRuntimeSendReceipt {
                    context_preview: self
                        .context_bridge
                        .latest_plan_preview()
                        .map(tui_context_preview_from_plan),
                    context_dry_run: self
                        .context_bridge
                        .latest_dry_run()
                        .map(tui_context_dry_run_from_build),
                    ..receipt
                }));
            }
        };
        let context_preview = Some(tui_context_preview_from_plan(&context_build.preview));
        let context_dry_run = self
            .context_bridge
            .latest_dry_run()
            .map(tui_context_dry_run_from_build);
        let prompt = context_build.prompt;

        let _ = self
            .hooks_config
            .run_pre_generation(context.session_id.as_ref());
        let thinking_mode = self.thinking_display_mode;
        self.pending_generation = Some(
            self.start_generation_task(
                active_branch.branch.branch_id.clone(),
                request_id.clone(),
                prompt,
                thinking_mode,
            )
            .unwrap_or_else(|error| {
                PendingGeneration::failed(active_branch.branch.branch_id.clone(), request_id, error)
            }),
        );

        Ok(Some(TuiRuntimeSendReceipt {
            context_preview,
            context_dry_run,
            ..receipt
        }))
    }

    fn poll_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<GenerationPoll>, Self::Error> {
        let mut pending = match self.pending_generation.take() {
            Some(pending) => pending,
            None => return Ok(None),
        };

        let mut latest_failure: Option<String> = None;
        let mut finished = false;
        let mut progress_changed = false;

        loop {
            match pending.receiver.try_recv() {
                Ok(WorkerEvent::Token(token)) => {
                    let outputs = pending.thinking_decoder.feed(&token);
                    let mode = pending.thinking_decoder.display_mode();
                    for output in outputs {
                        match output {
                            ThinkingOutput::Content(text) => {
                                pending.partial_content.push_str(&text);
                            }
                            ThinkingOutput::Thinking(text) => {
                                if mode == ThinkingDisplayMode::Debug {
                                    pending.partial_content.push_str(&text);
                                } else {
                                    pending.thinking_content.push_str(&text);
                                }
                            }
                            ThinkingOutput::ThinkingStart | ThinkingOutput::ThinkingEnd => {}
                        }
                    }
                    pending.tokens_generated = pending.tokens_generated.saturating_add(1);
                    progress_changed = true;
                }
                Ok(WorkerEvent::Finished) => {
                    finished = true;
                }
                Ok(WorkerEvent::Failed(error)) => {
                    latest_failure = Some(error);
                }
                Ok(WorkerEvent::Cancelled) => {
                    let state = GenerationState::Cancelled {
                        request_id: pending.request_id.clone(),
                        partial_content: if pending.partial_content.is_empty() {
                            None
                        } else {
                            Some(pending.partial_content.clone())
                        },
                        tokens_generated: pending.tokens_generated,
                        reason: CancelReason::UserRequested,
                    };
                    self.set_generation_state(pending.branch_id, state)?;
                    return Ok(None);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if let Some(error) = latest_failure {
            let failure = self.mark_generation_failure(pending, error)?;
            return Ok(Some(GenerationPoll::Failed(failure)));
        }

        if finished {
            let completion = self.complete_generation(context, pending)?;
            return Ok(Some(GenerationPoll::Completed(completion)));
        }

        if progress_changed {
            self.set_generation_state(
                pending.branch_id.clone(),
                GenerationState::Streaming {
                    request_id: pending.request_id.clone(),
                    tokens_so_far: pending.tokens_generated,
                },
            )?;
            let progress = TuiRuntimeProgress {
                request_id: pending.request_id.to_string(),
                partial_content: pending.partial_content.clone(),
            };
            self.pending_generation = Some(pending);
            return Ok(Some(GenerationPoll::Pending {
                partial: Some(progress),
            }));
        }

        self.pending_generation = Some(pending);
        Ok(Some(GenerationPoll::Pending { partial: None }))
    }

    fn cancel_generation(
        &mut self,
        _context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeCancellation>, Self::Error> {
        let mut pending = match self.pending_generation.take() {
            Some(pending) => pending,
            None => return Ok(None),
        };

        while let Ok(event) = pending.receiver.try_recv() {
            if let WorkerEvent::Token(token) = event {
                pending.partial_content.push_str(&token);
                pending.tokens_generated = pending.tokens_generated.saturating_add(1);
            }
        }

        if let Some(cancel_tx) = pending.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }

        let partial = if pending.partial_content.is_empty() {
            None
        } else {
            Some(pending.partial_content.clone())
        };
        self.set_generation_state(
            pending.branch_id,
            GenerationState::Cancelled {
                request_id: pending.request_id.clone(),
                partial_content: partial.clone(),
                tokens_generated: pending.tokens_generated,
                reason: CancelReason::UserRequested,
            },
        )?;

        Ok(Some(TuiRuntimeCancellation {
            request_id: pending.request_id.to_string(),
            reason: CancelReason::UserRequested,
            partial_assistant_message: partial
                .map(|text| TuiTranscriptItem::new("assistant", text)),
        }))
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
        let message_id = match ozone_core::engine::MessageId::parse(message_id) {
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

    fn toggle_pinned_memory(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        let message_id = match ozone_core::engine::MessageId::parse(message_id) {
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

    fn run_command(
        &mut self,
        context: &TuiSessionContext,
        input: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        let command = match parse_shell_command(input) {
            Ok(command) => command,
            Err(error) => return Ok(Some(Self::status_only_refresh(error))),
        };

        match command {
            ShellCommand::Session(SessionCommand::Show) => {
                let snapshot = self.load_session_snapshot(context)?;
                let status = format!(
                    "Session {} · character {} · tags {}",
                    snapshot.session_title,
                    snapshot
                        .metadata
                        .character_name
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("—"),
                    format_tags(&snapshot.metadata.tags),
                );
                Ok(Some(TuiRuntimeContextRefresh {
                    status_line: Some(status),
                    session_title: Some(snapshot.session_title),
                    transcript: Some(snapshot.transcript),
                    session_metadata: Some(snapshot.metadata),
                    session_stats: Some(snapshot.stats),
                    context_preview: self
                        .context_bridge
                        .latest_plan_preview()
                        .map(tui_context_preview_from_plan),
                    context_dry_run: self
                        .context_bridge
                        .latest_dry_run()
                        .map(tui_context_dry_run_from_build),
                    ..TuiRuntimeContextRefresh::default()
                }))
            }
            ShellCommand::Session(SessionCommand::Rename(name)) => {
                let name =
                    require_non_empty("session name", name).map_err(|error| error.to_string())?;
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            name: Some(name.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                self.build_session_refresh(context, format!("Session renamed to {name}"))
                    .map(Some)
            }
            ShellCommand::Session(SessionCommand::Character(character_name)) => {
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            character_name: Some(character_name.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let status = match character_name {
                    Some(character_name) => format!("Character set to {character_name}"),
                    None => "Character cleared".to_owned(),
                };
                self.build_session_refresh(context, status).map(Some)
            }
            ShellCommand::Session(SessionCommand::Tags(tags)) => {
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            tags: Some(tags.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let status = if tags.is_empty() {
                    "Session tags cleared".to_owned()
                } else {
                    format!("Session tags set to {}", format_tags(&tags))
                };
                self.build_session_refresh(context, status).map(Some)
            }
            ShellCommand::Memory(MemoryCommand::List) => self
                .build_recall_browser_refresh(context, "Loaded pinned memories")
                .map(Some),
            ShellCommand::Memory(MemoryCommand::Note(text)) => {
                let text =
                    require_non_empty("memory note", text).map_err(|error| error.to_string())?;
                self.repo
                    .create_note_memory(
                        &context.session_id,
                        CreateNoteMemoryRequest::new(
                            text,
                            AuthorId::User,
                            Provenance::UserAuthored,
                        ),
                    )
                    .map_err(|error| error.to_string())?;
                self.refresh_context_cache(context);
                self.build_recall_browser_refresh(context, "Created pinned note memory")
                    .map(Some)
            }
            ShellCommand::Memory(MemoryCommand::Unpin(artifact_id)) => {
                let removed = self
                    .repo
                    .remove_pinned_memory(&context.session_id, &artifact_id)
                    .map_err(|error| error.to_string())?;
                if !removed {
                    return Ok(Some(Self::status_only_refresh(format!(
                        "Pinned memory {} was not found",
                        artifact_id
                    ))));
                }
                self.refresh_context_cache(context);
                self.build_recall_browser_refresh(
                    context,
                    format!("Removed pinned memory {artifact_id}"),
                )
                .map(Some)
            }
            ShellCommand::Search(SearchCommand::Session(query)) => {
                let query =
                    require_non_empty("search query", query).map_err(|error| error.to_string())?;
                let result = HybridSearchService::new(&self.repo, &self.inference.config().memory)
                    .search_session(&context.session_id, &query)?;
                self.recent_search = Some(recent_search_section("session", &result, false));
                self.build_recall_browser_refresh(
                    context,
                    format!(
                        "Session search `{query}` · {} · {} hit{}",
                        result.status.summary_line(),
                        result.hits.len(),
                        if result.hits.len() == 1 { "" } else { "s" }
                    ),
                )
                .map(Some)
            }
            ShellCommand::Search(SearchCommand::Global(query)) => {
                let query =
                    require_non_empty("search query", query).map_err(|error| error.to_string())?;
                let result = HybridSearchService::new(&self.repo, &self.inference.config().memory)
                    .search_global(&query)?;
                self.recent_search = Some(recent_search_section("global", &result, true));
                self.build_recall_browser_refresh(
                    context,
                    format!(
                        "Global search `{query}` · {} · {} hit{}",
                        result.status.summary_line(),
                        result.hits.len(),
                        if result.hits.len() == 1 { "" } else { "s" }
                    ),
                )
                .map(Some)
            }
            ShellCommand::Summarize(SummarizeShellCommand::Session) => {
                let transcript = self
                    .repo
                    .get_active_branch_transcript(&context.session_id)
                    .map_err(|error| error.to_string())?;

                if transcript.len() < 2 {
                    return Ok(Some(Self::status_only_refresh(
                        "Need at least 2 messages to generate a synopsis".to_string(),
                    )));
                }

                let turns: Vec<ozone_memory::summary::SummaryInputTurn> = transcript
                    .iter()
                    .map(|msg| ozone_memory::summary::SummaryInputTurn {
                        role: msg.author_kind.clone(),
                        content: msg.content.clone(),
                    })
                    .collect();

                let config = ozone_memory::summary::SummaryConfig::default();
                let status = match ozone_memory::summary::generate_session_synopsis(&turns, &config)
                {
                    Some(synopsis) => {
                        let _ = self.repo.store_session_synopsis(
                            &context.session_id,
                            &synopsis,
                            transcript.len(),
                            0,
                        );
                        format!("Synopsis: {synopsis}")
                    }
                    None => format!(
                        "Not enough assistant content to generate a synopsis ({} messages)",
                        transcript.len()
                    ),
                };

                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::Thinking(ThinkingCommand::Status) => {
                let mode = match self.thinking_display_mode {
                    ThinkingDisplayMode::Hidden => "hidden",
                    ThinkingDisplayMode::Assisted => "assisted",
                    ThinkingDisplayMode::Debug => "debug",
                };
                Ok(Some(Self::status_only_refresh(format!(
                    "Thinking display: {mode}"
                ))))
            }
            ShellCommand::Thinking(ThinkingCommand::SetMode(mode)) => {
                self.thinking_display_mode = mode;
                let label = match mode {
                    ThinkingDisplayMode::Hidden => "hidden (thinking blocks stripped)",
                    ThinkingDisplayMode::Assisted => "assisted (thinking accumulated, not inline)",
                    ThinkingDisplayMode::Debug => "debug (thinking shown inline)",
                };
                Ok(Some(Self::status_only_refresh(format!(
                    "Thinking display set to {label}"
                ))))
            }
            ShellCommand::TierB(TierBCommand::Status) => {
                let active = self.is_tier_b_active();
                let tier_b = &self.inference.config().memory.tier_b;
                let status = if self.safe_mode {
                    "Tier B: OFF (safe mode)".to_owned()
                } else if !tier_b.enabled {
                    "Tier B: OFF (disabled in config)".to_owned()
                } else {
                    format!(
                        "Tier B: ON · importance_proposals={} retrieval_keys={} thinking_summaries={}",
                        tier_b.importance_proposals,
                        tier_b.retrieval_keys,
                        tier_b.thinking_summaries,
                    )
                };
                let _ = active;
                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::TierB(TierBCommand::Toggle) => {
                self.safe_mode = !self.safe_mode;
                let status = if self.safe_mode {
                    "Safe mode ON — Tier B features disabled"
                } else {
                    "Safe mode OFF — Tier B features enabled"
                };
                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::Hooks(HooksCommand::Status) => {
                let has_pre = self.hooks_config.pre_generation.is_some();
                let has_post = self.hooks_config.post_generation.is_some();
                Ok(Some(Self::status_only_refresh(format!(
                    "Hooks: pre_generation={} post_generation={}",
                    if has_pre { "configured" } else { "—" },
                    if has_post { "configured" } else { "—" },
                ))))
            }
            ShellCommand::Hooks(HooksCommand::List) => {
                if self.custom_commands.is_empty() {
                    Ok(Some(Self::status_only_refresh(
                        "No custom commands found in $XDG_CONFIG_HOME/ozone/commands/".to_owned(),
                    )))
                } else {
                    let list = self
                        .custom_commands
                        .iter()
                        .map(|cmd| {
                            if let Some(desc) = &cmd.description {
                                format!("  {}  — {desc}", cmd.name)
                            } else {
                                format!("  {}", cmd.name)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(Some(Self::status_only_refresh(format!(
                        "Custom commands:\n{list}"
                    ))))
                }
            }
            ShellCommand::SafeMode(SafeModeCommand::Status) => Ok(Some(Self::status_only_refresh(
                format!("Safe mode: {}", if self.safe_mode { "ON" } else { "OFF" }),
            ))),
            ShellCommand::SafeMode(SafeModeCommand::On) => {
                self.safe_mode = true;
                Ok(Some(Self::status_only_refresh(
                    "Safe mode ON — Tier B features disabled".to_owned(),
                )))
            }
            ShellCommand::SafeMode(SafeModeCommand::Off) => {
                self.safe_mode = false;
                Ok(Some(Self::status_only_refresh(
                    "Safe mode OFF — Tier B features enabled".to_owned(),
                )))
            }
            ShellCommand::SafeMode(SafeModeCommand::Toggle) => {
                self.safe_mode = !self.safe_mode;
                let status = if self.safe_mode {
                    "Safe mode ON — Tier B features disabled"
                } else {
                    "Safe mode OFF — Tier B features enabled"
                };
                Ok(Some(Self::status_only_refresh(status)))
            }
        }
    }

    fn persist_draft(
        &mut self,
        context: &TuiSessionContext,
        draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.save_persisted_draft(&context.session_id, draft)
    }

    fn list_sessions(&mut self) -> Result<Vec<ozone_tui::SessionListEntry>, Self::Error> {
        let sessions = self.repo.list_sessions().map_err(|e| e.to_string())?;
        Ok(sessions
            .into_iter()
            .map(|s| ozone_tui::SessionListEntry {
                session_id: s.session_id.to_string(),
                name: s.name.clone(),
                character_name: s.character_name.clone(),
                message_count: s.message_count as usize,
                last_active: Some(crate::format_timestamp_short(s.last_opened_at)),
                folder: s.folder().map(|f| f.to_owned()),
            })
            .collect())
    }

    fn get_settings(&mut self) -> Result<Vec<ozone_tui::SettingsEntry>, Self::Error> {
        let config = self.inference.config();
        let prefs = crate::load_prefs_sync();
        let mut entries = Vec::new();

        // ── Session (read-only diagnostics) ───────────────────────────────
        entries.push(ozone_tui::SettingsEntry {
            category: "Session".into(),
            key: "Session ID".into(),
            value: self.session_id.to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Session".into(),
            key: "Lock instance".into(),
            value: self.lock_instance_id.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        // ── Backend (read-only diagnostics) ───────────────────────────────
        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "Type".into(),
            value: config.backend.r#type.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "URL".into(),
            value: config.backend.url.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "Prompt template".into(),
            value: self.inference.selected_template().to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        // ── Model (read-only diagnostics) ─────────────────────────────────
        entries.push(ozone_tui::SettingsEntry {
            category: "Model".into(),
            key: "Max tokens".into(),
            value: config.context.max_tokens.to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Model".into(),
            key: "Safety margin".into(),
            value: format!("{}%", config.context.safety_margin_pct),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        // ── Display (editable) ────────────────────────────────────────────
        let ts_options = vec![
            "relative".to_string(),
            "absolute".to_string(),
            "off".to_string(),
        ];
        let ts_cur = ts_options
            .iter()
            .position(|o| o == &prefs.timestamp_style)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Display".into(),
            key: "Timestamp style".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: ts_options,
                current: ts_cur,
            },
            pref_key: "timestamp_style".into(),
        });

        let density_options = vec!["comfortable".to_string(), "compact".to_string()];
        let density_cur = density_options
            .iter()
            .position(|o| o == &prefs.message_density)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Display".into(),
            key: "Message density".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: density_options,
                current: density_cur,
            },
            pref_key: "message_density".into(),
        });

        // ── Appearance (editable) ─────────────────────────────────────────
        let theme_options = vec![
            "dark-mint".to_string(),
            "ozone-dark".to_string(),
            "high-contrast".to_string(),
        ];
        let theme_cur = theme_options
            .iter()
            .position(|o| o == &prefs.theme_preset)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Appearance".into(),
            key: "Theme".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: theme_options,
                current: theme_cur,
            },
            pref_key: "theme_preset".into(),
        });

        // ── Launch (editable) ─────────────────────────────────────────────
        entries.push(ozone_tui::SettingsEntry {
            category: "Launch".into(),
            key: "Side-by-side monitor".into(),
            value: String::new(),
            kind: EntryKind::Toggle(prefs.side_by_side_monitor),
            pref_key: "side_by_side_monitor".into(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Launch".into(),
            key: "Inspector on start".into(),
            value: String::new(),
            kind: EntryKind::Toggle(prefs.show_inspector),
            pref_key: "show_inspector".into(),
        });

        Ok(entries)
    }

    fn save_pref(&mut self, pref_key: &str, value: &str) -> Result<(), Self::Error> {
        let mut prefs = crate::load_prefs_sync();
        match pref_key {
            "theme_preset" => prefs.theme_preset = value.to_string(),
            "timestamp_style" => prefs.timestamp_style = value.to_string(),
            "message_density" => prefs.message_density = value.to_string(),
            "side_by_side_monitor" => {
                prefs.side_by_side_monitor = value.parse::<bool>().unwrap_or(false)
            }
            "show_inspector" => {
                prefs.show_inspector = value.parse::<bool>().unwrap_or(false)
            }
            _ => {}
        }
        crate::save_prefs_sync(&prefs)
    }

    fn set_session_folder(
        &mut self,
        session_id: &str,
        folder: Option<&str>,
    ) -> Result<(), Self::Error> {
        let sid = SessionId::parse(session_id).map_err(|e| e.to_string())?;
        self.repo
            .set_session_folder(&sid, folder)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn list_characters(&mut self) -> Result<Vec<ozone_tui::CharacterEntry>, Self::Error> {
        let chars = self
            .repo
            .list_characters_global()
            .map_err(|e| e.to_string())?;
        Ok(chars
            .into_iter()
            .map(|c| ozone_tui::CharacterEntry {
                card_id: c.card_id,
                name: c.name,
                description: c.description,
                session_count: 0,
            })
            .collect())
    }

    fn create_character(
        &mut self,
        name: String,
        system_prompt: String,
    ) -> Result<ozone_tui::CharacterEntry, Self::Error> {
        let stored = self
            .repo
            .create_character(&name, "", &system_prompt)
            .map_err(|e| e.to_string())?;
        Ok(ozone_tui::CharacterEntry {
            card_id: stored.card_id,
            name: stored.name,
            description: stored.description,
            session_count: 0,
        })
    }

    fn import_character(&mut self, path: String) -> Result<ozone_tui::CharacterEntry, Self::Error> {
        let contents =
            fs::read_to_string(&path).map_err(|e| format!("failed to read {path}: {e}"))?;
        let card =
            ozone_persist::CharacterCard::from_json_str(&contents).map_err(|e| e.to_string())?;
        let stored = self
            .repo
            .create_character_full(
                &card.name,
                card.description.as_deref().unwrap_or(""),
                card.system_prompt.as_deref().unwrap_or(""),
                card.personality.as_deref().unwrap_or(""),
                card.scenario.as_deref().unwrap_or(""),
                card.greeting.as_deref().unwrap_or(""),
                card.example_dialogue.as_deref().unwrap_or(""),
            )
            .map_err(|e| e.to_string())?;
        Ok(ozone_tui::CharacterEntry {
            card_id: stored.card_id,
            name: stored.name,
            description: stored.description,
            session_count: 0,
        })
    }
}

fn tui_branch_from_record(record: ConversationBranchRecord) -> TuiBranchItem {
    TuiBranchItem::new(
        record.branch.branch_id.to_string(),
        record.branch.name,
        record.branch.state == BranchState::Active,
    )
}

fn tui_transcript_item_from_message(
    message: ConversationMessage,
    is_bookmarked: bool,
) -> TuiTranscriptItem {
    let author = message
        .author_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| message.author_kind.clone());

    TuiTranscriptItem::persisted(
        message.message_id.to_string(),
        author,
        message.content,
        is_bookmarked,
    )
    .with_timestamp(crate::format_message_time(message.created_at))
}

fn tui_context_preview_from_plan(preview: &ContextPlanPreview) -> TuiContextPreview {
    let source = match preview.source {
        crate::context_bridge::ContextPlanSource::EnginePlan => "engine-plan",
        crate::context_bridge::ContextPlanSource::TranscriptFallback => "transcript-fallback",
    };

    let mut inline_status = format!("{} · {}", source, preview.summary);
    if let Some(token_budget) = preview.token_budget.as_ref() {
        inline_status.push_str(&format!(
            " · {} / {} tokens",
            token_budget.used_tokens, token_budget.max_tokens
        ));
    }

    TuiContextPreview {
        source: source.to_string(),
        summary: preview.summary.clone(),
        lines: preview.lines.clone(),
        selected_items: preview.selected_items,
        omitted_items: preview.omitted_items,
        token_budget: preview
            .token_budget
            .as_ref()
            .map(|budget| TuiContextTokenBudget {
                used_tokens: budget.used_tokens,
                max_tokens: budget.max_tokens,
            }),
        inline_status,
    }
}

fn tui_context_dry_run_from_build(dry_run: &DryRunContextBuild) -> TuiContextDryRunPreview {
    TuiContextDryRunPreview {
        summary: dry_run.result.preview.summary.clone(),
        built_at: dry_run.built_at,
    }
}

fn tui_recall_browser_from_state(
    pinned_memories: &[PinnedMemoryView],
    recent_search: Option<&RecentSearchSection>,
    memory: &MemoryConfig,
) -> TuiRecallBrowser {
    const MAX_SECTION_LINES: usize = 5;

    let active = pinned_memories
        .iter()
        .filter(|memory| memory.is_active)
        .collect::<Vec<_>>();
    let expired = pinned_memories
        .iter()
        .filter(|memory| memory.is_expired())
        .collect::<Vec<_>>();

    let mut lines = vec![format!("active pinned {}", active.len())];
    if active.is_empty() {
        lines.push("— none".into());
    } else {
        lines.extend(
            active
                .iter()
                .take(MAX_SECTION_LINES)
                .map(|pinned_memory| format_pinned_memory_browser_line(pinned_memory, memory)),
        );
        let omitted = active.len().saturating_sub(MAX_SECTION_LINES);
        if omitted > 0 {
            lines.push(format!("+{omitted} more active memories"));
        }
    }

    if !expired.is_empty() {
        lines.push(format!("expired pinned {}", expired.len()));
        lines.extend(
            expired
                .iter()
                .take(MAX_SECTION_LINES)
                .map(|pinned_memory| format_pinned_memory_browser_line(pinned_memory, memory)),
        );
        let omitted = expired.len().saturating_sub(MAX_SECTION_LINES);
        if omitted > 0 {
            lines.push(format!("+{omitted} more expired memories"));
        }
    }

    if let Some(search) = recent_search {
        lines.push(search.summary.clone());
        if search.lines.is_empty() {
            lines.push("— none".into());
        } else {
            lines.extend(search.lines.iter().take(MAX_SECTION_LINES).cloned());
            let omitted = search.lines.len().saturating_sub(MAX_SECTION_LINES);
            if omitted > 0 {
                lines.push(format!("+{omitted} more search hits"));
            }
        }
    }

    let mut summary_parts = vec![format!("{} active", active.len())];
    if !expired.is_empty() {
        summary_parts.push(format!("{} expired", expired.len()));
    }
    if let Some(search) = recent_search {
        summary_parts.push(format!(
            "{} recent hit{}",
            search.hit_count,
            hit_suffix(search.hit_count)
        ));
    }

    TuiRecallBrowser {
        title: "Recall".into(),
        summary: summary_parts.join(" · "),
        lines,
    }
}

fn recent_search_section(
    scope: &str,
    result: &ozone_memory::RetrievalResultSet,
    include_session: bool,
) -> RecentSearchSection {
    RecentSearchSection {
        summary: format!(
            "{scope} search \"{}\" · {} · {} hit{}",
            result.query,
            result.status.summary_line(),
            result.hits.len(),
            hit_suffix(result.hits.len())
        ),
        hit_count: result.hits.len(),
        lines: result
            .hits
            .iter()
            .map(|hit| format_retrieval_browser_line(hit, include_session))
            .collect(),
    }
}

fn format_retrieval_browser_line(
    hit: &ozone_memory::RetrievalHit,
    include_session: bool,
) -> String {
    let target = match hit.hit_kind {
        ozone_memory::RetrievalHitKind::Message => format!(
            "msg {}",
            hit.message_id
                .as_ref()
                .map(|message_id| short_id(message_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
        ozone_memory::RetrievalHitKind::PinnedMemory => format!(
            "memory {}",
            hit.artifact_id
                .as_ref()
                .map(|artifact_id| short_id(artifact_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
        ozone_memory::RetrievalHitKind::NoteMemory => format!(
            "note {}",
            hit.artifact_id
                .as_ref()
                .map(|artifact_id| short_id(artifact_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
    };
    let session_label = if include_session {
        match hit.session.character_name.as_deref() {
            Some(character_name) if !character_name.is_empty() => format!(
                "{} [{}] / {} · ",
                hit.session.session_name,
                character_name,
                short_id(hit.session.session_id.as_str())
            ),
            _ => format!(
                "{} / {} · ",
                hit.session.session_name,
                short_id(hit.session.session_id.as_str())
            ),
        }
    } else {
        String::new()
    };
    let actor = hit
        .author_kind
        .clone()
        .unwrap_or_else(|| hit.provenance.to_string());
    let state = if hit.source_state == ozone_memory::RetrievalSourceState::Current {
        String::new()
    } else {
        format!(" · {}", hit.source_state)
    };
    let lifecycle = hit
        .lifecycle
        .as_ref()
        .map(|lifecycle| crate::lifecycle_badges(lifecycle, true, true))
        .filter(|badges| !badges.is_empty())
        .map(|badges| format!(" · {}", badges.join(" · ")))
        .unwrap_or_default();

    format!(
        "{}{} · s={:.2} t={:.2} v={:.2} p={:.2} · {}{}{} · {}",
        session_label,
        target,
        hit.overall_score(),
        hit.score.text_contribution,
        hit.score.vector_contribution,
        hit.score.provenance_contribution,
        actor,
        state,
        lifecycle,
        compact_line(&hit.text, 56)
    )
}

fn format_pinned_memory_browser_line(
    pinned_memory: &PinnedMemoryView,
    memory_config: &MemoryConfig,
) -> String {
    let source = pinned_memory
        .record
        .source_message_id
        .as_ref()
        .map(|message_id| format!("src {}", short_id(message_id.as_str())))
        .unwrap_or_else(|| "note".to_owned());
    let expiry = match pinned_memory.remaining_turns {
        Some(remaining) if pinned_memory.is_active => format!("{remaining} turns left"),
        Some(_) => "expired".to_owned(),
        None => "no expiry".to_owned(),
    };
    let lifecycle = crate::pinned_memory_lifecycle_summary(memory_config, pinned_memory);
    let lifecycle = crate::lifecycle_badges(&lifecycle, false, false);
    let lifecycle = if lifecycle.is_empty() {
        String::new()
    } else {
        format!(" · {}", lifecycle.join(" · "))
    };

    format!(
        "{} · {} · {} · {}{} · {}",
        short_id(pinned_memory.record.artifact_id.as_str()),
        pinned_memory.record.provenance,
        source,
        expiry,
        lifecycle,
        compact_line(&pinned_memory.record.content.text, 72)
    )
}

fn compact_line(content: &str, max_chars: usize) -> String {
    let flattened = content.replace('\n', " ");
    let snippet: String = flattened.chars().take(max_chars).collect();
    if flattened.chars().count() > max_chars {
        format!("{snippet}…")
    } else {
        snippet
    }
}

fn short_id(value: &str) -> String {
    let snippet: String = value.chars().take(8).collect();
    if value.chars().count() > 8 {
        format!("{snippet}…")
    } else {
        snippet
    }
}

fn hit_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn parse_shell_command(input: &str) -> Result<ShellCommand, String> {
    let trimmed = input.trim();

    if let Some(alias) = trimmed.strip_prefix(':') {
        return match alias.trim() {
            "memories" => Ok(ShellCommand::Memory(MemoryCommand::List)),
            _ => Err(unknown_shell_command_message()),
        };
    }

    let command = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = command.splitn(2, char::is_whitespace);
    let root = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();

    match root {
        "session" => parse_session_subcommand(remainder).map(ShellCommand::Session),
        "memory" => parse_memory_subcommand(remainder).map(ShellCommand::Memory),
        "memories" if remainder.is_empty() => Ok(ShellCommand::Memory(MemoryCommand::List)),
        "search" => parse_search_subcommand(remainder).map(ShellCommand::Search),
        "summarize" => parse_summarize_subcommand(remainder).map(ShellCommand::Summarize),
        "thinking" => parse_thinking_subcommand(remainder).map(ShellCommand::Thinking),
        "tierb" => parse_tierb_subcommand(remainder).map(ShellCommand::TierB),
        "hooks" => parse_hooks_subcommand(remainder).map(ShellCommand::Hooks),
        "safemode" => parse_safemode_subcommand(remainder).map(ShellCommand::SafeMode),
        _ => Err(unknown_shell_command_message()),
    }
}

fn parse_session_subcommand(remainder: &str) -> Result<SessionCommand, String> {
    if remainder.eq_ignore_ascii_case("show") {
        return Ok(SessionCommand::Show);
    }

    let mut subcommand_parts = remainder.splitn(2, char::is_whitespace);
    let subcommand = subcommand_parts.next().unwrap_or_default();
    let argument = subcommand_parts.next().unwrap_or_default().trim();

    match subcommand {
        "rename" => Ok(SessionCommand::Rename(require_non_empty(
            "session name",
            argument.to_owned(),
        )?)),
        "character" => {
            if argument.eq_ignore_ascii_case("clear") || argument == "-" {
                Ok(SessionCommand::Character(None))
            } else {
                Ok(SessionCommand::Character(Some(require_non_empty(
                    "character name",
                    argument.to_owned(),
                )?)))
            }
        }
        "tags" => {
            if argument.eq_ignore_ascii_case("clear") || argument == "-" {
                Ok(SessionCommand::Tags(Vec::new()))
            } else {
                let tags = normalize_tags(argument);
                if tags.is_empty() {
                    Err("Session tags command expects comma-separated tags or `clear`".to_owned())
                } else {
                    Ok(SessionCommand::Tags(tags))
                }
            }
        }
        _ => Err("Unknown session command. Try show, rename, character, or tags".to_owned()),
    }
}

fn parse_memory_subcommand(remainder: &str) -> Result<MemoryCommand, String> {
    if remainder.is_empty() || remainder.eq_ignore_ascii_case("list") {
        return Ok(MemoryCommand::List);
    }

    let mut parts = remainder.splitn(2, char::is_whitespace);
    let subcommand = parts.next().unwrap_or_default();
    let argument = parts.next().unwrap_or_default().trim();

    match subcommand {
        "note" => Ok(MemoryCommand::Note(require_non_empty(
            "memory note",
            argument.to_owned(),
        )?)),
        "unpin" => Ok(MemoryCommand::Unpin(
            MemoryArtifactId::parse(require_non_empty("artifact id", argument.to_owned())?)
                .map_err(|error| error.to_string())?,
        )),
        _ => Err(
            "Unknown memory command. Try /memory list | /memory note TEXT | /memory unpin <artifact-id> | :memories"
                .to_owned(),
        ),
    }
}

fn parse_search_subcommand(remainder: &str) -> Result<SearchCommand, String> {
    let mut parts = remainder.splitn(2, char::is_whitespace);
    let scope = parts.next().unwrap_or_default();
    let query = parts.next().unwrap_or_default().trim();

    match scope {
        "session" => Ok(SearchCommand::Session(require_non_empty(
            "search query",
            query.to_owned(),
        )?)),
        "global" => Ok(SearchCommand::Global(require_non_empty(
            "search query",
            query.to_owned(),
        )?)),
        _ => Err(
            "Unknown search command. Try /search session QUERY | /search global QUERY".to_owned(),
        ),
    }
}

fn parse_summarize_subcommand(remainder: &str) -> Result<SummarizeShellCommand, String> {
    match remainder.trim() {
        "session" | "" => Ok(SummarizeShellCommand::Session),
        _ => Err("Usage: /summarize session".to_string()),
    }
}

fn parse_thinking_subcommand(remainder: &str) -> Result<ThinkingCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(ThinkingCommand::Status),
        "hidden" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Hidden)),
        "assisted" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Assisted)),
        "debug" => Ok(ThinkingCommand::SetMode(ThinkingDisplayMode::Debug)),
        _ => Err("Usage: /thinking [status|hidden|assisted|debug]".to_string()),
    }
}

fn parse_tierb_subcommand(remainder: &str) -> Result<TierBCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(TierBCommand::Status),
        "toggle" => Ok(TierBCommand::Toggle),
        _ => Err("Usage: /tierb [status|toggle]".to_string()),
    }
}

fn parse_hooks_subcommand(remainder: &str) -> Result<HooksCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(HooksCommand::Status),
        "list" => Ok(HooksCommand::List),
        _ => Err("Usage: /hooks [status|list]".to_string()),
    }
}

fn parse_safemode_subcommand(remainder: &str) -> Result<SafeModeCommand, String> {
    match remainder.trim() {
        "status" | "" => Ok(SafeModeCommand::Status),
        "on" => Ok(SafeModeCommand::On),
        "off" => Ok(SafeModeCommand::Off),
        "toggle" => Ok(SafeModeCommand::Toggle),
        _ => Err("Usage: /safemode [status|on|off|toggle]".to_string()),
    }
}

fn unknown_shell_command_message() -> String {
    "Unknown command. Try /session show|rename|character|tags | /memory list|note|unpin | \
/search session|global | /summarize session | /thinking status|hidden|assisted|debug | \
/tierb status|toggle | /hooks status|list | /safemode status|on|off|toggle | :memories"
        .to_owned()
}

fn require_non_empty(label: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }

    Ok(trimmed.to_owned())
}

fn normalize_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|tag| {
            let trimmed = tag.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .collect()
}

fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".to_owned()
    } else {
        tags.join(", ")
    }
}

fn repository_template_dir() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join("templates");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_core::{engine::MessageId, session::SessionId};
    use ozone_persist::{MemoryArtifactId, PinnedMemoryContent, PinnedMemoryRecord};

    fn pinned_memory(
        text: &str,
        ordinal: u8,
        remaining_turns: Option<u32>,
        is_active: bool,
    ) -> PinnedMemoryView {
        pinned_memory_with_turns(
            text,
            ordinal,
            remaining_turns,
            if is_active { 0 } else { 1 },
            is_active,
        )
    }

    fn pinned_memory_with_turns(
        text: &str,
        ordinal: u8,
        remaining_turns: Option<u32>,
        turns_elapsed: u64,
        is_active: bool,
    ) -> PinnedMemoryView {
        let artifact_id =
            MemoryArtifactId::parse(format!("123e4567-e89b-12d3-a456-4266141740{ordinal:02}"))
                .unwrap();
        let message_id =
            MessageId::parse(format!("223e4567-e89b-12d3-a456-4266141740{ordinal:02}")).unwrap();
        let session_id = SessionId::parse("323e4567-e89b-12d3-a456-426614174000").unwrap();

        PinnedMemoryView {
            record: PinnedMemoryRecord {
                artifact_id,
                session_id,
                content: PinnedMemoryContent {
                    text: text.to_owned(),
                    pinned_by: AuthorId::User,
                    expires_after_turns: remaining_turns.or(Some(1)),
                },
                source_message_id: Some(message_id),
                provenance: Provenance::UserAuthored,
                created_at: crate::now_timestamp_ms(),
                snapshot_version: 1,
            },
            turns_elapsed,
            remaining_turns,
            is_active,
        }
    }

    #[test]
    fn parses_memory_search_and_memories_commands() {
        assert_eq!(
            parse_shell_command("/memory list"),
            Ok(ShellCommand::Memory(MemoryCommand::List))
        );
        assert_eq!(
            parse_shell_command(":memories"),
            Ok(ShellCommand::Memory(MemoryCommand::List))
        );
        assert_eq!(
            parse_shell_command("/memory note Remember the blue lamp"),
            Ok(ShellCommand::Memory(MemoryCommand::Note(
                "Remember the blue lamp".into()
            )))
        );
        assert_eq!(
            parse_shell_command("/memory unpin 123e4567-e89b-12d3-a456-426614174000"),
            Ok(ShellCommand::Memory(MemoryCommand::Unpin(
                MemoryArtifactId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap()
            )))
        );
        assert_eq!(
            parse_shell_command("/search session observatory key"),
            Ok(ShellCommand::Search(SearchCommand::Session(
                "observatory key".into()
            )))
        );
        assert_eq!(
            parse_shell_command("/search global observatory"),
            Ok(ShellCommand::Search(SearchCommand::Global(
                "observatory".into()
            )))
        );
    }

    #[test]
    fn recall_browser_includes_lifecycle_labels_for_memories_and_hits() {
        let memory = MemoryConfig::default();
        let result = ozone_memory::RetrievalResultSet {
            query: "observatory".into(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 0,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("423e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Observatory".into(),
                    character_name: None,
                    tags: vec!["phase2b".into()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(MessageId::parse("523e4567-e89b-12d3-a456-426614174000").unwrap()),
                source_message_id: None,
                author_kind: Some("assistant".into()),
                text: "The key rests under the lamp.".into(),
                created_at: crate::now_timestamp_ms(),
                provenance: Provenance::UtilityModel,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                    storage_tier: ozone_memory::StorageTier::Minimal,
                    age_messages: 800,
                    age_hours: 2,
                    is_stale: true,
                    adjusted_provenance_score: 0.61,
                }),
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: Some(-1.2),
                    text_score: 0.9,
                    vector_similarity: Some(0.8),
                    importance_score: 0.45,
                    recency_score: 0.7,
                    provenance: Provenance::UtilityModel,
                    stale_penalty: 1.0,
                }
                .score(
                    &ozone_memory::RetrievalWeights::default(),
                    &ozone_memory::ProvenanceWeights::default(),
                ),
            }],
        };
        let browser = tui_recall_browser_from_state(
            &[
                pinned_memory("Remember the observatory key.", 1, Some(2), true),
                pinned_memory_with_turns("Expired fallback.", 2, Some(0), 1_000, false),
            ],
            Some(&recent_search_section("session", &result, false)),
            &memory,
        );

        assert_eq!(browser.title, "Recall");
        assert!(browser.summary.contains("1 active"));
        assert!(browser.summary.contains("1 expired"));
        assert!(browser.summary.contains("1 recent hit"));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Remember the observatory key.")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("session search \"observatory\"")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.") && line.contains("tier minimal")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.") && line.contains("⚠ stale")));
        assert!(browser.lines.iter().any(
            |line| line.contains("The key rests under the lamp.") && line.contains("prov 0.61")
        ));
    }

    #[test]
    fn recent_search_section_displays_mode_and_score_breakdown() {
        let result = ozone_memory::RetrievalResultSet {
            query: "observatory".into(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 1,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("423e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Observatory".into(),
                    character_name: None,
                    tags: vec!["phase2b".into()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(MessageId::parse("523e4567-e89b-12d3-a456-426614174000").unwrap()),
                source_message_id: None,
                author_kind: Some("assistant".into()),
                text: "The key rests under the lamp.".into(),
                created_at: 1_700_000_000_100,
                provenance: Provenance::UtilityModel,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                    storage_tier: ozone_memory::StorageTier::Minimal,
                    age_messages: 600,
                    age_hours: 12,
                    is_stale: true,
                    adjusted_provenance_score: 0.61,
                }),
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: Some(-1.2),
                    text_score: 0.9,
                    vector_similarity: Some(0.8),
                    importance_score: 0.45,
                    recency_score: 0.7,
                    provenance: Provenance::UtilityModel,
                    stale_penalty: 1.0,
                }
                .score(
                    &ozone_memory::RetrievalWeights::default(),
                    &ozone_memory::ProvenanceWeights::default(),
                ),
            }],
        };

        let section = recent_search_section("session", &result, false);
        assert!(section.summary.contains("hybrid"));
        assert!(section.summary.contains("filtered 1 stale embedding"));
        assert!(section.lines[0].contains("s="));
        assert!(section.lines[0].contains("t="));
        assert!(section.lines[0].contains("v="));
        assert!(section.lines[0].contains("tier minimal"));
        assert!(section.lines[0].contains("⚠ stale"));
        assert!(section.lines[0].contains("prov 0.61"));
        assert!(section.lines[0].contains("The key rests under the lamp."));
    }

    // ── Phase 3 cleanup-e: integration tests ────────────────────────────────

    #[test]
    fn parse_thinking_commands() {
        assert_eq!(
            parse_shell_command("/thinking"),
            Ok(ShellCommand::Thinking(ThinkingCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/thinking status"),
            Ok(ShellCommand::Thinking(ThinkingCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/thinking hidden"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Hidden
            )))
        );
        assert_eq!(
            parse_shell_command("/thinking assisted"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Assisted
            )))
        );
        assert_eq!(
            parse_shell_command("/thinking debug"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Debug
            )))
        );
        assert!(parse_shell_command("/thinking bogus").is_err());
    }

    #[test]
    fn parse_tierb_commands() {
        assert_eq!(
            parse_shell_command("/tierb"),
            Ok(ShellCommand::TierB(TierBCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/tierb status"),
            Ok(ShellCommand::TierB(TierBCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/tierb toggle"),
            Ok(ShellCommand::TierB(TierBCommand::Toggle))
        );
        assert!(parse_shell_command("/tierb bogus").is_err());
    }

    #[test]
    fn parse_hooks_commands() {
        assert_eq!(
            parse_shell_command("/hooks"),
            Ok(ShellCommand::Hooks(HooksCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/hooks status"),
            Ok(ShellCommand::Hooks(HooksCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/hooks list"),
            Ok(ShellCommand::Hooks(HooksCommand::List))
        );
        assert!(parse_shell_command("/hooks bogus").is_err());
    }

    #[test]
    fn parse_safemode_commands() {
        assert_eq!(
            parse_shell_command("/safemode"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/safemode status"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Status))
        );
        assert_eq!(
            parse_shell_command("/safemode on"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::On))
        );
        assert_eq!(
            parse_shell_command("/safemode off"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Off))
        );
        assert_eq!(
            parse_shell_command("/safemode toggle"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Toggle))
        );
        assert!(parse_shell_command("/safemode bogus").is_err());
    }

    #[test]
    fn thinking_decoder_feed_splits_think_blocks() {
        use ozone_engine::{ThinkingDisplayMode, ThinkingOutput};
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let outputs = dec.feed("hello <think>reasoning</think> world");
        let texts: Vec<_> = outputs
            .iter()
            .filter_map(|o| {
                if let ThinkingOutput::Content(t) = o {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        let thinking: Vec<_> = outputs
            .iter()
            .filter_map(|o| {
                if let ThinkingOutput::Thinking(t) = o {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains("hello")));
        assert!(texts.iter().any(|t| t.contains("world")));
        assert!(thinking.iter().any(|t| t.contains("reasoning")));
    }

    #[test]
    fn thinking_decoder_feed_empty_input() {
        use ozone_engine::ThinkingDisplayMode;
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Hidden);
        let outputs = dec.feed("");
        assert!(outputs.is_empty());
    }

    #[test]
    fn thinking_decoder_feed_partial_chunks() {
        use ozone_engine::{ThinkingDisplayMode, ThinkingOutput};
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Assisted);
        // feed in two partial chunks that together form a complete think block
        let o1 = dec.feed("start <thi");
        let o2 = dec.feed("nk>inner</think> end");
        let all: Vec<_> = o1.into_iter().chain(o2).collect();
        let has_content = all.iter().any(|o| matches!(o, ThinkingOutput::Content(_)));
        assert!(
            has_content,
            "expected some Content output across both chunks"
        );
    }

    #[test]
    fn unknown_shell_command_message_lists_new_commands() {
        let msg = unknown_shell_command_message();
        assert!(msg.contains("/thinking"));
        assert!(msg.contains("/tierb"));
        assert!(msg.contains("/hooks"));
        assert!(msg.contains("/safemode"));
    }
}
