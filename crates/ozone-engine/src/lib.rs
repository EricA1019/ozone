use std::{collections::BTreeMap, error::Error, fmt};

use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
        ConversationMessage, CreateBranchCommand, GenerationState, MessageId, OzoneEvent,
        RecordSwipeCandidateCommand, SetGenerationStateCommand, SwipeCandidate, SwipeGroup,
        SwipeGroupId,
    },
    session::{SessionId, UnixTimestamp},
};
use tokio::sync::broadcast;

pub mod context;
pub mod thinking;

pub use thinking::{ThinkingBlockDecoder, ThinkingDisplayMode, ThinkingOutput, ThinkingState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditMessageCommand {
    pub session_id: SessionId,
    pub message_id: MessageId,
    pub content: String,
    pub edited_at: Option<UnixTimestamp>,
}

impl EditMessageCommand {
    pub fn new(session_id: SessionId, message_id: MessageId, content: impl Into<String>) -> Self {
        Self {
            session_id,
            message_id,
            content: content.into(),
            edited_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateBranchCommand {
    pub session_id: SessionId,
    pub branch_id: BranchId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSwipeCandidateRequest {
    pub session_id: SessionId,
    pub command: RecordSwipeCandidateCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateSwipeRequest {
    pub session_id: SessionId,
    pub command: ActivateSwipeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationBranchRecord {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwipeGroupSnapshot {
    pub group: SwipeGroup,
    pub candidates: Vec<SwipeCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchGenerationState {
    pub branch_id: BranchId,
    pub state: GenerationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSnapshot {
    pub session_id: SessionId,
    pub active_branch: Option<ConversationBranchRecord>,
    pub branches: Vec<ConversationBranchRecord>,
    pub transcript: Vec<ConversationMessage>,
    pub swipe_groups: Vec<SwipeGroupSnapshot>,
    pub generation_states: Vec<BranchGenerationState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCommand {
    CommitMessage(CommitMessageCommand),
    EditMessage(EditMessageCommand),
    CreateBranch(CreateBranchCommand),
    ActivateBranch(ActivateBranchCommand),
    RecordSwipeCandidate(RecordSwipeCandidateRequest),
    ActivateSwipe(ActivateSwipeRequest),
    SetGenerationState(SetGenerationStateCommand),
}

impl EngineCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CommitMessage(_) => "commit_message",
            Self::EditMessage(_) => "edit_message",
            Self::CreateBranch(_) => "create_branch",
            Self::ActivateBranch(_) => "activate_branch",
            Self::RecordSwipeCandidate(_) => "record_swipe_candidate",
            Self::ActivateSwipe(_) => "activate_swipe",
            Self::SetGenerationState(_) => "set_generation_state",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCommandResult {
    MessageCommitted(ConversationMessage),
    MessageEdited(ConversationMessage),
    BranchCreated(ConversationBranchRecord),
    BranchActivated(ConversationBranchRecord),
    SwipeCandidateRecorded(SwipeCandidate),
    SwipeActivated(SwipeGroup),
    GenerationStateUpdated {
        branch_id: BranchId,
        state: GenerationState,
    },
}

#[derive(Debug)]
pub enum EngineError<E> {
    Store(E),
}

impl<E: fmt::Display> fmt::Display for EngineError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(f, "engine store error: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for EngineError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
        }
    }
}

pub trait ConversationStore {
    type Error: Error + Send + Sync + 'static;

    fn commit_message(
        &mut self,
        command: CommitMessageCommand,
    ) -> Result<ConversationMessage, Self::Error>;

    fn edit_message(
        &mut self,
        command: EditMessageCommand,
    ) -> Result<ConversationMessage, Self::Error>;

    fn create_branch(
        &mut self,
        command: CreateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error>;

    fn list_branches(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationBranchRecord>, Self::Error>;

    fn get_active_branch(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<ConversationBranchRecord>, Self::Error>;

    fn activate_branch(
        &mut self,
        command: ActivateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error>;

    fn record_swipe_candidate(
        &mut self,
        command: RecordSwipeCandidateRequest,
    ) -> Result<SwipeCandidate, Self::Error>;

    fn activate_swipe_candidate(
        &mut self,
        command: ActivateSwipeRequest,
    ) -> Result<SwipeGroup, Self::Error>;

    fn list_swipe_groups(&self, session_id: &SessionId) -> Result<Vec<SwipeGroup>, Self::Error>;

    fn list_swipe_candidates(
        &self,
        session_id: &SessionId,
        swipe_group_id: &SwipeGroupId,
    ) -> Result<Vec<SwipeCandidate>, Self::Error>;

    fn list_branch_messages(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<Vec<ConversationMessage>, Self::Error>;

    fn get_active_branch_transcript(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationMessage>, Self::Error>;
}

pub trait ConversationEngine {
    type Error: Error + Send + Sync + 'static;

    fn process(
        &mut self,
        command: EngineCommand,
    ) -> Result<EngineCommandResult, EngineError<Self::Error>>;

    fn snapshot(
        &self,
        session_id: &SessionId,
    ) -> Result<ConversationSnapshot, EngineError<Self::Error>>;

    fn subscribe(&self) -> broadcast::Receiver<OzoneEvent>;
}

#[derive(Debug)]
pub struct SingleWriterConversationEngine<S> {
    store: S,
    event_tx: broadcast::Sender<OzoneEvent>,
    generation_states: BTreeMap<BranchId, GenerationState>,
}

impl<S> SingleWriterConversationEngine<S> {
    pub fn new(store: S) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            store,
            event_tx,
            generation_states: BTreeMap::new(),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    fn emit(&self, event: OzoneEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl<S> ConversationEngine for SingleWriterConversationEngine<S>
where
    S: ConversationStore,
{
    type Error = S::Error;

    fn process(
        &mut self,
        command: EngineCommand,
    ) -> Result<EngineCommandResult, EngineError<Self::Error>> {
        match command {
            EngineCommand::CommitMessage(command) => {
                let branch_id = command.branch_id.clone();
                let message = self
                    .store
                    .commit_message(command)
                    .map_err(EngineError::Store)?;
                self.emit(OzoneEvent::MessageCommitted {
                    message_id: message.message_id.clone(),
                    branch_id,
                });
                Ok(EngineCommandResult::MessageCommitted(message))
            }
            EngineCommand::EditMessage(command) => {
                let message = self
                    .store
                    .edit_message(command)
                    .map_err(EngineError::Store)?;
                Ok(EngineCommandResult::MessageEdited(message))
            }
            EngineCommand::CreateBranch(command) => {
                let session_id = command.branch.session_id.clone();
                let requested_state = command.branch.state;
                let new_branch_id = command.branch.branch_id.clone();
                let forked_from = command.forked_from.clone();
                let previous_active = if requested_state == BranchState::Active {
                    self.store
                        .get_active_branch(&session_id)
                        .map_err(EngineError::Store)?
                } else {
                    None
                };
                let record = self
                    .store
                    .create_branch(command)
                    .map_err(EngineError::Store)?;
                self.emit(OzoneEvent::BranchCreated {
                    branch_id: record.branch.branch_id.clone(),
                    forked_from,
                });
                if requested_state == BranchState::Active {
                    if let Some(previous_active) = previous_active {
                        if previous_active.branch.branch_id != record.branch.branch_id {
                            self.emit(OzoneEvent::BranchStateChanged {
                                branch_id: previous_active.branch.branch_id,
                                old_state: BranchState::Active,
                                new_state: BranchState::Inactive,
                            });
                        }
                    }
                    self.emit(OzoneEvent::BranchStateChanged {
                        branch_id: new_branch_id,
                        old_state: BranchState::Inactive,
                        new_state: BranchState::Active,
                    });
                }
                Ok(EngineCommandResult::BranchCreated(record))
            }
            EngineCommand::ActivateBranch(command) => {
                let previous_active = self
                    .store
                    .get_active_branch(&command.session_id)
                    .map_err(EngineError::Store)?;
                let record = self
                    .store
                    .activate_branch(command.clone())
                    .map_err(EngineError::Store)?;
                if let Some(previous_active) = previous_active {
                    if previous_active.branch.branch_id != record.branch.branch_id {
                        self.emit(OzoneEvent::BranchStateChanged {
                            branch_id: previous_active.branch.branch_id,
                            old_state: BranchState::Active,
                            new_state: BranchState::Inactive,
                        });
                    }
                }
                self.emit(OzoneEvent::BranchStateChanged {
                    branch_id: record.branch.branch_id.clone(),
                    old_state: BranchState::Inactive,
                    new_state: BranchState::Active,
                });
                Ok(EngineCommandResult::BranchActivated(record))
            }
            EngineCommand::RecordSwipeCandidate(command) => {
                let candidate = self
                    .store
                    .record_swipe_candidate(command)
                    .map_err(EngineError::Store)?;
                Ok(EngineCommandResult::SwipeCandidateRecorded(candidate))
            }
            EngineCommand::ActivateSwipe(command) => {
                let group = self
                    .store
                    .activate_swipe_candidate(command.clone())
                    .map_err(EngineError::Store)?;
                self.emit(OzoneEvent::SwipeActivated {
                    swipe_group_id: group.swipe_group_id.clone(),
                    ordinal: group.active_ordinal,
                });
                Ok(EngineCommandResult::SwipeActivated(group))
            }
            EngineCommand::SetGenerationState(command) => {
                let branch_id = command.branch_id.clone();
                let state = command.state.clone();
                self.generation_states
                    .insert(branch_id.clone(), state.clone());
                self.emit(OzoneEvent::GenerationStateChanged {
                    branch_id: branch_id.clone(),
                    state: state.clone(),
                });
                Ok(EngineCommandResult::GenerationStateUpdated { branch_id, state })
            }
        }
    }

    fn snapshot(
        &self,
        session_id: &SessionId,
    ) -> Result<ConversationSnapshot, EngineError<Self::Error>> {
        let branches = self
            .store
            .list_branches(session_id)
            .map_err(EngineError::Store)?;
        let active_branch = self
            .store
            .get_active_branch(session_id)
            .map_err(EngineError::Store)?;
        let transcript = self
            .store
            .get_active_branch_transcript(session_id)
            .map_err(EngineError::Store)?;
        let swipe_groups = self
            .store
            .list_swipe_groups(session_id)
            .map_err(EngineError::Store)?
            .into_iter()
            .map(|group| {
                self.store
                    .list_swipe_candidates(session_id, &group.swipe_group_id)
                    .map(|candidates| SwipeGroupSnapshot { group, candidates })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Store)?;
        let generation_states = self
            .generation_states
            .iter()
            .filter(|(branch_id, _)| {
                branches
                    .iter()
                    .any(|record| &record.branch.branch_id == *branch_id)
            })
            .map(|(branch_id, state)| BranchGenerationState {
                branch_id: branch_id.clone(),
                state: state.clone(),
            })
            .collect();

        Ok(ConversationSnapshot {
            session_id: session_id.clone(),
            active_branch,
            branches,
            transcript,
            swipe_groups,
            generation_states,
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<OzoneEvent> {
        self.event_tx.subscribe()
    }
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryConversationStore {
    branches: BTreeMap<BranchId, ConversationBranchRecord>,
    messages: BTreeMap<MessageId, ConversationMessage>,
    swipe_groups: BTreeMap<SwipeGroupId, SwipeGroup>,
    swipe_candidates: BTreeMap<SwipeGroupId, BTreeMap<u16, SwipeCandidate>>,
}

impl InMemoryConversationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_message(&mut self, message: ConversationMessage) {
        self.messages.insert(message.message_id.clone(), message);
    }

    pub fn insert_branch(&mut self, record: ConversationBranchRecord) {
        self.branches
            .insert(record.branch.branch_id.clone(), record);
    }

    fn message_chain_contains(&self, descendant_id: &MessageId, ancestor_id: &MessageId) -> bool {
        let mut cursor = Some(descendant_id.clone());
        while let Some(message_id) = cursor {
            if &message_id == ancestor_id {
                return true;
            }
            cursor = self
                .messages
                .get(&message_id)
                .and_then(|message| message.parent_id.clone());
        }
        false
    }

    fn branch_sort_key(record: &ConversationBranchRecord) -> (u8, UnixTimestamp, String) {
        let state_rank = match record.branch.state {
            BranchState::Active => 0,
            BranchState::Inactive => 1,
            BranchState::Archived => 2,
            BranchState::Deleted => 3,
        };
        (
            state_rank,
            record.branch.created_at,
            record.branch.branch_id.to_string(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryStoreError {
    message: String,
}

impl InMemoryStoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InMemoryStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for InMemoryStoreError {}

impl ConversationStore for InMemoryConversationStore {
    type Error = InMemoryStoreError;

    fn commit_message(
        &mut self,
        command: CommitMessageCommand,
    ) -> Result<ConversationMessage, Self::Error> {
        let branch = self.branches.get_mut(&command.branch_id).ok_or_else(|| {
            InMemoryStoreError::new(format!("missing branch {}", command.branch_id))
        })?;
        if branch.branch.session_id != command.message.session_id {
            return Err(InMemoryStoreError::new(format!(
                "branch {} belongs to session {}, not {}",
                command.branch_id, branch.branch.session_id, command.message.session_id
            )));
        }
        if command.message.parent_id.as_ref() != Some(&branch.branch.tip_message_id) {
            return Err(InMemoryStoreError::new(format!(
                "message {} must append to branch {} tip {}",
                command.message.message_id, command.branch_id, branch.branch.tip_message_id
            )));
        }
        self.messages
            .insert(command.message.message_id.clone(), command.message.clone());
        branch.branch.tip_message_id = command.message.message_id.clone();
        Ok(command.message)
    }

    fn edit_message(
        &mut self,
        command: EditMessageCommand,
    ) -> Result<ConversationMessage, Self::Error> {
        let message = self.messages.get_mut(&command.message_id).ok_or_else(|| {
            InMemoryStoreError::new(format!("missing message {}", command.message_id))
        })?;
        if message.session_id != command.session_id {
            return Err(InMemoryStoreError::new(format!(
                "message {} belongs to session {}, not {}",
                command.message_id, message.session_id, command.session_id
            )));
        }
        message.content = command.content;
        message.edited_at = command.edited_at.or(Some(0));
        Ok(message.clone())
    }

    fn create_branch(
        &mut self,
        command: CreateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error> {
        let branch = command.branch;
        if !self.messages.contains_key(&branch.tip_message_id) {
            return Err(InMemoryStoreError::new(format!(
                "missing message {}",
                branch.tip_message_id
            )));
        }
        if !self.messages.contains_key(&command.forked_from) {
            return Err(InMemoryStoreError::new(format!(
                "missing message {}",
                command.forked_from
            )));
        }
        if !self.message_chain_contains(&branch.tip_message_id, &command.forked_from) {
            return Err(InMemoryStoreError::new(format!(
                "message {} is not an ancestor of {}",
                command.forked_from, branch.tip_message_id
            )));
        }
        if branch.state == BranchState::Active {
            for record in self.branches.values_mut() {
                if record.branch.session_id == branch.session_id
                    && record.branch.state == BranchState::Active
                {
                    record.branch.state = BranchState::Inactive;
                }
            }
        }
        let record = ConversationBranchRecord {
            branch: branch.clone(),
            forked_from: command.forked_from,
        };
        self.branches
            .insert(branch.branch_id.clone(), record.clone());
        Ok(record)
    }

    fn list_branches(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationBranchRecord>, Self::Error> {
        let mut branches = self
            .branches
            .values()
            .filter(|record| &record.branch.session_id == session_id)
            .cloned()
            .collect::<Vec<_>>();
        branches.sort_by_key(Self::branch_sort_key);
        Ok(branches)
    }

    fn get_active_branch(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<ConversationBranchRecord>, Self::Error> {
        Ok(self
            .branches
            .values()
            .filter(|record| {
                &record.branch.session_id == session_id
                    && record.branch.state == BranchState::Active
            })
            .cloned()
            .max_by_key(|record| record.branch.created_at))
    }

    fn activate_branch(
        &mut self,
        command: ActivateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error> {
        let exists = self
            .branches
            .get(&command.branch_id)
            .cloned()
            .ok_or_else(|| {
                InMemoryStoreError::new(format!("missing branch {}", command.branch_id))
            })?;
        if exists.branch.session_id != command.session_id {
            return Err(InMemoryStoreError::new(format!(
                "branch {} belongs to session {}, not {}",
                command.branch_id, exists.branch.session_id, command.session_id
            )));
        }
        for record in self.branches.values_mut() {
            if record.branch.session_id == command.session_id
                && record.branch.state == BranchState::Active
            {
                record.branch.state = BranchState::Inactive;
            }
        }
        let branch = self
            .branches
            .get_mut(&command.branch_id)
            .expect("branch existence checked above");
        branch.branch.state = BranchState::Active;
        Ok(branch.clone())
    }

    fn record_swipe_candidate(
        &mut self,
        command: RecordSwipeCandidateRequest,
    ) -> Result<SwipeCandidate, Self::Error> {
        if self
            .messages
            .get(&command.command.group.parent_message_id)
            .map(|message| message.session_id.clone())
            .ok_or_else(|| {
                InMemoryStoreError::new(format!(
                    "missing parent message {}",
                    command.command.group.parent_message_id
                ))
            })?
            != command.session_id
        {
            return Err(InMemoryStoreError::new(format!(
                "swipe group {} does not belong to session {}",
                command.command.group.swipe_group_id, command.session_id
            )));
        }
        if let Some(parent_context_message_id) = &command.command.group.parent_context_message_id {
            if self
                .messages
                .get(parent_context_message_id)
                .map(|message| &message.session_id)
                != Some(&command.session_id)
            {
                return Err(InMemoryStoreError::new(format!(
                    "missing parent context message {} in session {}",
                    parent_context_message_id, command.session_id
                )));
            }
        }
        if self
            .messages
            .get(&command.command.candidate.message_id)
            .map(|message| &message.session_id)
            != Some(&command.session_id)
        {
            return Err(InMemoryStoreError::new(format!(
                "missing swipe candidate message {} in session {}",
                command.command.candidate.message_id, command.session_id
            )));
        }

        self.swipe_groups
            .entry(command.command.group.swipe_group_id.clone())
            .or_insert_with(|| command.command.group.clone());
        let candidates = self
            .swipe_candidates
            .entry(command.command.candidate.swipe_group_id.clone())
            .or_default();
        candidates.insert(
            command.command.candidate.ordinal,
            command.command.candidate.clone(),
        );
        Ok(command.command.candidate)
    }

    fn activate_swipe_candidate(
        &mut self,
        command: ActivateSwipeRequest,
    ) -> Result<SwipeGroup, Self::Error> {
        let group = self
            .swipe_groups
            .get_mut(&command.command.swipe_group_id)
            .ok_or_else(|| {
                InMemoryStoreError::new(format!(
                    "missing swipe group {}",
                    command.command.swipe_group_id
                ))
            })?;
        let parent_session_id = self
            .messages
            .get(&group.parent_message_id)
            .map(|message| message.session_id.clone())
            .ok_or_else(|| {
                InMemoryStoreError::new(format!(
                    "missing parent message {}",
                    group.parent_message_id
                ))
            })?;
        if parent_session_id != command.session_id {
            return Err(InMemoryStoreError::new(format!(
                "swipe group {} belongs to session {}, not {}",
                command.command.swipe_group_id, parent_session_id, command.session_id
            )));
        }
        let candidates = self
            .swipe_candidates
            .get(&command.command.swipe_group_id)
            .ok_or_else(|| {
                InMemoryStoreError::new(format!(
                    "missing swipe group {} candidates",
                    command.command.swipe_group_id
                ))
            })?;
        let selected_candidate = candidates
            .get(&command.command.ordinal)
            .cloned()
            .ok_or_else(|| {
                InMemoryStoreError::new(format!(
                    "missing swipe candidate {} in group {}",
                    command.command.ordinal, command.command.swipe_group_id
                ))
            })?;

        group.active_ordinal = command.command.ordinal;

        let candidate_message_ids = candidates
            .values()
            .map(|candidate| candidate.message_id.clone())
            .collect::<Vec<_>>();
        for record in self.branches.values_mut() {
            if record.branch.session_id == command.session_id
                && (candidate_message_ids.contains(&record.branch.tip_message_id)
                    || record.branch.tip_message_id == group.parent_message_id)
            {
                record.branch.tip_message_id = selected_candidate.message_id.clone();
            }
        }

        Ok(group.clone())
    }

    fn list_swipe_groups(&self, session_id: &SessionId) -> Result<Vec<SwipeGroup>, Self::Error> {
        let mut groups = self
            .swipe_groups
            .values()
            .filter(|group| {
                self.messages
                    .get(&group.parent_message_id)
                    .map(|message| &message.session_id)
                    == Some(session_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| group.swipe_group_id.to_string());
        Ok(groups)
    }

    fn list_swipe_candidates(
        &self,
        session_id: &SessionId,
        swipe_group_id: &SwipeGroupId,
    ) -> Result<Vec<SwipeCandidate>, Self::Error> {
        let group = self.swipe_groups.get(swipe_group_id).ok_or_else(|| {
            InMemoryStoreError::new(format!("missing swipe group {}", swipe_group_id))
        })?;
        if self
            .messages
            .get(&group.parent_message_id)
            .map(|message| &message.session_id)
            != Some(session_id)
        {
            return Err(InMemoryStoreError::new(format!(
                "swipe group {} does not belong to session {}",
                swipe_group_id, session_id
            )));
        }
        let mut candidates = self
            .swipe_candidates
            .get(swipe_group_id)
            .cloned()
            .unwrap_or_default()
            .into_values()
            .collect::<Vec<_>>();
        candidates.sort_by_key(|candidate| candidate.ordinal);
        Ok(candidates)
    }

    fn list_branch_messages(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<Vec<ConversationMessage>, Self::Error> {
        let branch = self
            .branches
            .get(branch_id)
            .ok_or_else(|| InMemoryStoreError::new(format!("missing branch {}", branch_id)))?;
        if &branch.branch.session_id != session_id {
            return Err(InMemoryStoreError::new(format!(
                "branch {} belongs to session {}, not {}",
                branch_id, branch.branch.session_id, session_id
            )));
        }
        let mut transcript = Vec::new();
        let mut cursor = Some(branch.branch.tip_message_id.clone());
        while let Some(message_id) = cursor {
            let message = self.messages.get(&message_id).ok_or_else(|| {
                InMemoryStoreError::new(format!("missing message {}", message_id))
            })?;
            transcript.push(message.clone());
            cursor = message.parent_id.clone();
        }
        transcript.reverse();
        Ok(transcript)
    }

    fn get_active_branch_transcript(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationMessage>, Self::Error> {
        match self.get_active_branch(session_id)? {
            Some(branch) => self.list_branch_messages(session_id, &branch.branch.branch_id),
            None => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_core::engine::{SwipeCandidateState, SwipeGroup};

    fn session_id(seed: u16) -> SessionId {
        SessionId::parse(format!("00000000-0000-4000-8000-{:012x}", seed)).unwrap()
    }

    fn message_id(seed: u16) -> MessageId {
        MessageId::parse(format!("10000000-0000-4000-8000-{:012x}", seed)).unwrap()
    }

    fn branch_id(seed: u16) -> BranchId {
        BranchId::parse(format!("20000000-0000-4000-8000-{:012x}", seed)).unwrap()
    }

    fn swipe_group_id(seed: u16) -> SwipeGroupId {
        SwipeGroupId::parse(format!("30000000-0000-4000-8000-{:012x}", seed)).unwrap()
    }

    fn request_id(seed: u16) -> ozone_core::engine::RequestId {
        ozone_core::engine::RequestId::parse(format!("40000000-0000-4000-8000-{:012x}", seed))
            .unwrap()
    }

    fn message(
        session_id: &SessionId,
        message_id: MessageId,
        parent_id: Option<MessageId>,
        author_kind: &str,
        content: &str,
        created_at: UnixTimestamp,
    ) -> ConversationMessage {
        let mut message = ConversationMessage::new(
            session_id.clone(),
            message_id,
            author_kind,
            content,
            created_at,
        );
        message.parent_id = parent_id;
        message
    }

    fn seeded_store() -> (
        InMemoryConversationStore,
        SessionId,
        BranchId,
        MessageId,
        MessageId,
    ) {
        let session_id = session_id(1);
        let root_message_id = message_id(1);
        let tip_message_id = message_id(2);
        let mut store = InMemoryConversationStore::new();
        store.insert_message(message(
            &session_id,
            root_message_id.clone(),
            None,
            "user",
            "root",
            10,
        ));
        store.insert_message(message(
            &session_id,
            tip_message_id.clone(),
            Some(root_message_id.clone()),
            "assistant",
            "tip",
            20,
        ));
        let mut branch = ConversationBranch::new(
            branch_id(1),
            session_id.clone(),
            "main",
            tip_message_id.clone(),
            20,
        );
        branch.state = BranchState::Active;
        store.insert_branch(ConversationBranchRecord {
            branch: branch.clone(),
            forked_from: root_message_id.clone(),
        });
        (
            store,
            session_id,
            branch.branch_id.clone(),
            root_message_id,
            tip_message_id,
        )
    }

    #[test]
    fn commit_message_updates_branch_tip_and_emits_event() {
        let (store, session_id, branch_id, _root_message_id, tip_message_id) = seeded_store();
        let mut engine = SingleWriterConversationEngine::new(store);
        let mut events = engine.subscribe();
        let new_message = message(
            &session_id,
            message_id(3),
            Some(tip_message_id.clone()),
            "user",
            "next",
            30,
        );

        let result = engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id: branch_id.clone(),
                message: new_message.clone(),
            }))
            .unwrap();

        assert_eq!(
            result,
            EngineCommandResult::MessageCommitted(new_message.clone())
        );
        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::MessageCommitted {
                message_id: new_message.message_id.clone(),
                branch_id: branch_id.clone(),
            }
        );

        let snapshot = engine.snapshot(&session_id).unwrap();
        assert_eq!(snapshot.transcript.len(), 3);
        assert_eq!(
            snapshot.active_branch.unwrap().branch.tip_message_id,
            new_message.message_id
        );
    }

    #[test]
    fn edit_message_updates_content_and_snapshot() {
        let (store, session_id, branch_id, root_message_id, _tip_message_id) = seeded_store();
        let mut engine = SingleWriterConversationEngine::new(store);

        let edited = engine
            .process(EngineCommand::EditMessage(EditMessageCommand {
                session_id: session_id.clone(),
                message_id: root_message_id.clone(),
                content: "rewritten root".to_owned(),
                edited_at: Some(99),
            }))
            .unwrap();

        match edited {
            EngineCommandResult::MessageEdited(message) => {
                assert_eq!(message.content, "rewritten root");
                assert_eq!(message.edited_at, Some(99));
            }
            other => panic!("unexpected result: {other:?}"),
        }

        let transcript = engine
            .store()
            .list_branch_messages(&session_id, &branch_id)
            .unwrap();
        assert_eq!(transcript[0].content, "rewritten root");
        assert_eq!(transcript[0].edited_at, Some(99));
    }

    #[test]
    fn create_branch_with_active_state_inactivates_previous_active_branch() {
        let (store, session_id, _branch_id, root_message_id, tip_message_id) = seeded_store();
        let mut engine = SingleWriterConversationEngine::new(store);
        let mut events = engine.subscribe();
        let mut branch = ConversationBranch::new(
            branch_id(2),
            session_id.clone(),
            "fork",
            tip_message_id.clone(),
            40,
        );
        branch.state = BranchState::Active;

        let result = engine
            .process(EngineCommand::CreateBranch(CreateBranchCommand {
                branch: branch.clone(),
                forked_from: root_message_id.clone(),
            }))
            .unwrap();

        match result {
            EngineCommandResult::BranchCreated(record) => {
                assert_eq!(record.branch.branch_id, branch.branch_id);
                assert_eq!(record.branch.state, BranchState::Active);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::BranchCreated {
                branch_id: branch.branch_id.clone(),
                forked_from: root_message_id.clone(),
            }
        );
        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::BranchStateChanged {
                branch_id: branch_id(1),
                old_state: BranchState::Active,
                new_state: BranchState::Inactive,
            }
        );
        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::BranchStateChanged {
                branch_id: branch.branch_id.clone(),
                old_state: BranchState::Inactive,
                new_state: BranchState::Active,
            }
        );

        let branches = engine.store().list_branches(&session_id).unwrap();
        assert_eq!(branches[0].branch.branch_id, branch.branch_id);
        assert_eq!(branches[0].branch.state, BranchState::Active);
        assert_eq!(branches[1].branch.state, BranchState::Inactive);
    }

    #[test]
    fn activate_branch_switches_active_branch_and_emits_events() {
        let (mut store, session_id, _branch_id, root_message_id, _tip_message_id) = seeded_store();
        let branch = ConversationBranch::new(
            branch_id(2),
            session_id.clone(),
            "side",
            root_message_id.clone(),
            30,
        );
        store.insert_branch(ConversationBranchRecord {
            branch: branch.clone(),
            forked_from: root_message_id,
        });
        let mut engine = SingleWriterConversationEngine::new(store);
        let mut events = engine.subscribe();

        let result = engine
            .process(EngineCommand::ActivateBranch(ActivateBranchCommand {
                session_id: session_id.clone(),
                branch_id: branch.branch_id.clone(),
            }))
            .unwrap();

        match result {
            EngineCommandResult::BranchActivated(record) => {
                assert_eq!(record.branch.branch_id, branch.branch_id);
                assert_eq!(record.branch.state, BranchState::Active);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::BranchStateChanged {
                branch_id: branch_id(1),
                old_state: BranchState::Active,
                new_state: BranchState::Inactive,
            }
        );
        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::BranchStateChanged {
                branch_id: branch.branch_id.clone(),
                old_state: BranchState::Inactive,
                new_state: BranchState::Active,
            }
        );
    }

    #[test]
    fn activate_swipe_updates_active_ordinal_and_transcript_tip() {
        let (mut store, session_id, branch_id, root_message_id, tip_message_id) = seeded_store();
        let alternative_message_id = message_id(4);
        store.insert_message(message(
            &session_id,
            alternative_message_id.clone(),
            Some(root_message_id.clone()),
            "assistant",
            "alternative",
            25,
        ));
        let mut engine = SingleWriterConversationEngine::new(store);
        let mut events = engine.subscribe();
        let swipe_group = SwipeGroup::new(swipe_group_id(1), root_message_id.clone());

        engine
            .process(EngineCommand::RecordSwipeCandidate(
                RecordSwipeCandidateRequest {
                    session_id: session_id.clone(),
                    command: RecordSwipeCandidateCommand {
                        group: swipe_group.clone(),
                        candidate: SwipeCandidate::new(
                            swipe_group.swipe_group_id.clone(),
                            0,
                            tip_message_id.clone(),
                        ),
                    },
                },
            ))
            .unwrap();
        engine
            .process(EngineCommand::RecordSwipeCandidate(
                RecordSwipeCandidateRequest {
                    session_id: session_id.clone(),
                    command: RecordSwipeCandidateCommand {
                        group: swipe_group.clone(),
                        candidate: SwipeCandidate {
                            swipe_group_id: swipe_group.swipe_group_id.clone(),
                            ordinal: 1,
                            message_id: alternative_message_id.clone(),
                            state: SwipeCandidateState::Active,
                            partial_content: None,
                            tokens_generated: None,
                        },
                    },
                },
            ))
            .unwrap();

        let result = engine
            .process(EngineCommand::ActivateSwipe(ActivateSwipeRequest {
                session_id: session_id.clone(),
                command: ActivateSwipeCommand {
                    swipe_group_id: swipe_group.swipe_group_id.clone(),
                    ordinal: 1,
                },
            }))
            .unwrap();

        match result {
            EngineCommandResult::SwipeActivated(group) => {
                assert_eq!(group.active_ordinal, 1);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::SwipeActivated {
                swipe_group_id: swipe_group.swipe_group_id.clone(),
                ordinal: 1,
            }
        );

        let transcript = engine
            .store()
            .list_branch_messages(&session_id, &branch_id)
            .unwrap();
        assert_eq!(
            transcript.last().unwrap().message_id,
            alternative_message_id
        );
    }

    #[test]
    fn generation_state_updates_are_tracked_and_emitted() {
        let (store, session_id, branch_id, _root_message_id, _tip_message_id) = seeded_store();
        let mut engine = SingleWriterConversationEngine::new(store);
        let mut events = engine.subscribe();
        let state = GenerationState::Queued {
            request_id: request_id(1),
        };

        let result = engine
            .process(EngineCommand::SetGenerationState(
                SetGenerationStateCommand {
                    branch_id: branch_id.clone(),
                    state: state.clone(),
                },
            ))
            .unwrap();

        assert_eq!(
            result,
            EngineCommandResult::GenerationStateUpdated {
                branch_id: branch_id.clone(),
                state: state.clone(),
            }
        );
        assert_eq!(
            events.try_recv().unwrap(),
            OzoneEvent::GenerationStateChanged {
                branch_id: branch_id.clone(),
                state: state.clone(),
            }
        );

        let snapshot = engine.snapshot(&session_id).unwrap();
        assert_eq!(
            snapshot.generation_states,
            vec![BranchGenerationState { branch_id, state }]
        );
    }
}
