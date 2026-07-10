use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

use ozone_core::engine::{
    ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationCommand,
    CreateBranchCommand, MessageId, OzoneEvent, RecordSwipeCandidateCommand,
    SetGenerationStateCommand, SwipeGroupId,
};

use crate::{
    command::{ActivateBranchCommand, EditMessageCommand, EngineCommand},
    error::{EngineError, EngineResult},
    snapshot::ConversationSnapshot,
    store::{ConversationState, ConversationStore},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationEvent {
    Domain(OzoneEvent),
    MessageEdited {
        message_id: MessageId,
    },
    SwipeCandidateRecorded {
        swipe_group_id: SwipeGroupId,
        ordinal: u16,
        message_id: MessageId,
    },
}

impl ConversationEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Domain(event) => event.event_type(),
            Self::MessageEdited { .. } => "message_edited",
            Self::SwipeCandidateRecorded { .. } => "swipe_candidate_recorded",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub events: Vec<ConversationEvent>,
    pub snapshot: Arc<ConversationSnapshot>,
}

#[derive(Debug, Clone)]
pub struct EngineUpdate {
    pub sequence: u64,
    pub event: ConversationEvent,
    pub snapshot: Arc<ConversationSnapshot>,
}

pub struct EventSubscription {
    receiver: mpsc::Receiver<EngineUpdate>,
}

impl EventSubscription {
    pub fn recv(&self) -> Result<EngineUpdate, mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<EngineUpdate, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<EngineUpdate, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

pub trait ConversationEngine {
    fn execute(&self, command: EngineCommand) -> EngineResult<CommandOutcome>;

    fn snapshot(&self) -> EngineResult<Arc<ConversationSnapshot>>;

    fn subscribe(&self) -> EngineResult<EventSubscription>;

    fn append_message(&self, command: CommitMessageCommand) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::Conversation(
            ConversationCommand::CommitMessage(command),
        ))
    }

    fn send_message(&self, command: CommitMessageCommand) -> EngineResult<CommandOutcome> {
        self.append_message(command)
    }

    fn edit_message(&self, command: EditMessageCommand) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::EditMessage(command))
    }

    fn create_branch(&self, command: CreateBranchCommand) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::Conversation(
            ConversationCommand::CreateBranch(command),
        ))
    }

    fn activate_branch(&self, command: ActivateBranchCommand) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::ActivateBranch(command))
    }

    fn record_swipe_candidate(
        &self,
        command: RecordSwipeCandidateCommand,
    ) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::Conversation(
            ConversationCommand::RecordSwipeCandidate(command),
        ))
    }

    fn activate_swipe(&self, command: ActivateSwipeCommand) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::Conversation(
            ConversationCommand::ActivateSwipe(command),
        ))
    }

    fn set_generation_state(
        &self,
        command: SetGenerationStateCommand,
    ) -> EngineResult<CommandOutcome> {
        self.execute(EngineCommand::Conversation(
            ConversationCommand::SetGenerationState(command),
        ))
    }
}

#[derive(Clone)]
pub struct SingleWriterConversationEngine<S> {
    inner: Arc<Mutex<EngineInner<S>>>,
}

struct EngineInner<S> {
    store: S,
    subscribers: Vec<mpsc::Sender<EngineUpdate>>,
    next_sequence: u64,
}

impl<S: ConversationStore> SingleWriterConversationEngine<S> {
    pub fn new(store: S) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EngineInner {
                store,
                subscribers: Vec::new(),
                next_sequence: 0,
            })),
        }
    }

    fn lock(&self) -> EngineResult<MutexGuard<'_, EngineInner<S>>> {
        self.inner
            .lock()
            .map_err(|_| EngineError::invalid_command("conversation engine mutex poisoned"))
    }

    fn execute_locked(
        inner: &mut EngineInner<S>,
        command: EngineCommand,
    ) -> EngineResult<CommandOutcome> {
        let before = inner.store.load()?;

        match &command {
            EngineCommand::Conversation(conversation_command) => match conversation_command {
                ConversationCommand::CommitMessage(command) => {
                    inner.store.commit_message(command.clone())?
                }
                ConversationCommand::CreateBranch(command) => {
                    inner.store.create_branch(command.clone())?
                }
                ConversationCommand::RecordSwipeCandidate(command) => {
                    inner.store.record_swipe_candidate(command.clone())?
                }
                ConversationCommand::ActivateSwipe(command) => {
                    inner.store.activate_swipe(command.clone())?
                }
                ConversationCommand::SetGenerationState(command) => {
                    inner.store.set_generation_state(command.clone())?
                }
            },
            EngineCommand::EditMessage(command) => inner.store.edit_message(command.clone())?,
            EngineCommand::ActivateBranch(command) => {
                inner.store.activate_branch(&command.branch_id)?
            }
        }

        let after = inner.store.load()?;
        let snapshot = Arc::new(ConversationSnapshot::from_state(&after));
        let events = events_for_command(&command, &before, &after);
        publish_updates(inner, &snapshot, &events);

        Ok(CommandOutcome { events, snapshot })
    }
}

impl<S: ConversationStore> ConversationEngine for SingleWriterConversationEngine<S> {
    fn execute(&self, command: EngineCommand) -> EngineResult<CommandOutcome> {
        let mut inner = self.lock()?;
        Self::execute_locked(&mut inner, command)
    }

    fn snapshot(&self) -> EngineResult<Arc<ConversationSnapshot>> {
        let inner = self.lock()?;
        let state = inner.store.load()?;
        Ok(Arc::new(ConversationSnapshot::from_state(&state)))
    }

    fn subscribe(&self) -> EngineResult<EventSubscription> {
        let (sender, receiver) = mpsc::channel();
        let mut inner = self.lock()?;
        inner.subscribers.push(sender);
        Ok(EventSubscription { receiver })
    }
}

fn publish_updates<S>(
    inner: &mut EngineInner<S>,
    snapshot: &Arc<ConversationSnapshot>,
    events: &[ConversationEvent],
) {
    for event in events {
        inner.next_sequence += 1;
        let update = EngineUpdate {
            sequence: inner.next_sequence,
            event: event.clone(),
            snapshot: Arc::clone(snapshot),
        };
        inner
            .subscribers
            .retain(|subscriber| subscriber.send(update.clone()).is_ok());
    }
}

fn events_for_command(
    command: &EngineCommand,
    before: &ConversationState,
    after: &ConversationState,
) -> Vec<ConversationEvent> {
    match command {
        EngineCommand::Conversation(conversation_command) => match conversation_command {
            ConversationCommand::CommitMessage(command) => {
                vec![ConversationEvent::Domain(OzoneEvent::MessageCommitted {
                    message_id: command.message.message_id.clone(),
                    branch_id: command.branch_id.clone(),
                })]
            }
            ConversationCommand::CreateBranch(command) => {
                let mut events = vec![ConversationEvent::Domain(OzoneEvent::BranchCreated {
                    branch_id: command.branch.branch_id.clone(),
                    forked_from: command.forked_from.clone(),
                })];
                events.extend(branch_state_events(
                    before,
                    after,
                    &command.branch.branch_id,
                ));
                events
            }
            ConversationCommand::RecordSwipeCandidate(command) => {
                vec![ConversationEvent::SwipeCandidateRecorded {
                    swipe_group_id: command.group.swipe_group_id.clone(),
                    ordinal: command.candidate.ordinal,
                    message_id: command.candidate.message_id.clone(),
                }]
            }
            ConversationCommand::ActivateSwipe(command) => {
                vec![ConversationEvent::Domain(OzoneEvent::SwipeActivated {
                    swipe_group_id: command.swipe_group_id.clone(),
                    ordinal: command.ordinal,
                })]
            }
            ConversationCommand::SetGenerationState(command) => {
                vec![ConversationEvent::Domain(
                    OzoneEvent::GenerationStateChanged {
                        branch_id: command.branch_id.clone(),
                        state: command.state.clone(),
                    },
                )]
            }
        },
        EngineCommand::EditMessage(command) => vec![ConversationEvent::MessageEdited {
            message_id: command.message_id.clone(),
        }],
        EngineCommand::ActivateBranch(command) => {
            branch_state_events(before, after, &command.branch_id)
        }
    }
}

fn branch_state_events(
    before: &ConversationState,
    after: &ConversationState,
    target_branch_id: &BranchId,
) -> Vec<ConversationEvent> {
    let mut events = Vec::new();

    if before.active_branch_id != after.active_branch_id {
        if let Some(previous_active_branch_id) = before.active_branch_id.as_ref() {
            if let (Some(before_branch), Some(after_branch)) = (
                before.branches.get(previous_active_branch_id),
                after.branches.get(previous_active_branch_id),
            ) {
                if before_branch.branch.state != after_branch.branch.state {
                    events.push(ConversationEvent::Domain(OzoneEvent::BranchStateChanged {
                        branch_id: previous_active_branch_id.clone(),
                        old_state: before_branch.branch.state,
                        new_state: after_branch.branch.state,
                    }));
                }
            }
        }
    }

    if let (Some(before_branch), Some(after_branch)) = (
        before.branches.get(target_branch_id),
        after.branches.get(target_branch_id),
    ) {
        if before_branch.branch.state != after_branch.branch.state {
            events.push(ConversationEvent::Domain(OzoneEvent::BranchStateChanged {
                branch_id: target_branch_id.clone(),
                old_state: before_branch.branch.state,
                new_state: after_branch.branch.state,
            }));
        }
    } else if before.branches.get(target_branch_id).is_none() {
        if let Some(after_branch) = after.branches.get(target_branch_id) {
            if after_branch.branch.state == BranchState::Active {
                events.push(ConversationEvent::Domain(OzoneEvent::BranchStateChanged {
                    branch_id: target_branch_id.clone(),
                    old_state: BranchState::Inactive,
                    new_state: BranchState::Active,
                }));
            }
        }
    }

    events
}
