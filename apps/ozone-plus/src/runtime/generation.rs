use ozone_core::engine::{
    BranchId, CancelReason, CommitMessageCommand, ConversationMessage, GenerationState,
    RequestId, MessageId,
};
use ozone_engine::{
    ConversationEngine, ConversationStore, EngineCommand, EngineCommandResult,
    ThinkingBlockDecoder, ThinkingDisplayMode, ThinkingOutput,
};
use ozone_inference::{InferenceError, StreamChunk};
use ozone_persist::{AuthorId, CreateNoteMemoryRequest, Provenance, UpdateSessionRequest};
use ozone_tui::{
    GenerationPoll, RuntimeCancellation as TuiRuntimeCancellation,
    RuntimeCompletion as TuiRuntimeCompletion, RuntimeFailure as TuiRuntimeFailure,
    RuntimeProgress as TuiRuntimeProgress, RuntimeSendReceipt as TuiRuntimeSendReceipt,
    SessionContext as TuiSessionContext, TranscriptItem as TuiTranscriptItem,
};
use std::{sync::mpsc::{self, TryRecvError}, time::Instant};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};
use super::{
    tui_context_dry_run_from_build, tui_context_preview_from_plan,
    tui_transcript_item_from_message, Phase1dRuntime,
};

// Types related to generation worker events and pending generation state.
// These were previously declared in `runtime.rs`; moving them here keeps the
// generation orchestration colocated while runtime re-exports them for tests.
#[derive(Debug)]
pub(crate) enum WorkerEvent {
    Token(String),
    Finished,
    Failed(String),
    Cancelled,
}

pub(crate) struct PendingGeneration {
    pub(crate) branch_id: BranchId,
    pub(crate) request_id: RequestId,
    pub(crate) started_at: Instant,
    pub(crate) partial_content: String,
    pub(crate) thinking_content: String,
    pub(crate) thinking_decoder: ThinkingBlockDecoder,
    pub(crate) tokens_generated: u64,
    pub(crate) receiver: std::sync::mpsc::Receiver<WorkerEvent>,
    pub(crate) cancel_tx: Option<oneshot::Sender<()>>,
    pub(crate) completion: PendingCompletion,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum PendingCompletion {
    Standard,
    Reroll(PendingReroll),
}

#[derive(Debug, Clone)]
pub(crate) struct PendingReroll {
    pub(crate) source: RerollSource,
    pub(crate) branch_mode: RerollBranchMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RerollBranchMode {
    CurrentBranch,
    NewBranch,
}

#[derive(Debug, Clone)]
pub(crate) struct RerollSource {
    pub(crate) active_branch_id: BranchId,
    pub(crate) assistant_message: ConversationMessage,
    pub(crate) parent_user_message: ConversationMessage,
    pub(crate) parent_context_message_id: Option<MessageId>,
    pub(crate) transcript_prefix: Vec<ConversationMessage>,
}

impl PendingGeneration {
    pub(crate) fn failed(branch_id: BranchId, request_id: RequestId, error: String) -> Self {
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
            completion: PendingCompletion::Standard,
        }
    }
}
use crate::session_title;

fn apply_stream_token(pending: &mut PendingGeneration, token: &str) {
    let outputs = pending.thinking_decoder.feed(token);
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
}

impl Phase1dRuntime {
    pub(super) fn start_generation_task(
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

        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            if let Some(max_ctx) = gateway.probe_max_context_length().await {
                let prompt_chars = request.prompt.len();
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

            if !saw_done {
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
            }
        });

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
            completion: PendingCompletion::Standard,
        })
    }

    pub(super) fn send_draft_impl(
        &mut self,
        context: &TuiSessionContext,
        prompt: &str,
    ) -> Result<Option<TuiRuntimeSendReceipt>, String> {
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
            refresh: None,
            context_compression: None,
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
            refresh: None,
            ..receipt
        }))
    }

    pub(super) fn poll_generation_impl(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<GenerationPoll>, String> {
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
                    apply_stream_token(&mut pending, &token);
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
                content: pending.partial_content.clone(),
            };
            self.pending_generation = Some(pending);
            return Ok(Some(GenerationPoll::Pending {
                partial: Some(progress),
            }));
        }

        self.pending_generation = Some(pending);
        Ok(Some(GenerationPoll::Pending { partial: None }))
    }

    pub(super) fn cancel_generation_impl(
        &mut self,
        _context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeCancellation>, String> {
        let mut pending = match self.pending_generation.take() {
            Some(pending) => pending,
            None => return Ok(None),
        };

        while let Ok(event) = pending.receiver.try_recv() {
            if let WorkerEvent::Token(token) = event {
                apply_stream_token(&mut pending, &token);
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
            partial_assistant_message: partial.map(|text| TuiTranscriptItem::new("assistant", text)),
        }))
    }

    pub(super) fn complete_generation(
        &mut self,
        context: &TuiSessionContext,
        pending: PendingGeneration,
    ) -> Result<TuiRuntimeCompletion, String> {
        let thinking_content = pending.thinking_content.clone();
        let session_id_str = context.session_id.to_string();
        let (committed, refresh, session_title) = match &pending.completion {
            PendingCompletion::Standard => {
                let branch = self.branch_by_id(&context.session_id, &pending.branch_id)?;
                let mut assistant_message = ConversationMessage::new(
                    context.session_id.clone(),
                    crate::generate_message_id()?,
                    "assistant",
                    pending.partial_content.clone(),
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
                let session_title = self.maybe_auto_title_session(context)?;
                (committed, None, session_title)
            }
            PendingCompletion::Reroll(reroll) => {
                let (committed, refresh) =
                    self.complete_reroll_generation(context, &pending, reroll)?;
                (committed, Some(refresh), None)
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

        if self.is_tier_b_active() {
            let tier_b = self.inference.config().memory.tier_b.clone();

            if tier_b.importance_proposals {
                if let Some(_proposal) =
                    self.importance_scorer.propose(&committed.content, false, 0)
                {
                }
            }

            if tier_b.retrieval_keys {
                let _retrieval_key = self.keyword_extractor.to_retrieval_key(&committed.content);
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

        let _ = self
            .hooks_config
            .run_post_generation(&session_id_str, &committed.content);

        Ok(TuiRuntimeCompletion {
            request_id: pending.request_id.to_string(),
            message: tui_transcript_item_from_message(committed, false),
            session_title,
            refresh,
        })
    }

    pub(super) fn maybe_auto_title_session(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<String>, String> {
        let session = self
            .repo
            .get_session(&context.session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {} was not found", context.session_id))?;
        if !session_title::should_auto_title(&session.name) {
            return Ok(None);
        }

        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        let Some(title) =
            session_title::generate_session_title(session.character_name.as_deref(), &transcript)
        else {
            return Ok(None);
        };
        if title == session.name {
            return Ok(None);
        }

        self.repo
            .update_session_metadata(
                &context.session_id,
                UpdateSessionRequest {
                    name: Some(title.clone()),
                    ..UpdateSessionRequest::default()
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(Some(title))
    }

    pub(super) fn generate_session_title_for_current_context(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<String>, String> {
        let session = self
            .repo
            .get_session(&context.session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {} was not found", context.session_id))?;
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        Ok(session_title::generate_session_title(
            session.character_name.as_deref(),
            &transcript,
        ))
    }

    pub(super) fn mark_generation_failure(
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
            prompt: String::new(),
            message: error.clone(),
            reason: error,
        })
    }
}