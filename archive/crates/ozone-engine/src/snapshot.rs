use ozone_core::{
    engine::{
        BranchId, ConversationBranch, ConversationMessage, GenerationState, MessageId,
        SwipeCandidate, SwipeGroup, SwipeGroupId,
    },
    session::SessionId,
};

use crate::store::ConversationState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSnapshot {
    pub session_id: SessionId,
    pub active_branch_id: Option<BranchId>,
    pub messages: Vec<ConversationMessage>,
    pub branches: Vec<BranchSnapshot>,
    pub swipe_groups: Vec<SwipeGroupSnapshot>,
}

impl ConversationSnapshot {
    pub fn from_state(state: &ConversationState) -> Self {
        let mut messages = state.messages.values().cloned().collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.message_id.cmp(&right.message_id))
        });

        let mut branches = state
            .branches
            .values()
            .map(|stored_branch| BranchSnapshot {
                branch: stored_branch.branch.clone(),
                forked_from: stored_branch.forked_from.clone(),
                generation_state: stored_branch.generation_state.clone(),
                transcript: collect_transcript(
                    &stored_branch.branch.tip_message_id,
                    &state.messages,
                ),
            })
            .collect::<Vec<_>>();
        branches.sort_by(|left, right| {
            left.branch
                .created_at
                .cmp(&right.branch.created_at)
                .then_with(|| left.branch.branch_id.cmp(&right.branch.branch_id))
        });

        let swipe_groups = state
            .swipe_groups
            .values()
            .map(|stored_group| SwipeGroupSnapshot {
                group: stored_group.group.clone(),
                candidates: stored_group
                    .candidates
                    .values()
                    .map(|candidate| SwipeCandidateSnapshot {
                        message: state.messages.get(&candidate.message_id).cloned(),
                        candidate: candidate.clone(),
                    })
                    .collect(),
                active_message: stored_group
                    .candidates
                    .get(&stored_group.group.active_ordinal)
                    .and_then(|candidate| state.messages.get(&candidate.message_id))
                    .cloned(),
            })
            .collect::<Vec<_>>();

        Self {
            session_id: state.session_id.clone(),
            active_branch_id: state.active_branch_id.clone(),
            messages,
            branches,
            swipe_groups,
        }
    }

    pub fn active_branch(&self) -> Option<&BranchSnapshot> {
        let active_branch_id = self.active_branch_id.as_ref()?;
        self.branch(active_branch_id)
    }

    pub fn branch(&self, branch_id: &BranchId) -> Option<&BranchSnapshot> {
        self.branches
            .iter()
            .find(|branch| &branch.branch.branch_id == branch_id)
    }

    pub fn message(&self, message_id: &MessageId) -> Option<&ConversationMessage> {
        self.messages
            .iter()
            .find(|message| &message.message_id == message_id)
    }

    pub fn swipe_group(&self, swipe_group_id: &SwipeGroupId) -> Option<&SwipeGroupSnapshot> {
        self.swipe_groups
            .iter()
            .find(|group| &group.group.swipe_group_id == swipe_group_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSnapshot {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
    pub generation_state: GenerationState,
    pub transcript: Vec<ConversationMessage>,
}

impl BranchSnapshot {
    pub fn tip(&self) -> Option<&ConversationMessage> {
        self.transcript.last()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwipeGroupSnapshot {
    pub group: SwipeGroup,
    pub candidates: Vec<SwipeCandidateSnapshot>,
    pub active_message: Option<ConversationMessage>,
}

impl SwipeGroupSnapshot {
    pub fn active_candidate(&self) -> Option<&SwipeCandidateSnapshot> {
        self.candidates
            .iter()
            .find(|candidate| candidate.candidate.ordinal == self.group.active_ordinal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwipeCandidateSnapshot {
    pub candidate: SwipeCandidate,
    pub message: Option<ConversationMessage>,
}

fn collect_transcript(
    tip_message_id: &MessageId,
    messages: &std::collections::BTreeMap<MessageId, ConversationMessage>,
) -> Vec<ConversationMessage> {
    let mut transcript = Vec::new();
    let mut cursor = Some(tip_message_id.clone());

    while let Some(message_id) = cursor {
        let Some(message) = messages.get(&message_id) else {
            break;
        };

        transcript.push(message.clone());
        cursor = message.parent_id.clone();
    }

    transcript.reverse();
    transcript
}
