use std::{error::Error, fmt};

use ozone_core::{engine::MessageId, session::UnixTimestamp};
use serde::{Deserialize, Serialize};

use crate::{MemoryArtifactId, Provenance, SearchSessionMetadata};

const RETRIEVAL_WEIGHT_EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSearchMode {
    Hybrid,
    FtsOnly,
}

impl RetrievalSearchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::FtsOnly => "fts_only",
        }
    }
}

impl fmt::Display for RetrievalSearchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalHitKind {
    Message,
    PinnedMemory,
    NoteMemory,
}

impl RetrievalHitKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::PinnedMemory => "pinned_memory",
            Self::NoteMemory => "note_memory",
        }
    }
}

impl fmt::Display for RetrievalHitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSourceState {
    Current,
    InactiveMemory,
    SourceChanged,
    SourceMissing,
}

impl RetrievalSourceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::InactiveMemory => "inactive_memory",
            Self::SourceChanged => "source_changed",
            Self::SourceMissing => "source_missing",
        }
    }
}

impl fmt::Display for RetrievalSourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalStatus {
    pub mode: RetrievalSearchMode,
    pub reason: Option<String>,
    pub filtered_stale_embeddings: usize,
    pub downranked_embeddings: usize,
}

impl RetrievalStatus {
    pub fn summary_line(&self) -> String {
        let mut summary = self.mode.to_string();
        if let Some(reason) = self.reason.as_ref() {
            summary.push_str(": ");
            summary.push_str(reason);
        }

        let mut details = Vec::new();
        if self.filtered_stale_embeddings > 0 {
            details.push(format!(
                "filtered {} stale embedding{}",
                self.filtered_stale_embeddings,
                if self.filtered_stale_embeddings == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if self.downranked_embeddings > 0 {
            details.push(format!(
                "downranked {} inactive hit{}",
                self.downranked_embeddings,
                if self.downranked_embeddings == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }

        if !details.is_empty() {
            summary.push_str(" · ");
            summary.push_str(&details.join(", "));
        }

        summary
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalScoreBreakdown {
    pub overall_score: f32,
    pub hybrid_alpha: f32,
    pub bm25_score: Option<f32>,
    pub text_score: f32,
    pub text_contribution: f32,
    pub vector_similarity: Option<f32>,
    pub vector_contribution: f32,
    pub semantic_score: f32,
    pub semantic_contribution: f32,
    pub importance_score: f32,
    pub importance_contribution: f32,
    pub recency_score: f32,
    pub recency_contribution: f32,
    pub provenance_score: f32,
    pub provenance_config_weight: f32,
    pub provenance_contribution: f32,
    pub stale_penalty: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactLifecycleSummary {
    pub storage_tier: crate::StorageTier,
    pub age_messages: u64,
    pub age_hours: u64,
    pub is_stale: bool,
    pub adjusted_provenance_score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalHit {
    pub session: SearchSessionMetadata,
    pub hit_kind: RetrievalHitKind,
    pub artifact_id: Option<MemoryArtifactId>,
    pub message_id: Option<MessageId>,
    pub source_message_id: Option<MessageId>,
    pub author_kind: Option<String>,
    pub text: String,
    pub created_at: UnixTimestamp,
    pub provenance: Provenance,
    pub source_state: RetrievalSourceState,
    pub is_active_memory: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<ArtifactLifecycleSummary>,
    pub score: RetrievalScoreBreakdown,
}

impl RetrievalHit {
    pub fn overall_score(&self) -> f32 {
        self.score.overall_score
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalResultSet {
    pub query: String,
    pub status: RetrievalStatus,
    pub hits: Vec<RetrievalHit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridScoreInput {
    pub mode: RetrievalSearchMode,
    pub hybrid_alpha: f32,
    pub bm25_score: Option<f32>,
    pub text_score: f32,
    pub vector_similarity: Option<f32>,
    pub importance_score: f32,
    pub recency_score: f32,
    pub provenance: Provenance,
    pub stale_penalty: f32,
}

impl HybridScoreInput {
    pub fn score(
        &self,
        weights: &RetrievalWeights,
        provenance_weights: &ProvenanceWeights,
    ) -> RetrievalScoreBreakdown {
        let hybrid_alpha = self.hybrid_alpha.clamp(0.0, 1.0);
        let text_score = self.text_score.clamp(0.0, 1.0);
        let vector_similarity = self.vector_similarity.map(|value| value.clamp(0.0, 1.0));
        let vector_score = vector_similarity.unwrap_or(0.0);
        let importance_score = self.importance_score.clamp(0.0, 1.0);
        let recency_score = self.recency_score.clamp(0.0, 1.0);
        let provenance_score = provenance_weights
            .weight_for(self.provenance)
            .clamp(0.0, 1.0);
        let stale_penalty = self.stale_penalty.clamp(0.0, 1.0);

        let (text_ratio, vector_ratio) = match self.mode {
            RetrievalSearchMode::Hybrid => (1.0 - hybrid_alpha, hybrid_alpha),
            RetrievalSearchMode::FtsOnly => (1.0, 0.0),
        };

        let semantic_score =
            (text_ratio * text_score + vector_ratio * vector_score).clamp(0.0, 1.0);
        let text_contribution = weights.semantic * text_ratio * text_score * stale_penalty;
        let vector_contribution = weights.semantic * vector_ratio * vector_score * stale_penalty;
        let semantic_contribution = text_contribution + vector_contribution;
        let importance_contribution = weights.importance * importance_score * stale_penalty;
        let recency_contribution = weights.recency * recency_score * stale_penalty;
        let provenance_contribution = weights.provenance * provenance_score * stale_penalty;
        let overall_score = (semantic_contribution
            + importance_contribution
            + recency_contribution
            + provenance_contribution)
            .clamp(0.0, 1.0);

        RetrievalScoreBreakdown {
            overall_score,
            hybrid_alpha,
            bm25_score: self.bm25_score,
            text_score,
            text_contribution,
            vector_similarity,
            vector_contribution,
            semantic_score,
            semantic_contribution,
            importance_score,
            importance_contribution,
            recency_score,
            recency_contribution,
            provenance_score,
            provenance_config_weight: weights.provenance,
            provenance_contribution,
            stale_penalty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalWeights {
    pub semantic: f32,
    pub importance: f32,
    pub recency: f32,
    pub provenance: f32,
}

impl RetrievalWeights {
    pub fn sum(&self) -> f32 {
        self.semantic + self.importance + self.recency + self.provenance
    }

    pub fn validate(&self) -> Result<(), WeightValidationError> {
        validate_ratio("semantic", self.semantic)?;
        validate_ratio("importance", self.importance)?;
        validate_ratio("recency", self.recency)?;
        validate_ratio("provenance", self.provenance)?;

        let sum = self.sum();
        if (sum - 1.0).abs() > RETRIEVAL_WEIGHT_EPSILON {
            return Err(WeightValidationError::new(
                "retrieval weights",
                format!("must sum to 1.0 (got {sum:.3})"),
            ));
        }

        Ok(())
    }
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            semantic: 0.35,
            importance: 0.25,
            recency: 0.20,
            provenance: 0.20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalScoreInput {
    pub semantic_similarity: Option<f32>,
    pub importance_score: Option<f32>,
    pub recency_score: f32,
    pub provenance_score: f32,
}

impl RetrievalScoreInput {
    pub fn weighted_score(&self, weights: &RetrievalWeights) -> f32 {
        let semantic = self.semantic_similarity.unwrap_or(0.0).clamp(0.0, 1.0);
        let importance = self.importance_score.unwrap_or(0.5).clamp(0.0, 1.0);
        let recency = self.recency_score.clamp(0.0, 1.0);
        let provenance = self.provenance_score.clamp(0.0, 1.0);

        (weights.semantic * semantic
            + weights.importance * importance
            + weights.recency * recency
            + weights.provenance * provenance)
            .clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceWeights {
    pub user_authored: f32,
    pub character_card: f32,
    pub lorebook: f32,
    pub system_generated: f32,
    pub utility_model: f32,
    pub imported_external: f32,
}

impl ProvenanceWeights {
    pub fn weight_for(&self, provenance: Provenance) -> f32 {
        match provenance {
            Provenance::UserAuthored => self.user_authored,
            Provenance::CharacterCard => self.character_card,
            Provenance::Lorebook => self.lorebook,
            Provenance::SystemGenerated => self.system_generated,
            Provenance::UtilityModel => self.utility_model,
            Provenance::ImportedExternal => self.imported_external,
        }
    }

    pub fn validate(&self) -> Result<(), WeightValidationError> {
        validate_ratio("user_authored", self.user_authored)?;
        validate_ratio("character_card", self.character_card)?;
        validate_ratio("lorebook", self.lorebook)?;
        validate_ratio("system_generated", self.system_generated)?;
        validate_ratio("utility_model", self.utility_model)?;
        validate_ratio("imported_external", self.imported_external)?;
        Ok(())
    }
}

impl Default for ProvenanceWeights {
    fn default() -> Self {
        Self {
            user_authored: 1.0,
            character_card: 0.9,
            lorebook: 0.85,
            system_generated: 0.7,
            utility_model: 0.6,
            imported_external: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightValidationError {
    kind: &'static str,
    reason: String,
}

impl WeightValidationError {
    fn new(kind: &'static str, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for WeightValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.reason)
    }
}

impl Error for WeightValidationError {}

fn validate_ratio(name: &'static str, value: f32) -> Result<(), WeightValidationError> {
    if !value.is_finite() {
        return Err(WeightValidationError::new(name, "must be finite"));
    }

    if !(0.0..=1.0).contains(&value) {
        return Err(WeightValidationError::new(
            name,
            "must be in the range [0.0, 1.0]",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_weights_validate_defaults() {
        let weights = RetrievalWeights::default();
        assert_eq!(weights.sum(), 1.0);
        weights.validate().expect("defaults should be normalized");
    }

    #[test]
    fn retrieval_score_input_clamps_weighted_scores() {
        let weights = RetrievalWeights::default();
        let score = RetrievalScoreInput {
            semantic_similarity: Some(1.5),
            importance_score: Some(-0.5),
            recency_score: 0.5,
            provenance_score: 0.8,
        }
        .weighted_score(&weights);

        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn provenance_weights_map_from_provenance() {
        let weights = ProvenanceWeights::default();
        weights.validate().expect("defaults should be valid");
        assert_eq!(weights.weight_for(Provenance::UserAuthored), 1.0);
        assert_eq!(weights.weight_for(Provenance::UtilityModel), 0.6);
    }

    #[test]
    fn hybrid_score_blends_text_and_vector_contributions() {
        let weights = RetrievalWeights::default();
        let breakdown = HybridScoreInput {
            mode: RetrievalSearchMode::Hybrid,
            hybrid_alpha: 0.25,
            bm25_score: Some(-1.25),
            text_score: 1.0,
            vector_similarity: Some(0.4),
            importance_score: 0.6,
            recency_score: 0.5,
            provenance: Provenance::UserAuthored,
            stale_penalty: 1.0,
        }
        .score(&weights, &ProvenanceWeights::default());

        assert!(breakdown.text_contribution > breakdown.vector_contribution);
        assert!(breakdown.overall_score <= 1.0);
        assert!(
            (breakdown.overall_score
                - (breakdown.semantic_contribution
                    + breakdown.importance_contribution
                    + breakdown.recency_contribution
                    + breakdown.provenance_contribution))
                .abs()
                < 0.0001
        );
    }

    #[test]
    fn fts_only_mode_ignores_vector_signal() {
        let weights = RetrievalWeights::default();
        let breakdown = HybridScoreInput {
            mode: RetrievalSearchMode::FtsOnly,
            hybrid_alpha: 0.75,
            bm25_score: Some(-0.7),
            text_score: 0.4,
            vector_similarity: Some(1.0),
            importance_score: 0.5,
            recency_score: 0.5,
            provenance: Provenance::UserAuthored,
            stale_penalty: 1.0,
        }
        .score(&weights, &ProvenanceWeights::default());

        assert_eq!(breakdown.vector_contribution, 0.0);
        assert!((breakdown.semantic_score - 0.4).abs() < 0.0001);
    }

    #[test]
    fn retrieval_status_summary_mentions_fallback_and_stale_counts() {
        let status = RetrievalStatus {
            mode: RetrievalSearchMode::FtsOnly,
            reason: Some("vector index missing".to_owned()),
            filtered_stale_embeddings: 2,
            downranked_embeddings: 1,
        };

        let summary = status.summary_line();
        assert!(summary.contains("fts_only"));
        assert!(summary.contains("vector index missing"));
        assert!(summary.contains("filtered 2 stale embeddings"));
        assert!(summary.contains("downranked 1 inactive hit"));
    }
}
