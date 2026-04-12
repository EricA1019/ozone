use clap::{Args, Parser, Subcommand};
use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, CancelReason, CommitMessageCommand,
        ConversationBranch, ConversationMessage, CreateBranchCommand, GenerationState, MessageId,
        RequestId, SetGenerationStateCommand, SwipeCandidate, SwipeCandidateState, SwipeGroup,
        SwipeGroupId,
    },
    paths::{benchmarks_db_path, data_dir, kobold_log_path, preferences_path},
    product::{ProductTier, OZONE_PLUS_DESIGN_DOC_PATH, OZONE_PLUS_DOC_PATH},
};
use ozone_engine::{
    ActivateBranchCommand, ActivateSwipeRequest, ConversationBranchRecord, ConversationEngine,
    ConversationStore, EditMessageCommand, EngineCommand, EngineCommandResult,
    RecordSwipeCandidateRequest, SingleWriterConversationEngine, SwipeGroupSnapshot,
};
use ozone_persist::{
    BranchRecord, CreateMessageRequest, CreateSessionRequest, PersistError, PersistencePaths,
    SessionId, SessionSummary, SqliteRepository,
};
use ozone_tui::{
    run_terminal_session, AppBootstrap as TuiBootstrap, BranchItem as TuiBranchItem,
    DraftState as TuiDraftState, RuntimeCancellation as TuiRuntimeCancellation,
    RuntimeCompletion as TuiRuntimeCompletion, RuntimeSendReceipt as TuiRuntimeSendReceipt,
    SessionContext as TuiSessionContext, SessionRuntime, TranscriptItem as TuiTranscriptItem,
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Parser)]
#[command(
    name = "ozone-plus",
    version,
    about = "Phase 1C ozone+ chat shell with engine-backed persistence",
    long_about = "Phase 1C ozone+ chat shell with engine-backed persistence.\n\nThis binary now opens a real chat-first terminal shell for persisted sessions, while still exposing the lower-level Phase 1B CLI surfaces for transcripts, branches, and manual swipe seeding. The frontend still uses a mock backend for assistant replies, but user turns, transcript state, session locks, and draft persistence all run through the real ozone+ persistence layer.",
    after_help = "Examples:\n  ozone-plus create \"First Session\"\n  ozone-plus open <session-id>\n  ozone-plus send <session-id> \"Hello there\"\n  ozone-plus transcript <session-id>\n  ozone-plus branch create <session-id> fork --activate\n  ozone-plus swipe add <session-id> <parent-message-id> \"Alternate reply\"\n  ozone-plus swipe activate <session-id> <swipe-group-id> 1"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show the shared product identity for the ozone family
    Identity,
    /// Show the ozone+ documentation entry points tracked in ozone-core
    Docs,
    /// Show the shared filesystem paths ozone+ expects to use
    Paths,
    /// Create a persisted ozone+ session
    Create(CreateArgs),
    /// List persisted ozone+ sessions
    List,
    /// Resolve and open a persisted session record
    #[command(visible_alias = "show")]
    Open(OpenArgs),
    /// Send a message through the conversation engine
    Send(SendArgs),
    /// Show the active transcript or a specific branch transcript
    #[command(visible_alias = "messages")]
    Transcript(TranscriptArgs),
    /// Edit an existing message
    Edit(EditArgs),
    /// Inspect and manipulate branches
    Branch(BranchArgs),
    /// Inspect and manipulate swipe groups/candidates
    Swipe(SwipeArgs),
}

#[derive(Args)]
struct CreateArgs {
    /// Human-readable session name stored in the global index
    name: String,
    /// Optional character name associated with the session
    #[arg(long = "character", value_name = "NAME")]
    character_name: Option<String>,
    /// Optional session tag (repeat --tag for multiple values)
    #[arg(long = "tag", short = 't', value_name = "TAG")]
    tags: Vec<String>,
}

#[derive(Args)]
struct OpenArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Print the Phase 1B metadata summary instead of launching the TUI shell
    #[arg(long)]
    metadata: bool,
}

#[derive(Args)]
struct SendArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Message content to append to the active branch
    content: String,
    /// Author role written into the transcript
    #[arg(long = "author", default_value = "user")]
    author_kind: String,
    /// Optional display name for the author
    #[arg(long = "author-name", value_name = "NAME")]
    author_name: Option<String>,
}

#[derive(Args)]
struct TranscriptArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Optional branch UUID; defaults to the active branch
    #[arg(long = "branch", value_name = "BRANCH_ID")]
    branch_id: Option<String>,
}

#[derive(Args)]
struct EditArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Message UUID to edit
    message_id: String,
    /// Replacement message content
    content: String,
}

#[derive(Args)]
struct BranchArgs {
    #[command(subcommand)]
    command: BranchCommand,
}

#[derive(Subcommand)]
enum BranchCommand {
    /// List all persisted branches for a session
    List(SessionArgs),
    /// Create a new branch from a message (defaults to the active branch tip)
    Create(BranchCreateArgs),
    /// Activate an existing branch
    Activate(BranchActivateArgs),
}

#[derive(Args)]
struct BranchCreateArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Human-readable branch name
    name: String,
    /// Optional source message UUID; defaults to the active branch tip
    #[arg(long = "from", value_name = "MESSAGE_ID")]
    from_message_id: Option<String>,
    /// Activate the new branch immediately
    #[arg(long)]
    activate: bool,
}

#[derive(Args)]
struct BranchActivateArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Branch UUID to activate
    branch_id: String,
}

#[derive(Args)]
struct SwipeArgs {
    #[command(subcommand)]
    command: SwipeCommand,
}

#[derive(Subcommand)]
enum SwipeCommand {
    /// List persisted swipe groups and their candidates
    List(SessionArgs),
    /// Add a manual swipe candidate beneath a parent message
    Add(SwipeAddArgs),
    /// Activate a swipe candidate by ordinal
    Activate(SwipeActivateArgs),
}

#[derive(Args)]
struct SwipeAddArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Parent message UUID shared by the swipe candidates
    parent_message_id: String,
    /// Candidate content to persist
    content: String,
    /// Optional context parent UUID for the swipe group
    #[arg(long = "context", value_name = "MESSAGE_ID")]
    parent_context_message_id: Option<String>,
    /// Optional existing swipe group UUID; omitted means create/reuse by parent message
    #[arg(long = "group-id", value_name = "SWIPE_GROUP_ID")]
    swipe_group_id: Option<String>,
    /// Optional explicit ordinal; omitted means append after the current highest ordinal
    #[arg(long, value_name = "ORDINAL")]
    ordinal: Option<u16>,
    /// Author role written into the candidate message
    #[arg(long = "author", default_value = "assistant")]
    author_kind: String,
    /// Optional display name for the candidate author
    #[arg(long = "author-name", value_name = "NAME")]
    author_name: Option<String>,
    /// Candidate state (`active`, `discarded`, `failed_mid_stream`)
    #[arg(long, default_value = "active")]
    state: String,
}

#[derive(Args)]
struct SwipeActivateArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Swipe group UUID to update
    swipe_group_id: String,
    /// Candidate ordinal to activate
    ordinal: u16,
}

#[derive(Args)]
struct SessionArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Identity) => {
            print_identity();
            Ok(())
        }
        Some(Command::Docs) => {
            print_docs();
            Ok(())
        }
        Some(Command::Paths) => {
            print_paths();
            Ok(())
        }
        Some(Command::Create(args)) => create_session(args),
        Some(Command::List) => list_sessions(),
        Some(Command::Open(args)) => open_session(args),
        Some(Command::Send(args)) => send_message(args),
        Some(Command::Transcript(args)) => show_transcript(args),
        Some(Command::Edit(args)) => edit_message(args),
        Some(Command::Branch(args)) => handle_branch_command(args.command),
        Some(Command::Swipe(args)) => handle_swipe_command(args.command),
        None => {
            print_bootstrap_summary();
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
struct RepoConversationStore {
    repo: SqliteRepository,
}

struct ManualSwipeCandidateRequest {
    session_id: SessionId,
    parent_message_id: MessageId,
    parent_context_message_id: Option<MessageId>,
    swipe_group_id: Option<SwipeGroupId>,
    ordinal: Option<u16>,
    author_kind: String,
    author_name: Option<String>,
    content: String,
    state: SwipeCandidateState,
}

impl RepoConversationStore {
    fn new(repo: SqliteRepository) -> Self {
        Self { repo }
    }

    fn ensure_session_exists(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionSummary, PersistError> {
        self.repo
            .get_session(session_id)?
            .ok_or_else(|| PersistError::SessionNotFound(session_id.to_string()))
    }

    fn create_swipe_candidate(
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

        let mut group = existing_group.unwrap_or_else(|| {
            let mut group = SwipeGroup::new(
                request
                    .swipe_group_id
                    .unwrap_or_else(|| generate_swipe_group_id().expect("valid UUID")),
                request.parent_message_id.clone(),
            );
            group.parent_context_message_id = request.parent_context_message_id.clone();
            group
        });
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
            ozone_persist::RecordSwipeCandidateCommand {
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
            let transcript = self
                .repo
                .list_branch_messages(&command.session_id, &active_branch.branch.branch_id)?;
            if transcript
                .iter()
                .any(|message| message.message_id == group.parent_message_id)
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

struct Phase1bCliEngine {
    engine: SingleWriterConversationEngine<RepoConversationStore>,
}

impl Phase1bCliEngine {
    fn open() -> Result<Self, String> {
        let repo = open_repository()?;
        Ok(Self {
            engine: SingleWriterConversationEngine::new(RepoConversationStore::new(repo)),
        })
    }

    fn send(&mut self, args: SendArgs) -> Result<(ConversationMessage, bool), String> {
        let session_id = parse_session_id(&args.session_id)?;
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
            .unwrap_or(generate_branch_id()?);
        let mut message = ConversationMessage::new(
            session_id.clone(),
            generate_message_id()?,
            require_non_empty("author kind", args.author_kind)?,
            require_non_empty("message content", args.content)?,
            now_timestamp_ms(),
        );
        message.parent_id = active_branch
            .as_ref()
            .map(|record| record.branch.tip_message_id.clone());
        message.author_name = optional_value(args.author_name);

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

    fn transcript(
        &self,
        args: TranscriptArgs,
    ) -> Result<(Option<ConversationBranchRecord>, Vec<ConversationMessage>), String> {
        let session_id = parse_session_id(&args.session_id)?;
        self.engine
            .store()
            .ensure_session_exists(&session_id)
            .map_err(|error| error.to_string())?;

        if let Some(branch_id) = args.branch_id {
            let branch_id = parse_branch_id(&branch_id)?;
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

    fn edit(&mut self, args: EditArgs) -> Result<ConversationMessage, String> {
        let session_id = parse_session_id(&args.session_id)?;
        let message_id = parse_message_id(&args.message_id)?;
        match self
            .engine
            .process(EngineCommand::EditMessage(EditMessageCommand {
                session_id,
                message_id,
                content: require_non_empty("message content", args.content)?,
                edited_at: Some(now_timestamp_ms()),
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::MessageEdited(message) => Ok(message),
            other => Err(format!("unexpected engine result for edit: {other:?}")),
        }
    }

    fn list_branches(
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

    fn create_branch(
        &mut self,
        args: BranchCreateArgs,
    ) -> Result<ConversationBranchRecord, String> {
        let session_id = parse_session_id(&args.session_id)?;
        self.engine
            .store()
            .ensure_session_exists(&session_id)
            .map_err(|error| error.to_string())?;

        let forked_from = match args.from_message_id {
            Some(message_id) => parse_message_id(&message_id)?,
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
            generate_branch_id()?,
            session_id,
            require_non_empty("branch name", args.name)?,
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

    fn activate_branch(
        &mut self,
        args: BranchActivateArgs,
    ) -> Result<ConversationBranchRecord, String> {
        let session_id = parse_session_id(&args.session_id)?;
        let branch_id = parse_branch_id(&args.branch_id)?;
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

    fn list_swipes(&self, session_id: &SessionId) -> Result<Vec<SwipeGroupSnapshot>, String> {
        self.engine
            .store()
            .ensure_session_exists(session_id)
            .map_err(|error| error.to_string())?;
        self.engine
            .snapshot(session_id)
            .map(|snapshot| snapshot.swipe_groups)
            .map_err(|error| error.to_string())
    }

    fn add_swipe_candidate(
        &mut self,
        args: SwipeAddArgs,
    ) -> Result<(SwipeGroup, SwipeCandidate), String> {
        let session_id = parse_session_id(&args.session_id)?;
        let parent_message_id = parse_message_id(&args.parent_message_id)?;
        let parent_context_message_id = args
            .parent_context_message_id
            .as_deref()
            .map(parse_message_id)
            .transpose()?;
        let swipe_group_id = args
            .swipe_group_id
            .as_deref()
            .map(parse_swipe_group_id)
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
                author_kind: require_non_empty("author kind", args.author_kind)?,
                author_name: optional_value(args.author_name),
                content: require_non_empty("message content", args.content)?,
                state,
            })
            .map_err(|error| error.to_string())
    }

    fn activate_swipe(&mut self, args: SwipeActivateArgs) -> Result<SwipeGroup, String> {
        let session_id = parse_session_id(&args.session_id)?;
        let swipe_group_id = parse_swipe_group_id(&args.swipe_group_id)?;
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

#[derive(Debug)]
struct PendingMockGeneration {
    branch_id: BranchId,
    request_id: RequestId,
    prompt: String,
    started_at: Instant,
}

#[derive(Debug)]
struct Phase1cRuntime {
    repo: SqliteRepository,
    engine: SingleWriterConversationEngine<RepoConversationStore>,
    session_id: SessionId,
    lock_instance_id: String,
    pending_generation: Option<PendingMockGeneration>,
}

impl Phase1cRuntime {
    fn open(repo: SqliteRepository, session_id: SessionId) -> Result<Self, String> {
        repo.get_session(&session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {session_id} was not found"))?;

        let instance_id = format!("ozone-plus-phase1c-{}", std::process::id());
        repo.acquire_session_lock(&session_id, &instance_id)
            .map_err(|error| match error {
                PersistError::SessionLocked {
                    instance_id,
                    acquired_at,
                } => format!(
                    "session {session_id} is locked by instance {instance_id} (since {})",
                    format_timestamp(acquired_at)
                ),
                other => other.to_string(),
            })?;

        Ok(Self {
            engine: SingleWriterConversationEngine::new(RepoConversationStore::new(repo.clone())),
            repo,
            session_id,
            lock_instance_id: instance_id,
            pending_generation: None,
        })
    }

    fn release_lock(&mut self) -> Result<(), String> {
        if !self
            .repo
            .release_session_lock(&self.session_id, &self.lock_instance_id)
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "session {} lock was acquired but could not be released cleanly",
                self.session_id
            ));
        }

        Ok(())
    }

    fn load_bootstrap(&self, context: &TuiSessionContext) -> Result<TuiBootstrap, String> {
        let session = self
            .repo
            .get_session(&context.session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {} was not found", context.session_id))?;
        let branches = self
            .engine
            .store()
            .list_branches(&context.session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(tui_branch_from_record)
            .collect();
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(tui_transcript_item_from_message)
            .collect();

        Ok(TuiBootstrap {
            transcript,
            branches,
            status_line: Some(format!(
                "mock backend ready · session locked by {} · Enter sends, Ctrl+C cancels, Ctrl+I toggles inspector",
                self.lock_instance_id
            )),
            draft: self.load_persisted_draft(&session.session_id)?,
            screen: None,
        })
    }

    fn load_persisted_draft(&self, session_id: &SessionId) -> Result<Option<TuiDraftState>, String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        match fs::read_to_string(&draft_path) {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(TuiDraftState::restore(ozone_tui::app::DraftCheckpoint::new(
                text.clone(),
                text.chars().count(),
            )))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!(
                "failed to read persisted draft at {}: {error}",
                draft_path.display()
            )),
        }
    }

    fn save_persisted_draft(
        &self,
        session_id: &SessionId,
        draft: Option<&str>,
    ) -> Result<(), String> {
        let draft_path = self.repo.paths().session_draft_path(session_id);
        let parent = draft_path
            .parent()
            .ok_or_else(|| format!("draft path {} has no parent directory", draft_path.display()))?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create draft directory {}: {error}",
                parent.display()
            )
        })?;

        match draft.filter(|text| !text.is_empty()) {
            Some(text) => fs::write(&draft_path, text.as_bytes()).map_err(|error| {
                format!("failed to write persisted draft {}: {error}", draft_path.display())
            })?,
            None => match fs::remove_file(&draft_path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "failed to remove persisted draft {}: {error}",
                        draft_path.display()
                    ))
                }
            },
        }

        Ok(())
    }

    fn active_branch(&self, session_id: &SessionId) -> Result<ConversationBranchRecord, String> {
        self.engine
            .store()
            .get_active_branch(session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "session {session_id} has no active branch yet; send the first message to bootstrap the conversation"
                )
            })
    }

    fn set_generation_state(
        &mut self,
        branch_id: BranchId,
        state: GenerationState,
    ) -> Result<(), String> {
        match self
            .engine
            .process(EngineCommand::SetGenerationState(SetGenerationStateCommand {
                branch_id,
                state,
            }))
            .map_err(|error| error.to_string())?
        {
            EngineCommandResult::GenerationStateUpdated { .. } => Ok(()),
            other => Err(format!(
                "unexpected engine result for generation state update: {other:?}"
            )),
        }
    }
}

impl SessionRuntime for Phase1cRuntime {
    type Error = String;

    fn bootstrap(&mut self, context: &TuiSessionContext) -> Result<TuiBootstrap, Self::Error> {
        self.load_bootstrap(context)
    }

    fn send_draft(
        &mut self,
        context: &TuiSessionContext,
        prompt: &str,
    ) -> Result<Option<TuiRuntimeSendReceipt>, Self::Error> {
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
            .unwrap_or(generate_branch_id()?);
        let mut message = ConversationMessage::new(
            context.session_id.clone(),
            generate_message_id()?,
            "user",
            prompt.to_owned(),
            now_timestamp_ms(),
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
        let request_id = generate_request_id()?;
        self.set_generation_state(
            active_branch.branch.branch_id.clone(),
            GenerationState::Queued {
                request_id: request_id.clone(),
            },
        )?;
        self.pending_generation = Some(PendingMockGeneration {
            branch_id: active_branch.branch.branch_id.clone(),
            request_id: request_id.clone(),
            prompt: prompt.to_owned(),
            started_at: Instant::now(),
        });

        Ok(Some(TuiRuntimeSendReceipt {
            request_id: request_id.to_string(),
            user_message: tui_transcript_item_from_message(committed),
        }))
    }

    fn complete_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeCompletion>, Self::Error> {
        let pending = match self.pending_generation.take() {
            Some(pending) => pending,
            None => return Ok(None),
        };

        self.set_generation_state(
            pending.branch_id.clone(),
            GenerationState::Streaming {
                request_id: pending.request_id.clone(),
                tokens_so_far: 8,
            },
        )?;

        let active_branch = self.active_branch(&context.session_id)?;
        let mut assistant_message = ConversationMessage::new(
            context.session_id.clone(),
            generate_message_id()?,
            "assistant",
            mock_assistant_reply(&pending.prompt),
            now_timestamp_ms(),
        );
        assistant_message.author_name = Some("mock backend".into());
        assistant_message.parent_id = Some(active_branch.branch.tip_message_id.clone());

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

        let duration_ms = u64::try_from(pending.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let tokens_generated = u64::try_from(committed.content.split_whitespace().count()).unwrap_or(u64::MAX);
        self.set_generation_state(
            pending.branch_id,
            GenerationState::Completed {
                request_id: pending.request_id.clone(),
                message_id: committed.message_id.clone(),
                tokens_generated,
                duration_ms,
            },
        )?;

        Ok(Some(TuiRuntimeCompletion {
            request_id: pending.request_id.to_string(),
            assistant_message: tui_transcript_item_from_message(committed),
        }))
    }

    fn cancel_generation(
        &mut self,
        _context: &TuiSessionContext,
    ) -> Result<Option<TuiRuntimeCancellation>, Self::Error> {
        let pending = match self.pending_generation.take() {
            Some(pending) => pending,
            None => return Ok(None),
        };

        let partial = format!("Mock response cancelled for: {}", pending.prompt);
        let tokens_generated = u64::try_from(partial.split_whitespace().count()).unwrap_or(u64::MAX);
        self.set_generation_state(
            pending.branch_id,
            GenerationState::Cancelled {
                request_id: pending.request_id.clone(),
                partial_content: Some(partial.clone()),
                tokens_generated,
                reason: CancelReason::UserRequested,
            },
        )?;

        Ok(Some(TuiRuntimeCancellation {
            request_id: pending.request_id.to_string(),
            reason: CancelReason::UserRequested,
            partial_assistant_message: Some(TuiTranscriptItem::new("mock backend", partial)),
        }))
    }

    fn persist_draft(
        &mut self,
        context: &TuiSessionContext,
        draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.save_persisted_draft(&context.session_id, draft)
    }
}

fn tui_branch_from_record(record: ConversationBranchRecord) -> TuiBranchItem {
    TuiBranchItem::new(
        record.branch.branch_id.to_string(),
        record.branch.name,
        record.branch.state == BranchState::Active,
    )
}

fn tui_transcript_item_from_message(message: ConversationMessage) -> TuiTranscriptItem {
    let author = message
        .author_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| message.author_kind.clone());

    TuiTranscriptItem::new(author, message.content)
}

fn mock_assistant_reply(prompt: &str) -> String {
    let summary = prompt.trim();
    if summary.chars().count() <= 48 {
        format!("Mock assistant reply: {summary}")
    } else {
        let compact = summary.chars().take(48).collect::<String>();
        format!("Mock assistant reply: {compact}…")
    }
}

fn print_bootstrap_summary() {
    println!(
        "{} ({}) — {}",
        ProductTier::OzonePlus.display_name(),
        ProductTier::OzonePlus.slug(),
        ProductTier::OzonePlus.status_label()
    );
    println!("Phase 1B engine-backed CLI for ozone+ sessions and transcripts.");
    println!("It can create/open sessions, send and edit messages, branch transcripts, and manually seed or activate swipes.");
    println!("It does not launch the final ozone+ chat UI yet.");
    println!();
    println!("Try one of:");
    println!("  ozone-plus create \"First Session\"");
    println!("  ozone-plus send <session-id> \"Hello there\"");
    println!("  ozone-plus transcript <session-id>");
    println!("  ozone-plus branch list <session-id>");
    println!("  ozone-plus swipe list <session-id>");
}

fn print_identity() {
    println!("Current target");
    println!("  name:   {}", ProductTier::OzonePlus.display_name());
    println!("  slug:   {}", ProductTier::OzonePlus.slug());
    println!("  status: {}", ProductTier::OzonePlus.status_label());
    println!();
    println!("Ozone family");
    for (name, slug, status) in [
        (
            ProductTier::Ozonelite.display_name(),
            ProductTier::Ozonelite.slug(),
            ProductTier::Ozonelite.status_label(),
        ),
        (
            ProductTier::Ozone.display_name(),
            ProductTier::Ozone.slug(),
            ProductTier::Ozone.status_label(),
        ),
        (
            ProductTier::OzonePlus.display_name(),
            ProductTier::OzonePlus.slug(),
            ProductTier::OzonePlus.status_label(),
        ),
    ] {
        println!("  - {:<10} ({}) [{}]", name, slug, status);
    }
}

fn print_docs() {
    println!("ozone+ documentation entry points");
    println!("  family guide:    {OZONE_PLUS_DOC_PATH}");
    println!("  baseline design: {OZONE_PLUS_DESIGN_DOC_PATH}");
    println!();
    println!("These docs describe the future ozone+ scope.");
    println!("This CLI currently exercises the Phase 1B conversation engine.");
}

fn print_paths() {
    println!("Shared ozone+ filesystem paths");
    print_optional_path("data dir", data_dir());
    print_optional_path("preferences", preferences_path());
    print_optional_path("benchmarks db", benchmarks_db_path());
    print_optional_path("kobold log", kobold_log_path());
    println!();
    println!("Persistence layout");
    match PersistencePaths::from_xdg() {
        Ok(paths) => {
            print_resolved_path("global db", paths.global_db_path());
            print_resolved_path("sessions dir", paths.sessions_dir());
        }
        Err(error) => println!("  unavailable   {error}"),
    }
    println!();
    println!("This CLI now talks to the Phase 1B engine, but it still does not launch the final ozone+ chat UI.");
}

fn create_session(args: CreateArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let mut request = CreateSessionRequest::new(require_non_empty("session name", args.name)?);
    request.character_name = optional_value(args.character_name);
    request.tags = normalize_tags(args.tags);

    let session = repo
        .create_session(request)
        .map_err(|error| error.to_string())?;

    println!("Created persisted ozone+ session.");
    print_session_details(&session);
    println!();
    println!("Paths");
    print_session_paths(repo.paths(), &session.session_id);
    println!();
    println!("Phase 1B note");
    println!(
        "  Send the first message with `ozone-plus send {}`.",
        session.session_id
    );

    Ok(())
}

fn list_sessions() -> Result<(), String> {
    let repo = open_repository()?;
    let sessions = repo.list_sessions().map_err(|error| error.to_string())?;

    println!("Persisted ozone+ sessions");
    print_resolved_path("data dir", repo.paths().data_dir());
    print_resolved_path("global db", repo.paths().global_db_path());
    println!();

    if sessions.is_empty() {
        println!("No persisted sessions found yet.");
        println!("Create one with `ozone-plus create \"First Session\"`.");
        return Ok(());
    }

    for (index, session) in sessions.iter().enumerate() {
        if index != 0 {
            println!();
        }
        print_session_details(session);
    }

    println!();
    println!("Phase 1B note");
    println!("  Use `ozone-plus send <session-id> \"Hello\"` to bootstrap the active transcript.");

    Ok(())
}

fn open_session(args: OpenArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let session = repo
        .get_session(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {session_id} was not found"))?;

    if args.metadata {
        return open_session_metadata(repo, &session, &session_id);
    }

    let context = TuiSessionContext::new(session_id.clone(), session.name.clone());
    let mut runtime = Phase1cRuntime::open(repo, session_id)?;
    let session_result = run_terminal_session(context, &mut runtime).map_err(|error| error.to_string());
    let release_result = runtime.release_lock();

    match (session_result, release_result) {
        (Ok(_), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(session_error), Err(release_error)) => Err(format!(
            "{session_error}; also failed to release session lock cleanly: {release_error}"
        )),
    }
}

fn open_session_metadata(
    repo: SqliteRepository,
    session: &SessionSummary,
    session_id: &SessionId,
) -> Result<(), String> {
    let instance_id = format!("ozone-plus-phase1b-{}", std::process::id());
    let lock = repo
        .acquire_session_lock(session_id, &instance_id)
        .map_err(|error| match error {
            PersistError::SessionLocked {
                instance_id,
                acquired_at,
            } => format!(
                "session {session_id} is locked by instance {instance_id} (since {})",
                format_timestamp(acquired_at)
            ),
            other => other.to_string(),
        })?;

    if !repo
        .release_session_lock(session_id, &lock.instance_id)
        .map_err(|error| error.to_string())?
    {
        return Err(format!(
            "session {session_id} lock was acquired but could not be released cleanly"
        ));
    }

    println!("Resolved persisted ozone+ session.");
    print_session_details(session);
    println!();
    if let Some(active_branch) = repo
        .get_active_branch(session_id)
        .map_err(|error| error.to_string())?
    {
        println!("Active branch");
        print_branch_record(&active_branch, true);
        let transcript = repo
            .get_active_branch_transcript(session_id)
            .map_err(|error| error.to_string())?;
        println!("  transcript messages  {}", transcript.len());
    } else {
        println!("Active branch");
        println!("  none yet — send the first message to bootstrap the conversation");
    }
    println!();
    println!("Session open check");
    println!("  advisory lock instance   {}", lock.instance_id);
    println!(
        "  acquired at              {}",
        format_timestamp(lock.acquired_at)
    );
    println!(
        "  heartbeat at             {}",
        format_timestamp(lock.heartbeat_at)
    );
    println!("  lock release             ok");
    println!();
    println!("Paths");
    print_session_paths(repo.paths(), &session.session_id);

    Ok(())
}

fn send_message(args: SendArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let (message, bootstrapped) = engine.send(args)?;

    println!("Committed engine-backed message.");
    print_message(&message);
    if bootstrapped {
        println!();
        println!("Bootstrap note");
        println!("  This was the first transcript message, so the engine created the initial active branch.");
    }

    Ok(())
}

fn show_transcript(args: TranscriptArgs) -> Result<(), String> {
    let engine = Phase1bCliEngine::open()?;
    let (branch, transcript) = engine.transcript(args)?;

    println!("Transcript");
    match branch {
        Some(branch) => print_branch_record_from_engine(&branch, true),
        None => println!("  active branch    none"),
    }
    println!();
    print_transcript(&transcript);
    Ok(())
}

fn edit_message(args: EditArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let message = engine.edit(args)?;

    println!("Edited message.");
    print_message(&message);
    Ok(())
}

fn handle_branch_command(command: BranchCommand) -> Result<(), String> {
    match command {
        BranchCommand::List(args) => list_branches(args),
        BranchCommand::Create(args) => create_branch(args),
        BranchCommand::Activate(args) => activate_branch(args),
    }
}

fn list_branches(args: SessionArgs) -> Result<(), String> {
    let engine = Phase1bCliEngine::open()?;
    let session_id = parse_session_id(&args.session_id)?;
    let branches = engine.list_branches(&session_id)?;

    println!("Branches");
    if branches.is_empty() {
        println!("  none yet — send the first message to bootstrap the active branch");
        return Ok(());
    }
    for branch in branches {
        print_branch_record_from_engine(&branch, true);
        println!();
    }
    Ok(())
}

fn create_branch(args: BranchCreateArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let branch = engine.create_branch(args)?;

    println!("Created branch.");
    print_branch_record_from_engine(&branch, true);
    Ok(())
}

fn activate_branch(args: BranchActivateArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let branch = engine.activate_branch(args)?;

    println!("Activated branch.");
    print_branch_record_from_engine(&branch, true);
    Ok(())
}

fn handle_swipe_command(command: SwipeCommand) -> Result<(), String> {
    match command {
        SwipeCommand::List(args) => list_swipes(args),
        SwipeCommand::Add(args) => add_swipe_candidate(args),
        SwipeCommand::Activate(args) => activate_swipe(args),
    }
}

fn list_swipes(args: SessionArgs) -> Result<(), String> {
    let engine = Phase1bCliEngine::open()?;
    let session_id = parse_session_id(&args.session_id)?;
    let swipe_groups = engine.list_swipes(&session_id)?;

    println!("Swipe groups");
    if swipe_groups.is_empty() {
        println!("  none yet");
        return Ok(());
    }
    for snapshot in swipe_groups {
        print_swipe_group_snapshot(&snapshot);
        println!();
    }
    Ok(())
}

fn add_swipe_candidate(args: SwipeAddArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let (group, candidate) = engine.add_swipe_candidate(args)?;

    println!("Added swipe candidate.");
    println!("  group id         {}", group.swipe_group_id);
    println!("  parent message   {}", group.parent_message_id);
    println!("  active ordinal   {}", group.active_ordinal);
    println!("  candidate ord    {}", candidate.ordinal);
    println!("  candidate id     {}", candidate.message_id);
    println!("  state            {}", candidate.state);
    Ok(())
}

fn activate_swipe(args: SwipeActivateArgs) -> Result<(), String> {
    let session_id = parse_session_id(&args.session_id)?;
    let mut engine = Phase1bCliEngine::open()?;
    let group = engine.activate_swipe(args)?;
    let transcript = engine
        .engine
        .store()
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    println!("Activated swipe candidate.");
    println!("  group id         {}", group.swipe_group_id);
    println!("  active ordinal   {}", group.active_ordinal);
    println!();
    println!("Updated active transcript");
    print_transcript(&transcript);
    Ok(())
}

fn open_repository() -> Result<SqliteRepository, String> {
    SqliteRepository::from_xdg().map_err(|error| error.to_string())
}

fn map_branch_record(record: BranchRecord) -> ConversationBranchRecord {
    ConversationBranchRecord {
        branch: record.branch,
        forked_from: record.forked_from,
    }
}

fn conversation_message_from_record(
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

fn print_session_details(session: &SessionSummary) {
    println!("Session");
    println!("  id:           {}", session.session_id);
    println!("  name:         {}", session.name);
    println!(
        "  character:    {}",
        session.character_name.as_deref().unwrap_or("—")
    );
    println!("  created:      {}", format_timestamp(session.created_at));
    println!(
        "  last opened:  {}",
        format_timestamp(session.last_opened_at)
    );
    println!("  messages:     {}", session.message_count);
    println!(
        "  db size:      {}",
        session
            .db_size_bytes
            .map(|size| format!("{size} bytes"))
            .unwrap_or_else(|| "unknown".to_owned())
    );
    println!("  tags:         {}", format_tags(&session.tags));
}

fn print_branch_record(record: &BranchRecord, include_description: bool) {
    println!("  branch id       {}", record.branch.branch_id);
    println!("  state           {}", record.branch.state);
    println!("  name            {}", record.branch.name);
    println!("  forked from     {}", record.forked_from);
    println!("  tip message     {}", record.branch.tip_message_id);
    println!(
        "  description     {}",
        if include_description {
            record.branch.description.as_deref().unwrap_or("—")
        } else {
            "—"
        }
    );
}

fn print_branch_record_from_engine(record: &ConversationBranchRecord, include_description: bool) {
    println!("  branch id       {}", record.branch.branch_id);
    println!("  state           {}", record.branch.state);
    println!("  name            {}", record.branch.name);
    println!("  forked from     {}", record.forked_from);
    println!("  tip message     {}", record.branch.tip_message_id);
    println!(
        "  created         {}",
        format_timestamp(record.branch.created_at)
    );
    println!(
        "  description     {}",
        if include_description {
            record.branch.description.as_deref().unwrap_or("—")
        } else {
            "—"
        }
    );
}

fn print_transcript(messages: &[ConversationMessage]) {
    if messages.is_empty() {
        println!("  no messages yet");
        return;
    }

    for message in messages {
        print_message(message);
        println!();
    }
}

fn print_message(message: &ConversationMessage) {
    println!("  message id      {}", message.message_id);
    println!(
        "  parent          {}",
        message
            .parent_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "root".to_owned())
    );
    println!("  author          {}", message.author_kind);
    println!(
        "  author name     {}",
        message.author_name.as_deref().unwrap_or("—")
    );
    println!("  created         {}", format_timestamp(message.created_at));
    println!(
        "  edited          {}",
        message
            .edited_at
            .map(format_timestamp)
            .unwrap_or_else(|| "—".to_owned())
    );
    println!(
        "  hidden          {}",
        if message.is_hidden { "yes" } else { "no" }
    );
    println!("  content         {}", message.content);
}

fn print_swipe_group_snapshot(snapshot: &SwipeGroupSnapshot) {
    println!("  group id         {}", snapshot.group.swipe_group_id);
    println!("  parent message   {}", snapshot.group.parent_message_id);
    println!(
        "  context parent   {}",
        snapshot
            .group
            .parent_context_message_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "—".to_owned())
    );
    println!("  active ordinal   {}", snapshot.group.active_ordinal);
    if snapshot.candidates.is_empty() {
        println!("  candidates       none");
        return;
    }
    println!("  candidates");
    for candidate in &snapshot.candidates {
        let marker = if candidate.ordinal == snapshot.group.active_ordinal {
            "*"
        } else {
            "-"
        };
        println!(
            "    {marker} [{}] {} ({})",
            candidate.ordinal, candidate.message_id, candidate.state
        );
    }
}

fn print_session_paths(paths: &PersistencePaths, session_id: &SessionId) {
    print_resolved_path("data dir", paths.data_dir());
    print_resolved_path("global db", paths.global_db_path());
    print_resolved_path("sessions dir", paths.sessions_dir());
    print_resolved_path("session dir", paths.session_dir(session_id));
    print_resolved_path("session db", paths.session_db_path(session_id));
    print_resolved_path("config", paths.session_config_path(session_id));
    print_resolved_path("draft", paths.session_draft_path(session_id));
}

fn print_optional_path(label: &str, path: Option<PathBuf>) {
    match path {
        Some(path) => println!("  {label:<13} {}", path.display()),
        None => println!("  {label:<13} unavailable on this machine"),
    }
}

fn print_resolved_path(label: &str, path: impl AsRef<Path>) {
    println!("  {label:<13} {}", path.as_ref().display());
}

fn require_non_empty(label: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }

    Ok(trimmed.to_owned())
}

fn optional_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .filter_map(|tag| {
            let trimmed = tag.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .collect()
}

fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".to_owned()
    } else {
        tags.join(", ")
    }
}

fn format_timestamp(timestamp: i64) -> String {
    format!("{timestamp} ms since Unix epoch")
}

fn now_timestamp_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn generate_message_id() -> Result<MessageId, String> {
    MessageId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

fn generate_branch_id() -> Result<BranchId, String> {
    BranchId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

fn generate_request_id() -> Result<RequestId, String> {
    RequestId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

fn generate_swipe_group_id() -> Result<SwipeGroupId, String> {
    SwipeGroupId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

fn generate_uuid_like() -> String {
    let counter = u128::from(ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let pid = u128::from(std::process::id());
    let mut bytes = (nanos ^ (counter << 64) ^ (pid << 32)).to_be_bytes();

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn parse_session_id(value: &str) -> Result<SessionId, String> {
    SessionId::parse(value.trim()).map_err(|error| error.to_string())
}

fn parse_message_id(value: &str) -> Result<MessageId, String> {
    MessageId::parse(value.trim()).map_err(|error| error.to_string())
}

fn parse_branch_id(value: &str) -> Result<BranchId, String> {
    BranchId::parse(value.trim()).map_err(|error| error.to_string())
}

fn parse_swipe_group_id(value: &str) -> Result<SwipeGroupId, String> {
    SwipeGroupId::parse(value.trim()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-plus-{prefix}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn repo(&self) -> SqliteRepository {
            SqliteRepository::new(PersistencePaths::from_data_dir(self.root.clone()))
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn phase1c_runtime_restores_persisted_draft_on_bootstrap() {
        let sandbox = TestSandbox::new("phase1c-draft");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Draft Session"))
            .unwrap();
        let context = TuiSessionContext::new(session.session_id.clone(), session.name.clone());

        let mut runtime = Phase1cRuntime::open(repo.clone(), session.session_id.clone()).unwrap();
        runtime
            .persist_draft(&context, Some("restored from app runtime"))
            .unwrap();
        runtime.release_lock().unwrap();

        let mut reopened = Phase1cRuntime::open(repo, session.session_id.clone()).unwrap();
        let bootstrap = reopened.bootstrap(&context).unwrap();
        reopened.release_lock().unwrap();

        assert_eq!(
            bootstrap.draft,
            Some(TuiDraftState::restore(ozone_tui::app::DraftCheckpoint::new(
                "restored from app runtime",
                "restored from app runtime".chars().count()
            )))
        );
    }
}
