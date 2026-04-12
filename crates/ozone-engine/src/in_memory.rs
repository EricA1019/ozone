use ozone_core::engine::{
    ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
    ConversationMessage, CreateBranchCommand, GenerationState, RecordSwipeCandidateCommand,
    SetGenerationStateCommand, SwipeCandidateState,
};
use ozone_core::session::SessionId;

use crate::command::EditMessageCommand;
use crate::error::{EngineError, EngineResult};
use crate::store::{ConversationState, ConversationStore, StoredBranch, StoredSwipeGroup};

#[derive(Debug, Clone)]
pub struct InMemoryConversationStore {
    state: ConversationState,
}

impl InMemoryConversationStore {
    pub fn bootstrap(
        root_branch: ConversationBranch,
        root_message: ConversationMessage,
    ) -> EngineResult<Self> {
        if root_branch.session_id != root_message.session_id {
            return Err(EngineError::invalid_command(
                "root branch and root message must belong to the same session",
            ));
        }

        if root_branch.tip_message_id != root_message.message_id {
            return Err(EngineError::invalid_command(
                "root branch tip must point at the root message",
            ));
        }

        let session_id = root_message.session_id.clone();
        let mut root_branch = root_branch;
        root_branch.state = BranchState::Active;

        let mut state = ConversationState::new(session_id);
        state
            .messages
            .insert(root_message.message_id.clone(), root_message.clone());
        state.branches.insert(
            root_branch.branch_id.clone(),
            StoredBranch::new(root_branch.clone(), root_message.message_id.clone()),
        );
        state.active_branch_id = Some(root_branch.branch_id.clone());

        Self::from_state(state)
    }

    pub fn from_state(state: ConversationState) -> EngineResult<Self> {
        validate_state(&state)?;
        Ok(Self { state })
    }

    pub fn session_id(&self) -> &SessionId {
        &self.state.session_id
    }
}

impl ConversationStore for InMemoryConversationStore {
    fn load(&self) -> EngineResult<ConversationState> {
        Ok(self.state.clone())
    }

    fn commit_message(&mut self, command: CommitMessageCommand) -> EngineResult<()> {
        let CommitMessageCommand {
            branch_id,
            mut message,
        } = command;

        if self.state.messages.contains_key(&message.message_id) {
            return Err(EngineError::already_exists(
                "message",
                message.message_id.to_string(),
            ));
        }

        if message.session_id != self.state.session_id {
            return Err(EngineError::invalid_command(format!(
                "message `{}` belongs to the wrong session",
                message.message_id
            )));
        }

        let branch_tip_message_id = self
            .state
            .branches
            .get(&branch_id)
            .map(|branch| branch.branch.tip_message_id.clone())
            .ok_or_else(|| EngineError::not_found("branch", branch_id.to_string()))?;

        match message.parent_id.as_ref() {
            Some(parent_id) if *parent_id != branch_tip_message_id => {
                return Err(EngineError::invalid_command(format!(
                    "message `{}` must append to branch `{branch_id}` tip `{branch_tip_message_id}`",
                    message.message_id
                )));
            }
            Some(parent_id) => {
                if !self.state.messages.contains_key(parent_id) {
                    return Err(EngineError::not_found("message", parent_id.to_string()));
                }
            }
            None => {
                message.parent_id = Some(branch_tip_message_id.clone());
            }
        }

        self.state
            .messages
            .insert(message.message_id.clone(), message.clone());
        let branch = self
            .state
            .branches
            .get_mut(&branch_id)
            .expect("branch existence was checked above");
        branch.branch.tip_message_id = message.message_id;

        Ok(())
    }

    fn edit_message(&mut self, command: EditMessageCommand) -> EngineResult<()> {
        let message = self
            .state
            .messages
            .get_mut(&command.message_id)
            .ok_or_else(|| EngineError::not_found("message", command.message_id.to_string()))?;

        message.content = command.new_content;
        message.edited_at = Some(command.edited_at);

        Ok(())
    }

    fn create_branch(&mut self, command: CreateBranchCommand) -> EngineResult<()> {
        let CreateBranchCommand {
            branch,
            forked_from,
        } = command;

        if self.state.branches.contains_key(&branch.branch_id) {
            return Err(EngineError::already_exists(
                "branch",
                branch.branch_id.to_string(),
            ));
        }

        if branch.session_id != self.state.session_id {
            return Err(EngineError::invalid_command(format!(
                "branch `{}` belongs to the wrong session",
                branch.branch_id
            )));
        }

        if !self.state.messages.contains_key(&forked_from) {
            return Err(EngineError::not_found("message", forked_from.to_string()));
        }

        if branch.tip_message_id != forked_from {
            return Err(EngineError::invalid_command(
                "new branches must begin at their forked_from message",
            ));
        }

        if branch.state == BranchState::Active {
            if let Some(previous_active_branch_id) = self.state.active_branch_id.clone() {
                if let Some(previous_active_branch) =
                    self.state.branches.get_mut(&previous_active_branch_id)
                {
                    previous_active_branch.branch.state = BranchState::Inactive;
                }
            }
            self.state.active_branch_id = Some(branch.branch_id.clone());
        }

        self.state.branches.insert(
            branch.branch_id.clone(),
            StoredBranch::new(branch, forked_from),
        );

        Ok(())
    }

    fn activate_branch(&mut self, branch_id: &BranchId) -> EngineResult<()> {
        if !self.state.branches.contains_key(branch_id) {
            return Err(EngineError::not_found("branch", branch_id.to_string()));
        }

        if self.state.active_branch_id.as_ref() == Some(branch_id) {
            return Ok(());
        }

        if let Some(previous_active_branch_id) = self.state.active_branch_id.clone() {
            if let Some(previous_active_branch) =
                self.state.branches.get_mut(&previous_active_branch_id)
            {
                previous_active_branch.branch.state = BranchState::Inactive;
            }
        }

        let branch = self
            .state
            .branches
            .get_mut(branch_id)
            .expect("branch existence was checked above");
        if matches!(
            branch.branch.state,
            BranchState::Archived | BranchState::Deleted
        ) {
            return Err(EngineError::invalid_command(format!(
                "branch `{branch_id}` cannot be activated from state `{}`",
                branch.branch.state
            )));
        }
        branch.branch.state = BranchState::Active;
        self.state.active_branch_id = Some(branch_id.clone());

        Ok(())
    }

    fn record_swipe_candidate(&mut self, command: RecordSwipeCandidateCommand) -> EngineResult<()> {
        let RecordSwipeCandidateCommand {
            group,
            mut candidate,
        } = command;

        if !self.state.messages.contains_key(&group.parent_message_id) {
            return Err(EngineError::not_found(
                "message",
                group.parent_message_id.to_string(),
            ));
        }

        if let Some(parent_context_message_id) = group.parent_context_message_id.as_ref() {
            if !self.state.messages.contains_key(parent_context_message_id) {
                return Err(EngineError::not_found(
                    "message",
                    parent_context_message_id.to_string(),
                ));
            }
        }

        if !self.state.messages.contains_key(&candidate.message_id) {
            return Err(EngineError::not_found(
                "message",
                candidate.message_id.to_string(),
            ));
        }

        let stored_group = self
            .state
            .swipe_groups
            .entry(group.swipe_group_id.clone())
            .or_insert_with(|| StoredSwipeGroup::new(group.clone()));

        if stored_group.group.parent_message_id != group.parent_message_id
            || stored_group.group.parent_context_message_id != group.parent_context_message_id
        {
            return Err(EngineError::invalid_command(format!(
                "swipe group `{}` was recorded with conflicting parent metadata",
                group.swipe_group_id
            )));
        }

        if stored_group.candidates.contains_key(&candidate.ordinal) {
            return Err(EngineError::already_exists(
                "swipe candidate",
                format!("{}:{}", group.swipe_group_id, candidate.ordinal),
            ));
        }

        candidate.state = if candidate.ordinal == stored_group.group.active_ordinal {
            SwipeCandidateState::Active
        } else {
            SwipeCandidateState::Discarded
        };

        stored_group.candidates.insert(candidate.ordinal, candidate);

        Ok(())
    }

    fn activate_swipe(&mut self, command: ActivateSwipeCommand) -> EngineResult<()> {
        let ActivateSwipeCommand {
            swipe_group_id,
            ordinal,
        } = command;

        let stored_group = self
            .state
            .swipe_groups
            .get_mut(&swipe_group_id)
            .ok_or_else(|| EngineError::not_found("swipe group", swipe_group_id.to_string()))?;

        if !stored_group.candidates.contains_key(&ordinal) {
            return Err(EngineError::not_found(
                "swipe candidate",
                format!("{swipe_group_id}:{ordinal}"),
            ));
        }

        let previously_active_ordinal = stored_group.group.active_ordinal;
        let activated_message_id = stored_group
            .candidates
            .get(&ordinal)
            .expect("candidate existence was checked above")
            .message_id
            .clone();

        stored_group.group.active_ordinal = ordinal;

        if let Some(previously_active_candidate) =
            stored_group.candidates.get_mut(&previously_active_ordinal)
        {
            if previously_active_ordinal != ordinal {
                previously_active_candidate.state = SwipeCandidateState::Discarded;
            }
        }

        let candidate_message_ids = stored_group
            .candidates
            .values()
            .map(|candidate| candidate.message_id.clone())
            .collect::<Vec<_>>();

        let activated_candidate = stored_group
            .candidates
            .get_mut(&ordinal)
            .expect("candidate existence was checked above");
        activated_candidate.state = SwipeCandidateState::Active;

        for branch in self.state.branches.values_mut() {
            if candidate_message_ids
                .iter()
                .any(|candidate_message_id| branch.branch.tip_message_id == *candidate_message_id)
            {
                branch.branch.tip_message_id = activated_message_id.clone();
            }
        }

        Ok(())
    }

    fn set_generation_state(&mut self, command: SetGenerationStateCommand) -> EngineResult<()> {
        let SetGenerationStateCommand { branch_id, state } = command;

        let branch = self
            .state
            .branches
            .get_mut(&branch_id)
            .ok_or_else(|| EngineError::not_found("branch", branch_id.to_string()))?;

        validate_generation_transition(&branch.branch.branch_id, &branch.generation_state, &state)?;
        branch.generation_state = state;

        Ok(())
    }
}

fn validate_state(state: &ConversationState) -> EngineResult<()> {
    if let Some(active_branch_id) = state.active_branch_id.as_ref() {
        let active_branch = state
            .branches
            .get(active_branch_id)
            .ok_or_else(|| EngineError::not_found("branch", active_branch_id.to_string()))?;
        if active_branch.branch.state != BranchState::Active {
            return Err(EngineError::invalid_command(format!(
                "active branch `{active_branch_id}` must be in the active state"
            )));
        }
    }

    for message in state.messages.values() {
        if message.session_id != state.session_id {
            return Err(EngineError::invalid_command(format!(
                "message `{}` belongs to the wrong session",
                message.message_id
            )));
        }

        if let Some(parent_id) = message.parent_id.as_ref() {
            if !state.messages.contains_key(parent_id) {
                return Err(EngineError::not_found("message", parent_id.to_string()));
            }
        }
    }

    for stored_branch in state.branches.values() {
        if stored_branch.branch.session_id != state.session_id {
            return Err(EngineError::invalid_command(format!(
                "branch `{}` belongs to the wrong session",
                stored_branch.branch.branch_id
            )));
        }

        if !state
            .messages
            .contains_key(&stored_branch.branch.tip_message_id)
        {
            return Err(EngineError::not_found(
                "message",
                stored_branch.branch.tip_message_id.to_string(),
            ));
        }

        if !state.messages.contains_key(&stored_branch.forked_from) {
            return Err(EngineError::not_found(
                "message",
                stored_branch.forked_from.to_string(),
            ));
        }
    }

    for stored_group in state.swipe_groups.values() {
        if !state
            .messages
            .contains_key(&stored_group.group.parent_message_id)
        {
            return Err(EngineError::not_found(
                "message",
                stored_group.group.parent_message_id.to_string(),
            ));
        }

        if let Some(parent_context_message_id) =
            stored_group.group.parent_context_message_id.as_ref()
        {
            if !state.messages.contains_key(parent_context_message_id) {
                return Err(EngineError::not_found(
                    "message",
                    parent_context_message_id.to_string(),
                ));
            }
        }

        if !stored_group
            .candidates
            .contains_key(&stored_group.group.active_ordinal)
            && !stored_group.candidates.is_empty()
        {
            return Err(EngineError::invalid_command(format!(
                "swipe group `{}` is missing its active ordinal `{}`",
                stored_group.group.swipe_group_id, stored_group.group.active_ordinal
            )));
        }

        for candidate in stored_group.candidates.values() {
            if !state.messages.contains_key(&candidate.message_id) {
                return Err(EngineError::not_found(
                    "message",
                    candidate.message_id.to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_generation_transition(
    branch_id: &BranchId,
    current: &GenerationState,
    next: &GenerationState,
) -> EngineResult<()> {
    if current == next {
        return Ok(());
    }

    let is_valid = match current {
        GenerationState::Idle => true,
        GenerationState::Queued { request_id } => match next {
            GenerationState::Queued {
                request_id: next_request_id,
            }
            | GenerationState::Streaming {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Completed {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Cancelled {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Failed {
                request_id: next_request_id,
                ..
            }
            | GenerationState::FailedMidStream {
                request_id: next_request_id,
                ..
            } => next_request_id == request_id,
            GenerationState::Idle => false,
        },
        GenerationState::Streaming { request_id, .. } => match next {
            GenerationState::Streaming {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Completed {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Cancelled {
                request_id: next_request_id,
                ..
            }
            | GenerationState::Failed {
                request_id: next_request_id,
                ..
            }
            | GenerationState::FailedMidStream {
                request_id: next_request_id,
                ..
            } => next_request_id == request_id,
            GenerationState::Idle | GenerationState::Queued { .. } => false,
        },
        GenerationState::Completed { .. }
        | GenerationState::Cancelled { .. }
        | GenerationState::Failed { .. }
        | GenerationState::FailedMidStream { .. } => {
            matches!(next, GenerationState::Idle | GenerationState::Queued { .. })
        }
    };

    if is_valid {
        Ok(())
    } else {
        Err(EngineError::InvalidGenerationTransition {
            branch_id: branch_id.clone(),
            from: current.clone(),
            to: next.clone(),
        })
    }
}
