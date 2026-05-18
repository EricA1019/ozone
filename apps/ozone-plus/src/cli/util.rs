use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ozone_core::engine::RequestId;
use ozone_persist::{
    AuthorId, BranchId, BranchRecord,
    CreateNoteMemoryRequest, MemoryArtifactId, MessageId, PersistError, Provenance, SessionId,
    SqliteRepository, SwipeGroupId, TranscriptExport,
};
use ozone_inference::MemoryConfig;

use crate::cli::print;
use crate::hybrid_search::load_memory_config as _load_memory_config;

pub fn open_repository() -> Result<SqliteRepository, String> {
    SqliteRepository::from_xdg().map_err(|error| error.to_string())
}

pub fn open_cli_engine(
) -> Result<
    crate::store::Phase1bCliEngine,
    String,
> {
    crate::store::Phase1bCliEngine::open()
}

pub fn require_non_empty(label: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }

    Ok(trimmed.to_owned())
}

pub fn optional_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

pub fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .filter_map(|tag| {
            let trimmed = tag.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .collect()
}

pub fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".to_owned()
    } else {
        tags.join(", ")
    }
}

pub fn format_timestamp(timestamp: i64) -> String {
    print::format_timestamp(timestamp)
}

pub fn format_timestamp_short(timestamp: i64) -> String {
    print::format_timestamp_short(timestamp)
}

pub fn format_message_time(timestamp: i64) -> String {
    print::format_message_time(timestamp)
}

pub fn format_author_id(author: &AuthorId) -> String {
    print::format_author_id(author)
}

pub fn now_timestamp_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn generate_message_id() -> Result<MessageId, String> {
    MessageId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

pub fn generate_branch_id() -> Result<BranchId, String> {
    BranchId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

pub fn generate_request_id() -> Result<RequestId, String> {
    RequestId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

pub fn generate_swipe_group_id() -> Result<SwipeGroupId, String> {
    SwipeGroupId::parse(generate_uuid_like()).map_err(|error| error.to_string())
}

pub fn generate_uuid_like() -> String {
    static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
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

pub fn parse_session_id(value: &str) -> Result<SessionId, String> {
    SessionId::parse(value.trim()).map_err(|error| error.to_string())
}

pub fn parse_message_id(value: &str) -> Result<MessageId, String> {
    MessageId::parse(value.trim()).map_err(|error| error.to_string())
}

pub fn parse_memory_artifact_id(value: &str) -> Result<MemoryArtifactId, String> {
    MemoryArtifactId::parse(value.trim()).map_err(|error| error.to_string())
}

pub fn parse_branch_id(value: &str) -> Result<BranchId, String> {
    BranchId::parse(value.trim()).map_err(|error| error.to_string())
}

pub fn parse_swipe_group_id(value: &str) -> Result<SwipeGroupId, String> {
    SwipeGroupId::parse(value.trim()).map_err(|error| error.to_string())
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

pub fn require_existing_file(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to read {label} at {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} at {} is not a file", path.display()));
    }

    Ok(path.to_path_buf())
}

pub fn read_utf8_file(path: &Path, label: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read {label} at {}: {error}", path.display()))
}

pub fn render_transcript_text(export: &TranscriptExport) -> String {
    print::render_transcript_text(export)
}

pub fn load_memory_config(
    repo: &SqliteRepository,
    session_id: Option<&SessionId>,
) -> Result<MemoryConfig, String> {
    _load_memory_config(repo, session_id).map_err(|e| e.to_string())
}

pub fn create_note_memory_request(
    text: String,
    author: AuthorId,
    provenance: Provenance,
) -> CreateNoteMemoryRequest {
    CreateNoteMemoryRequest::new(text, author, provenance)
}

// Re-exports for convenience
pub use ozone_engine::ConversationBranchRecord;
pub use ozone_core::engine::ConversationMessage;
pub use ozone_engine::ConversationStore;
pub use ozone_engine::EngineCommand;
pub use ozone_engine::EngineCommandResult;