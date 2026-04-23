use clap::{Args, Parser, Subcommand, ValueEnum};
use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, CommitMessageCommand, ConversationBranch,
        ConversationMessage, CreateBranchCommand, MessageId, RequestId, SwipeCandidate,
        SwipeCandidateState, SwipeGroup, SwipeGroupId,
    },
    paths::{benchmarks_db_path, data_dir, kobold_log_path, preferences_path},
    product::{ProductTier, OZONE_PLUS_DESIGN_DOC_PATH, OZONE_PLUS_DOC_PATH},
};
use ozone_engine::{
    ActivateBranchCommand, ActivateSwipeRequest, ConversationBranchRecord, ConversationEngine,
    ConversationStore, EditMessageCommand, EngineCommand, EngineCommandResult,
    RecordSwipeCandidateRequest, SingleWriterConversationEngine, SwipeGroupSnapshot,
};
use ozone_inference::MemoryConfig;

pub mod config;
mod context_bridge;
pub mod hooks;
mod hybrid_search;
mod index_rebuild;
mod inference_adapter;
mod runtime;
mod session_title;

use hybrid_search::{load_memory_config, HybridSearchService};
use index_rebuild::rebuild_index;
use ozone_persist::{
    AuthorId, BranchRecord, CharacterCard, CreateMessageRequest, CreateNoteMemoryRequest,
    CreateSessionRequest, GarbageCollectionOutcome, GarbageCollectionPlan, GarbageCollectionPolicy,
    GarbageCollectionReason, ImportCharacterCardRequest, MemoryArtifactId, PersistError,
    PersistencePaths, PinMessageMemoryRequest, PinnedMemoryView, Provenance, SessionId,
    SessionSummary, SqliteRepository, TranscriptExport, UpdateSessionRequest,
};
use ozone_tui::{
    run_terminal_session, GenerationPoll, SessionContext as TuiSessionContext, SessionRuntime,
};
use runtime::Phase1dRuntime;
use std::{
    fmt::Write as _,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);
const LAUNCHER_SESSION_NAME: &str = "Launcher Session";

#[derive(Parser)]
#[command(
    name = "ozone-plus",
    version = concat!(env!("CARGO_PKG_VERSION"), "+", env!("OZONE_GIT_HASH")),
    about = "⬡ ozone+ — local-LLM chat shell with persistent memory and sessions",
    long_about = "⬡ ozone+ — a chat-first terminal shell for local LLM conversations with persistent memory across sessions.\n\nFeatures: session management, pinned memories, freeform notes, session and global FTS search, branching and swipes, character card import, transcript and session export, hybrid vector/keyword recall, and streaming inference via the current local-backend runtime path.",
    after_help = "Examples:\n  ozone-plus create \"First Session\"\n  ozone-plus open <session-id>\n  ozone-plus send <session-id> \"Hello there\"\n  ozone-plus transcript <session-id>\n  ozone-plus memory pin <session-id> <message-id>\n  ozone-plus memory note <session-id> \"Remember the observatory key\"\n  ozone-plus search session <session-id> nebula\n  ozone-plus search global nebula\n  ozone-plus index rebuild\n  ozone-plus branch create <session-id> fork --activate\n  ozone-plus swipe add <session-id> <parent-message-id> \"Alternate reply\"\n  ozone-plus swipe list <session-id>\n  ozone-plus swipe activate <session-id> <swipe-group-id> <ordinal>\n  ozone-plus import card ./aster.json\n  ozone-plus export transcript <session-id> --output ./transcript.txt\n  ozone-plus export session <session-id> --output ./session.json"
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
    /// Internal launcher handoff entrypoint; opens a predictable session shell
    #[command(hide = true)]
    Handoff(HandoffArgs),
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
    /// Import data into ozone+
    Import(ImportArgs),
    /// Export persisted ozone+ data
    Export(ExportArgs),
    /// Manage pinned memories and note memories
    Memory(MemoryArgs),
    /// Search within one session or across all sessions
    Search(SearchArgs),
    /// Rebuild the persisted vector index from recallable text sources
    Index(IndexArgs),
    /// Generate and store summaries for a session
    Summarize(SummarizeArgs),
    /// Inspect derived artifact lifecycle metadata
    Lifecycle(LifecycleArgs),
    /// Plan or run garbage collection on derived artifacts
    Gc(GcArgs),
    /// Manage the session events log
    Events(EventsArgs),
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
    /// Print session metadata instead of launching the TUI shell
    #[arg(long)]
    metadata: bool,
    /// Force open even if session is locked (clears stale locks)
    #[arg(long, short = 'f')]
    force: bool,
}

#[derive(Args, Debug, Clone, Copy, Default)]
struct HandoffArgs {
    /// Prefer a dedicated launcher-managed session instead of the freshest session
    #[arg(long, hide = true)]
    launcher_session: bool,
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
struct ImportArgs {
    #[command(subcommand)]
    command: ImportCommand,
}

#[derive(Subcommand)]
enum ImportCommand {
    /// Import a character card JSON file into a new session
    #[command(visible_alias = "character-card")]
    Card(ImportCharacterCardArgs),
}

#[derive(Args)]
struct ImportCharacterCardArgs {
    /// Path to a character card JSON file
    input: PathBuf,
    /// Optional session name override; defaults to the card name
    #[arg(long = "session-name", value_name = "NAME")]
    session_name: Option<String>,
    /// Extra session tag (repeat --tag for multiple values)
    #[arg(long = "tag", short = 't', value_name = "TAG")]
    tags: Vec<String>,
}

#[derive(Args)]
struct ExportArgs {
    #[command(subcommand)]
    command: ExportCommand,
}

#[derive(Subcommand)]
enum ExportCommand {
    /// Export a full session snapshot as JSON
    Session(ExportSessionArgs),
    /// Export a transcript as JSON or plain text
    Transcript(ExportTranscriptArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SessionExportFormat {
    Json,
}

#[derive(Args)]
struct ExportSessionArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Export format (currently JSON only)
    #[arg(long, value_enum, default_value_t = SessionExportFormat::Json)]
    format: SessionExportFormat,
    /// Explicit output path for the exported file
    #[arg(long, short = 'o', value_name = "PATH")]
    output: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum TranscriptExportFormat {
    Json,
    Text,
}

#[derive(Args)]
struct ExportTranscriptArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Optional branch UUID; defaults to the active branch
    #[arg(long = "branch", value_name = "BRANCH_ID")]
    branch_id: Option<String>,
    /// Export format (JSON or plain text)
    #[arg(long, value_enum, default_value_t = TranscriptExportFormat::Text)]
    format: TranscriptExportFormat,
    /// Explicit output path for the exported file
    #[arg(long, short = 'o', value_name = "PATH")]
    output: PathBuf,
}

#[derive(Args)]
struct MemoryArgs {
    #[command(subcommand)]
    command: MemoryCommand,
}

#[derive(Subcommand)]
enum MemoryCommand {
    /// Pin an existing message into hard context
    Pin(MemoryPinArgs),
    /// Create a note memory for the session
    Note(MemoryNoteArgs),
    /// List active and expired pinned memories
    List(SessionArgs),
    /// Remove a pinned memory by artifact ID
    Unpin(MemoryUnpinArgs),
}

#[derive(Args)]
struct MemoryPinArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Message UUID to pin into hard context
    message_id: String,
    /// Optional number of turns before the memory expires
    #[arg(long = "expires-after-turns", value_name = "N")]
    expires_after_turns: Option<u32>,
}

#[derive(Args)]
struct MemoryNoteArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Note text to pin into hard context
    text: String,
    /// Optional number of turns before the note expires
    #[arg(long = "expires-after-turns", value_name = "N")]
    expires_after_turns: Option<u32>,
}

#[derive(Args)]
struct MemoryUnpinArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Memory artifact UUID to remove
    artifact_id: String,
}

#[derive(Args)]
struct SearchArgs {
    #[command(subcommand)]
    command: SearchCommand,
}

#[derive(Args)]
struct IndexArgs {
    #[command(subcommand)]
    command: IndexCommand,
}

#[derive(Subcommand)]
enum SearchCommand {
    /// Search within a single session transcript
    Session(SessionSearchArgs),
    /// Search across all indexed sessions
    Global(GlobalSearchArgs),
}

#[derive(Subcommand)]
enum IndexCommand {
    /// Derive embeddings, persist them, and rebuild the disk-backed vector index
    Rebuild,
}

#[derive(Args)]
struct SummarizeArgs {
    #[command(subcommand)]
    command: SummarizeCommand,
}

#[derive(Subcommand)]
enum SummarizeCommand {
    /// Generate a synopsis for an entire session
    Session {
        /// Session ID to summarize
        session_id: String,
    },
    /// Generate a chunk summary for a message range
    Chunk {
        /// Session ID containing the messages
        session_id: String,
        /// Starting message ID for the range
        start_message_id: String,
        /// Ending message ID for the range
        end_message_id: String,
    },
}

#[derive(Args)]
struct LifecycleArgs {
    #[command(subcommand)]
    command: LifecycleCommand,
}

#[derive(Subcommand)]
enum LifecycleCommand {
    /// List derived artifacts with lifecycle metadata for a session
    Inspect {
        /// Session UUID in 8-4-4-4-12 format (omit to inspect all sessions)
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
    },
    /// Check disk space status for the ozone+ data directory
    DiskStatus,
}

#[derive(Args)]
struct GcArgs {
    #[command(subcommand)]
    command: GcCommand,
}

#[derive(Subcommand)]
enum GcCommand {
    /// Plan (dry-run) garbage collection without deleting anything
    Plan {
        /// Session UUID to scope the plan (omit for all sessions)
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Maximum active embeddings before oldest are purged (default: unlimited)
        #[arg(long, value_name = "N", default_value_t = usize::MAX)]
        max_embeddings: usize,
        /// Purge derived artifacts whose source message/memory no longer exists
        #[arg(long)]
        purge_orphans: bool,
    },
    /// Apply a garbage collection plan (deletes derived artifacts only)
    Run {
        /// Session UUID to scope GC (omit for all sessions)
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Maximum active embeddings before oldest are purged (default: unlimited)
        #[arg(long, value_name = "N", default_value_t = usize::MAX)]
        max_embeddings: usize,
        /// Purge derived artifacts whose source message/memory no longer exists
        #[arg(long)]
        purge_orphans: bool,
        /// Actually apply the plan (omit for dry-run preview)
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Args)]
struct EventsArgs {
    #[command(subcommand)]
    command: EventsCommand,
}

#[derive(Subcommand)]
enum EventsCommand {
    /// Delete old events from the session events log
    Compact {
        /// Session UUID to scope the compact (omit for all sessions)
        #[arg(long, value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Delete events older than N days
        #[arg(long, value_name = "N", default_value_t = 90u64)]
        retention_days: u64,
    },
}

#[derive(Args)]
struct SessionSearchArgs {
    /// Session UUID in 8-4-4-4-12 format
    session_id: String,
    /// Full-text search query
    query: String,
}

#[derive(Args)]
struct GlobalSearchArgs {
    /// Full-text search query
    query: String,
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
    if ozone_core::install::maybe_prompt_for_local_install_update("ozone-plus")
        .map_err(|error| error.to_string())?
    {
        ozone_core::install::relaunch_current_process().map_err(|error| error.to_string())?;
    }

    run_cli(Cli::parse())
}

fn run_cli(cli: Cli) -> Result<(), String> {
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
        Some(Command::Handoff(args)) => handoff_session(args),
        Some(Command::Open(args)) => open_session(args),
        Some(Command::Send(args)) => send_message(args),
        Some(Command::Transcript(args)) => show_transcript(args),
        Some(Command::Edit(args)) => edit_message(args),
        Some(Command::Branch(args)) => handle_branch_command(args.command),
        Some(Command::Swipe(args)) => handle_swipe_command(args.command),
        Some(Command::Import(args)) => handle_import_command(args.command),
        Some(Command::Export(args)) => handle_export_command(args.command),
        Some(Command::Memory(args)) => handle_memory_command(args.command),
        Some(Command::Search(args)) => handle_search_command(args.command),
        Some(Command::Index(args)) => handle_index_command(args.command),
        Some(Command::Summarize(args)) => handle_summarize_command(args.command),
        Some(Command::Lifecycle(args)) => handle_lifecycle_command(args.command),
        Some(Command::Gc(args)) => handle_gc_command(args.command),
        Some(Command::Events(args)) => handle_events_command(args.command),
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

fn print_bootstrap_summary() {
    println!(
        "{} ({}) — {}",
        ProductTier::OzonePlus.display_name(),
        ProductTier::OzonePlus.slug(),
        ProductTier::OzonePlus.status_label()
    );
    println!("⬡ Local-LLM chat shell with persistent memory and sessions.");
    println!(
        "Create sessions, chat with streaming inference, pin memories, search across sessions,"
    );
    println!("branch transcripts, import characters, and export your data.");
    println!();
    println!("Try one of:");
    println!("  ozone-plus create \"First Session\"");
    println!("  ozone-plus send <session-id> \"Hello there\"");
    println!("  ozone-plus transcript <session-id>");
    println!("  ozone-plus branch list <session-id>");
    println!("  ozone-plus swipe list <session-id>");
    println!("  ozone-plus import card ./aster.json");
    println!("  ozone-plus export session <session-id> --output ./session.json");
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
    println!("Run `ozone-plus --help` for the full command reference.");
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
    println!("Run `ozone-plus open <session-id>` to launch the chat TUI.");
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
    println!("Next step");
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
    println!("Tip");
    println!("  Use `ozone-plus send <session-id> \"Hello\"` to bootstrap the active transcript.");

    Ok(())
}

fn handoff_session(args: HandoffArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let candidates = handoff_candidates(&repo, args)?;
    let mut last_lock_error = None;

    for session in candidates {
        match open_session_record(repo.clone(), session) {
            Ok(()) => return Ok(()),
            Err(error) if is_session_locked_error(&error) => {
                last_lock_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    let fallback = create_handoff_session(&repo)?;
    match open_session_record(repo, fallback) {
        Ok(()) => Ok(()),
        Err(error) => match last_lock_error {
            Some(lock_error) if is_session_locked_error(&error) => Err(format!(
                "{lock_error}; also could not open a fresh launcher session: {error}"
            )),
            _ => Err(error),
        },
    }
}

fn open_session(args: OpenArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;

    if args.force {
        eprintln!("Force-clearing session lock for {session_id}...");
        repo.force_clear_session_lock(&session_id)
            .map_err(|error| error.to_string())?;
    }

    let session = repo
        .get_session(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {session_id} was not found"))?;

    if args.metadata {
        return open_session_metadata(repo, &session, &session_id);
    }

    open_session_record(repo, session)
}

fn handoff_candidates(
    repo: &SqliteRepository,
    args: HandoffArgs,
) -> Result<Vec<SessionSummary>, String> {
    if args.launcher_session {
        if let Some(session) = repo
            .list_sessions()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|session| session.name == LAUNCHER_SESSION_NAME)
        {
            return Ok(vec![session]);
        }
        return Ok(vec![create_handoff_session(repo)?]);
    }

    let sessions = repo.list_sessions().map_err(|error| error.to_string())?;
    if !sessions.is_empty() {
        return Ok(sessions);
    }

    Ok(vec![create_handoff_session(repo)?])
}

fn create_handoff_session(repo: &SqliteRepository) -> Result<SessionSummary, String> {
    repo.create_session(CreateSessionRequest::new(LAUNCHER_SESSION_NAME))
        .map_err(|error| error.to_string())
}

fn open_session_record(repo: SqliteRepository, session: SessionSummary) -> Result<(), String> {
    run_session_shell(repo, session.session_id, session.name)
}

fn run_session_shell(
    repo: SqliteRepository,
    session_id: SessionId,
    session_name: String,
) -> Result<(), String> {
    // Initialise the TUI theme from the shared preferences file.
    ozone_tui::theme::set_preset(load_theme_preset());

    let mut runtime = Phase1dRuntime::open(repo.clone(), session_id.clone())?;
    if let Err(error) = repo
        .update_session_metadata(&session_id, UpdateSessionRequest::default())
        .map_err(|error| error.to_string())
    {
        let release_result = runtime.release_lock();
        return match release_result {
            Ok(()) => Err(error),
            Err(release_error) => Err(format!(
                "{error}; also failed to release session lock cleanly: {release_error}"
            )),
        };
    }

    let context = TuiSessionContext::new(session_id.clone(), session_name);
    let session_result =
        run_terminal_session(context, &mut runtime).map_err(|error| error.to_string());
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

fn is_session_locked_error(error: &str) -> bool {
    error.contains("is locked by instance")
}

/// Read `theme_preset` from the shared ozone preferences JSON file and
/// return the corresponding `ThemePreset`.  Falls back to `DarkMint` on any
/// I/O or parse error so the TUI always starts in a valid state.
fn load_theme_preset() -> ozone_tui::ThemePreset {
    let Some(path) = ozone_core::paths::preferences_path() else {
        return ozone_tui::ThemePreset::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return ozone_tui::ThemePreset::default(),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return ozone_tui::ThemePreset::default(),
    };
    value
        .get("theme_preset")
        .and_then(|v| v.as_str())
        .map(ozone_tui::ThemePreset::from_pref_str)
        .unwrap_or_default()
}

/// Minimal snapshot of the ozone preferences that ozone+ reads and writes.
/// Fields unknown to this struct are preserved in the JSON file when saving.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct OzonePlusPrefs {
    #[serde(default = "default_theme_preset_str")]
    pub theme_preset: String,
    #[serde(default)]
    pub side_by_side_monitor: bool,
    #[serde(default)]
    pub show_inspector: bool,
    #[serde(default = "default_timestamp_style_str")]
    pub timestamp_style: String,
    #[serde(default = "default_message_density_str")]
    pub message_density: String,
}

fn default_theme_preset_str() -> String {
    "dark-mint".to_string()
}
fn default_timestamp_style_str() -> String {
    "relative".to_string()
}
fn default_message_density_str() -> String {
    "comfortable".to_string()
}

impl Default for OzonePlusPrefs {
    fn default() -> Self {
        Self {
            theme_preset: default_theme_preset_str(),
            side_by_side_monitor: false,
            show_inspector: false,
            timestamp_style: default_timestamp_style_str(),
            message_density: default_message_density_str(),
        }
    }
}

/// Load the ozone preferences from disk synchronously.  Returns default values
/// on any I/O or parse error so ozone+ always starts in a valid state.
pub(crate) fn load_prefs_sync() -> OzonePlusPrefs {
    let Some(path) = ozone_core::paths::preferences_path() else {
        return OzonePlusPrefs::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => OzonePlusPrefs::default(),
    }
}

/// Persist a changed ozone preference field without discarding other JSON keys.
/// Reads the existing JSON, updates (or inserts) `pref_key`, then writes back.
pub(crate) fn save_prefs_sync(prefs: &OzonePlusPrefs) -> Result<(), String> {
    let Some(path) = ozone_core::paths::preferences_path() else {
        return Ok(());
    };
    // Round-trip through serde_json::Value so we preserve unknown fields.
    let existing_text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut existing: serde_json::Value =
        serde_json::from_str(&existing_text).unwrap_or(serde_json::json!({}));
    // Merge our known fields into the existing object.
    if let Some(obj) = existing.as_object_mut() {
        let new_val = serde_json::to_value(prefs).map_err(|e| e.to_string())?;
        if let Some(new_obj) = new_val.as_object() {
            for (k, v) in new_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&existing).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{text}\n")).map_err(|e| e.to_string())?;
    Ok(())
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
    if !args.author_kind.eq_ignore_ascii_case("user") || args.author_name.is_some() {
        return send_message_legacy(args);
    }

    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let session = repo
        .get_session(&session_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {session_id} was not found"))?;
    let mut runtime = Phase1dRuntime::open(repo.clone(), session_id.clone())?;
    let context = TuiSessionContext::new(session_id.clone(), session.name);

    let send_result = (|| -> Result<(), String> {
        runtime.check_backend_health()?;

        runtime
            .send_draft(&context, &args.content)?
            .ok_or_else(|| "message content must not be empty".to_string())?;

        loop {
            match runtime.poll_generation(&context)? {
                Some(GenerationPoll::Completed(_)) => {
                    let transcript = repo
                        .get_active_branch_transcript(&session_id)
                        .map_err(|error| error.to_string())?;
                    println!("Completed runtime-backed turn.");
                    let start = transcript.len().saturating_sub(2);
                    print_transcript(&transcript[start..]);
                    return Ok(());
                }
                Some(GenerationPoll::Failed(failure)) => {
                    return Err(failure.message);
                }
                Some(GenerationPoll::Pending { .. }) | None => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    })();

    let release_result = runtime.release_lock();
    match (send_result, release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(send_error), Err(release_error)) => Err(format!(
            "{send_error}; also failed to release session lock cleanly: {release_error}"
        )),
    }
}

fn send_message_legacy(args: SendArgs) -> Result<(), String> {
    let mut engine = Phase1bCliEngine::open()?;
    let (message, bootstrapped) = engine.send(args)?;

    println!("Committed engine-backed message without generation.");
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

fn handle_import_command(command: ImportCommand) -> Result<(), String> {
    match command {
        ImportCommand::Card(args) => import_character_card(args),
    }
}

fn import_character_card(args: ImportCharacterCardArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let input_path = require_existing_file(&args.input, "character card JSON")?;
    let contents = read_utf8_file(&input_path, "character card JSON")?;
    let card = CharacterCard::from_json_str(&contents).map_err(|error| error.to_string())?;
    let imported = repo
        .import_character_card(ImportCharacterCardRequest {
            card: card.clone(),
            session_name: optional_value(args.session_name),
            tags: normalize_tags(args.tags),
            provenance: input_path.display().to_string(),
        })
        .map_err(|error| error.to_string())?;

    println!("Imported character card.");
    println!("  card name       {}", card.name);
    println!("  source format   {}", card.source_format);
    println!(
        "  greeting seeded {}",
        if imported.seeded_message_id.is_some() {
            "yes"
        } else {
            "no"
        }
    );
    println!();
    print_session_details(&imported.session);
    println!();
    println!("Paths");
    print_session_paths(repo.paths(), &imported.session.session_id);

    if let Some(branch_id) = imported.seeded_branch_id {
        println!();
        println!("Seeded branch");
        println!("  branch id       {}", branch_id);
    }

    if let Some(message_id) = imported.seeded_message_id {
        println!("  greeting id     {}", message_id);
    }

    Ok(())
}

fn handle_export_command(command: ExportCommand) -> Result<(), String> {
    match command {
        ExportCommand::Session(args) => export_session(args),
        ExportCommand::Transcript(args) => export_transcript(args),
    }
}

fn export_session(args: ExportSessionArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let export = repo
        .export_session(&session_id)
        .map_err(|error| error.to_string())?;
    let contents = match args.format {
        SessionExportFormat::Json => export.to_pretty_json().map_err(|error| error.to_string())?,
    };
    let output_path = write_output_file(&args.output, &contents)?;

    println!("Exported session.");
    println!("  session id      {}", session_id);
    println!("  format          {:?}", args.format);
    println!("  output          {}", output_path.display());
    Ok(())
}

fn export_transcript(args: ExportTranscriptArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let branch_id = args.branch_id.as_deref().map(parse_branch_id).transpose()?;
    let export = repo
        .export_transcript(&session_id, branch_id.as_ref())
        .map_err(|error| error.to_string())?;
    let contents = match args.format {
        TranscriptExportFormat::Json => {
            export.to_pretty_json().map_err(|error| error.to_string())?
        }
        TranscriptExportFormat::Text => render_transcript_text(&export),
    };
    let output_path = write_output_file(&args.output, &contents)?;

    println!("Exported transcript.");
    println!("  session id      {}", session_id);
    println!(
        "  branch id       {}",
        branch_id.map(|id| id.to_string()).unwrap_or_else(|| export
            .branch
            .as_ref()
            .map(|branch| branch.branch_id.clone())
            .unwrap_or_else(|| "active branch unavailable".to_owned()))
    );
    println!("  format          {:?}", args.format);
    println!("  output          {}", output_path.display());
    Ok(())
}

fn handle_memory_command(command: MemoryCommand) -> Result<(), String> {
    match command {
        MemoryCommand::Pin(args) => pin_memory(args),
        MemoryCommand::Note(args) => create_note_memory(args),
        MemoryCommand::List(args) => list_memories(args),
        MemoryCommand::Unpin(args) => unpin_memory(args),
    }
}

fn pin_memory(args: MemoryPinArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let message_id = parse_message_id(&args.message_id)?;
    let memory = repo
        .pin_message_memory(
            &session_id,
            &message_id,
            PinMessageMemoryRequest {
                pinned_by: AuthorId::User,
                expires_after_turns: args.expires_after_turns,
                provenance: Provenance::UserAuthored,
            },
        )
        .map_err(|error| error.to_string())?
        .into_view(
            repo.get_session(&session_id)
                .map_err(|error| error.to_string())?
                .map(|session| session.message_count)
                .ok_or_else(|| format!("session {session_id} was not found"))?,
        );

    println!("Pinned memory.");
    print_pinned_memory_view(&memory);
    Ok(())
}

fn create_note_memory(args: MemoryNoteArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let mut request = CreateNoteMemoryRequest::new(
        require_non_empty("note text", args.text)?,
        AuthorId::User,
        Provenance::UserAuthored,
    );
    request.content.expires_after_turns = args.expires_after_turns;
    let memory = repo
        .create_note_memory(&session_id, request)
        .map_err(|error| error.to_string())?
        .into_view(
            repo.get_session(&session_id)
                .map_err(|error| error.to_string())?
                .map(|session| session.message_count)
                .ok_or_else(|| format!("session {session_id} was not found"))?,
        );

    println!("Created note memory.");
    print_pinned_memory_view(&memory);
    Ok(())
}

fn list_memories(args: SessionArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let memories = repo
        .list_pinned_memories(&session_id)
        .map_err(|error| error.to_string())?;

    println!("Pinned memories");
    println!("  session id      {}", session_id);
    println!(
        "  active          {}",
        memories.iter().filter(|memory| memory.is_active).count()
    );
    println!(
        "  expired         {}",
        memories.iter().filter(|memory| memory.is_expired()).count()
    );

    if memories.is_empty() {
        println!("  none");
        return Ok(());
    }

    for memory in &memories {
        println!();
        print_pinned_memory_view(memory);
    }

    Ok(())
}

fn unpin_memory(args: MemoryUnpinArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let artifact_id = parse_memory_artifact_id(&args.artifact_id)?;
    let removed = repo
        .remove_pinned_memory(&session_id, &artifact_id)
        .map_err(|error| error.to_string())?;

    if !removed {
        return Err(format!(
            "pinned memory {} was not found in session {}",
            artifact_id, session_id
        ));
    }

    println!("Removed pinned memory.");
    println!("  session id      {}", session_id);
    println!("  artifact id     {}", artifact_id);
    Ok(())
}

fn handle_search_command(command: SearchCommand) -> Result<(), String> {
    match command {
        SearchCommand::Session(args) => search_session(args),
        SearchCommand::Global(args) => search_global(args),
    }
}

fn search_session(args: SessionSearchArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&args.session_id)?;
    let query = require_non_empty("query", args.query)?;
    let memory = load_memory_config(&repo, Some(&session_id))?;
    let result = HybridSearchService::new(&repo, &memory).search_session(&session_id, &query)?;

    println!(
        "{}",
        format_search_report("Session search", Some(&session_id), &result, false)
    );

    Ok(())
}

fn search_global(args: GlobalSearchArgs) -> Result<(), String> {
    let repo = open_repository()?;
    let query = require_non_empty("query", args.query)?;
    let memory = load_memory_config(&repo, None)?;
    let result = HybridSearchService::new(&repo, &memory).search_global(&query)?;

    println!(
        "{}",
        format_search_report("Global search", None, &result, true)
    );

    Ok(())
}

fn handle_index_command(command: IndexCommand) -> Result<(), String> {
    match command {
        IndexCommand::Rebuild => rebuild_vector_index(),
    }
}

fn rebuild_vector_index() -> Result<(), String> {
    let repo = open_repository()?;
    let result = rebuild_index(&repo)?;

    println!("Vector index rebuilt.");
    println!("  sessions        {}", result.session_count);
    println!("  sources         {}", result.source_count());
    println!("  message sources {}", result.message_source_count);
    println!("  memory sources  {}", result.memory_source_count);
    println!("  artifacts       {}", result.persisted_artifact_count);
    println!("  provider        {}", result.provider.provider);
    println!("  model           {}", result.provider.model);
    println!("  dimensions      {}", result.provider.dimensions);
    println!("  index path      {}", result.index_path().display());
    println!("  metadata path   {}", result.metadata_path().display());
    Ok(())
}

fn handle_summarize_command(command: SummarizeCommand) -> Result<(), String> {
    match command {
        SummarizeCommand::Session { session_id } => summarize_session(session_id),
        SummarizeCommand::Chunk {
            session_id,
            start_message_id,
            end_message_id,
        } => summarize_chunk(session_id, start_message_id, end_message_id),
    }
}

fn summarize_session(session_id_raw: String) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&session_id_raw)?;
    let messages = repo
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    let turns: Vec<ozone_memory::summary::SummaryInputTurn> = messages
        .iter()
        .map(|msg| ozone_memory::summary::SummaryInputTurn {
            role: msg.author_kind.clone(),
            content: msg.content.clone(),
        })
        .collect();

    let config = ozone_memory::summary::SummaryConfig::default();
    match ozone_memory::summary::generate_session_synopsis(&turns, &config) {
        Some(synopsis) => {
            println!("Session synopsis");
            println!("  session         {session_id}");
            println!("  messages        {}", messages.len());
            println!();
            println!("{synopsis}");
            match repo.store_session_synopsis(&session_id, &synopsis, messages.len(), 0) {
                Ok(record) => println!("  stored as       {}", record.artifact_id),
                Err(err) => eprintln!("  warning: failed to persist synopsis: {err}"),
            }
        }
        None => {
            println!(
                "Not enough content to generate a synopsis ({} messages, minimum {}).",
                messages.len(),
                config.synopsis_min_messages
            );
        }
    }

    Ok(())
}

fn summarize_chunk(
    session_id_raw: String,
    start_message_id_raw: String,
    end_message_id_raw: String,
) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = parse_session_id(&session_id_raw)?;
    let start_id = parse_message_id(&start_message_id_raw)?;
    let end_id = parse_message_id(&end_message_id_raw)?;
    let messages = repo
        .get_active_branch_transcript(&session_id)
        .map_err(|error| error.to_string())?;

    let start_idx = messages
        .iter()
        .position(|m| m.message_id == start_id)
        .ok_or_else(|| format!("start message {start_id} not found in active branch transcript"))?;
    let end_idx = messages
        .iter()
        .position(|m| m.message_id == end_id)
        .ok_or_else(|| format!("end message {end_id} not found in active branch transcript"))?;

    if end_idx < start_idx {
        return Err("end message must come after start message in the transcript".to_owned());
    }

    let chunk = &messages[start_idx..=end_idx];
    let turns: Vec<ozone_memory::summary::SummaryInputTurn> = chunk
        .iter()
        .map(|msg| ozone_memory::summary::SummaryInputTurn {
            role: msg.author_kind.clone(),
            content: msg.content.clone(),
        })
        .collect();

    let config = ozone_memory::summary::SummaryConfig::default();
    match ozone_memory::summary::generate_chunk_summary(&turns, &config) {
        Some(summary) => {
            println!("Chunk summary");
            println!("  session         {session_id}");
            println!("  range           {start_id} → {end_id}");
            println!("  messages        {}", chunk.len());
            println!();
            println!("{summary}");
            match repo.store_chunk_summary(
                &session_id,
                &summary,
                chunk.len(),
                &start_id,
                &end_id,
                0,
            ) {
                Ok(record) => println!("  stored as       {}", record.artifact_id),
                Err(err) => eprintln!("  warning: failed to persist chunk summary: {err}"),
            }
        }
        None => {
            println!(
                "Not enough content to generate a chunk summary ({} messages in range).",
                chunk.len()
            );
        }
    }

    Ok(())
}

fn handle_lifecycle_command(command: LifecycleCommand) -> Result<(), String> {
    match command {
        LifecycleCommand::Inspect { session_id } => lifecycle_inspect(session_id),
        LifecycleCommand::DiskStatus => lifecycle_disk_status(),
    }
}

fn lifecycle_inspect(session_id_raw: Option<String>) -> Result<(), String> {
    let repo = open_repository()?;
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let config = load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let policy = ozone_memory::StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let records = repo
        .inspect_derived_artifacts(
            session_id.as_ref(),
            &policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
        )
        .map_err(|error| error.to_string())?;

    if records.is_empty() {
        println!("No derived artifacts found.");
        return Ok(());
    }

    println!("Derived artifacts  ({} total)", records.len());
    println!();
    for record in &records {
        println!(
            "  {}  {}  {}",
            record.artifact_id, record.kind, record.session_id
        );
        println!("    tier       {}", record.storage_tier);
        println!(
            "    stale      {}",
            if record.staleness.is_stale {
                "yes ⚠"
            } else {
                "no"
            }
        );
        println!(
            "    age        {} messages  {} hours",
            record.age_messages, record.staleness.age_hours
        );
        println!(
            "    source     {}",
            if record.source_exists {
                "present"
            } else {
                "missing ⚠"
            }
        );
        println!("    created    {}", record.created_at);
        println!();
    }
    Ok(())
}

fn lifecycle_disk_status() -> Result<(), String> {
    let repo = open_repository()?;
    let data_dir = repo.paths().data_dir();
    let policy = ozone_memory::DiskMonitorPolicy::default();
    match ozone_memory::check_disk_space(data_dir, &policy) {
        Some(result) => {
            println!("Disk status");
            println!("  path            {}", data_dir.display());
            println!(
                "  free            {} MiB",
                result.free_bytes / (1024 * 1024)
            );
            println!("  status          {}", result.status);
            if result.status.should_pause_background_jobs() {
                println!("  ⚠ emergency: background artifact jobs should be paused");
            }
        }
        None => println!("Disk space check not available on this platform."),
    }
    Ok(())
}

fn handle_gc_command(command: GcCommand) -> Result<(), String> {
    match command {
        GcCommand::Plan {
            session_id,
            max_embeddings,
            purge_orphans,
        } => gc_plan(session_id, max_embeddings, purge_orphans),
        GcCommand::Run {
            session_id,
            max_embeddings,
            purge_orphans,
            apply,
        } => gc_run(session_id, max_embeddings, purge_orphans, apply),
    }
}

fn build_gc_policy_and_session(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
) -> Result<(Option<SessionId>, GarbageCollectionPolicy), String> {
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let policy = GarbageCollectionPolicy::new(max_embeddings, purge_orphans);
    Ok((session_id, policy))
}

fn gc_plan(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
) -> Result<(), String> {
    let (session_id, policy) =
        build_gc_policy_and_session(session_id_raw, max_embeddings, purge_orphans)?;
    let repo = open_repository()?;
    let config = load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let storage_policy = ozone_memory::StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let plan = repo
        .plan_garbage_collection(
            session_id.as_ref(),
            &storage_policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
            &policy,
        )
        .map_err(|error| error.to_string())?;
    print_gc_plan(&plan);
    Ok(())
}

fn gc_run(
    session_id_raw: Option<String>,
    max_embeddings: usize,
    purge_orphans: bool,
    apply: bool,
) -> Result<(), String> {
    let (session_id, policy) =
        build_gc_policy_and_session(session_id_raw, max_embeddings, purge_orphans)?;
    let repo = open_repository()?;
    let config = load_memory_config(&repo, session_id.as_ref()).unwrap_or_default();
    let storage_policy = ozone_memory::StorageTierPolicy::new(
        config.lifecycle.storage_tiers.reduced_after_messages as u64,
        config.lifecycle.storage_tiers.minimal_after_messages as u64,
    );
    let plan = repo
        .plan_garbage_collection(
            session_id.as_ref(),
            &storage_policy,
            config.lifecycle.stale_artifacts.max_age_messages,
            config.lifecycle.stale_artifacts.max_age_hours,
            &policy,
        )
        .map_err(|error| error.to_string())?;

    print_gc_plan(&plan);

    if !apply {
        println!();
        println!("Dry-run mode — no artifacts deleted. Pass --apply to commit.");
        return Ok(());
    }

    if plan.candidate_count == 0 {
        println!();
        println!("Nothing to delete.");
        return Ok(());
    }

    let outcome = repo
        .apply_garbage_collection_plan(&plan)
        .map_err(|error| error.to_string())?;
    print_gc_outcome(&outcome);
    Ok(())
}

fn print_gc_plan(plan: &GarbageCollectionPlan) {
    println!("GC plan");
    println!("  inspected       {}", plan.inspected_count);
    println!("  candidates      {}", plan.candidate_count);
    if !plan.reason_counts.is_empty() {
        println!("  reasons:");
        for (reason, count) in &plan.reason_counts {
            println!("    {:<28} {count}", reason_label(*reason));
        }
    }
    if !plan.candidates.is_empty() {
        println!();
        println!("Candidates:");
        for candidate in &plan.candidates {
            let reasons: Vec<&str> = candidate.reasons.iter().map(|r| r.as_str()).collect();
            println!(
                "  {}  {}  {} — {}",
                candidate.artifact.artifact_id,
                candidate.artifact.kind,
                candidate.artifact.session_id,
                reasons.join(", ")
            );
        }
    }
}

fn print_gc_outcome(outcome: &GarbageCollectionOutcome) {
    println!();
    println!("GC applied");
    println!("  deleted         {}", outcome.deleted_count);
    for (session_id, ids) in &outcome.deleted_artifact_ids {
        println!("  session {session_id}  {} artifact(s)", ids.len());
    }
}

fn reason_label(reason: GarbageCollectionReason) -> &'static str {
    match reason {
        GarbageCollectionReason::OrphanedSource => "orphaned_source",
        GarbageCollectionReason::MinimalTier => "minimal_tier",
        GarbageCollectionReason::SupersededSynopsis => "superseded_synopsis",
        GarbageCollectionReason::OverEmbeddingLimit => "over_embedding_limit",
    }
}

fn open_repository() -> Result<SqliteRepository, String> {
    SqliteRepository::from_xdg().map_err(|error| error.to_string())
}

fn handle_events_command(command: EventsCommand) -> Result<(), String> {
    match command {
        EventsCommand::Compact {
            session_id,
            retention_days,
        } => events_compact(session_id, retention_days),
    }
}

fn events_compact(session_id_raw: Option<String>, retention_days: u64) -> Result<(), String> {
    let session_id = session_id_raw
        .as_deref()
        .map(parse_session_id)
        .transpose()?;
    let now_ms = u64::try_from(now_timestamp_ms()).unwrap_or(0);
    let older_than_ms = now_ms.saturating_sub(retention_days * 24 * 3600 * 1000);
    let repo = open_repository()?;
    let count = repo
        .compact_events(session_id.as_ref(), older_than_ms)
        .map_err(|e| e.to_string())?;
    println!("Events compacted");
    println!("  deleted  {count}");
    println!("  older than  {retention_days} days");
    Ok(())
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

fn print_pinned_memory_view(memory: &PinnedMemoryView) {
    println!("  artifact id     {}", memory.record.artifact_id);
    println!(
        "  state           {}",
        if memory.is_active {
            "active"
        } else {
            "expired"
        }
    );
    println!("  provenance      {}", memory.record.provenance);
    println!(
        "  pinned by       {}",
        format_author_id(&memory.record.content.pinned_by)
    );
    println!(
        "  source message  {}",
        memory
            .record
            .source_message_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "note".to_owned())
    );
    println!(
        "  created         {}",
        format_timestamp(memory.record.created_at)
    );
    println!("  turns elapsed   {}", memory.turns_elapsed);
    println!(
        "  remaining turns {}",
        memory
            .remaining_turns
            .map(|remaining| remaining.to_string())
            .unwrap_or_else(|| "∞".to_owned())
    );
    println!("  content         {}", memory.record.content.text.as_str());
}

fn format_search_report(
    title: &str,
    session_id: Option<&SessionId>,
    result: &ozone_memory::RetrievalResultSet,
    include_session_details: bool,
) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "{title}");
    if let Some(session_id) = session_id {
        let _ = writeln!(output, "  session id      {}", session_id);
    }
    let _ = writeln!(output, "  query           {}", result.query);
    let _ = writeln!(output, "  mode            {}", result.status.mode);
    let _ = writeln!(
        output,
        "  status          {}",
        format_search_status(&result.status)
    );
    let _ = writeln!(output, "  hits            {}", result.hits.len());
    if result.hits.is_empty() {
        let _ = writeln!(output, "  none");
        return output.trim_end().to_owned();
    }

    for hit in &result.hits {
        let _ = writeln!(output);
        output.push_str(&format_search_hit(hit, include_session_details));
    }

    output.trim_end().to_owned()
}

fn format_search_status(status: &ozone_memory::RetrievalStatus) -> String {
    let mut details = Vec::new();
    if let Some(reason) = status.reason.as_ref() {
        details.push(reason.clone());
    }
    if status.filtered_stale_embeddings > 0 {
        details.push(format!(
            "filtered {} stale embedding{}",
            status.filtered_stale_embeddings,
            if status.filtered_stale_embeddings == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    if status.downranked_embeddings > 0 {
        details.push(format!(
            "downranked {} inactive hit{}",
            status.downranked_embeddings,
            if status.downranked_embeddings == 1 {
                ""
            } else {
                "s"
            }
        ));
    }

    if details.is_empty() {
        "ok".to_owned()
    } else {
        details.join(" · ")
    }
}

fn format_search_hit(hit: &ozone_memory::RetrievalHit, include_session_details: bool) -> String {
    let mut output = String::new();
    if include_session_details {
        let _ = writeln!(output, "  session id      {}", hit.session.session_id);
        let _ = writeln!(output, "  session name    {}", hit.session.session_name);
        let _ = writeln!(
            output,
            "  character       {}",
            hit.session.character_name.as_deref().unwrap_or("—")
        );
        let _ = writeln!(
            output,
            "  tags            {}",
            format_tags(&hit.session.tags)
        );
    }
    let _ = writeln!(output, "  hit             {}", hit.hit_kind);
    let target = hit
        .message_id
        .as_ref()
        .map(|message_id| format!("message {}", message_id))
        .or_else(|| {
            hit.artifact_id
                .as_ref()
                .map(|artifact_id| format!("artifact {}", artifact_id))
        })
        .unwrap_or_else(|| "source unknown".to_owned());
    let _ = writeln!(output, "  target          {}", target);
    if let Some(source_message_id) = hit.source_message_id.as_ref() {
        let _ = writeln!(output, "  source          message {}", source_message_id);
    }
    if let Some(author_kind) = hit.author_kind.as_ref() {
        let _ = writeln!(output, "  author          {}", author_kind);
    }
    let _ = writeln!(output, "  provenance      {}", hit.provenance);
    let _ = writeln!(output, "  state           {}", hit.source_state);
    let _ = writeln!(
        output,
        "  created         {}",
        format_timestamp(hit.created_at)
    );
    let _ = writeln!(output, "  score           {:.3}", hit.overall_score());
    let _ = writeln!(
        output,
        "  text/vector     text {:.3} raw {:.3} bm25 {} · vector {:.3} sim {}",
        hit.score.text_contribution,
        hit.score.text_score,
        hit.score
            .bm25_score
            .map(|score| format!("{score:.3}"))
            .unwrap_or_else(|| "—".to_owned()),
        hit.score.vector_contribution,
        hit.score
            .vector_similarity
            .map(|score| format!("{score:.3}"))
            .unwrap_or_else(|| "—".to_owned()),
    );
    let _ = writeln!(
        output,
        "  ranking         provenance {:.3} (score {:.2}, weight {:.2}) · recency {:.3} · importance {:.3} · stale {:.2}",
        hit.score.provenance_contribution,
        hit.score.provenance_score,
        hit.score.provenance_config_weight,
        hit.score.recency_contribution,
        hit.score.importance_contribution,
        hit.score.stale_penalty,
    );
    if let Some(lifecycle) = hit.lifecycle.as_ref() {
        let _ = writeln!(
            output,
            "  lifecycle       {}",
            lifecycle_detail_line(lifecycle)
        );
    }
    let _ = writeln!(output, "  content         {}", hit.text.replace('\n', " "));
    output
}

pub(crate) fn artifact_lifecycle_summary(
    memory: &MemoryConfig,
    snapshot_version: u64,
    created_at: i64,
    current_message_count: u64,
    provenance: Provenance,
) -> ozone_memory::ArtifactLifecycleSummary {
    let storage_tiers = ozone_memory::StorageTierPolicy::new(
        u64::try_from(memory.lifecycle.storage_tiers.reduced_after_messages).unwrap_or(u64::MAX),
        u64::try_from(memory.lifecycle.storage_tiers.minimal_after_messages).unwrap_or(u64::MAX),
    );
    let staleness = ozone_memory::assess_artifact_staleness(
        snapshot_version,
        current_message_count,
        created_at,
        now_timestamp_ms(),
        memory.lifecycle.stale_artifacts.max_age_messages,
        memory.lifecycle.stale_artifacts.max_age_hours,
    );

    ozone_memory::ArtifactLifecycleSummary {
        storage_tier: ozone_memory::storage_tier_for_age(staleness.age_messages, &storage_tiers),
        age_messages: staleness.age_messages,
        age_hours: staleness.age_hours,
        is_stale: staleness.is_stale,
        adjusted_provenance_score: ozone_memory::adjusted_provenance_weight(
            memory.provenance_weights.weight_for(provenance),
            provenance,
            u32::try_from(staleness.age_messages).unwrap_or(u32::MAX),
        )
        .clamp(0.0, 1.0),
    }
}

pub(crate) fn pinned_memory_lifecycle_summary(
    memory: &MemoryConfig,
    pinned_memory: &PinnedMemoryView,
) -> ozone_memory::ArtifactLifecycleSummary {
    artifact_lifecycle_summary(
        memory,
        pinned_memory.record.snapshot_version,
        pinned_memory.record.created_at,
        pinned_memory
            .record
            .snapshot_version
            .saturating_add(pinned_memory.turns_elapsed),
        pinned_memory.record.provenance,
    )
}

pub(crate) fn lifecycle_badges(
    lifecycle: &ozone_memory::ArtifactLifecycleSummary,
    include_full_tier: bool,
    include_provenance: bool,
) -> Vec<String> {
    let mut badges = Vec::new();
    if include_full_tier
        || lifecycle.storage_tier != ozone_memory::StorageTier::Full
        || lifecycle.is_stale
    {
        badges.push(format!("tier {}", lifecycle.storage_tier));
    }
    if lifecycle.is_stale {
        badges.push("⚠ stale".to_owned());
    }
    if include_provenance {
        badges.push(format!("prov {:.2}", lifecycle.adjusted_provenance_score));
    }
    badges
}

pub(crate) fn lifecycle_detail_line(lifecycle: &ozone_memory::ArtifactLifecycleSummary) -> String {
    let mut parts = lifecycle_badges(lifecycle, true, true);
    parts.push(format!(
        "age {} msg/{}h",
        lifecycle.age_messages, lifecycle.age_hours
    ));
    parts.join(" · ")
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

fn require_existing_file(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to read {label} at {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} at {} is not a file", path.display()));
    }

    Ok(path.to_path_buf())
}

fn read_utf8_file(path: &Path, label: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read {label} at {}: {error}", path.display()))
}

fn write_output_file(path: &Path, contents: &str) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("output path must not be empty".to_owned());
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("output path {} already exists", path.display())
            } else {
                format!("failed to create output file {}: {error}", path.display())
            }
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("failed to write output file {}: {error}", path.display()))?;

    Ok(path.to_path_buf())
}

fn render_transcript_text(export: &TranscriptExport) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "# ozone+ transcript export");
    let _ = writeln!(output, "format: {}", export.format);
    let _ = writeln!(
        output,
        "exported_at: {}",
        format_timestamp(export.exported_at)
    );
    let _ = writeln!(output, "session_id: {}", export.session.session_id);
    let _ = writeln!(output, "session_name: {}", export.session.name);
    let _ = writeln!(
        output,
        "character_name: {}",
        export.session.character_name.as_deref().unwrap_or("—")
    );
    match export.branch.as_ref() {
        Some(branch) => {
            let _ = writeln!(output, "branch_id: {}", branch.branch_id);
            let _ = writeln!(output, "branch_name: {}", branch.name);
            let _ = writeln!(output, "branch_state: {}", branch.state);
            let _ = writeln!(output, "branch_tip_message_id: {}", branch.tip_message_id);
            let _ = writeln!(
                output,
                "branch_forked_from_message_id: {}",
                branch.forked_from_message_id
            );
        }
        None => {
            let _ = writeln!(output, "branch_id: —");
            let _ = writeln!(output, "branch_name: —");
            let _ = writeln!(output, "branch_state: —");
            let _ = writeln!(output, "branch_tip_message_id: —");
            let _ = writeln!(output, "branch_forked_from_message_id: —");
        }
    }
    let _ = writeln!(output, "message_count: {}", export.messages.len());
    let _ = writeln!(output);

    if export.messages.is_empty() {
        let _ = writeln!(output, "No transcript messages.");
        return output;
    }

    for (index, message) in export.messages.iter().enumerate() {
        let _ = writeln!(output, "## Message {}", index + 1);
        let _ = writeln!(output, "message_id: {}", message.message_id);
        let _ = writeln!(
            output,
            "parent_id: {}",
            message.parent_id.as_deref().unwrap_or("root")
        );
        let _ = writeln!(output, "author_kind: {}", message.author_kind);
        let _ = writeln!(
            output,
            "author_name: {}",
            message.author_name.as_deref().unwrap_or("—")
        );
        let _ = writeln!(
            output,
            "created_at: {}",
            format_timestamp(message.created_at)
        );
        let _ = writeln!(
            output,
            "edited_at: {}",
            message
                .edited_at
                .map(format_timestamp)
                .unwrap_or_else(|| "—".to_owned())
        );
        let _ = writeln!(
            output,
            "hidden: {}",
            if message.is_hidden { "yes" } else { "no" }
        );
        let _ = writeln!(output, "content:");
        if message.content.is_empty() {
            let _ = writeln!(output, "  ");
        } else {
            for line in message.content.lines() {
                let _ = writeln!(output, "  {line}");
            }
        }
        let _ = writeln!(output);
    }

    output
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
    use chrono::{Local, TimeZone, Utc};
    let secs = timestamp / 1000;
    let Some(dt) = Utc.timestamp_opt(secs, 0).single() else {
        return format!("{timestamp} ms");
    };
    let local = dt.with_timezone(&Local);
    let formatted = local.format("%Y-%m-%d %H:%M").to_string();

    let now = Utc::now();
    let diff = now.signed_duration_since(dt);
    let ago = if diff.num_seconds() < 60 {
        "just now".to_owned()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 30 {
        format!("{}d ago", diff.num_days())
    } else {
        format!("{}mo ago", diff.num_days() / 30)
    };
    format!("{formatted} ({ago})")
}

/// Compact timestamp for the session list — fits in ~12 chars.
fn format_timestamp_short(timestamp: i64) -> String {
    use chrono::{Datelike, Local, TimeZone, Utc};
    let secs = timestamp / 1000;
    let Some(dt) = Utc.timestamp_opt(secs, 0).single() else {
        return "—".to_owned();
    };
    let local = dt.with_timezone(&Local);
    let now_local = Utc::now().with_timezone(&Local);
    let diff = now_local.signed_duration_since(local);

    if diff.num_seconds() < 60 {
        "just now".to_owned()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 && local.date_naive() == now_local.date_naive() {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        local.format("%a %H:%M").to_string()
    } else if local.year() == now_local.year() {
        local.format("%b %d").to_string()
    } else {
        local.format("%Y-%m-%d").to_string()
    }
}

/// Time-only timestamp for inline message display, e.g. "2:15 PM".
fn format_message_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone, Utc};
    let secs = timestamp / 1000;
    let Some(dt) = Utc.timestamp_opt(secs, 0).single() else {
        return String::new();
    };
    let local = dt.with_timezone(&Local);
    local.format("%-I:%M %p").to_string()
}

fn format_author_id(author: &AuthorId) -> String {
    match author {
        AuthorId::User => "user".to_owned(),
        AuthorId::Character(name) => format!("character:{name}"),
        AuthorId::System => "system".to_owned(),
        AuthorId::Narrator => "narrator".to_owned(),
    }
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

fn parse_memory_artifact_id(value: &str) -> Result<MemoryArtifactId, String> {
    MemoryArtifactId::parse(value.trim()).map_err(|error| error.to_string())
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
    use clap::Parser;
    use ozone_memory::VectorIndexManager;
    use ozone_tui::DraftState as TuiDraftState;
    use ozone_tui::SessionRuntime;
    use std::{
        fs,
        path::Path,
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex,
        },
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ozone-plus-tests")
                .join(format!(
                    "{prefix}-{}-{}",
                    std::process::id(),
                    TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
                ));
            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn repo(&self) -> SqliteRepository {
            SqliteRepository::new(PersistencePaths::from_data_dir(self.root.clone()))
        }

        fn xdg_data_home(&self) -> PathBuf {
            self.root.join("xdg-data")
        }

        fn global_config_path(&self) -> PathBuf {
            self.root
                .join("home")
                .join(".config")
                .join("ozone")
                .join("config.toml")
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    struct ScopedEnvVar {
        key: &'static str,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &'static str, value: impl AsRef<Path>) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value.as_ref());
            Self { key, previous }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn write_global_config(sandbox: &TestSandbox, contents: &str) {
        let path = sandbox.global_config_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn formatted_search_report_surfaces_mode_and_breakdown() {
        let result = ozone_memory::RetrievalResultSet {
            query: "observatory key".into(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 1,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Observatory".into(),
                    character_name: Some("Aster".into()),
                    tags: vec!["phase2b".into()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(MessageId::parse("223e4567-e89b-12d3-a456-426614174000").unwrap()),
                source_message_id: None,
                author_kind: Some("assistant".into()),
                text: "The key rests under the blue lamp.".into(),
                created_at: 1_700_000_000_000,
                provenance: Provenance::UtilityModel,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                    storage_tier: ozone_memory::StorageTier::Minimal,
                    age_messages: 1_024,
                    age_hours: 169,
                    is_stale: true,
                    adjusted_provenance_score: 0.61,
                }),
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: Some(-1.1),
                    text_score: 0.8,
                    vector_similarity: Some(0.9),
                    importance_score: 0.45,
                    recency_score: 0.7,
                    provenance: Provenance::UtilityModel,
                    stale_penalty: 1.0,
                }
                .score(
                    &ozone_memory::RetrievalWeights::default(),
                    &ozone_memory::ProvenanceWeights::default(),
                ),
            }],
        };

        let report = format_search_report("Session search", None, &result, true);
        assert!(report.contains("mode            hybrid"));
        assert!(report.contains("status          filtered 1 stale embedding"));
        assert!(report.contains("text/vector     text"));
        assert!(report.contains("ranking         provenance"));
        assert!(report.contains("lifecycle       tier minimal · ⚠ stale · prov 0.61"));
        assert!(report.contains("session name    Observatory"));
    }

    #[test]
    fn phase1d_runtime_restores_persisted_draft_on_bootstrap() {
        let sandbox = TestSandbox::new("phase1d-draft");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Draft Session"))
            .unwrap();
        let context = TuiSessionContext::new(session.session_id.clone(), session.name.clone());

        let mut runtime = Phase1dRuntime::open(repo.clone(), session.session_id.clone()).unwrap();
        runtime
            .persist_draft(&context, Some("restored from app runtime"))
            .unwrap();
        runtime.release_lock().unwrap();

        let mut reopened = Phase1dRuntime::open(repo, session.session_id.clone()).unwrap();
        let bootstrap = reopened.bootstrap(&context).unwrap();
        reopened.release_lock().unwrap();

        assert_eq!(
            bootstrap.draft,
            Some(TuiDraftState::restore(
                ozone_tui::app::DraftCheckpoint::new(
                    "restored from app runtime",
                    "restored from app runtime".chars().count()
                )
            ))
        );
    }

    #[test]
    fn import_and_export_commands_use_xdg_paths() {
        let _env_guard = ENV_LOCK.lock().unwrap();
        let sandbox = TestSandbox::new("import-export-smoke");
        fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let card_path = sandbox.root.join("fixtures").join("aster.json");
        fs::create_dir_all(card_path.parent().unwrap()).unwrap();
        fs::write(
            &card_path,
            r#"{
                "name": "Aster",
                "description": "A patient observatory guide.",
                "first_mes": "Welcome back to the observatory."
            }"#,
        )
        .unwrap();

        import_character_card(ImportCharacterCardArgs {
            input: card_path.clone(),
            session_name: Some("Smoke Session".to_owned()),
            tags: vec!["smoke".to_owned()],
        })
        .unwrap();

        let repo = open_repository().unwrap();
        let sessions = repo.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let session = sessions[0].clone();
        assert_eq!(session.name, "Smoke Session");
        assert_eq!(session.character_name.as_deref(), Some("Aster"));

        let transcript_path = sandbox.root.join("exports").join("transcript.txt");
        export_transcript(ExportTranscriptArgs {
            session_id: session.session_id.to_string(),
            branch_id: None,
            format: TranscriptExportFormat::Text,
            output: transcript_path.clone(),
        })
        .unwrap();

        let session_path = sandbox.root.join("exports").join("session.json");
        export_session(ExportSessionArgs {
            session_id: session.session_id.to_string(),
            format: SessionExportFormat::Json,
            output: session_path.clone(),
        })
        .unwrap();

        let transcript_text = fs::read_to_string(&transcript_path).unwrap();
        assert!(transcript_text.contains("ozone+ transcript export"));
        assert!(transcript_text.contains("Welcome back to the observatory."));

        let session_json = fs::read_to_string(&session_path).unwrap();
        assert!(session_json.contains("\"format\": \"ozone-plus.session-export.v1\""));
        assert!(session_json.contains("\"name\": \"Smoke Session\""));
    }

    #[test]
    fn handoff_candidates_create_launcher_session_when_empty() {
        let sandbox = TestSandbox::new("handoff-empty");
        let repo = sandbox.repo();

        let candidates = handoff_candidates(&repo, HandoffArgs::default()).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Launcher Session");
        let sessions = repo.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "Launcher Session");
    }

    #[test]
    fn handoff_candidates_reuse_existing_sessions() {
        let sandbox = TestSandbox::new("handoff-existing");
        let repo = sandbox.repo();
        let existing = repo
            .create_session(CreateSessionRequest::new("Existing Session"))
            .unwrap();

        let candidates = handoff_candidates(&repo, HandoffArgs::default()).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].session_id, existing.session_id);
        assert_eq!(candidates[0].name, "Existing Session");
    }

    #[test]
    fn swipe_activation_does_not_retip_unrelated_active_branch() {
        let sandbox = TestSandbox::new("swipe-branch-activation");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Swipe Branch Session"))
            .unwrap();

        let user_record = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("hello from user".to_owned()),
            )
            .unwrap();
        let user_message_id = MessageId::parse(user_record.message_id.clone()).unwrap();

        let main_branch_id = generate_branch_id().unwrap();
        let mut main_branch = ConversationBranch::new(
            main_branch_id.clone(),
            session.session_id.clone(),
            "main",
            user_message_id.clone(),
            user_record.created_at,
        );
        main_branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch: main_branch,
            forked_from: user_message_id.clone(),
        })
        .unwrap();

        let mut assistant_message = ConversationMessage::new(
            session.session_id.clone(),
            generate_message_id().unwrap(),
            "assistant",
            "assistant reply".to_owned(),
            now_timestamp_ms(),
        );
        assistant_message.parent_id = Some(user_message_id.clone());
        let assistant_message = repo
            .commit_message(CommitMessageCommand {
                branch_id: main_branch_id.clone(),
                message: assistant_message,
            })
            .unwrap();

        let fork_branch_id = generate_branch_id().unwrap();
        let mut fork_branch = ConversationBranch::new(
            fork_branch_id.clone(),
            session.session_id.clone(),
            "deep-fork",
            assistant_message.message_id.clone(),
            now_timestamp_ms(),
        );
        fork_branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch: fork_branch,
            forked_from: assistant_message.message_id.clone(),
        })
        .unwrap();

        let mut store = RepoConversationStore::new(repo.clone());
        let (group, candidate) = store
            .create_swipe_candidate(ManualSwipeCandidateRequest {
                session_id: session.session_id.clone(),
                parent_message_id: user_message_id.clone(),
                parent_context_message_id: None,
                swipe_group_id: Some(generate_swipe_group_id().unwrap()),
                ordinal: Some(0),
                author_kind: "assistant".to_owned(),
                author_name: None,
                content: "alternate reply".to_owned(),
                state: SwipeCandidateState::Active,
            })
            .unwrap();

        let activated = store
            .activate_swipe_candidate(ActivateSwipeRequest {
                session_id: session.session_id.clone(),
                command: ActivateSwipeCommand {
                    swipe_group_id: group.swipe_group_id.clone(),
                    ordinal: candidate.ordinal,
                },
            })
            .unwrap();

        assert_eq!(activated.active_ordinal, candidate.ordinal);

        let active_branch = repo
            .get_active_branch(&session.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(active_branch.branch.branch_id, fork_branch_id);
        assert_eq!(
            active_branch.branch.tip_message_id,
            assistant_message.message_id
        );
    }

    #[test]
    fn handoff_candidates_create_or_reuse_launcher_session_when_requested() {
        let sandbox = TestSandbox::new("handoff-launcher-session");
        let repo = sandbox.repo();
        repo.create_session(CreateSessionRequest::new("Existing Session"))
            .unwrap();

        let candidates = handoff_candidates(
            &repo,
            HandoffArgs {
                launcher_session: true,
            },
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, LAUNCHER_SESSION_NAME);

        let second = handoff_candidates(
            &repo,
            HandoffArgs {
                launcher_session: true,
            },
        )
        .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].session_id, candidates[0].session_id);
    }

    #[test]
    fn memory_and_search_commands_parse() {
        let cli = Cli::try_parse_from([
            "ozone-plus",
            "memory",
            "pin",
            "session-1",
            "message-1",
            "--expires-after-turns",
            "3",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Memory(MemoryArgs {
                command: MemoryCommand::Pin(args),
            })) => {
                assert_eq!(args.session_id, "session-1");
                assert_eq!(args.message_id, "message-1");
                assert_eq!(args.expires_after_turns, Some(3));
            }
            _ => panic!("unexpected cli parse result"),
        }

        let cli = Cli::try_parse_from([
            "ozone-plus",
            "search",
            "session",
            "session-2",
            "observatory key",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Search(SearchArgs {
                command: SearchCommand::Session(args),
            })) => {
                assert_eq!(args.session_id, "session-2");
                assert_eq!(args.query, "observatory key");
            }
            _ => panic!("unexpected cli parse result"),
        }

        let cli = Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Index(IndexArgs {
                command: IndexCommand::Rebuild
            }))
        ));

        let cli = Cli::try_parse_from(["ozone-plus", "handoff", "--launcher-session"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Handoff(HandoffArgs {
                launcher_session: true
            }))
        ));
    }

    #[test]
    fn memory_and_search_commands_execute_against_xdg_repo() {
        let _env_guard = ENV_LOCK.lock().unwrap();
        let sandbox = TestSandbox::new("memory-search-smoke");
        fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));
        let keyword = "observatory-phase2a";

        let repo = open_repository().unwrap();
        let session = repo
            .create_session(CreateSessionRequest::new("Memory Search Session"))
            .unwrap();
        let message = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user(format!("The {keyword} rests under the blue lamp.")),
            )
            .unwrap();

        run_cli(
            Cli::try_parse_from([
                "ozone-plus",
                "memory",
                "pin",
                session.session_id.as_str(),
                &message.message_id,
            ])
            .unwrap(),
        )
        .unwrap();

        let repo = open_repository().unwrap();
        assert_eq!(
            repo.list_pinned_memories(&session.session_id)
                .unwrap()
                .len(),
            1
        );

        run_cli(
            Cli::try_parse_from([
                "ozone-plus",
                "search",
                "session",
                session.session_id.as_str(),
                keyword,
            ])
            .unwrap(),
        )
        .unwrap();
        run_cli(Cli::try_parse_from(["ozone-plus", "search", "global", keyword]).unwrap()).unwrap();

        assert_eq!(
            repo.search_messages(&session.session_id, keyword)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(repo.search_across_sessions(keyword).unwrap().len(), 1);
    }

    #[test]
    fn index_rebuild_command_persists_embeddings_and_builds_vector_index() {
        let _env_guard = ENV_LOCK.lock().unwrap();
        let sandbox = TestSandbox::new("index-rebuild");
        fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));
        write_global_config(
            &sandbox,
            r#"
[memory.embedding]
provider = "mock"
model = "mock/stable"
expected_dimensions = 8
batch_size = 2
mock_seed = 11
"#,
        );

        let repo = open_repository().unwrap();
        let session = repo
            .create_session(CreateSessionRequest::new("Index Session"))
            .unwrap();
        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("Remember the brass lantern under the stairs."),
        )
        .unwrap();
        repo.create_note_memory(
            &session.session_id,
            CreateNoteMemoryRequest::new(
                "Pack the spare lens before leaving camp.",
                AuthorId::User,
                Provenance::UserAuthored,
            ),
        )
        .unwrap();

        run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap()).unwrap();
        let repo = open_repository().unwrap();
        let first_records = repo.list_embedding_artifacts(None).unwrap();
        assert_eq!(first_records.len(), 2);
        let first_ids = first_records
            .iter()
            .map(|record| record.artifact_id.clone())
            .collect::<Vec<_>>();

        let manager = VectorIndexManager::new(repo.paths().data_dir().join("vector-index"));
        let first_state = manager.open().unwrap().unwrap();
        assert_eq!(first_state.vector_count, 2);
        assert_eq!(first_state.metadata.model, "mock/stable");
        assert_eq!(first_state.metadata.dimensions, 8);

        run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap()).unwrap();
        let repo = open_repository().unwrap();
        let second_records = repo.list_embedding_artifacts(None).unwrap();
        let second_ids = second_records
            .iter()
            .map(|record| record.artifact_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(first_ids, second_ids);
        let second_state = manager.open().unwrap().unwrap();
        assert_eq!(first_state.metadata, second_state.metadata);
    }

    #[test]
    fn index_rebuild_fails_cleanly_when_provider_is_disabled() {
        let _env_guard = ENV_LOCK.lock().unwrap();
        let sandbox = TestSandbox::new("index-rebuild-disabled");
        fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let repo = open_repository().unwrap();
        let session = repo
            .create_session(CreateSessionRequest::new("Disabled Index Session"))
            .unwrap();
        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("This should remain FTS-only."),
        )
        .unwrap();

        let err =
            run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap()).unwrap_err();
        assert!(
            err.contains("enabled embedding provider"),
            "unexpected error: {err}"
        );

        let repo = open_repository().unwrap();
        assert!(repo.list_embedding_artifacts(None).unwrap().is_empty());
        let manager = VectorIndexManager::new(repo.paths().data_dir().join("vector-index"));
        assert!(manager.load_metadata().unwrap().is_none());
    }
}
