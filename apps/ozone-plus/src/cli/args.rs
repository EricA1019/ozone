use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

pub const LAUNCHER_SESSION_NAME: &str = "Launcher Session";

#[derive(Parser)]
#[command(
    name = "ozone-plus",
    version = concat!(env!("CARGO_PKG_VERSION"), "+", env!("OZONE_GIT_HASH")),
    about = "⬡ ozone+ — local-LLM chat shell with persistent memory and sessions",
    long_about = "⬡ ozone+ — a chat-first terminal shell for local LLM conversations with persistent memory across sessions.\n\nFeatures: session management, pinned memories, freeform notes, session and global FTS search, branching and swipes, character card import, transcript and session export, hybrid vector/keyword recall, and streaming inference through a running KoboldCpp or llama.cpp backend.",
    after_help = "Examples:\n  ozone-plus create \"First Session\"\n  ozone-plus open <session-id>\n  ozone-plus send <session-id> \"Hello there\"\n  ozone-plus transcript <session-id>\n  ozone-plus memory pin <session-id> <message-id>\n  ozone-plus memory note <session-id> \"Remember the observatory key\"\n  ozone-plus search session <session-id> nebula\n  ozone-plus search global nebula\n  ozone-plus index rebuild\n  ozone-plus branch create <session-id> fork --activate\n  ozone-plus swipe add <session-id> <parent-message-id> \"Alternate reply\"\n  ozone-plus swipe list <session-id>\n  ozone-plus swipe activate <session-id> <swipe-group-id> <ordinal>\n  ozone-plus import card ./aster.json\n  ozone-plus export transcript <session-id> --output ./transcript.txt\n  ozone-plus export session <session-id> --output ./session.json\n\nChat generation currently supports KoboldCpp and llama.cpp. Start one from `ozone`, or point your ozone+ config/session URL at an already running endpoint before sending the first prompt."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
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
    /// Manage saved memories: pinned-message memories and note memories
    Memory(MemoryArgs),
    /// Search messages and saved memory text within one session or across all sessions
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
pub struct CreateArgs {
    /// Human-readable session name stored in the global index
    pub name: String,
    /// Optional character name associated with the session
    #[arg(long = "character", value_name = "NAME")]
    pub character_name: Option<String>,
    /// Optional session tag (repeat --tag for multiple values)
    #[arg(long = "tag", short = 't', value_name = "TAG")]
    pub tags: Vec<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Print session metadata instead of launching the TUI shell
    #[arg(long)]
    pub metadata: bool,
    /// Force open even if session is locked (clears stale locks)
    #[arg(long, short = 'f')]
    pub force: bool,
}

#[derive(Args, Debug, Clone, Copy, Default)]
pub struct HandoffArgs {
    /// Prefer a dedicated launcher-managed session instead of the freshest session
    #[arg(long, hide = true)]
    pub launcher_session: bool,
}

#[derive(Args)]
pub struct SendArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Message content to append to the active branch
    pub content: String,
    /// Author role written into the transcript
    #[arg(long = "author", default_value = "user")]
    pub author_kind: String,
    /// Optional display name for the author
    #[arg(long = "author-name", value_name = "NAME")]
    pub author_name: Option<String>,
}

#[derive(Args)]
pub struct TranscriptArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Optional branch UUID; defaults to the active branch
    #[arg(long = "branch", value_name = "BRANCH_ID")]
    pub branch_id: Option<String>,
}

#[derive(Args)]
pub struct EditArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Message UUID to edit
    pub message_id: String,
    /// Replacement message content
    pub content: String,
}

#[derive(Args)]
pub struct BranchArgs {
    #[command(subcommand)]
    pub command: BranchCommand,
}

#[derive(Subcommand)]
pub enum BranchCommand {
    /// List all persisted branches for a session
    List(SessionArgs),
    /// Create a new branch from a message (defaults to the active branch tip)
    Create(BranchCreateArgs),
    /// Activate an existing branch
    Activate(BranchActivateArgs),
}

#[derive(Args)]
pub struct BranchCreateArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Human-readable branch name
    pub name: String,
    /// Optional source message UUID; defaults to the active branch tip
    #[arg(long = "from", value_name = "MESSAGE_ID")]
    pub from_message_id: Option<String>,
    /// Activate the new branch immediately
    #[arg(long)]
    pub activate: bool,
}

#[derive(Args)]
pub struct BranchActivateArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Branch UUID to activate
    pub branch_id: String,
}

#[derive(Args)]
pub struct SwipeArgs {
    #[command(subcommand)]
    pub command: SwipeCommand,
}

#[derive(Subcommand)]
pub enum SwipeCommand {
    /// List persisted swipe groups and their candidates
    List(SessionArgs),
    /// Add a manual swipe candidate beneath a parent message
    Add(SwipeAddArgs),
    /// Activate a swipe candidate by ordinal
    Activate(SwipeActivateArgs),
}

#[derive(Args)]
pub struct SwipeAddArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Parent message UUID shared by the swipe candidates
    pub parent_message_id: String,
    /// Candidate content to persist
    pub content: String,
    /// Optional context parent UUID for the swipe group
    #[arg(long = "context", value_name = "MESSAGE_ID")]
    pub parent_context_message_id: Option<String>,
    /// Optional existing swipe group UUID; omitted means create/reuse by parent message
    #[arg(long = "group-id", value_name = "SWIPE_GROUP_ID")]
    pub swipe_group_id: Option<String>,
    /// Optional explicit ordinal; omitted means append after the current highest ordinal
    #[arg(long, value_name = "ORDINAL")]
    pub ordinal: Option<u16>,
    /// Author role written into the candidate message
    #[arg(long = "author", default_value = "assistant")]
    pub author_kind: String,
    /// Optional display name for the candidate author
    #[arg(long = "author-name", value_name = "NAME")]
    pub author_name: Option<String>,
    /// Candidate state (`active`, `discarded`, `failed_mid_stream`)
    #[arg(long, default_value = "active")]
    pub state: String,
}

#[derive(Args)]
pub struct SwipeActivateArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Swipe group UUID to update
    pub swipe_group_id: String,
    /// Candidate ordinal to activate
    pub ordinal: u16,
}

#[derive(Args)]
pub struct ImportArgs {
    #[command(subcommand)]
    pub command: ImportCommand,
}

#[derive(Subcommand)]
pub enum ImportCommand {
    /// Import a character card JSON file into a new session
    #[command(visible_alias = "character-card")]
    Card(ImportCharacterCardArgs),
}

#[derive(Args)]
pub struct ImportCharacterCardArgs {
    /// Path to a character card JSON file
    pub input: PathBuf,
    /// Optional session name override; defaults to the card name
    #[arg(long = "session-name", value_name = "NAME")]
    pub session_name: Option<String>,
    /// Extra session tag (repeat --tag for multiple values)
    #[arg(long = "tag", short = 't', value_name = "TAG")]
    pub tags: Vec<String>,
}

#[derive(Args)]
pub struct ExportArgs {
    #[command(subcommand)]
    pub command: ExportCommand,
}

#[derive(Subcommand)]
pub enum ExportCommand {
    /// Export a full session snapshot as JSON
    Session(ExportSessionArgs),
    /// Export a transcript as JSON or plain text
    Transcript(ExportTranscriptArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SessionExportFormat {
    Json,
}

#[derive(Args)]
pub struct ExportSessionArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Export format (currently JSON only)
    #[arg(long, value_enum, default_value_t = SessionExportFormat::Json)]
    pub format: SessionExportFormat,
    /// Explicit output path for the exported file
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TranscriptExportFormat {
    Json,
    Text,
}

#[derive(Args)]
pub struct ExportTranscriptArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Optional branch UUID; defaults to the active branch
    #[arg(long = "branch", value_name = "BRANCH_ID")]
    pub branch_id: Option<String>,
    /// Export format (JSON or plain text)
    #[arg(long, value_enum, default_value_t = TranscriptExportFormat::Text)]
    pub format: TranscriptExportFormat,
    /// Explicit output path for the exported file
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: PathBuf,
}

#[derive(Args)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommand,
}

#[derive(Subcommand)]
pub enum MemoryCommand {
    /// Pin an existing message into hard context
    Pin(MemoryPinArgs),
    /// Create a searchable note memory for the session
    Note(MemoryNoteArgs),
    /// List active and expired pinned-message and note memories
    List(SessionArgs),
    /// Remove a saved memory by artifact ID
    Unpin(MemoryUnpinArgs),
}

#[derive(Args)]
pub struct MemoryPinArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Message UUID to pin into hard context
    pub message_id: String,
    /// Optional number of turns before the memory expires
    #[arg(long = "expires-after-turns", value_name = "N")]
    pub expires_after_turns: Option<u32>,
}

#[derive(Args)]
pub struct MemoryNoteArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Note text to save for later retrieval and optional prompt recall
    pub text: String,
    /// Optional number of turns before the note expires
    #[arg(long = "expires-after-turns", value_name = "N")]
    pub expires_after_turns: Option<u32>,
}

#[derive(Args)]
pub struct MemoryUnpinArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Memory artifact UUID to remove
    pub artifact_id: String,
}

#[derive(Args)]
pub struct SearchArgs {
    #[command(subcommand)]
    pub command: SearchCommand,
}

#[derive(Args)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub command: IndexCommand,
}

#[derive(Subcommand)]
pub enum SearchCommand {
    /// Search within a single session transcript
    Session(SessionSearchArgs),
    /// Search across all indexed sessions
    Global(GlobalSearchArgs),
}

#[derive(Subcommand)]
pub enum IndexCommand {
    /// Derive embeddings, persist them, and rebuild the disk-backed vector index
    Rebuild,
}

#[derive(Args)]
pub struct SummarizeArgs {
    #[command(subcommand)]
    pub command: SummarizeCommand,
}

#[derive(Subcommand)]
pub enum SummarizeCommand {
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
pub struct LifecycleArgs {
    #[command(subcommand)]
    pub command: LifecycleCommand,
}

#[derive(Subcommand)]
pub enum LifecycleCommand {
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
pub struct GcArgs {
    #[command(subcommand)]
    pub command: GcCommand,
}

#[derive(Subcommand)]
pub enum GcCommand {
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
pub struct EventsArgs {
    #[command(subcommand)]
    pub command: EventsCommand,
}

#[derive(Subcommand)]
pub enum EventsCommand {
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
pub struct SessionSearchArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
    /// Full-text search query
    pub query: String,
}

#[derive(Args)]
pub struct GlobalSearchArgs {
    /// Full-text search query
    pub query: String,
}

#[derive(Args)]
pub struct SessionArgs {
    /// Session UUID in 8-4-4-4-12 format
    pub session_id: String,
}