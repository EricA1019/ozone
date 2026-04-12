use std::{error::Error, fmt, str::FromStr};

use crate::session::{SessionId, UnixTimestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineIdError {
    kind: &'static str,
}

impl EngineIdError {
    const fn new(kind: &'static str) -> Self {
        Self { kind }
    }
}

impl fmt::Display for EngineIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} IDs must be UUID strings in 8-4-4-4-12 format",
            self.kind
        )
    }
}

impl Error for EngineIdError {}

macro_rules! define_uuid_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl AsRef<str>) -> Result<Self, EngineIdError> {
                value.as_ref().parse()
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = EngineIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                normalize_uuid_string(value)
                    .map(Self)
                    .map_err(|()| EngineIdError::new($label))
            }
        }
    };
}

define_uuid_id!(MessageId, "message");
define_uuid_id!(BranchId, "branch");
define_uuid_id!(SwipeGroupId, "swipe group");
define_uuid_id!(RequestId, "request");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainParseError {
    kind: &'static str,
    value: String,
}

impl DomainParseError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }
}

impl fmt::Display for DomainParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unrecognized {} value `{}`", self.kind, self.value)
    }
}

impl Error for DomainParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchState {
    Active,
    Inactive,
    Archived,
    Deleted,
}

impl BranchState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
        }
    }
}

impl fmt::Display for BranchState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BranchState {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_value(value).as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            _ => Err(DomainParseError::new("branch state", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SwipeCandidateState {
    #[default]
    Active,
    Discarded,
    FailedMidStream,
}

impl SwipeCandidateState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Discarded => "discarded",
            Self::FailedMidStream => "failed_mid_stream",
        }
    }
}

impl fmt::Display for SwipeCandidateState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SwipeCandidateState {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_value(value).as_str() {
            "active" => Ok(Self::Active),
            "discarded" => Ok(Self::Discarded),
            "failed_mid_stream" => Ok(Self::FailedMidStream),
            _ => Err(DomainParseError::new("swipe candidate state", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelReason {
    UserRequested,
    BackpressureTimeout,
    BackendDisconnect,
    RateLimited,
}

impl CancelReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserRequested => "user_requested",
            Self::BackpressureTimeout => "backpressure_timeout",
            Self::BackendDisconnect => "backend_disconnect",
            Self::RateLimited => "rate_limited",
        }
    }
}

impl fmt::Display for CancelReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CancelReason {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_value(value).as_str() {
            "user_requested" => Ok(Self::UserRequested),
            "backpressure_timeout" => Ok(Self::BackpressureTimeout),
            "backend_disconnect" => Ok(Self::BackendDisconnect),
            "rate_limited" => Ok(Self::RateLimited),
            _ => Err(DomainParseError::new("cancel reason", value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GenerationState {
    #[default]
    Idle,
    Queued {
        request_id: RequestId,
    },
    Streaming {
        request_id: RequestId,
        tokens_so_far: u64,
    },
    Completed {
        request_id: RequestId,
        message_id: MessageId,
        tokens_generated: u64,
        duration_ms: u64,
    },
    Cancelled {
        request_id: RequestId,
        partial_content: Option<String>,
        tokens_generated: u64,
        reason: CancelReason,
    },
    Failed {
        request_id: RequestId,
        error: String,
    },
    FailedMidStream {
        request_id: RequestId,
        partial_content: String,
        tokens_generated: u64,
        error: String,
    },
}

impl GenerationState {
    pub fn request_id(&self) -> Option<&RequestId> {
        match self {
            Self::Idle => None,
            Self::Queued { request_id }
            | Self::Streaming { request_id, .. }
            | Self::Completed { request_id, .. }
            | Self::Cancelled { request_id, .. }
            | Self::Failed { request_id, .. }
            | Self::FailedMidStream { request_id, .. } => Some(request_id),
        }
    }

    pub const fn is_inflight(&self) -> bool {
        matches!(self, Self::Queued { .. } | Self::Streaming { .. })
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. }
                | Self::Cancelled { .. }
                | Self::Failed { .. }
                | Self::FailedMidStream { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationMessage {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub parent_id: Option<MessageId>,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub content: String,
    pub created_at: UnixTimestamp,
    pub edited_at: Option<UnixTimestamp>,
    pub is_hidden: bool,
}

impl ConversationMessage {
    pub fn new(
        session_id: SessionId,
        message_id: MessageId,
        author_kind: impl Into<String>,
        content: impl Into<String>,
        created_at: UnixTimestamp,
    ) -> Self {
        Self {
            message_id,
            session_id,
            parent_id: None,
            author_kind: author_kind.into(),
            author_name: None,
            content: content.into(),
            created_at,
            edited_at: None,
            is_hidden: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationBranch {
    pub branch_id: BranchId,
    pub session_id: SessionId,
    pub name: String,
    pub tip_message_id: MessageId,
    pub created_at: UnixTimestamp,
    pub state: BranchState,
    pub description: Option<String>,
}

impl ConversationBranch {
    pub fn new(
        branch_id: BranchId,
        session_id: SessionId,
        name: impl Into<String>,
        tip_message_id: MessageId,
        created_at: UnixTimestamp,
    ) -> Self {
        Self {
            branch_id,
            session_id,
            name: name.into(),
            tip_message_id,
            created_at,
            state: BranchState::Inactive,
            description: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwipeGroup {
    pub swipe_group_id: SwipeGroupId,
    pub parent_message_id: MessageId,
    pub parent_context_message_id: Option<MessageId>,
    pub active_ordinal: u16,
}

impl SwipeGroup {
    pub fn new(swipe_group_id: SwipeGroupId, parent_message_id: MessageId) -> Self {
        Self {
            swipe_group_id,
            parent_message_id,
            parent_context_message_id: None,
            active_ordinal: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwipeCandidate {
    pub swipe_group_id: SwipeGroupId,
    pub ordinal: u16,
    pub message_id: MessageId,
    pub state: SwipeCandidateState,
    pub partial_content: Option<String>,
    pub tokens_generated: Option<u64>,
}

impl SwipeCandidate {
    pub fn new(swipe_group_id: SwipeGroupId, ordinal: u16, message_id: MessageId) -> Self {
        Self {
            swipe_group_id,
            ordinal,
            message_id,
            state: SwipeCandidateState::Active,
            partial_content: None,
            tokens_generated: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessageCommand {
    pub branch_id: BranchId,
    pub message: ConversationMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateBranchCommand {
    pub branch: ConversationBranch,
    pub forked_from: MessageId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSwipeCandidateCommand {
    pub group: SwipeGroup,
    pub candidate: SwipeCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateSwipeCommand {
    pub swipe_group_id: SwipeGroupId,
    pub ordinal: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetGenerationStateCommand {
    pub branch_id: BranchId,
    pub state: GenerationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationCommand {
    CommitMessage(CommitMessageCommand),
    CreateBranch(CreateBranchCommand),
    RecordSwipeCandidate(RecordSwipeCandidateCommand),
    ActivateSwipe(ActivateSwipeCommand),
    SetGenerationState(SetGenerationStateCommand),
}

impl ConversationCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CommitMessage(_) => "commit_message",
            Self::CreateBranch(_) => "create_branch",
            Self::RecordSwipeCandidate(_) => "record_swipe_candidate",
            Self::ActivateSwipe(_) => "activate_swipe",
            Self::SetGenerationState(_) => "set_generation_state",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OzoneEvent {
    MessageCommitted {
        message_id: MessageId,
        branch_id: BranchId,
    },
    BranchCreated {
        branch_id: BranchId,
        forked_from: MessageId,
    },
    BranchStateChanged {
        branch_id: BranchId,
        old_state: BranchState,
        new_state: BranchState,
    },
    SwipeActivated {
        swipe_group_id: SwipeGroupId,
        ordinal: u16,
    },
    GenerationStateChanged {
        branch_id: BranchId,
        state: GenerationState,
    },
}

impl OzoneEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::MessageCommitted { .. } => "message_committed",
            Self::BranchCreated { .. } => "branch_created",
            Self::BranchStateChanged { .. } => "branch_state_changed",
            Self::SwipeActivated { .. } => "swipe_activated",
            Self::GenerationStateChanged { .. } => "generation_state_changed",
        }
    }
}

fn normalize_uuid_string(value: &str) -> Result<String, ()> {
    const HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

    let bytes = value.as_bytes();

    if bytes.len() != 36 {
        return Err(());
    }

    let mut normalized = String::with_capacity(36);

    for (idx, byte) in bytes.iter().enumerate() {
        if HYPHEN_POSITIONS.contains(&idx) {
            if *byte != b'-' {
                return Err(());
            }

            normalized.push('-');
            continue;
        }

        if !byte.is_ascii_hexdigit() {
            return Err(());
        }

        normalized.push((*byte as char).to_ascii_lowercase());
    }

    Ok(normalized)
}

fn normalize_enum_value(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_separator = false;
    let mut previous_was_lower_or_digit = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if previous_was_lower_or_digit && !normalized.is_empty() && !previous_was_separator
                {
                    normalized.push('_');
                }

                normalized.push(ch.to_ascii_lowercase());
                previous_was_lower_or_digit = false;
            } else {
                normalized.push(ch.to_ascii_lowercase());
                previous_was_lower_or_digit = true;
            }

            previous_was_separator = false;
        } else if matches!(ch, '-' | '_' | ' ') {
            if !normalized.is_empty() && !previous_was_separator {
                normalized.push('_');
            }

            previous_was_separator = true;
            previous_was_lower_or_digit = false;
        }
    }

    if normalized.ends_with('_') {
        normalized.pop();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::{
        ActivateSwipeCommand, BranchId, BranchState, CancelReason, CommitMessageCommand,
        ConversationBranch, ConversationCommand, ConversationMessage, GenerationState, MessageId,
        OzoneEvent, RequestId, SetGenerationStateCommand, SwipeCandidate, SwipeCandidateState,
        SwipeGroup, SwipeGroupId,
    };
    use crate::session::SessionId;

    const SESSION_ID: &str = "123e4567-e89b-12d3-a456-426614174000";
    const MESSAGE_ID: &str = "223e4567-e89b-12d3-a456-426614174000";
    const BRANCH_ID: &str = "323e4567-e89b-12d3-a456-426614174000";
    const SWIPE_GROUP_ID: &str = "423e4567-e89b-12d3-a456-426614174000";
    const REQUEST_ID: &str = "523e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn engine_identifier_parsing_normalizes_uuid_strings() {
        let message_id = MessageId::parse("223E4567-E89B-12D3-A456-426614174000").unwrap();
        let branch_id = BranchId::parse(BRANCH_ID).unwrap();
        let swipe_group_id = SwipeGroupId::parse(SWIPE_GROUP_ID).unwrap();
        let request_id = RequestId::parse(REQUEST_ID).unwrap();

        assert_eq!(message_id.as_str(), MESSAGE_ID);
        assert_eq!(branch_id.to_string(), BRANCH_ID);
        assert_eq!(swipe_group_id.as_str(), SWIPE_GROUP_ID);
        assert_eq!(request_id.as_str(), REQUEST_ID);
        assert!(MessageId::parse("not-a-message-id").is_err());
    }

    #[test]
    fn branch_and_swipe_states_round_trip_storage_strings() {
        assert_eq!(
            "Active".parse::<BranchState>().unwrap(),
            BranchState::Active
        );
        assert_eq!(
            "failed-mid-stream".parse::<SwipeCandidateState>().unwrap(),
            SwipeCandidateState::FailedMidStream
        );
        assert_eq!(BranchState::Archived.as_str(), "archived");
        assert_eq!(
            SwipeCandidateState::FailedMidStream.to_string(),
            "failed_mid_stream"
        );
    }

    #[test]
    fn cancel_reasons_accept_design_and_storage_spellings() {
        assert_eq!(
            "UserRequested".parse::<CancelReason>().unwrap(),
            CancelReason::UserRequested
        );
        assert_eq!(
            "backpressure-timeout".parse::<CancelReason>().unwrap(),
            CancelReason::BackpressureTimeout
        );
        assert_eq!(
            "backend_disconnect".parse::<CancelReason>().unwrap(),
            CancelReason::BackendDisconnect
        );
        assert!("something-else".parse::<CancelReason>().is_err());
    }

    #[test]
    fn generation_state_helpers_capture_request_lifecycle() {
        let request_id = RequestId::parse(REQUEST_ID).unwrap();
        let message_id = MessageId::parse(MESSAGE_ID).unwrap();

        let queued = GenerationState::Queued {
            request_id: request_id.clone(),
        };
        let completed = GenerationState::Completed {
            request_id: request_id.clone(),
            message_id,
            tokens_generated: 128,
            duration_ms: 900,
        };

        assert_eq!(GenerationState::default(), GenerationState::Idle);
        assert_eq!(queued.request_id(), Some(&request_id));
        assert!(queued.is_inflight());
        assert!(!queued.is_terminal());
        assert!(completed.is_terminal());
        assert!(!completed.is_inflight());
    }

    #[test]
    fn constructors_apply_phase1b_defaults() {
        let session_id = SessionId::parse(SESSION_ID).unwrap();
        let message_id = MessageId::parse(MESSAGE_ID).unwrap();
        let branch_id = BranchId::parse(BRANCH_ID).unwrap();
        let swipe_group_id = SwipeGroupId::parse(SWIPE_GROUP_ID).unwrap();

        let message = ConversationMessage::new(
            session_id.clone(),
            message_id.clone(),
            "assistant",
            "hello",
            1_725_647_200_123,
        );
        let branch = ConversationBranch::new(
            branch_id,
            session_id,
            "main",
            message_id.clone(),
            1_725_647_200_123,
        );
        let swipe_group = SwipeGroup::new(swipe_group_id.clone(), message_id.clone());
        let swipe_candidate = SwipeCandidate::new(swipe_group_id, 0, message_id);

        assert_eq!(message.author_name, None);
        assert_eq!(message.parent_id, None);
        assert!(!message.is_hidden);
        assert_eq!(branch.state, BranchState::Inactive);
        assert_eq!(branch.description, None);
        assert_eq!(swipe_group.parent_context_message_id, None);
        assert_eq!(swipe_group.active_ordinal, 0);
        assert_eq!(swipe_candidate.state, SwipeCandidateState::Active);
        assert_eq!(swipe_candidate.partial_content, None);
        assert_eq!(swipe_candidate.tokens_generated, None);
    }

    #[test]
    fn command_and_event_labels_stay_stable() {
        let session_id = SessionId::parse(SESSION_ID).unwrap();
        let message_id = MessageId::parse(MESSAGE_ID).unwrap();
        let branch_id = BranchId::parse(BRANCH_ID).unwrap();
        let swipe_group_id = SwipeGroupId::parse(SWIPE_GROUP_ID).unwrap();
        let request_id = RequestId::parse(REQUEST_ID).unwrap();

        let command = ConversationCommand::CommitMessage(CommitMessageCommand {
            branch_id: branch_id.clone(),
            message: ConversationMessage::new(
                session_id,
                message_id.clone(),
                "user",
                "hi",
                1_725_647_200_123,
            ),
        });
        let activate_swipe = ConversationCommand::ActivateSwipe(ActivateSwipeCommand {
            swipe_group_id: swipe_group_id.clone(),
            ordinal: 2,
        });
        let generation_event = OzoneEvent::GenerationStateChanged {
            branch_id: branch_id.clone(),
            state: GenerationState::Cancelled {
                request_id,
                partial_content: Some("partial".to_owned()),
                tokens_generated: 24,
                reason: CancelReason::UserRequested,
            },
        };
        let swipe_event = OzoneEvent::SwipeActivated {
            swipe_group_id,
            ordinal: 2,
        };
        let state_command = ConversationCommand::SetGenerationState(SetGenerationStateCommand {
            branch_id,
            state: GenerationState::Idle,
        });

        assert_eq!(command.kind(), "commit_message");
        assert_eq!(activate_swipe.kind(), "activate_swipe");
        assert_eq!(state_command.kind(), "set_generation_state");
        assert_eq!(generation_event.event_type(), "generation_state_changed");
        assert_eq!(swipe_event.event_type(), "swipe_activated");
    }
}
