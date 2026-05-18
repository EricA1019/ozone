use ozone_core::engine::{
    ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
    ConversationMessage, CreateBranchCommand, MessageId, RecordSwipeCandidateCommand,
    SwipeCandidate, SwipeCandidateState, SwipeGroup, SwipeGroupId,
};
use ozone_engine::{
    ActivateBranchCommand, ActivateSwipeRequest, ConversationBranchRecord,
    ConversationEngine, ConversationStore, EditMessageCommand, EngineCommand, EngineCommandResult,
    RecordSwipeCandidateRequest, SingleWriterConversationEngine, SwipeGroupSnapshot,
};
use ozone_persist::{
    BranchRecord, CreateMessageRequest, PersistError, SessionId,
    SessionSummary, SqliteRepository,
};

pub struct RepoConversationStore {
    pub repo: SqliteRepository,
}

pub struct ManualSwipeCandidateRequest {
    pub session_id: SessionId,
    pub parent_message_id: MessageId,
    pub parent_context_message_id: Option<MessageId>,
    pub swipe_group_id: Option<SwipeGroupId>,
    pub ordinal: Option<u16>,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub content: String,
    pub state: SwipeCandidateState,
}

impl RepoConversationStore {
    pub fn new(repo: SqliteRepository) -> Self {
        Self { repo }
    }

    pub fn ensure_session_exists(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionSummary, PersistError> {
        self.repo
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))
    }

    pub fn create_swipe_candidate(
        &mut self,
        request: ManualSwipeCandidateRequest,
    ) -> Result<(SwipeGroup, SwipeCandidate), PersistError> {
        self.ensure_session_exists(&request.session_id)?;

        let message_record = self.repo.insert_message(
            &request.session_id,
            CreateMessageRequest {
                parent_id: Some(request.parent_message_id.to_string()),
                author_kind: request.author_kind,
                author_name: request.author_name,
                content: request.content,
            },
        )?;
        let message_id = MessageId::parse(message_record.message_id.clone())?;

        let existing_group = match request.swipe_group_id.as_ref() {
            Some(group_id) => self.repo.get_swipe_group(&request.session_id, group_id)?,
            None => self
                .repo
                .list_swipe_groups(&request.session_id)?
                .into_iter()
                .find(|group| group.parent_message_id == request.parent_message_id),
        };

        let mut group = match existing_group {
            Some(group) => group,
            None => {
                let mut group = SwipeGroup::new(
                    next_swipe_group_id(request.swipe_group_id.clone())?,
                    request.parent_message_id.clone(),
                );
                group.parent_context_message_id = request.parent_context_message_id.clone();
                group
            }
        };
        if group.parent_context_message_id.is_none() {
            group.parent_context_message_id = request.parent_context_message_id;
        }

        let next_ordinal = match request.ordinal {
            Some(ordinal) => ordinal,
            None => match self
                .repo
                .list_swipe_candidates(&request.session_id, &group.swipe_group_id)
            {
                Ok(candidates) => candidates
                    .iter()
                    .map(|candidate| candidate.ordinal)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1),
                Err(PersistError::SwipeGroupNotFound(_)) => 0,
                Err(error) => return Err(error),
            },
        };

        let candidate = self.repo.record_swipe_candidate(
            &request.session_id,
            RecordSwipeCandidateCommand {
                group: group.clone(),
                candidate: SwipeCandidate {
                    swipe_group_id: group.swipe_group_id.clone(),
                    ordinal: next_ordinal,
                    message_id,
                    state: request.state,
                    partial_content: None,
                    tokens_generated: None,
                },
            },
        )?;

        Ok((group, candidate))
    }
}

impl ConversationStore for RepoConversationStore {
    type Error = PersistError;

    fn commit_message(
        &mut self,
        command: CommitMessageCommand,
    ) -> Result<ConversationMessage, Self::Error> {
        match self.repo.commit_message(command.clone()) {
            Ok(message) => Ok(message),
            Err(PersistError::BranchNotFound(_))
                if command.message.parent_id.is_none()
                    && self
                        .repo
                        .get_active_branch(&command.message.session_id)?
                        .is_none() =>
            {
                let record = self.repo.insert_message(
                    &command.message.session_id,
                    CreateMessageRequest {
                        parent_id: None,
                        author_kind: command.message.author_kind.clone(),
                        author_name: command.message.author_name.clone(),
                        content: command.message.content.clone(),
                    },
                )?;
                let persisted_message = conversation_message_from_record(record)?;
                let mut branch = ConversationBranch::new(
                    command.branch_id,
                    command.message.session_id.clone(),
                    "main",
                    persisted_message.message_id.clone(),
                    persisted_message.created_at,
                );
                branch.state = BranchState::Active;
                self.repo.create_branch(CreateBranchCommand {
                    branch,
                    forked_from: persisted_message.message_id.clone(),
                })?;
                Ok(persisted_message)
            }
            Err(error) => Err(error),
        }
    }

    fn edit_message(
        &mut self,
        command: EditMessageCommand,
    ) -> Result<ConversationMessage, Self::Error> {
        self.repo.edit_message(
            &command.session_id,
            &command.message_id,
            ozone_persist::EditMessageRequest {
                content: command.content,
                edited_at: command.edited_at,
            },
        )
    }

    fn create_branch(
        &mut self,
        command: CreateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error> {
        self.repo.create_branch(command).map(map_branch_record)
    }

    fn list_branches(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationBranchRecord>, Self::Error> {
        self.repo
            .list_branches(session_id)
            .map(|records| records.into_iter().map(map_branch_record).collect())
    }

    fn get_active_branch(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<ConversationBranchRecord>, Self::Error> {
        self.repo
            .get_active_branch(session_id)
            .map(|branch| branch.map(map_branch_record))
    }

    fn activate_branch(
        &mut self,
        command: ActivateBranchCommand,
    ) -> Result<ConversationBranchRecord, Self::Error> {
        self.repo
            .activate_branch(&command.session_id, &command.branch_id)
            .map(map_branch_record)
    }

    fn record_swipe_candidate(
        &mut self,
        command: RecordSwipeCandidateRequest,
    ) -> Result<SwipeCandidate, Self::Error> {
        self.repo
            .record_swipe_candidate(&command.session_id, command.command)
    }

    fn activate_swipe_candidate(
        &mut self,
        command: ActivateSwipeRequest,
    ) -> Result<SwipeGroup, Self::Error> {
        let group = self
            .repo
            .activate_swipe_candidate(&command.session_id, command.command.clone())?;
        let selected_candidate = self
            .repo
            .list_swipe_candidates(&command.session_id, &group.swipe_group_id)?
            .into_iter()
            .find(|candidate| candidate.ordinal == group.active_ordinal)
            .ok_or_else(|| PersistError::SwipeCandidateNotFound {
                swipe_group_id: group.swipe_group_id.to_string(),
                ordinal: group.active_ordinal,
            })?;

        if let Some(active_branch) = self.repo.get_active_branch(&command.session_id)? {
            let candidate_message_ids = self
                .repo
                .list_swipe_candidates(&command.session_id, &group.swipe_group_id)?
                .into_iter()
                .map(|candidate| candidate.message_id)
                .collect::<Vec<_>>();
            if active_branch.branch.tip_message_id == group.parent_message_id
                || candidate_message_ids.contains(&active_branch.branch.tip_message_id)
            {
                let _ = self.repo.set_branch_tip(
                    &command.session_id,
                    &active_branch.branch.branch_id,
                    &selected_candidate.message_id,
                )?;
            }
        }

        Ok(group)
    }

    fn list_swipe_groups(&self, session_id: &SessionId) -> Result<Vec<SwipeGroup>, Self::Error> {
        self.repo.list_swipe_groups(session_id)
    }

    fn list_swipe_candidates(
        &self,
        session_id: &SessionId,
        swipe_group_id: &SwipeGroupId,
    ) -> Result<Vec<SwipeCandidate>, Self::Error> {
        self.repo.list_swipe_candidates(session_id, swipe_group_id)
    }

    fn list_branch_messages(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<Vec<ConversationMessage>, Self::Error> {
        self.repo.list_branch_messages(session_id, branch_id)
    }

    fn get_active_branch_transcript(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationMessage>, Self::Error> {
        self.repo.get_active_branch_transcript(session_id)
    }
}

pub struct Phase1bCliEngine {
    pub(crate) engine: SingleWriterConversationEngine<RepoConversationStore>,
}

impl Phase1bCliEngine {
    pub fn open() -> Result<Self, String> {
        let repo = open_repository()?;
        Ok(Self {
            engine: SingleWriterConversationEngine::new(RepoConversationStore::new(repo)),
        })
    }

    pub fn send(
        &mut self,
        args: crate::cli::args::SendArgs,
    ) -> Result<(ConversationMessage, bool), String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        self.engine
            .store()
            .ensure_session_exists(&session_id)
            .map_err(|error| error.to_string())?;
        let active_branch = self
            .engine
            .store()
            .get_active_branch(&session_id)
            .map_err(|error| error.to_string())?;
        let bootstrapped = active_branch.is_none();
        let branch_id = active_branch
            .as_ref()
            .map(|record| record.branch.branch_id.clone())
            .map(Ok)
            .unwrap_or_else(crate::cli::util::generate_branch_id)?;
        let mut message = ConversationMessage::new(
            session_id.clone(),
            crate::cli::util::generate_message_id()?,
            crate::cli::util::require_non_empty("author kind", args.author_kind)?,
            crate::cli::util::require_non_empty("message content", args.content)?,
            now_timestamp_ms(),
        );
        message.parent_id = active_branch
            .as_ref()
            .map(|record| record.branch.tip_message_id.clone());
        message.author_name = crate::cli::util::optional_value(args.author_name);

        match self
            .engine
            .process(EngineCommand::CommitMessage(CommitMessageCommand {
                branch_id,
                message,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageCommitted(message) => Ok((message, bootstrapped)),
            other => Err(format!("unexpected engine result for send: {other:?}")),
        }
    }

    pub fn transcript(
        &self,
        args: crate::cli::args::TranscriptArgs,
    ) -> Result<(Option<ConversationBranchRecord>, Vec<ConversationMessage>), String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        self.engine
            .store()
            .ensure_session_exists(&session_id)
            .map_err(|error| error.to_string())?;

        if let Some(branch_id) = args.branch_id {
            let branch_id = crate::cli::util::parse_branch_id(&branch_id)?;
            let branch = self
                .engine
                .store()
                .list_branches(&session_id)
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|record| record.branch.branch_id == branch_id)
                .ok_or_else(|| format!("branch {branch_id} was not found"))?;
            let transcript = self
                .engine
                .store()
                .list_branch_messages(&session_id, &branch.branch.branch_id)
                .map_err(|error| error.to_string())?;
            Ok((Some(branch), transcript))
        } else {
            let snapshot = self
                .engine
                .snapshot(&session_id)
                .map_err(|error| error.to_string())?;
            Ok((snapshot.active_branch, snapshot.transcript))
        }
    }

    pub fn edit(&mut self, args: crate::cli::args::EditArgs) -> Result<ConversationMessage, String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        let message_id = crate::cli::util::parse_message_id(&args.message_id)?;
        match self
            .engine
            .process(EngineCommand::EditMessage(EditMessageCommand {
                session_id,
                message_id,
                content: crate::cli::util::require_non_empty("message content", args.content)?,
                edited_at: Some(now_timestamp_ms()),
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageEdited(message) => Ok(message),
            other => Err(format!("unexpected engine result for edit: {other:?}")),
        }
    }

    pub fn list_branches(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ConversationBranchRecord>, String> {
        self.engine
            .store()
            .ensure_session_exists(session_id)
            .map_err(|error| error.to_string())?;
        self.engine
            .store()
            .list_branches(session_id)
            .map_err(|error| error.to_string())
    }

    pub fn create_branch(
        &mut self,
        args: crate::cli::args::BranchCreateArgs,
    ) -> Result<ConversationBranchRecord, String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        self.engine
            .store()
            .ensure_session_exists(&session_id)
            .map_err(|error| error.to_string())?;

        let forked_from = match args.from_message_id {
            Some(ref message_id) => crate::cli::util::parse_message_id(message_id)?,
            None => self
                .engine
                .store()
                .get_active_branch(&session_id)
                .map_err(|error| error.to_string())?
                .map(|record| record.branch.tip_message_id)
                .ok_or_else(|| {
                    format!(
                        "session {session_id} has no active branch yet; send the first message before branching"
                    )
                })?,
        };

        let mut branch = ConversationBranch::new(
            crate::cli::util::generate_branch_id()?,
            session_id,
            crate::cli::util::require_non_empty("branch name", args.name)?,
            forked_from.clone(),
            now_timestamp_ms(),
        );
        if args.activate {
            branch.state = BranchState::Active;
        }

        match self
            .engine
            .process(EngineCommand::CreateBranch(CreateBranchCommand {
                branch,
                forked_from,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::BranchCreated(record) => Ok(record),
            other => Err(format!(
                "unexpected engine result for branch create: {other:?}"
            )),
        }
    }

    pub fn activate_branch(
        &mut self,
        args: crate::cli::args::BranchActivateArgs,
    ) -> Result<ConversationBranchRecord, String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        let branch_id = crate::cli::util::parse_branch_id(&args.branch_id)?;
        match self
            .engine
            .process(EngineCommand::ActivateBranch(ActivateBranchCommand {
                session_id,
                branch_id,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::BranchActivated(record) => Ok(record),
            other => Err(format!(
                "unexpected engine result for branch activation: {other:?}"
            )),
        }
    }

    pub fn list_swipes(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SwipeGroupSnapshot>, String> {
        self.engine
            .store()
            .ensure_session_exists(session_id)
            .map_err(|error| error.to_string())?;
        self.engine
            .snapshot(session_id)
            .map(|snapshot| snapshot.swipe_groups)
            .map_err(|error| error.to_string())
    }

    pub fn add_swipe_candidate(
        &mut self,
        args: crate::cli::args::SwipeAddArgs,
    ) -> Result<(SwipeGroup, SwipeCandidate), String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        let parent_message_id = crate::cli::util::parse_message_id(&args.parent_message_id)?;
        let parent_context_message_id = args
            .parent_context_message_id
            .as_deref()
            .map(crate::cli::util::parse_message_id)
            .transpose()?;
        let swipe_group_id = args
            .swipe_group_id
            .as_deref()
            .map(crate::cli::util::parse_swipe_group_id)
            .transpose()?;
        let state = args
            .state
            .trim()
            .parse::<SwipeCandidateState>()
            .map_err(|error| error.to_string())?;

        self.engine
            .store_mut()
            .create_swipe_candidate(ManualSwipeCandidateRequest {
                session_id,
                parent_message_id,
                parent_context_message_id,
                swipe_group_id,
                ordinal: args.ordinal,
                author_kind: crate::cli::util::require_non_empty("author kind", args.author_kind)?,
                author_name: crate::cli::util::optional_value(args.author_name),
                content: crate::cli::util::require_non_empty("message content", args.content)?,
                state,
            })
            .map_err(|error| error.to_string())
    }

    pub fn activate_swipe(
        &mut self,
        args: crate::cli::args::SwipeActivateArgs,
    ) -> Result<SwipeGroup, String> {
        let session_id = crate::cli::util::parse_session_id(&args.session_id)?;
        let swipe_group_id = crate::cli::util::parse_swipe_group_id(&args.swipe_group_id)?;
        match self
            .engine
            .process(EngineCommand::ActivateSwipe(ActivateSwipeRequest {
                session_id,
                command: ActivateSwipeCommand {
                    swipe_group_id,
                    ordinal: args.ordinal,
                },
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::SwipeActivated(group) => Ok(group),
            other => Err(format!(
                "unexpected engine result for swipe activation: {other:?}"
            )),
        }
    }
}

static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn now_timestamp_ms() -> i64 {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn generate_swipe_group_id() -> Result<SwipeGroupId, String> {
    SwipeGroupId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

fn next_swipe_group_id(
    requested: Option<SwipeGroupId>,
) -> Result<SwipeGroupId, PersistError> {
    match requested {
        Some(id) => Ok(id),
        None => generate_swipe_group_id().map_err(PersistError::InvalidData),
    }
}

pub fn generate_uuid_like() -> String {
    let counter = u128::from(ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_nanos();
    let pid = u128::from(std::process::id());
    let mut bytes = (nanos ^ (counter << 64) ^ (pid << 32)).to_be_bytes();

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

pub fn open_repository() -> Result<SqliteRepository, String> {
    SqliteRepository::from_xdg().map_err(|error| error.to_string())
}

pub fn map_branch_record(record: BranchRecord) -> ConversationBranchRecord {
    ConversationBranchRecord {
        branch: record.branch,
        forked_from: record.forked_from,
    }
}

pub fn conversation_message_from_record(
    record: ozone_persist::MessageRecord,
) -> Result<ConversationMessage, PersistError> {
    Ok(ConversationMessage {
        message_id: MessageId::parse(record.message_id)?,
        session_id: record.session_id,
        parent_id: record
            .parent_id
            .as_deref()
            .map(MessageId::parse)
            .transpose()?,
        author_kind: record.author_kind,
        author_name: record.author_name,
        content: record.content,
        created_at: record.created_at,
        edited_at: None,
        is_hidden: false,
    })
}