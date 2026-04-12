use std::collections::BTreeMap;

use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, CommitMessageCommand, ConversationBranch,
        ConversationMessage, CreateBranchCommand, GenerationState, MessageId,
        RecordSwipeCandidateCommand, SetGenerationStateCommand, SwipeCandidate, SwipeGroup,
        SwipeGroupId,
    },
    session::SessionId,
};

use crate::{command::EditMessageCommand, error::EngineResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBranch {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
    pub generation_state: GenerationState,
}

impl StoredBranch {
    pub fn new(branch: ConversationBranch, forked_from: MessageId) -> Self {
        Self {
            branch,
            forked_from,
            generation_state: GenerationState::Idle,
        }
    }

    pub fn with_generation_state(mut self, generation_state: GenerationState) -> Self {
        self.generation_state = generation_state;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSwipeGroup {
    pub group: SwipeGroup,
    pub candidates: BTreeMap<u16, SwipeCandidate>,
}

impl StoredSwipeGroup {
    pub fn new(group: SwipeGroup) -> Self {
        Self {
            group,
            candidates: BTreeMap::new(),
        }
    }

    pub fn with_candidate(mut self, candidate: SwipeCandidate) -> Self {
        self.candidates.insert(candidate.ordinal, candidate);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationState {
    pub session_id: SessionId,
    pub messages: BTreeMap<MessageId, ConversationMessage>,
    pub branches: BTreeMap<BranchId, StoredBranch>,
    pub swipe_groups: BTreeMap<SwipeGroupId, StoredSwipeGroup>,
    pub active_branch_id: Option<BranchId>,
}

impl ConversationState {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            messages: BTreeMap::new(),
            branches: BTreeMap::new(),
            swipe_groups: BTreeMap::new(),
            active_branch_id: None,
        }
    }
}

pub trait ConversationStore: Send + 'static {
    fn load(&self) -> EngineResult<ConversationState>;

    fn commit_message(&mut self, command: CommitMessageCommand) -> EngineResult<()>;

    fn edit_message(&mut self, command: EditMessageCommand) -> EngineResult<()>;

    fn create_branch(&mut self, command: CreateBranchCommand) -> EngineResult<()>;

    fn activate_branch(&mut self, branch_id: &BranchId) -> EngineResult<()>;

    fn record_swipe_candidate(&mut self, command: RecordSwipeCandidateCommand) -> EngineResult<()>;

    fn activate_swipe(&mut self, command: ActivateSwipeCommand) -> EngineResult<()>;

    fn set_generation_state(&mut self, command: SetGenerationStateCommand) -> EngineResult<()>;
}
