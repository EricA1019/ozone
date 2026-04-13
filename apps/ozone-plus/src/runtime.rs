use crate::{
    context_bridge::{
        AppContextBridge, ContextBuildResult, ContextPlanPreview, DryRunContextBuild,
    },
    inference_adapter::{InferenceAdapter, InferenceAdapterInit},
};
use ozone_core::engine::{
    BranchId, BranchState, CancelReason, CommitMessageCommand, ConversationMessage,
    GenerationState, RequestId, SetGenerationStateCommand,
};
use ozone_engine::{
    ConversationBranchRecord, ConversationEngine, ConversationStore, EngineCommand,
    EngineCommandResult, SingleWriterConversationEngine,
};
use ozone_inference::{InferenceError, StreamChunk};
use ozone_persist::{PersistError, SessionId, SqliteRepository, UpdateSessionRequest};
use ozone_tui::{
    AppBootstrap as TuiBootstrap, BranchItem as TuiBranchItem,
    ContextDryRunPreview as TuiContextDryRunPreview, ContextPreview as TuiContextPreview,
    ContextTokenBudget as TuiContextTokenBudget, DraftState as TuiDraftState, GenerationPoll,
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

enum SessionCommand {
    Show,
    Rename(String),
    Character(Option<String>),
    Tags(Vec<String>),
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
        })
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

        let mut bootstrap = TuiBootstrap {
            transcript: snapshot.transcript,
            branches: snapshot.branches,
            status_line: Some(format!(
                "{} backend ready ({}, template {}) · session locked by {} · b bookmark · Ctrl+D dry run · Ctrl+I inspector",
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
                    let (stream_tx, mut stream_rx) = tokio_mpsc::channel::<StreamChunk>(128);
                    let stream_gateway = gateway.clone();
                    let stream_task = tokio::spawn(async move {
                        stream_gateway
                            .stream_with_retry(request, stream_tx, cancel_rx, 0)
                            .await
                    });

                    while let Some(chunk) = stream_rx.recv().await {
                        match chunk {
                            StreamChunk::Token(token) => {
                                if event_tx.send(WorkerEvent::Token(token)).is_err() {
                                    return;
                                }
                            }
                            StreamChunk::FinishReason(_) => {}
                            StreamChunk::Done => {
                                let _ = event_tx.send(WorkerEvent::Finished);
                            }
                        }
                    }

                    match stream_task.await {
                        Ok(Ok(_)) => {
                            let _ = event_tx.send(WorkerEvent::Finished);
                        }
                        Ok(Err(error)) => {
                            if error
                                .downcast_ref::<InferenceError>()
                                .is_some_and(|inner| matches!(inner, InferenceError::Cancelled))
                            {
                                let _ = event_tx.send(WorkerEvent::Cancelled);
                            } else {
                                let _ = event_tx.send(WorkerEvent::Failed(error.to_string()));
                            }
                        }
                        Err(error) => {
                            let _ = event_tx.send(WorkerEvent::Failed(format!(
                                "generation task join failure: {error}"
                            )));
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
        self.context_bridge
            .build_from_transcript(&transcript, &self.inference)
    }

    #[allow(dead_code)]
    pub fn latest_context_plan_preview(&self) -> Option<&ContextPlanPreview> {
        self.context_bridge.latest_plan_preview()
    }

    #[allow(dead_code)]
    pub fn latest_context_dry_run(&self) -> Option<&DryRunContextBuild> {
        self.context_bridge.latest_dry_run()
    }

    #[allow(dead_code)]
    pub fn status_line_context_preview_text(&self) -> String {
        self.context_bridge.status_line_preview_text()
    }

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
        self.context_bridge
            .dry_run_from_transcript(&transcript, &self.inference)
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

        self.pending_generation = Some(
            self.start_generation_task(
                active_branch.branch.branch_id.clone(),
                request_id.clone(),
                prompt,
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
                    pending.partial_content.push_str(&token);
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

    fn run_command(
        &mut self,
        context: &TuiSessionContext,
        input: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, Self::Error> {
        let command = match parse_session_command(input) {
            Ok(command) => command,
            Err(error) => return Ok(Some(Self::status_only_refresh(error))),
        };

        match command {
            SessionCommand::Show => {
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
                }))
            }
            SessionCommand::Rename(name) => {
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
            SessionCommand::Character(character_name) => {
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
            SessionCommand::Tags(tags) => {
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
        }
    }

    fn persist_draft(
        &mut self,
        context: &TuiSessionContext,
        draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.save_persisted_draft(&context.session_id, draft)
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

fn parse_session_command(input: &str) -> Result<SessionCommand, String> {
    let trimmed = input.trim();
    let command = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = command.splitn(2, char::is_whitespace);
    let root = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();

    if root != "session" {
        return Err("Unknown command. Try /session show | /session rename NAME | /session character NAME|clear | /session tags a,b|clear".to_owned());
    }

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
