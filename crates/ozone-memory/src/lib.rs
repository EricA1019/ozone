//! `ozone-memory` — memory system for ozone+.
//!
//! Provides pinned/note memories, embedding providers (fastembed),
//! vector index management (usearch), hybrid BM25+vector retrieval,
//! lifecycle tiering, garbage collection, and disk monitoring.

pub mod assistive;
pub mod disk_monitor;
mod index;
mod lifecycle;
mod provider;
mod scoring;
pub mod summary;

pub use assistive::{ImportanceScorer, KeywordExtractor, ScoringConfig};

use std::{error::Error, fmt, str::FromStr};

use ozone_core::{
    engine::MessageId,
    session::{SessionId, UnixTimestamp},
};
use serde::{Deserialize, Serialize};

pub use disk_monitor::{check_disk_space, DiskCheckResult, DiskMonitorPolicy, DiskStatus};
pub use index::{
    artifact_index_key, VectorIndexError, VectorIndexManager, VectorIndexMetadata,
    VectorIndexPaths, VectorIndexQueryMatch, VectorIndexQueryResult, VectorIndexRebuildSummary,
    VectorIndexState, VersionCompatibilityResult,
};
pub use lifecycle::{
    adjusted_provenance_weight, assess_artifact_staleness, message_age_since_snapshot,
    storage_tier_for_age, ArtifactStaleness, StorageTier, StorageTierPolicy,
};
#[cfg(feature = "fastembed")]
pub use provider::FastembedEmbeddingProvider;
pub use provider::{
    build_embedding_provider, EmbeddingAvailability, EmbeddingBatch, EmbeddingProvider,
    EmbeddingProviderConfig, EmbeddingProviderError, EmbeddingProviderKind,
    EmbeddingProviderMetadata, EmbeddingPurpose, EmbeddingRequest, GeneratedEmbedding,
    MockEmbeddingProvider,
};
pub use scoring::{
    ArtifactLifecycleSummary, HybridScoreInput, ProvenanceWeights, RetrievalHit, RetrievalHitKind,
    RetrievalResultSet, RetrievalScoreBreakdown, RetrievalScoreInput, RetrievalSearchMode,
    RetrievalSourceState, RetrievalStatus, RetrievalWeights, WeightValidationError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryIdError;

impl fmt::Display for MemoryIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("memory artifact IDs must be UUID strings in 8-4-4-4-12 format")
    }
}

impl Error for MemoryIdError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryArtifactId(String);

impl MemoryArtifactId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MemoryIdError> {
        value.as_ref().parse()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MemoryArtifactId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MemoryArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryArtifactId {
    type Err = MemoryIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        normalize_uuid_string(value).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryParseError {
    kind: &'static str,
    value: String,
}

impl MemoryParseError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }
}

impl fmt::Display for MemoryParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unrecognized {} value `{}`", self.kind, self.value)
    }
}

impl Error for MemoryParseError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum AuthorId {
    User,
    Character(String),
    System,
    Narrator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedMemoryContent {
    pub text: String,
    pub pinned_by: AuthorId,
    pub expires_after_turns: Option<u32>,
}

impl PinnedMemoryContent {
    pub fn new(text: impl Into<String>, pinned_by: AuthorId) -> Self {
        Self {
            text: text.into(),
            pinned_by,
            expires_after_turns: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingContent {
    pub vector: Vec<f32>,
    pub source_text_hash: u64,
}

impl EmbeddingContent {
    pub fn new(vector: Vec<f32>, source_text_hash: u64) -> Self {
        Self {
            vector,
            source_text_hash,
        }
    }

    pub fn dimensions(&self) -> usize {
        self.vector.len()
    }
}

/// Content for a chunk summary — deterministic extraction from a message range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkSummaryContent {
    /// The summary text (first N sentences extracted from the chunk).
    pub text: String,
    /// Number of source messages that contributed to this summary.
    pub source_count: usize,
}

/// Content for a session synopsis — a one-paragraph session overview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSynopsisContent {
    /// The synopsis text.
    pub text: String,
    /// Number of messages in the session when this synopsis was generated.
    pub message_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryContent {
    PinnedMemory {
        text: String,
        pinned_by: AuthorId,
        expires_after_turns: Option<u32>,
    },
    Embedding {
        vector: Vec<f32>,
        source_text_hash: u64,
    },
    ChunkSummary {
        text: String,
        source_count: usize,
    },
    SessionSynopsis {
        text: String,
        message_count: usize,
    },
    ImportanceProposal {
        score: f32,
        justification: String,
    },
    RetrievalKey {
        keywords: Vec<String>,
    },
}

impl MemoryContent {
    pub fn pinned(
        text: impl Into<String>,
        pinned_by: AuthorId,
        expires_after_turns: Option<u32>,
    ) -> Self {
        Self::PinnedMemory {
            text: text.into(),
            pinned_by,
            expires_after_turns,
        }
    }

    pub fn embedding(vector: Vec<f32>, source_text_hash: u64) -> Self {
        Self::Embedding {
            vector,
            source_text_hash,
        }
    }

    pub fn chunk_summary(text: impl Into<String>, source_count: usize) -> Self {
        Self::ChunkSummary {
            text: text.into(),
            source_count,
        }
    }

    pub fn session_synopsis(text: impl Into<String>, message_count: usize) -> Self {
        Self::SessionSynopsis {
            text: text.into(),
            message_count,
        }
    }

    pub fn importance_proposal(score: f32, justification: impl Into<String>) -> Self {
        Self::ImportanceProposal {
            score,
            justification: justification.into(),
        }
    }

    pub fn retrieval_key(keywords: Vec<String>) -> Self {
        Self::RetrievalKey { keywords }
    }

    pub fn into_pinned(self) -> Option<PinnedMemoryContent> {
        match self {
            Self::PinnedMemory {
                text,
                pinned_by,
                expires_after_turns,
            } => Some(PinnedMemoryContent {
                text,
                pinned_by,
                expires_after_turns,
            }),
            _ => None,
        }
    }

    pub fn into_embedding(self) -> Option<EmbeddingContent> {
        match self {
            Self::Embedding {
                vector,
                source_text_hash,
            } => Some(EmbeddingContent {
                vector,
                source_text_hash,
            }),
            _ => None,
        }
    }

    pub fn into_chunk_summary(self) -> Option<ChunkSummaryContent> {
        match self {
            Self::ChunkSummary { text, source_count } => {
                Some(ChunkSummaryContent { text, source_count })
            }
            _ => None,
        }
    }

    pub fn into_session_synopsis(self) -> Option<SessionSynopsisContent> {
        match self {
            Self::SessionSynopsis {
                text,
                message_count,
            } => Some(SessionSynopsisContent {
                text,
                message_count,
            }),
            _ => None,
        }
    }
}

impl From<PinnedMemoryContent> for MemoryContent {
    fn from(value: PinnedMemoryContent) -> Self {
        Self::PinnedMemory {
            text: value.text,
            pinned_by: value.pinned_by,
            expires_after_turns: value.expires_after_turns,
        }
    }
}

impl From<EmbeddingContent> for MemoryContent {
    fn from(value: EmbeddingContent) -> Self {
        Self::Embedding {
            vector: value.vector,
            source_text_hash: value.source_text_hash,
        }
    }
}

impl From<ChunkSummaryContent> for MemoryContent {
    fn from(value: ChunkSummaryContent) -> Self {
        Self::ChunkSummary {
            text: value.text,
            source_count: value.source_count,
        }
    }
}

impl From<SessionSynopsisContent> for MemoryContent {
    fn from(value: SessionSynopsisContent) -> Self {
        Self::SessionSynopsis {
            text: value.text,
            message_count: value.message_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    UserAuthored,
    CharacterCard,
    Lorebook,
    SystemGenerated,
    UtilityModel,
    ImportedExternal,
}

impl Provenance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserAuthored => "user_authored",
            Self::CharacterCard => "character_card",
            Self::Lorebook => "lorebook",
            Self::SystemGenerated => "system_generated",
            Self::UtilityModel => "utility_model",
            Self::ImportedExternal => "imported_external",
        }
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Provenance {
    type Err = MemoryParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user_authored" => Ok(Self::UserAuthored),
            "character_card" => Ok(Self::CharacterCard),
            "lorebook" => Ok(Self::Lorebook),
            "system_generated" => Ok(Self::SystemGenerated),
            "utility_model" => Ok(Self::UtilityModel),
            "imported_external" => Ok(Self::ImportedExternal),
            _ => Err(MemoryParseError::new("provenance", value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedMemoryRecord {
    pub artifact_id: MemoryArtifactId,
    pub session_id: SessionId,
    pub content: PinnedMemoryContent,
    pub source_message_id: Option<MessageId>,
    pub provenance: Provenance,
    pub created_at: UnixTimestamp,
    pub snapshot_version: u64,
}

impl PinnedMemoryRecord {
    pub fn turns_elapsed(&self, current_message_count: u64) -> u64 {
        current_message_count.saturating_sub(self.snapshot_version)
    }

    pub fn remaining_turns(&self, current_message_count: u64) -> Option<u32> {
        let expires_after_turns = self.content.expires_after_turns?;
        let elapsed = self.turns_elapsed(current_message_count);
        let budget = u64::from(expires_after_turns);

        if elapsed >= budget {
            Some(0)
        } else {
            Some((budget - elapsed) as u32)
        }
    }

    pub fn is_active(&self, current_message_count: u64) -> bool {
        match self.content.expires_after_turns {
            None => true,
            Some(expires_after_turns) => {
                self.turns_elapsed(current_message_count) < u64::from(expires_after_turns)
            }
        }
    }

    pub fn into_view(self, current_message_count: u64) -> PinnedMemoryView {
        let turns_elapsed = self.turns_elapsed(current_message_count);
        let remaining_turns = self.remaining_turns(current_message_count);
        let is_active = self.is_active(current_message_count);

        PinnedMemoryView {
            record: self,
            turns_elapsed,
            remaining_turns,
            is_active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedMemoryView {
    pub record: PinnedMemoryRecord,
    pub turns_elapsed: u64,
    pub remaining_turns: Option<u32>,
    pub is_active: bool,
}

impl PinnedMemoryView {
    pub const fn is_expired(&self) -> bool {
        !self.is_active
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingRecordMetadata {
    pub provider: EmbeddingProviderKind,
    pub model: String,
    pub dimensions: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    pub artifact_id: MemoryArtifactId,
    pub session_id: SessionId,
    pub content: EmbeddingContent,
    pub source_message_id: Option<MessageId>,
    pub provenance: Provenance,
    pub created_at: UnixTimestamp,
    pub snapshot_version: u64,
    pub metadata: EmbeddingRecordMetadata,
}

impl EmbeddingRecord {
    pub fn matches_source_text(&self, text: &str) -> bool {
        self.content.source_text_hash == source_text_hash(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinMessageMemoryRequest {
    pub pinned_by: AuthorId,
    pub expires_after_turns: Option<u32>,
    pub provenance: Provenance,
}

impl PinMessageMemoryRequest {
    pub fn new(pinned_by: AuthorId, provenance: Provenance) -> Self {
        Self {
            pinned_by,
            expires_after_turns: None,
            provenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateNoteMemoryRequest {
    pub content: PinnedMemoryContent,
    pub provenance: Provenance,
}

impl CreateNoteMemoryRequest {
    pub fn new(text: impl Into<String>, pinned_by: AuthorId, provenance: Provenance) -> Self {
        Self {
            content: PinnedMemoryContent::new(text, pinned_by),
            provenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchSessionMetadata {
    pub session_id: SessionId,
    pub session_name: String,
    pub character_name: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossSessionSearchHit {
    pub session: SearchSessionMetadata,
    pub message_id: MessageId,
    pub author_kind: String,
    pub content: String,
    pub created_at: UnixTimestamp,
    pub bm25_score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallHit {
    pub session: SearchSessionMetadata,
    pub message_id: MessageId,
    pub author_kind: String,
    pub text: String,
    pub created_at: UnixTimestamp,
}

impl From<CrossSessionSearchHit> for RecallHit {
    fn from(value: CrossSessionSearchHit) -> Self {
        Self {
            session: value.session,
            message_id: value.message_id,
            author_kind: value.author_kind,
            text: value.content,
            created_at: value.created_at,
        }
    }
}

/// Request to generate a chunk summary from a message range.
#[derive(Debug, Clone)]
pub struct ChunkSummaryRequest {
    pub session_id: String,
    pub start_message_id: String,
    pub end_message_id: String,
    /// Max number of sentences to extract.
    pub max_sentences: usize,
}

/// Request to generate a session synopsis.
#[derive(Debug, Clone)]
pub struct SessionSynopsisRequest {
    pub session_id: String,
    /// Max length in characters for the synopsis.
    pub max_chars: usize,
}

pub fn source_text_hash(text: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn normalize_uuid_string(value: &str) -> Result<String, MemoryIdError> {
    const HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

    let bytes = value.as_bytes();

    if bytes.len() != 36 {
        return Err(MemoryIdError);
    }

    let mut normalized = String::with_capacity(36);

    for (idx, byte) in bytes.iter().enumerate() {
        if HYPHEN_POSITIONS.contains(&idx) {
            if *byte != b'-' {
                return Err(MemoryIdError);
            }

            normalized.push('-');
            continue;
        }

        if !byte.is_ascii_hexdigit() {
            return Err(MemoryIdError);
        }

        normalized.push((*byte as char).to_ascii_lowercase());
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_id(value: &str) -> SessionId {
        SessionId::parse(value).unwrap()
    }

    fn message_id(value: &str) -> MessageId {
        MessageId::parse(value).unwrap()
    }

    #[test]
    fn memory_content_round_trips_as_pinned_memory() {
        let content = MemoryContent::pinned(
            "Keep the observatory key hidden.",
            AuthorId::Character("Aster".to_owned()),
            Some(4),
        );

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"kind\":\"pinned_memory\""));

        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        let pinned = parsed.into_pinned().unwrap();
        assert_eq!(pinned.text, "Keep the observatory key hidden.");
        assert_eq!(pinned.expires_after_turns, Some(4));
        assert_eq!(pinned.pinned_by, AuthorId::Character("Aster".to_owned()));
    }

    #[test]
    fn memory_content_round_trips_as_embedding() {
        let content = MemoryContent::embedding(vec![0.25, -0.5, 0.75], source_text_hash("query"));

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"kind\":\"embedding\""));

        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        let embedding = parsed.into_embedding().unwrap();
        assert_eq!(embedding.vector, vec![0.25, -0.5, 0.75]);
        assert_eq!(embedding.source_text_hash, source_text_hash("query"));
    }

    #[test]
    fn pinned_memory_records_compute_active_and_expired_views() {
        let record = PinnedMemoryRecord {
            artifact_id: MemoryArtifactId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            session_id: session_id("223e4567-e89b-12d3-a456-426614174000"),
            content: PinnedMemoryContent {
                text: "Remember the fallback passphrase.".to_owned(),
                pinned_by: AuthorId::User,
                expires_after_turns: Some(3),
            },
            source_message_id: Some(message_id("323e4567-e89b-12d3-a456-426614174000")),
            provenance: Provenance::UserAuthored,
            created_at: 1_725_647_200_000,
            snapshot_version: 5,
        };

        assert_eq!(record.turns_elapsed(6), 1);
        assert_eq!(record.remaining_turns(6), Some(2));
        assert!(record.is_active(6));

        let expired = record.clone().into_view(8);
        assert_eq!(expired.turns_elapsed, 3);
        assert_eq!(expired.remaining_turns, Some(0));
        assert!(!expired.is_active);
        assert!(expired.is_expired());
    }

    #[test]
    fn embedding_records_track_source_text_hashes() {
        let record = EmbeddingRecord {
            artifact_id: MemoryArtifactId::parse("623e4567-e89b-12d3-a456-426614174000").unwrap(),
            session_id: session_id("723e4567-e89b-12d3-a456-426614174000"),
            content: EmbeddingContent::new(
                vec![0.1, 0.2, 0.3],
                source_text_hash("The key is blue"),
            ),
            source_message_id: Some(message_id("823e4567-e89b-12d3-a456-426614174000")),
            provenance: Provenance::UserAuthored,
            created_at: 1_725_647_200_200,
            snapshot_version: 11,
            metadata: EmbeddingRecordMetadata {
                provider: EmbeddingProviderKind::Mock,
                model: "mock/stable".to_owned(),
                dimensions: 3,
            },
        };

        assert!(record.matches_source_text("The key is blue"));
        assert!(!record.matches_source_text("The key is red"));
    }

    #[test]
    fn cross_session_search_hits_convert_into_recall_hits() {
        let hit = CrossSessionSearchHit {
            session: SearchSessionMetadata {
                session_id: session_id("423e4567-e89b-12d3-a456-426614174000"),
                session_name: "Observatory".to_owned(),
                character_name: Some("Aster".to_owned()),
                tags: vec!["stellar".to_owned(), "phase2a".to_owned()],
            },
            message_id: message_id("523e4567-e89b-12d3-a456-426614174000"),
            author_kind: "assistant".to_owned(),
            content: "The nebula gate only opens at dusk.".to_owned(),
            created_at: 1_725_647_200_100,
            bm25_score: -1.25,
        };

        let recall: RecallHit = hit.clone().into();
        assert_eq!(recall.session, hit.session);
        assert_eq!(recall.message_id, hit.message_id);
        assert_eq!(recall.author_kind, "assistant");
        assert_eq!(recall.text, "The nebula gate only opens at dusk.");
        assert_eq!(recall.created_at, 1_725_647_200_100);
    }

    #[test]
    fn chunk_summary_content_roundtrip() {
        let content = MemoryContent::chunk_summary("Alice and Bob discussed their plan.", 5);
        let json = serde_json::to_string(&content).unwrap();
        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        assert_eq!(content, parsed);
    }

    #[test]
    fn session_synopsis_content_roundtrip() {
        let content =
            MemoryContent::session_synopsis("A roleplay session about forest exploration.", 42);
        let json = serde_json::to_string(&content).unwrap();
        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        assert_eq!(content, parsed);
    }

    #[test]
    fn importance_proposal_content_roundtrip() {
        let content = MemoryContent::importance_proposal(
            0.85,
            "This memory is critical for plot continuity.",
        );
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"kind\":\"importance_proposal\""));
        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        assert_eq!(content, parsed);
    }

    #[test]
    fn retrieval_key_content_roundtrip() {
        let content = MemoryContent::retrieval_key(vec![
            "observatory".to_owned(),
            "lantern".to_owned(),
            "dusk".to_owned(),
        ]);
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"kind\":\"retrieval_key\""));
        let parsed = serde_json::from_str::<MemoryContent>(&json).unwrap();
        assert_eq!(content, parsed);
    }

    #[test]
    fn importance_proposal_constructor_works() {
        let content = MemoryContent::importance_proposal(0.75, "High relevance to current scene.");
        match content {
            MemoryContent::ImportanceProposal {
                score,
                justification,
            } => {
                assert!((score - 0.75).abs() < f32::EPSILON);
                assert_eq!(justification, "High relevance to current scene.");
            }
            _ => panic!("Expected ImportanceProposal variant"),
        }
    }

    #[test]
    fn retrieval_key_constructor_works() {
        let keywords = vec!["crystal".to_owned(), "hidden".to_owned()];
        let content = MemoryContent::retrieval_key(keywords.clone());
        match content {
            MemoryContent::RetrievalKey { keywords: kw } => {
                assert_eq!(kw, keywords);
            }
            _ => panic!("Expected RetrievalKey variant"),
        }
    }
}
