use std::path::PathBuf;

use ozone_core::{engine::MessageId, session::SessionId};
use ozone_inference::{ConfigLoader, OzoneConfig};
use ozone_memory::{
    build_embedding_provider, EmbeddingAvailability, EmbeddingProvider, EmbeddingProviderMetadata,
    EmbeddingRecord, EmbeddingRecordMetadata, EmbeddingRequest, MemoryArtifactId, Provenance,
    VectorIndexRebuildSummary,
};
use ozone_persist::{ConversationMessage, PinnedMemoryView, SqliteRepository};

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRebuildResult {
    pub provider: EmbeddingProviderMetadata,
    pub session_count: usize,
    pub message_source_count: usize,
    pub memory_source_count: usize,
    pub persisted_artifact_count: usize,
    pub index: VectorIndexRebuildSummary,
}

impl IndexRebuildResult {
    pub fn source_count(&self) -> usize {
        self.message_source_count + self.memory_source_count
    }

    pub fn index_path(&self) -> PathBuf {
        self.index.paths.index_path()
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.index.paths.metadata_path()
    }
}

#[derive(Debug, Clone)]
struct RebuildSources {
    session_count: usize,
    message_source_count: usize,
    memory_source_count: usize,
    sources: Vec<EmbeddingSource>,
}

#[derive(Debug, Clone)]
struct EmbeddingSource {
    artifact_id: MemoryArtifactId,
    session_id: SessionId,
    source_message_id: Option<MessageId>,
    provenance: Provenance,
    created_at: i64,
    snapshot_version: u64,
    text: String,
}

pub fn rebuild_index(repo: &SqliteRepository) -> Result<IndexRebuildResult, String> {
    let config = ConfigLoader::new()
        .build()
        .map_err(|error| format!("failed to load ozone+ config for index rebuild: {error}"))?;
    rebuild_index_with_config(repo, &config)
}

pub fn rebuild_index_with_config(
    repo: &SqliteRepository,
    config: &OzoneConfig,
) -> Result<IndexRebuildResult, String> {
    let provider = build_embedding_provider(config.memory.embedding.clone());
    let provider_metadata = provider.metadata();
    require_provider_ready(provider.availability())?;

    let rebuild_sources = collect_sources(repo)?;
    let records = derive_embedding_records(
        provider.as_ref(),
        &provider_metadata,
        config.memory.embedding.batch_size,
        &rebuild_sources.sources,
    )?;
    let persisted_artifact_count = repo
        .replace_embedding_artifacts(None, &records)
        .map_err(|error| error.to_string())?;
    let persisted_records = repo
        .list_embedding_artifacts(None)
        .map_err(|error| error.to_string())?;
    let manager =
        ozone_memory::VectorIndexManager::new(repo.paths().data_dir().join("vector-index"));
    let index = manager
        .rebuild(&provider_metadata, &persisted_records)
        .map_err(|error| error.to_string())?;

    Ok(IndexRebuildResult {
        provider: provider_metadata,
        session_count: rebuild_sources.session_count,
        message_source_count: rebuild_sources.message_source_count,
        memory_source_count: rebuild_sources.memory_source_count,
        persisted_artifact_count,
        index,
    })
}

fn require_provider_ready(availability: EmbeddingAvailability) -> Result<(), String> {
    match availability {
        EmbeddingAvailability::Ready => Ok(()),
        EmbeddingAvailability::Disabled { reason } => Err(format!(
            "index rebuild requires an enabled embedding provider: {reason}"
        )),
        EmbeddingAvailability::Unavailable { reason } => Err(format!(
            "index rebuild requires an available embedding provider: {reason}"
        )),
    }
}

fn collect_sources(repo: &SqliteRepository) -> Result<RebuildSources, String> {
    let sessions = repo.list_sessions().map_err(|error| error.to_string())?;
    let mut message_source_count = 0;
    let mut memory_source_count = 0;
    let mut sources = Vec::new();

    for session in &sessions {
        let messages = repo
            .list_session_messages(&session.session_id)
            .map_err(|error| error.to_string())?;
        append_message_sources(
            &mut sources,
            &mut message_source_count,
            &session.session_id,
            messages,
        );
        let memories = repo
            .list_pinned_memories(&session.session_id)
            .map_err(|error| error.to_string())?;
        append_memory_sources(
            &mut sources,
            &mut memory_source_count,
            &session.session_id,
            memories,
        );
    }

    sources.sort_by(|left, right| {
        left.session_id
            .as_str()
            .cmp(right.session_id.as_str())
            .then_with(|| {
                left.source_message_id
                    .as_ref()
                    .map(|message_id| message_id.as_str())
                    .cmp(
                        &right
                            .source_message_id
                            .as_ref()
                            .map(|message_id| message_id.as_str()),
                    )
            })
            .then_with(|| left.snapshot_version.cmp(&right.snapshot_version))
            .then_with(|| left.artifact_id.as_str().cmp(right.artifact_id.as_str()))
    });

    Ok(RebuildSources {
        session_count: sessions.len(),
        message_source_count,
        memory_source_count,
        sources,
    })
}

fn append_message_sources(
    sources: &mut Vec<EmbeddingSource>,
    message_source_count: &mut usize,
    session_id: &SessionId,
    messages: Vec<ConversationMessage>,
) {
    for (index, message) in messages.into_iter().enumerate() {
        let text = message.content.trim();
        if text.is_empty() {
            continue;
        }
        let artifact_id = message_embedding_artifact_id(session_id, message.message_id.as_str());
        let provenance = message_provenance(&message);
        let source_message_id = message.message_id;
        let created_at = message.edited_at.unwrap_or(message.created_at);
        let text = message.content;
        *message_source_count += 1;
        sources.push(EmbeddingSource {
            artifact_id,
            session_id: session_id.clone(),
            source_message_id: Some(source_message_id),
            provenance,
            created_at,
            snapshot_version: (index + 1) as u64,
            text,
        });
    }
}

fn append_memory_sources(
    sources: &mut Vec<EmbeddingSource>,
    memory_source_count: &mut usize,
    session_id: &SessionId,
    memories: Vec<PinnedMemoryView>,
) {
    for memory in memories {
        let text = memory.record.content.text.trim();
        if text.is_empty() {
            continue;
        }
        let artifact_id =
            derive_embedding_artifact_id("memory", session_id, memory.record.artifact_id.as_str());
        let source_message_id = memory.record.source_message_id;
        let provenance = memory.record.provenance;
        let created_at = memory.record.created_at;
        let snapshot_version = memory.record.snapshot_version;
        let text = memory.record.content.text;
        *memory_source_count += 1;
        sources.push(EmbeddingSource {
            artifact_id,
            session_id: session_id.clone(),
            source_message_id,
            provenance,
            created_at,
            snapshot_version,
            text,
        });
    }
}

fn derive_embedding_records(
    provider: &dyn EmbeddingProvider,
    provider_metadata: &EmbeddingProviderMetadata,
    batch_size: usize,
    sources: &[EmbeddingSource],
) -> Result<Vec<EmbeddingRecord>, String> {
    let batch_size = batch_size.max(1);
    let record_metadata = EmbeddingRecordMetadata {
        provider: provider_metadata.provider,
        model: provider_metadata.model.clone(),
        dimensions: provider_metadata.dimensions,
    };
    let mut records = Vec::with_capacity(sources.len());

    for source_batch in sources.chunks(batch_size) {
        let requests = source_batch
            .iter()
            .map(|source| EmbeddingRequest::document(source.text.clone()))
            .collect::<Vec<_>>();
        let generated_batch = provider
            .embed(&requests)
            .map_err(|error| format!("failed to derive embeddings for index rebuild: {error}"))?;
        if generated_batch.embeddings.len() != source_batch.len() {
            return Err(format!(
                "embedding provider returned {} vectors for {} rebuild sources",
                generated_batch.embeddings.len(),
                source_batch.len()
            ));
        }

        for (source, embedding) in source_batch
            .iter()
            .zip(generated_batch.embeddings.into_iter())
        {
            records.push(EmbeddingRecord {
                artifact_id: source.artifact_id.clone(),
                session_id: source.session_id.clone(),
                content: embedding.content,
                source_message_id: source.source_message_id.clone(),
                provenance: source.provenance,
                created_at: source.created_at,
                snapshot_version: source.snapshot_version,
                metadata: record_metadata.clone(),
            });
        }
    }

    Ok(records)
}

pub(crate) fn message_provenance(message: &ConversationMessage) -> Provenance {
    message_provenance_for_author_kind(&message.author_kind)
}

pub(crate) fn message_provenance_for_author_kind(author_kind: &str) -> Provenance {
    match author_kind.trim().to_ascii_lowercase().as_str() {
        "user" => Provenance::UserAuthored,
        "assistant" => Provenance::UtilityModel,
        "system" => Provenance::SystemGenerated,
        _ => Provenance::ImportedExternal,
    }
}

pub(crate) fn message_embedding_artifact_id(
    session_id: &SessionId,
    message_id: &str,
) -> MemoryArtifactId {
    derive_embedding_artifact_id("message", session_id, message_id)
}

pub(crate) fn memory_embedding_artifact_id(
    session_id: &SessionId,
    memory_artifact_id: &str,
) -> MemoryArtifactId {
    derive_embedding_artifact_id("memory", session_id, memory_artifact_id)
}

pub(crate) fn derive_embedding_artifact_id(
    kind: &str,
    session_id: &SessionId,
    source_id: &str,
) -> MemoryArtifactId {
    let mut bytes = [0_u8; 16];
    let hi = stable_hash(
        0x3b4f_9c41_a5d0_11ef,
        kind.as_bytes(),
        session_id.as_str().as_bytes(),
        source_id.as_bytes(),
    );
    let lo = stable_hash(
        0x9e67_d93c_1147_2d1b,
        kind.as_bytes(),
        session_id.as_str().as_bytes(),
        source_id.as_bytes(),
    );
    bytes[..8].copy_from_slice(&hi.to_be_bytes());
    bytes[8..].copy_from_slice(&lo.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    MemoryArtifactId::parse(format_uuid(bytes)).expect("deterministic embedding artifact ID")
}

fn stable_hash(seed: u64, first: &[u8], second: &[u8], third: &[u8]) -> u64 {
    let mut state = seed ^ FNV_OFFSET_BASIS;
    for segment in [first, second, third] {
        for byte in segment {
            state ^= u64::from(*byte);
            state = state.wrapping_mul(FNV_PRIME);
        }
        state ^= 0xff;
        state = state.wrapping_mul(FNV_PRIME);
    }
    state
}

fn format_uuid(bytes: [u8; 16]) -> String {
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
        bytes[15]
    )
}
