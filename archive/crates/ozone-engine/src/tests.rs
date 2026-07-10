use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
        ConversationMessage, CreateBranchCommand, GenerationState, MessageId, OzoneEvent,
        RecordSwipeCandidateCommand, RequestId, SetGenerationStateCommand, SwipeCandidate,
        SwipeCandidateState, SwipeGroup, SwipeGroupId,
    },
    session::SessionId,
};

use crate::{
    ActivateBranchCommand, CommandOutcome, ConversationEngine, ConversationEvent,
    ConversationState, EditMessageCommand, InMemoryConversationStore,
    SingleWriterConversationEngine, StoredBranch, StoredSwipeGroup,
};

#[test]
fn append_send_command_commits_message_and_broadcasts_snapshot() {
    let engine = seeded_engine();
    let subscription = engine.subscribe().unwrap();
    let session_id = session_id();
    let main_branch_id = main_branch_id();
    let root_message_id = root_message_id();
    let appended_message_id = message_id("00000000-0000-0000-0000-000000000102");

    let appended_message = message(
        appended_message_id.as_str(),
        &session_id,
        "user",
        "hello ozone",
        20,
    );

    let outcome = engine
        .send_message(CommitMessageCommand {
            branch_id: main_branch_id.clone(),
            message: appended_message,
        })
        .unwrap();

    assert_eq!(
        outcome.events,
        vec![ConversationEvent::Domain(OzoneEvent::MessageCommitted {
            message_id: appended_message_id.clone(),
            branch_id: main_branch_id.clone(),
        })]
    );

    let active_branch = outcome.snapshot.active_branch().unwrap();
    let tip = active_branch.tip().unwrap();
    assert_eq!(tip.message_id, appended_message_id);
    assert_eq!(tip.parent_id.as_ref(), Some(&root_message_id));
    assert_eq!(tip.content, "hello ozone");

    let update = subscription.try_recv().unwrap();
    assert_eq!(update.sequence, 1);
    assert_eq!(update.event, outcome.events[0]);
    assert_eq!(
        update
            .snapshot
            .active_branch()
            .unwrap()
            .tip()
            .unwrap()
            .content,
        "hello ozone"
    );
}

#[test]
fn edit_command_updates_message_content_and_timestamp() {
    let engine = seeded_engine();
    let session_id = session_id();
    let main_branch_id = main_branch_id();
    let appended_message_id = message_id("00000000-0000-0000-0000-000000000103");

    let appended_message = message(
        appended_message_id.as_str(),
        &session_id,
        "user",
        "draft text",
        20,
    );
    engine
        .append_message(CommitMessageCommand {
            branch_id: main_branch_id,
            message: appended_message,
        })
        .unwrap();

    let outcome = engine
        .edit_message(EditMessageCommand::new(
            appended_message_id.clone(),
            "edited text",
            25,
        ))
        .unwrap();

    assert_eq!(
        outcome.events,
        vec![ConversationEvent::MessageEdited {
            message_id: appended_message_id.clone()
        }]
    );

    let edited_message = outcome.snapshot.message(&appended_message_id).unwrap();
    assert_eq!(edited_message.content, "edited text");
    assert_eq!(edited_message.edited_at, Some(25));
}

#[test]
fn branch_creation_and_activation_update_branch_states() {
    let engine = seeded_engine();
    let session_id = session_id();
    let main_branch_id = main_branch_id();
    let fork_message_id = message_id("00000000-0000-0000-0000-000000000104");

    engine
        .append_message(CommitMessageCommand {
            branch_id: main_branch_id.clone(),
            message: message(
                fork_message_id.as_str(),
                &session_id,
                "user",
                "fork here",
                20,
            ),
        })
        .unwrap();

    let alt_branch_id = branch_id("00000000-0000-0000-0000-000000000202");
    let alt_branch = branch(
        alt_branch_id.as_str(),
        &session_id,
        "alternate",
        &fork_message_id,
        30,
        BranchState::Inactive,
    );

    let create_outcome = engine
        .create_branch(CreateBranchCommand {
            branch: alt_branch,
            forked_from: fork_message_id.clone(),
        })
        .unwrap();

    assert_eq!(
        create_outcome.events,
        vec![ConversationEvent::Domain(OzoneEvent::BranchCreated {
            branch_id: alt_branch_id.clone(),
            forked_from: fork_message_id.clone(),
        })]
    );
    assert_eq!(
        create_outcome.snapshot.active_branch_id.as_ref(),
        Some(&main_branch_id)
    );
    assert_eq!(
        create_outcome
            .snapshot
            .branch(&alt_branch_id)
            .unwrap()
            .branch
            .state,
        BranchState::Inactive
    );

    let activate_outcome = engine
        .activate_branch(ActivateBranchCommand::new(alt_branch_id.clone()))
        .unwrap();

    assert_eq!(activate_outcome.events.len(), 2);
    assert!(matches!(
        &activate_outcome.events[0],
        ConversationEvent::Domain(OzoneEvent::BranchStateChanged {
            branch_id,
            old_state: BranchState::Active,
            new_state: BranchState::Inactive,
        }) if branch_id == &main_branch_id
    ));
    assert!(matches!(
        &activate_outcome.events[1],
        ConversationEvent::Domain(OzoneEvent::BranchStateChanged {
            branch_id,
            old_state: BranchState::Inactive,
            new_state: BranchState::Active,
        }) if branch_id == &alt_branch_id
    ));
    assert_eq!(
        activate_outcome.snapshot.active_branch_id.as_ref(),
        Some(&alt_branch_id)
    );
    assert_eq!(
        activate_outcome
            .snapshot
            .branch(&main_branch_id)
            .unwrap()
            .branch
            .state,
        BranchState::Inactive
    );
    assert_eq!(
        activate_outcome
            .snapshot
            .branch(&alt_branch_id)
            .unwrap()
            .branch
            .state,
        BranchState::Active
    );
}

#[test]
fn swipe_activation_switches_active_display_message() {
    let engine = swipe_seeded_engine();
    let swipe_group_id = swipe_group_id();
    let alternate_candidate_message_id = message_id("00000000-0000-0000-0000-000000000106");

    let mut group = SwipeGroup::new(swipe_group_id.clone(), root_message_id());
    group.parent_context_message_id = Some(root_message_id());

    let record_outcome = engine
        .record_swipe_candidate(RecordSwipeCandidateCommand {
            group,
            candidate: SwipeCandidate::new(
                swipe_group_id.clone(),
                1,
                alternate_candidate_message_id.clone(),
            ),
        })
        .unwrap();

    assert_eq!(
        record_outcome.events,
        vec![ConversationEvent::SwipeCandidateRecorded {
            swipe_group_id: swipe_group_id.clone(),
            ordinal: 1,
            message_id: alternate_candidate_message_id.clone(),
        }]
    );

    let activate_outcome = engine
        .activate_swipe(ActivateSwipeCommand {
            swipe_group_id: swipe_group_id.clone(),
            ordinal: 1,
        })
        .unwrap();

    assert_eq!(
        activate_outcome.events,
        vec![ConversationEvent::Domain(OzoneEvent::SwipeActivated {
            swipe_group_id: swipe_group_id.clone(),
            ordinal: 1,
        })]
    );

    let active_branch_tip = activate_outcome
        .snapshot
        .active_branch()
        .unwrap()
        .tip()
        .unwrap();
    assert_eq!(active_branch_tip.message_id, alternate_candidate_message_id);
    assert_eq!(active_branch_tip.content, "assistant answer B");

    let swipe_group = activate_outcome
        .snapshot
        .swipe_group(&swipe_group_id)
        .unwrap();
    assert_eq!(swipe_group.group.active_ordinal, 1);
    assert_eq!(
        swipe_group.active_message.as_ref().unwrap().content,
        "assistant answer B"
    );
    assert_eq!(swipe_group.candidates.len(), 2);
    assert_eq!(
        swipe_group.candidates[0].candidate.state,
        SwipeCandidateState::Discarded
    );
    assert_eq!(
        swipe_group.candidates[1].candidate.state,
        SwipeCandidateState::Active
    );
}

#[test]
fn generation_state_transitions_follow_engine_lifecycle_rules() {
    let engine = seeded_engine();
    let main_branch_id = main_branch_id();
    let request = request_id("00000000-0000-0000-0000-000000000301");
    let second_request = request_id("00000000-0000-0000-0000-000000000302");

    let queued = GenerationState::Queued {
        request_id: request.clone(),
    };
    let streaming = GenerationState::Streaming {
        request_id: request.clone(),
        tokens_so_far: 32,
    };
    let completed = GenerationState::Completed {
        request_id: request.clone(),
        message_id: root_message_id(),
        tokens_generated: 64,
        duration_ms: 900,
    };

    assert_eq!(
        apply_generation_state(&engine, main_branch_id.clone(), queued.clone())
            .snapshot
            .active_branch()
            .unwrap()
            .generation_state,
        queued
    );
    assert_eq!(
        apply_generation_state(&engine, main_branch_id.clone(), streaming.clone())
            .snapshot
            .active_branch()
            .unwrap()
            .generation_state,
        streaming
    );
    assert_eq!(
        apply_generation_state(&engine, main_branch_id.clone(), completed.clone())
            .snapshot
            .active_branch()
            .unwrap()
            .generation_state,
        completed
    );

    let error = engine
        .set_generation_state(SetGenerationStateCommand {
            branch_id: main_branch_id.clone(),
            state: GenerationState::Streaming {
                request_id: second_request,
                tokens_so_far: 1,
            },
        })
        .unwrap_err();

    assert!(matches!(
        error,
        crate::EngineError::InvalidGenerationTransition { branch_id, .. }
        if branch_id == main_branch_id
    ));
}

fn apply_generation_state(
    engine: &SingleWriterConversationEngine<InMemoryConversationStore>,
    branch_id: BranchId,
    state: GenerationState,
) -> CommandOutcome {
    engine
        .set_generation_state(SetGenerationStateCommand { branch_id, state })
        .unwrap()
}

fn seeded_engine() -> SingleWriterConversationEngine<InMemoryConversationStore> {
    let session_id = session_id();
    let root_message_id = root_message_id();
    let root_message = message(root_message_id.as_str(), &session_id, "system", "root", 10);
    let root_branch = branch(
        main_branch_id().as_str(),
        &session_id,
        "main",
        &root_message_id,
        10,
        BranchState::Active,
    );

    SingleWriterConversationEngine::new(
        InMemoryConversationStore::bootstrap(root_branch, root_message).unwrap(),
    )
}

fn swipe_seeded_engine() -> SingleWriterConversationEngine<InMemoryConversationStore> {
    let session_id = session_id();
    let root_message_id = root_message_id();
    let active_candidate_message_id = message_id("00000000-0000-0000-0000-000000000105");
    let alternate_candidate_message_id = message_id("00000000-0000-0000-0000-000000000106");
    let main_branch_id = main_branch_id();
    let swipe_group_id = swipe_group_id();

    let root_message = message(root_message_id.as_str(), &session_id, "user", "prompt", 10);
    let active_candidate_message = message(
        active_candidate_message_id.as_str(),
        &session_id,
        "assistant",
        "assistant answer A",
        20,
    )
    .with_parent(root_message_id.clone());
    let alternate_candidate_message = message(
        alternate_candidate_message_id.as_str(),
        &session_id,
        "assistant",
        "assistant answer B",
        21,
    )
    .with_parent(root_message_id.clone());

    let mut state = ConversationState::new(session_id.clone());
    state
        .messages
        .insert(root_message_id.clone(), root_message.clone());
    state.messages.insert(
        active_candidate_message_id.clone(),
        active_candidate_message.clone(),
    );
    state.messages.insert(
        alternate_candidate_message_id.clone(),
        alternate_candidate_message,
    );

    state.branches.insert(
        main_branch_id.clone(),
        StoredBranch::new(
            branch(
                main_branch_id.as_str(),
                &session_id,
                "main",
                &active_candidate_message_id,
                10,
                BranchState::Active,
            ),
            root_message_id.clone(),
        ),
    );
    state.active_branch_id = Some(main_branch_id);

    let mut group = SwipeGroup::new(swipe_group_id.clone(), root_message_id.clone());
    group.parent_context_message_id = Some(root_message_id);
    state.swipe_groups.insert(
        swipe_group_id.clone(),
        StoredSwipeGroup::new(group).with_candidate(SwipeCandidate::new(
            swipe_group_id,
            0,
            active_candidate_message_id,
        )),
    );

    SingleWriterConversationEngine::new(InMemoryConversationStore::from_state(state).unwrap())
}

fn session_id() -> SessionId {
    SessionId::parse("00000000-0000-0000-0000-000000000001").unwrap()
}

fn main_branch_id() -> BranchId {
    branch_id("00000000-0000-0000-0000-000000000201")
}

fn root_message_id() -> MessageId {
    message_id("00000000-0000-0000-0000-000000000101")
}

fn swipe_group_id() -> SwipeGroupId {
    SwipeGroupId::parse("00000000-0000-0000-0000-000000000401").unwrap()
}

fn branch_id(value: &str) -> BranchId {
    BranchId::parse(value).unwrap()
}

fn message_id(value: &str) -> MessageId {
    MessageId::parse(value).unwrap()
}

fn request_id(value: &str) -> RequestId {
    RequestId::parse(value).unwrap()
}

fn branch(
    branch_id: &str,
    session_id: &SessionId,
    name: &str,
    tip_message_id: &MessageId,
    created_at: i64,
    state: BranchState,
) -> ConversationBranch {
    let mut branch = ConversationBranch::new(
        BranchId::parse(branch_id).unwrap(),
        session_id.clone(),
        name,
        tip_message_id.clone(),
        created_at,
    );
    branch.state = state;
    branch
}

fn message(
    message_id: &str,
    session_id: &SessionId,
    author_kind: &str,
    content: &str,
    created_at: i64,
) -> ConversationMessage {
    ConversationMessage::new(
        session_id.clone(),
        MessageId::parse(message_id).unwrap(),
        author_kind,
        content,
        created_at,
    )
}

trait MessageTestExt {
    fn with_parent(self, parent_id: MessageId) -> Self;
}

impl MessageTestExt for ConversationMessage {
    fn with_parent(mut self, parent_id: MessageId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}
