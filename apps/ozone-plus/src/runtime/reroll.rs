use ozone_core::engine::{
    ActivateSwipeCommand, BranchState, CommitMessageCommand, ConversationBranch,
    ConversationMessage, RecordSwipeCandidateCommand, SwipeCandidate, SwipeGroup,
};
use ozone_engine::{
    ActivateSwipeRequest, ConversationEngine, ConversationStore, EngineCommand,
    EngineCommandResult, RecordSwipeCandidateRequest,
};
use ozone_tui::{
    RuntimeContextRefresh as TuiRuntimeContextRefresh,
    RuntimeSendReceipt as TuiRuntimeSendReceipt, SessionContext as TuiSessionContext,
};

use super::{
    tui_context_dry_run_from_build, tui_context_preview_from_plan,
    tui_transcript_item_from_message, PendingCompletion, PendingGeneration, PendingReroll,
    OzonePlusRuntime, RerollBranchMode, RerollSource,
};

impl OzonePlusRuntime {
    pub(super) fn resolve_reroll_source(
        &self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<RerollSource, String> {
        let message_id = ozone_core::engine::MessageId::parse(message_id)
            .map_err(|error| error.to_string())?;
        let active_branch = self.active_branch(&context.session_id)?;
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        let assistant_index = transcript
            .iter()
            .position(|message| message.message_id == message_id)
            .ok_or_else(|| "Selected message is no longer on the active branch".to_owned())?;
        let assistant_message = transcript[assistant_index].clone();
        if assistant_message.author_kind != "assistant" {
            return Err("Only assistant messages can be rerolled".to_owned());
        }
        let parent_message_id = assistant_message.parent_id.clone().ok_or_else(|| {
            "Selected assistant message cannot be rerolled because it has no parent prompt"
                .to_owned()
        })?;
        let parent_user_index = transcript
            .iter()
            .position(|message| message.message_id == parent_message_id)
            .ok_or_else(|| {
                "Selected assistant message is missing its parent prompt on the active branch"
                    .to_owned()
            })?;
        let parent_user_message = transcript[parent_user_index].clone();
        if parent_user_message.author_kind != "user" {
            return Err(
                "Selected assistant message cannot be rerolled because its parent is not a user message"
                    .to_owned(),
            );
        }

        Ok(RerollSource {
            active_branch_id: active_branch.branch.branch_id,
            assistant_message,
            parent_user_message,
            parent_context_message_id: parent_user_index
                .checked_sub(1)
                .map(|index| transcript[index].message_id.clone()),
            transcript_prefix: transcript[..=parent_user_index].to_vec(),
        })
    }

    pub(super) fn ensure_reroll_swipe_group(
        &mut self,
        context: &TuiSessionContext,
        reroll: &PendingReroll,
        committed_message: &ConversationMessage,
    ) -> Result<(), String> {
        let existing_group = self
            .engine
            .store()
            .list_swipe_groups(&context.session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|group| group.parent_message_id == reroll.source.parent_user_message.message_id);

        let (group, next_ordinal) = match existing_group {
            Some(group) => {
                let next_ordinal = self
                    .engine
                    .store()
                    .list_swipe_candidates(&context.session_id, &group.swipe_group_id)
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|candidate| candidate.ordinal)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                (group, next_ordinal)
            }
            None => {
                let mut group = SwipeGroup::new(
                    crate::generate_swipe_group_id()?,
                    reroll.source.parent_user_message.message_id.clone(),
                );
                group.parent_context_message_id = reroll.source.parent_context_message_id.clone();
                match self
                    .engine
                    .process(EngineCommand::RecordSwipeCandidate(RecordSwipeCandidateRequest {
                        session_id: context.session_id.clone(),
                        command: RecordSwipeCandidateCommand {
                            group: group.clone(),
                            candidate: SwipeCandidate::new(
                                group.swipe_group_id.clone(),
                                0,
                                reroll.source.assistant_message.message_id.clone(),
                            ),
                        },
                    }))
                    .map_err(|error| error.to_string())?
                {
                    EngineCommandResult::SwipeCandidateRecorded(_) => {}
                    other => {
                        return Err(format!(
                            "unexpected engine result for original reroll swipe record: {other:?}"
                        ))
                    }
                }
                (group, 1)
            }
        };

        match self
            .engine
            .process(EngineCommand::RecordSwipeCandidate(RecordSwipeCandidateRequest {
                session_id: context.session_id.clone(),
                command: RecordSwipeCandidateCommand {
                    group: group.clone(),
                    candidate: SwipeCandidate::new(
                        group.swipe_group_id.clone(),
                        next_ordinal,
                        committed_message.message_id.clone(),
                    ),
                },
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::SwipeCandidateRecorded(_) => {}
            other => {
                return Err(format!(
                    "unexpected engine result for reroll swipe record: {other:?}"
                ))
            }
        }

        match self
            .engine
            .process(EngineCommand::ActivateSwipe(ActivateSwipeRequest {
                session_id: context.session_id.clone(),
                command: ActivateSwipeCommand {
                    swipe_group_id: group.swipe_group_id,
                    ordinal: next_ordinal,
                },
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::SwipeActivated(_) => Ok(()),
            other => Err(format!(
                "unexpected engine result for reroll swipe activation: {other:?}"
            )),
        }
    }

    pub(super) fn complete_reroll_generation(
        &mut self,
        context: &TuiSessionContext,
        pending: &PendingGeneration,
        reroll: &PendingReroll,
    ) -> Result<(ConversationMessage, TuiRuntimeContextRefresh), String> {
        let branch_id = match reroll.branch_mode {
            RerollBranchMode::CurrentBranch => {
                self.repo
                    .set_branch_tip(
                        &context.session_id,
                        &reroll.source.active_branch_id,
                        &reroll.source.parent_user_message.message_id,
                    )
                    .map_err(|error| error.to_string())?;
                reroll.source.active_branch_id.clone()
            }
            RerollBranchMode::NewBranch => {
                let mut branch = ConversationBranch::new(
                    crate::generate_branch_id()?,
                    context.session_id.clone(),
                    "reroll".to_owned(),
                    reroll.source.parent_user_message.message_id.clone(),
                    crate::now_timestamp_ms(),
                );
                branch.state = BranchState::Active;
                match self
                    .engine
                    .process(EngineCommand::CreateBranch(ozone_core::engine::CreateBranchCommand {
                        branch,
                        forked_from: reroll.source.parent_user_message.message_id.clone(),
                    }))
                    .map_err(|error| error.to_string())?
                {
                    EngineCommandResult::BranchCreated(record) => record.branch.branch_id,
                    other => {
                        return Err(format!(
                            "unexpected engine result for reroll branch create: {other:?}"
                        ))
                    }
                }
            }
        };

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
        assistant_message.parent_id = Some(reroll.source.parent_user_message.message_id.clone());

        let committed = match self
            .engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id: branch_id.clone(),
                message: assistant_message,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageCommitted(message) => message,
            other => {
                return Err(format!(
                    "unexpected engine result for reroll completion: {other:?}"
                ))
            }
        };

        self.ensure_reroll_swipe_group(context, reroll, &committed)?;
        let session_title = self.maybe_auto_title_session(context)?;
        self.refresh_context_cache(context);
        let status = match reroll.branch_mode {
            RerollBranchMode::CurrentBranch => "Rerolled reply on current branch",
            RerollBranchMode::NewBranch => "Rerolled reply on new branch",
        };
        let mut refresh = self.build_session_refresh(context, status)?;
        if let Some(session_title) = session_title {
            refresh.session_title = Some(session_title);
        }
        Ok((committed, refresh))
    }

    pub(super) fn reroll_message_impl(
        &mut self,
        context: &TuiSessionContext,
        message_id: &str,
    ) -> Result<Option<TuiRuntimeSendReceipt>, String> {
        if self.pending_generation.is_some() {
            return Ok(None);
        }

        let source = self.resolve_reroll_source(context, message_id)?;
        let request_id = crate::generate_request_id()?;
        self.set_generation_state(
            source.active_branch_id.clone(),
            ozone_core::engine::GenerationState::Queued {
                request_id: request_id.clone(),
            },
        )?;

        let receipt = TuiRuntimeSendReceipt {
            request_id: request_id.to_string(),
            user_message: tui_transcript_item_from_message(source.parent_user_message.clone(), false),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        };

        let context_build = match self.build_context_for_transcript(context, &source.transcript_prefix)
        {
            Ok(context_build) => context_build,
            Err(error) => {
                self.pending_generation = Some(PendingGeneration::failed(
                    source.active_branch_id.clone(),
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
        let mut pending = self
            .start_generation_task(
                source.active_branch_id.clone(),
                request_id.clone(),
                prompt,
                thinking_mode,
            )
            .unwrap_or_else(|error| {
                PendingGeneration::failed(source.active_branch_id.clone(), request_id, error)
            });
        pending.completion = PendingCompletion::Reroll(PendingReroll {
            branch_mode: if source.assistant_message.message_id
                == self.active_branch(&context.session_id)?.branch.tip_message_id
            {
                RerollBranchMode::CurrentBranch
            } else {
                RerollBranchMode::NewBranch
            },
            source,
        });
        self.pending_generation = Some(pending);

        Ok(Some(TuiRuntimeSendReceipt {
            context_preview,
            context_dry_run,
            ..receipt
        }))
    }
}