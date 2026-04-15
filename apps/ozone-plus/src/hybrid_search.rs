use std::collections::{BTreeMap, BTreeSet};

use ozone_inference::{ConfigLoader, MemoryConfig};
use ozone_memory::{
    artifact_index_key, build_embedding_provider, ArtifactLifecycleSummary, EmbeddingAvailability,
    EmbeddingRecord, EmbeddingRequest, HybridScoreInput, MemoryArtifactId, RetrievalHit,
    RetrievalHitKind, RetrievalResultSet, RetrievalSearchMode, RetrievalSourceState,
    RetrievalStatus, SearchSessionMetadata, VectorIndexManager,
};
use ozone_persist::{
    ConversationMessage, CrossSessionPinnedMemorySearchHit, MessageId, PinnedMemorySearchHit,
    PinnedMemoryView, Provenance, SessionId, SessionRecord, SqliteRepository,
};

use crate::index_rebuild::{
    memory_embedding_artifact_id, message_embedding_artifact_id, message_provenance_for_author_kind,
};

const MESSAGE_IMPORTANCE: f32 = 0.45;
const PINNED_MEMORY_IMPORTANCE: f32 = 0.80;
const NOTE_MEMORY_IMPORTANCE: f32 = 0.85;
const INACTIVE_MEMORY_PENALTY: f32 = 0.65;

pub struct HybridSearchService<'a> {
    repo: &'a SqliteRepository,
    memory: &'a MemoryConfig,
}

impl<'a> HybridSearchService<'a> {
    pub fn new(repo: &'a SqliteRepository, memory: &'a MemoryConfig) -> Self {
        Self { repo, memory }
    }

    pub fn search_session(
        &self,
        session_id: &SessionId,
        query: &str,
    ) -> Result<RetrievalResultSet, String> {
        let session = self
            .repo
            .get_session(session_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("session {session_id} was not found"))?;
        let session_meta = search_session_metadata(&session);
        let fts_hits = self
            .repo
            .search_messages(session_id, query)
            .map_err(|error| error.to_string())?;
        let mut fts_candidates = fts_hits
            .into_iter()
            .map(|hit| -> Result<Candidate, String> {
                let message_id =
                    MessageId::parse(&hit.message_id).map_err(|error| error.to_string())?;
                Ok(Candidate {
                    key: CandidateKey::Message {
                        session_id: session_id.as_str().to_owned(),
                        message_id: hit.message_id.clone(),
                    },
                    session: session_meta.clone(),
                    hit_kind: RetrievalHitKind::Message,
                    artifact_id: None,
                    message_id: Some(message_id),
                    source_message_id: None,
                    author_kind: Some(hit.author_kind.clone()),
                    text: hit.content,
                    created_at: hit.created_at,
                    provenance: message_provenance_for_author_kind(&hit.author_kind),
                    source_state: RetrievalSourceState::Current,
                    is_active_memory: None,
                    lifecycle: None,
                    bm25_score: Some(hit.bm25_score),
                    vector_similarity: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let memory_hits = self
            .repo
            .search_pinned_memories(session_id, query)
            .map_err(|error| error.to_string())?;
        fts_candidates.extend(
            memory_hits
                .into_iter()
                .map(|hit| memory_candidate_from_search_hit(session_meta.clone(), hit)),
        );

        self.build_result(
            query,
            SearchScope::Session {
                session_id: session_id.clone(),
                session: session_meta,
            },
            fts_candidates,
        )
    }

    pub fn search_global(&self, query: &str) -> Result<RetrievalResultSet, String> {
        let mut fts_candidates: Vec<Candidate> = self
            .repo
            .search_across_sessions(query)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|hit| Candidate {
                key: CandidateKey::Message {
                    session_id: hit.session.session_id.as_str().to_owned(),
                    message_id: hit.message_id.as_str().to_owned(),
                },
                session: hit.session,
                hit_kind: RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(hit.message_id),
                source_message_id: None,
                author_kind: Some(hit.author_kind.clone()),
                text: hit.content,
                created_at: hit.created_at,
                provenance: message_provenance_for_author_kind(&hit.author_kind),
                source_state: RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: None,
                bm25_score: Some(hit.bm25_score),
                vector_similarity: None,
            })
            .collect();
        fts_candidates.extend(
            self.repo
                .search_pinned_memories_across_sessions(query)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(memory_candidate_from_cross_session_hit),
        );

        self.build_result(query, SearchScope::Global, fts_candidates)
    }

    pub fn context_retrieval(
        &self,
        session_id: &SessionId,
        transcript: &[ConversationMessage],
        pinned_memories: &[PinnedMemoryView],
        limit: usize,
    ) -> Result<Option<RetrievalResultSet>, String> {
        let Some(query) = latest_user_query(transcript) else {
            return Ok(None);
        };

        let mut result = self.search_session(session_id, &query)?;
        let transcript_message_ids = transcript
            .iter()
            .map(|message| message.message_id.as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let active_memory_embeddings = pinned_memories
            .iter()
            .filter(|memory| memory.is_active)
            .map(|memory| {
                memory_embedding_artifact_id(session_id, memory.record.artifact_id.as_str())
                    .as_str()
                    .to_owned()
            })
            .collect::<BTreeSet<_>>();

        result.hits.retain(|hit| match hit.hit_kind {
            RetrievalHitKind::Message => hit
                .message_id
                .as_ref()
                .map(|message_id| !transcript_message_ids.contains(message_id.as_str()))
                .unwrap_or(true),
            RetrievalHitKind::PinnedMemory | RetrievalHitKind::NoteMemory => hit
                .artifact_id
                .as_ref()
                .map(|artifact_id| !active_memory_embeddings.contains(artifact_id.as_str()))
                .unwrap_or(true),
        });
        if result.hits.len() > limit {
            result.hits.truncate(limit);
        }

        Ok(Some(result))
    }

    fn build_result(
        &self,
        query: &str,
        scope: SearchScope,
        fts_candidates: Vec<Candidate>,
    ) -> Result<RetrievalResultSet, String> {
        let vector_outcome = self.vector_candidates(query, &scope)?;
        let mut candidates = BTreeMap::new();
        for candidate in fts_candidates {
            candidates.insert(candidate.key.clone(), candidate);
        }

        for candidate in vector_outcome.candidates {
            merge_candidate(&mut candidates, candidate);
        }

        let text_scores = normalize_bm25_scores(&candidates);
        let recency_scores = normalize_recency_scores(&candidates);
        let mut hits = candidates
            .into_values()
            .map(|candidate| {
                let text_score = text_scores.get(&candidate.key).copied().unwrap_or(0.0);
                let recency_score = recency_scores.get(&candidate.key).copied().unwrap_or(1.0);
                let score = HybridScoreInput {
                    mode: vector_outcome.status.mode,
                    hybrid_alpha: self.memory.hybrid_alpha,
                    bm25_score: candidate.bm25_score,
                    text_score,
                    vector_similarity: candidate.vector_similarity,
                    importance_score: importance_score(&candidate),
                    recency_score,
                    provenance: candidate.provenance,
                    stale_penalty: stale_penalty(candidate.source_state),
                }
                .score(
                    &self.memory.retrieval_weights,
                    &self.memory.provenance_weights,
                );

                RetrievalHit {
                    session: candidate.session,
                    hit_kind: candidate.hit_kind,
                    artifact_id: candidate.artifact_id,
                    message_id: candidate.message_id,
                    source_message_id: candidate.source_message_id,
                    author_kind: candidate.author_kind,
                    text: candidate.text,
                    created_at: candidate.created_at,
                    provenance: candidate.provenance,
                    source_state: candidate.source_state,
                    is_active_memory: candidate.is_active_memory,
                    lifecycle: candidate.lifecycle,
                    score,
                }
            })
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| {
            right
                .overall_score()
                .total_cmp(&left.overall_score())
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| {
                    left.session
                        .session_id
                        .as_str()
                        .cmp(right.session.session_id.as_str())
                })
                .then_with(|| {
                    left.message_id
                        .as_ref()
                        .map(|message_id| message_id.as_str())
                        .cmp(
                            &right
                                .message_id
                                .as_ref()
                                .map(|message_id| message_id.as_str()),
                        )
                })
                .then_with(|| {
                    left.artifact_id
                        .as_ref()
                        .map(|artifact_id| artifact_id.as_str())
                        .cmp(
                            &right
                                .artifact_id
                                .as_ref()
                                .map(|artifact_id| artifact_id.as_str()),
                        )
                })
        });

        Ok(RetrievalResultSet {
            query: query.to_owned(),
            status: vector_outcome.status,
            hits,
        })
    }

    fn vector_candidates(&self, query: &str, scope: &SearchScope) -> Result<VectorOutcome, String> {
        let provider = build_embedding_provider(self.memory.embedding.clone());
        let provider_metadata = provider.metadata();
        let availability = provider.availability();
        if availability != EmbeddingAvailability::Ready {
            return Ok(VectorOutcome::fallback(fallback_reason_from_availability(
                availability,
            )));
        }

        let query_embedding = match provider.embed(&[EmbeddingRequest::query(query.to_owned())]) {
            Ok(batch) if batch.embeddings.len() == 1 => batch.embeddings[0].content.vector.clone(),
            Ok(batch) => {
                return Ok(VectorOutcome::fallback(format!(
                    "embedding provider returned {} query vectors",
                    batch.embeddings.len()
                )))
            }
            Err(error) => {
                return Ok(VectorOutcome::fallback(format!(
                    "query embedding failed: {error}"
                )))
            }
        };

        let manager = VectorIndexManager::new(self.repo.paths().data_dir().join("vector-index"));
        let metadata = match manager.load_metadata() {
            Ok(Some(metadata)) => metadata,
            Ok(None) => {
                return Ok(VectorOutcome::fallback(
                    "vector index missing (run `ozone-plus index rebuild`)".to_owned(),
                ))
            }
            Err(error) => {
                return Ok(VectorOutcome::fallback(format!(
                    "vector index unavailable: {error}"
                )))
            }
        };

        if metadata.provider_metadata() != provider_metadata {
            return Ok(VectorOutcome::fallback(format!(
                "vector index provider {} / {} / {} did not match active provider {} / {} / {}",
                metadata.provider,
                metadata.model,
                metadata.dimensions,
                provider_metadata.provider,
                provider_metadata.model,
                provider_metadata.dimensions
            )));
        }

        let query_result = match manager.query(&query_embedding, metadata.artifact_count) {
            Ok(Some(query_result)) => query_result,
            Ok(None) => {
                return Ok(VectorOutcome::fallback(
                    "vector index missing (run `ozone-plus index rebuild`)".to_owned(),
                ))
            }
            Err(error) => {
                return Ok(VectorOutcome::fallback(format!(
                    "vector query unavailable: {error}"
                )))
            }
        };

        if query_result.metadata.artifact_count == 0 {
            return Ok(VectorOutcome::fallback("vector index is empty".to_owned()));
        }

        let records = match scope {
            SearchScope::Session { session_id, .. } => {
                match self.repo.list_embedding_artifacts(Some(session_id)) {
                    Ok(records) => records,
                    Err(error) => {
                        return Ok(VectorOutcome::fallback(format!(
                            "embedding artifacts unavailable: {error}"
                        )))
                    }
                }
            }
            SearchScope::Global => match self.repo.list_embedding_artifacts(None) {
                Ok(records) => records,
                Err(error) => {
                    return Ok(VectorOutcome::fallback(format!(
                        "embedding artifacts unavailable: {error}"
                    )))
                }
            },
        };
        let record_map = records
            .into_iter()
            .map(|record| (artifact_index_key(&record.artifact_id), record))
            .collect::<BTreeMap<_, _>>();

        let mut session_cache = BTreeMap::new();
        let mut candidates = Vec::new();
        let mut filtered_stale_embeddings = 0;
        let mut downranked_embeddings = 0;

        for vector_hit in query_result.matches {
            let Some(record) = record_map.get(&vector_hit.key) else {
                continue;
            };
            let Some(candidate) = self.resolve_vector_candidate(
                record,
                vector_hit.similarity,
                scope,
                &mut session_cache,
            )?
            else {
                filtered_stale_embeddings += 1;
                continue;
            };
            if candidate.source_state == RetrievalSourceState::InactiveMemory {
                downranked_embeddings += 1;
            }
            candidates.push(candidate);
        }

        Ok(VectorOutcome {
            status: RetrievalStatus {
                mode: RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings,
                downranked_embeddings,
            },
            candidates,
        })
    }

    fn resolve_vector_candidate(
        &self,
        record: &EmbeddingRecord,
        similarity: f32,
        scope: &SearchScope,
        session_cache: &mut BTreeMap<String, Option<CurrentSessionState>>,
    ) -> Result<Option<Candidate>, String> {
        let session_override = match scope {
            SearchScope::Session {
                session_id,
                session,
            } if session_id == &record.session_id => Some(session.clone()),
            _ => None,
        };
        let Some(session_state) =
            self.current_session_state(&record.session_id, session_override, session_cache)?
        else {
            return Ok(None);
        };
        let current_message_count = u64::try_from(session_state.messages.len()).unwrap_or(u64::MAX);
        let lifecycle = Some(crate::artifact_lifecycle_summary(
            self.memory,
            record.snapshot_version,
            record.created_at,
            current_message_count,
            record.provenance,
        ));

        match record_source_kind(record) {
            RetrievalHitKind::Message => {
                let Some(source_message_id) = record.source_message_id.as_ref() else {
                    return Ok(None);
                };
                let Some(message) = session_state.messages.get(source_message_id.as_str()) else {
                    return Ok(None);
                };
                if !record.matches_source_text(&message.content) {
                    return Ok(None);
                }

                Ok(Some(Candidate {
                    key: CandidateKey::Message {
                        session_id: record.session_id.as_str().to_owned(),
                        message_id: source_message_id.as_str().to_owned(),
                    },
                    session: session_state.session.clone(),
                    hit_kind: RetrievalHitKind::Message,
                    artifact_id: None,
                    message_id: Some(source_message_id.clone()),
                    source_message_id: None,
                    author_kind: Some(message.author_kind.clone()),
                    text: message.content.clone(),
                    created_at: message.edited_at.unwrap_or(message.created_at),
                    provenance: message_provenance_for_author_kind(&message.author_kind),
                    source_state: RetrievalSourceState::Current,
                    is_active_memory: None,
                    lifecycle: lifecycle.clone(),
                    bm25_score: None,
                    vector_similarity: Some(similarity),
                }))
            }
            RetrievalHitKind::PinnedMemory | RetrievalHitKind::NoteMemory => {
                let Some(memory) = session_state.memories.get(record.artifact_id.as_str()) else {
                    return Ok(None);
                };
                if !record.matches_source_text(&memory.record.content.text) {
                    return Ok(None);
                }
                let source_state = if memory.is_active {
                    RetrievalSourceState::Current
                } else {
                    RetrievalSourceState::InactiveMemory
                };

                Ok(Some(Candidate {
                    key: CandidateKey::Memory {
                        session_id: record.session_id.as_str().to_owned(),
                        artifact_id: record.artifact_id.as_str().to_owned(),
                    },
                    session: session_state.session.clone(),
                    hit_kind: record_source_kind(record),
                    artifact_id: Some(record.artifact_id.clone()),
                    message_id: None,
                    source_message_id: memory.record.source_message_id.clone(),
                    author_kind: None,
                    text: memory.record.content.text.clone(),
                    created_at: memory.record.created_at,
                    provenance: memory.record.provenance,
                    source_state,
                    is_active_memory: Some(memory.is_active),
                    lifecycle,
                    bm25_score: None,
                    vector_similarity: Some(similarity),
                }))
            }
        }
    }

    fn current_session_state(
        &self,
        session_id: &SessionId,
        session_override: Option<SearchSessionMetadata>,
        session_cache: &mut BTreeMap<String, Option<CurrentSessionState>>,
    ) -> Result<Option<CurrentSessionState>, String> {
        let cache_key = session_id.as_str().to_owned();
        if let Some(cached) = session_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let session_record = match self
            .repo
            .get_session(session_id)
            .map_err(|error| error.to_string())?
        {
            Some(record) => record,
            None => {
                session_cache.insert(cache_key, None);
                return Ok(None);
            }
        };
        let session = session_override.unwrap_or_else(|| search_session_metadata(&session_record));
        let messages = self
            .repo
            .list_session_messages(session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|message| (message.message_id.as_str().to_owned(), message))
            .collect();
        let memories = self
            .repo
            .list_pinned_memories(session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|memory| {
                (
                    memory_embedding_artifact_id(session_id, memory.record.artifact_id.as_str())
                        .as_str()
                        .to_owned(),
                    memory,
                )
            })
            .collect();
        let state = CurrentSessionState {
            session,
            messages,
            memories,
        };
        session_cache.insert(cache_key, Some(state.clone()));
        Ok(Some(state))
    }
}

pub fn load_memory_config(
    repo: &SqliteRepository,
    session_id: Option<&SessionId>,
) -> Result<MemoryConfig, String> {
    let mut loader = ConfigLoader::new();
    if let Some(session_id) = session_id {
        loader = loader.session_config_path(repo.paths().session_config_path(session_id));
    }

    loader
        .build()
        .map(|config| config.memory)
        .map_err(|error| format!("failed to load ozone+ memory config: {error}"))
}

#[derive(Debug, Clone)]
struct CurrentSessionState {
    session: SearchSessionMetadata,
    messages: BTreeMap<String, ConversationMessage>,
    memories: BTreeMap<String, PinnedMemoryView>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateKey {
    Message {
        session_id: String,
        message_id: String,
    },
    Memory {
        session_id: String,
        artifact_id: String,
    },
}

#[derive(Debug, Clone)]
struct Candidate {
    key: CandidateKey,
    session: SearchSessionMetadata,
    hit_kind: RetrievalHitKind,
    artifact_id: Option<MemoryArtifactId>,
    message_id: Option<MessageId>,
    source_message_id: Option<MessageId>,
    author_kind: Option<String>,
    text: String,
    created_at: i64,
    provenance: Provenance,
    source_state: RetrievalSourceState,
    is_active_memory: Option<bool>,
    lifecycle: Option<ArtifactLifecycleSummary>,
    bm25_score: Option<f32>,
    vector_similarity: Option<f32>,
}

#[derive(Debug, Clone)]
enum SearchScope {
    Session {
        session_id: SessionId,
        session: SearchSessionMetadata,
    },
    Global,
}

struct VectorOutcome {
    status: RetrievalStatus,
    candidates: Vec<Candidate>,
}

impl VectorOutcome {
    fn fallback(reason: String) -> Self {
        Self {
            status: RetrievalStatus {
                mode: RetrievalSearchMode::FtsOnly,
                reason: Some(reason),
                filtered_stale_embeddings: 0,
                downranked_embeddings: 0,
            },
            candidates: Vec::new(),
        }
    }
}

fn search_session_metadata(session: &SessionRecord) -> SearchSessionMetadata {
    SearchSessionMetadata {
        session_id: session.session_id.clone(),
        session_name: session.name.clone(),
        character_name: session.character_name.clone(),
        tags: session.tags.clone(),
    }
}

fn fallback_reason_from_availability(availability: EmbeddingAvailability) -> String {
    match availability {
        EmbeddingAvailability::Ready => "embeddings ready".to_owned(),
        EmbeddingAvailability::Disabled { reason } => reason,
        EmbeddingAvailability::Unavailable { reason } => reason,
    }
}

fn record_source_kind(record: &EmbeddingRecord) -> RetrievalHitKind {
    match record.source_message_id.as_ref() {
        None => RetrievalHitKind::NoteMemory,
        Some(source_message_id)
            if record.artifact_id
                == message_embedding_artifact_id(
                    &record.session_id,
                    source_message_id.as_str(),
                ) =>
        {
            RetrievalHitKind::Message
        }
        Some(_) => RetrievalHitKind::PinnedMemory,
    }
}

fn pinned_memory_source_kind(memory: &PinnedMemoryView) -> RetrievalHitKind {
    match memory.record.source_message_id {
        Some(_) => RetrievalHitKind::PinnedMemory,
        None => RetrievalHitKind::NoteMemory,
    }
}

fn memory_candidate_from_search_hit(
    session: SearchSessionMetadata,
    hit: PinnedMemorySearchHit,
) -> Candidate {
    let source_state = if hit.memory.is_active {
        RetrievalSourceState::Current
    } else {
        RetrievalSourceState::InactiveMemory
    };
    Candidate {
        key: CandidateKey::Memory {
            session_id: hit.memory.record.session_id.as_str().to_owned(),
            artifact_id: hit.memory.record.artifact_id.as_str().to_owned(),
        },
        session,
        hit_kind: pinned_memory_source_kind(&hit.memory),
        artifact_id: Some(hit.memory.record.artifact_id.clone()),
        message_id: None,
        source_message_id: hit.memory.record.source_message_id.clone(),
        author_kind: None,
        text: hit.memory.record.content.text.clone(),
        created_at: hit.memory.record.created_at,
        provenance: hit.memory.record.provenance,
        source_state,
        is_active_memory: Some(hit.memory.is_active),
        lifecycle: None,
        bm25_score: Some(hit.bm25_score),
        vector_similarity: None,
    }
}

fn memory_candidate_from_cross_session_hit(hit: CrossSessionPinnedMemorySearchHit) -> Candidate {
    memory_candidate_from_search_hit(
        hit.session,
        PinnedMemorySearchHit {
            memory: hit.memory,
            bm25_score: hit.bm25_score,
        },
    )
}

fn merge_candidate(candidates: &mut BTreeMap<CandidateKey, Candidate>, candidate: Candidate) {
    if let Some(existing) = candidates.get_mut(&candidate.key) {
        if candidate.vector_similarity.is_some() {
            existing.vector_similarity = candidate.vector_similarity;
            existing.session = candidate.session;
            existing.hit_kind = candidate.hit_kind;
            existing.artifact_id = candidate.artifact_id;
            existing.message_id = candidate.message_id;
            existing.source_message_id = candidate.source_message_id;
            existing.author_kind = candidate.author_kind;
            existing.text = candidate.text;
            existing.created_at = candidate.created_at;
            existing.provenance = candidate.provenance;
            existing.source_state = candidate.source_state;
            existing.is_active_memory = candidate.is_active_memory;
            existing.lifecycle = candidate.lifecycle;
        }
        if existing.bm25_score.is_none() {
            existing.bm25_score = candidate.bm25_score;
        }
        return;
    }

    candidates.insert(candidate.key.clone(), candidate);
}

fn normalize_bm25_scores(
    candidates: &BTreeMap<CandidateKey, Candidate>,
) -> BTreeMap<CandidateKey, f32> {
    let scored = candidates
        .iter()
        .filter_map(|(key, candidate)| candidate.bm25_score.map(|score| (key.clone(), score)))
        .collect::<Vec<_>>();
    if scored.is_empty() {
        return BTreeMap::new();
    }

    let best = scored
        .iter()
        .map(|(_, score)| *score)
        .fold(f32::INFINITY, f32::min);
    let worst = scored
        .iter()
        .map(|(_, score)| *score)
        .fold(f32::NEG_INFINITY, f32::max);

    scored
        .into_iter()
        .map(|(key, score)| {
            let normalized = if (worst - best).abs() < f32::EPSILON {
                1.0
            } else {
                (1.0 - ((score - best) / (worst - best))).clamp(0.0, 1.0)
            };
            (key, normalized)
        })
        .collect()
}

fn normalize_recency_scores(
    candidates: &BTreeMap<CandidateKey, Candidate>,
) -> BTreeMap<CandidateKey, f32> {
    if candidates.is_empty() {
        return BTreeMap::new();
    }

    let newest = candidates
        .values()
        .map(|candidate| candidate.created_at)
        .max()
        .unwrap_or_default();
    let oldest = candidates
        .values()
        .map(|candidate| candidate.created_at)
        .min()
        .unwrap_or_default();
    let span = newest.saturating_sub(oldest);

    candidates
        .iter()
        .map(|(key, candidate)| {
            let normalized = if span == 0 {
                1.0
            } else {
                ((candidate.created_at - oldest) as f32 / span as f32).clamp(0.0, 1.0)
            };
            (key.clone(), normalized)
        })
        .collect()
}

fn importance_score(candidate: &Candidate) -> f32 {
    match candidate.hit_kind {
        RetrievalHitKind::Message => MESSAGE_IMPORTANCE,
        RetrievalHitKind::PinnedMemory => PINNED_MEMORY_IMPORTANCE,
        RetrievalHitKind::NoteMemory => NOTE_MEMORY_IMPORTANCE,
    }
}

fn stale_penalty(state: RetrievalSourceState) -> f32 {
    match state {
        RetrievalSourceState::Current => 1.0,
        RetrievalSourceState::InactiveMemory => INACTIVE_MEMORY_PENALTY,
        RetrievalSourceState::SourceChanged | RetrievalSourceState::SourceMissing => 0.0,
    }
}

fn latest_user_query(transcript: &[ConversationMessage]) -> Option<String> {
    transcript
        .iter()
        .rev()
        .find(|message| message.author_kind.eq_ignore_ascii_case("user"))
        .and_then(|message| {
            let trimmed = message.content.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_rebuild::rebuild_index_with_config;
    use ozone_persist::{
        AuthorId, CreateMessageRequest, CreateNoteMemoryRequest, CreateSessionRequest,
        EditMessageRequest, PersistencePaths, Provenance,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ozone-plus-search-tests")
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
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn mock_config() -> ozone_inference::OzoneConfig {
        ConfigLoader::new()
            .global_config_path("/nonexistent/ozone-plus-hybrid-search.toml")
            .extra_toml_override(
                r#"
[memory.embedding]
provider = "mock"
model = "mock/stable"
expected_dimensions = 8
batch_size = 4
mock_seed = 11
"#,
            )
            .build()
            .unwrap()
    }

    #[test]
    fn hybrid_search_combines_fts_and_vector_scores() {
        let sandbox = TestSandbox::new("hybrid-combines");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Hybrid Search"))
            .unwrap();
        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("The observatory key rests under the blue lamp."),
        )
        .unwrap();
        rebuild_index_with_config(&repo, &mock_config()).unwrap();

        let config = mock_config();
        let service = HybridSearchService::new(&repo, &config.memory);
        let result = service
            .search_session(&session.session_id, "observatory key")
            .unwrap();

        assert_eq!(result.status.mode, RetrievalSearchMode::Hybrid);
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].score.bm25_score.is_some());
        assert!(result.hits[0].score.vector_similarity.is_some());
        let lifecycle = result.hits[0]
            .lifecycle
            .as_ref()
            .expect("vector-backed hits should carry lifecycle metadata");
        assert_eq!(lifecycle.storage_tier, ozone_memory::StorageTier::Full);
        assert_eq!(lifecycle.age_messages, 0);
        assert!((lifecycle.adjusted_provenance_score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn falls_back_to_fts_only_when_embeddings_are_disabled() {
        let sandbox = TestSandbox::new("fts-fallback-disabled");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("FTS Only"))
            .unwrap();
        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::user("The lantern code remains purely lexical."),
        )
        .unwrap();

        let memory = MemoryConfig::default();
        let service = HybridSearchService::new(&repo, &memory);
        let result = service
            .search_session(&session.session_id, "lantern code")
            .unwrap();

        assert_eq!(result.status.mode, RetrievalSearchMode::FtsOnly);
        assert!(result
            .status
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("disabled"));
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].score.vector_contribution, 0.0);
    }

    #[test]
    fn fts_only_search_includes_note_memories() {
        let sandbox = TestSandbox::new("fts-note-memory");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("FTS Note Memory"))
            .unwrap();
        repo.create_note_memory(
            &session.session_id,
            CreateNoteMemoryRequest::new(
                "Remember the observatory dome rendezvous point.",
                AuthorId::User,
                Provenance::UserAuthored,
            ),
        )
        .unwrap();

        let memory = MemoryConfig::default();
        let service = HybridSearchService::new(&repo, &memory);

        let session_result = service
            .search_session(&session.session_id, "observatory dome")
            .unwrap();
        assert_eq!(session_result.status.mode, RetrievalSearchMode::FtsOnly);
        assert!(session_result
            .hits
            .iter()
            .any(|hit| hit.hit_kind == RetrievalHitKind::NoteMemory));

        let global_result = service.search_global("observatory dome").unwrap();
        assert_eq!(global_result.status.mode, RetrievalSearchMode::FtsOnly);
        assert!(global_result
            .hits
            .iter()
            .any(|hit| hit.hit_kind == RetrievalHitKind::NoteMemory));
    }

    #[test]
    fn stale_embeddings_are_filtered_and_inactive_memories_are_downranked() {
        let sandbox = TestSandbox::new("stale-and-inactive");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Stale Search"))
            .unwrap();
        let message = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("The observatory code is 451."),
            )
            .unwrap();
        let message_id = MessageId::parse(&message.message_id).unwrap();
        let mut note = CreateNoteMemoryRequest::new(
            "Remember the observatory code.",
            AuthorId::User,
            Provenance::UserAuthored,
        );
        note.content.expires_after_turns = Some(1);
        repo.create_note_memory(&session.session_id, note).unwrap();
        let config = mock_config();
        rebuild_index_with_config(&repo, &config).unwrap();

        repo.edit_message(
            &session.session_id,
            &message_id,
            EditMessageRequest::new("The lantern code is 88."),
        )
        .unwrap();
        repo.insert_message(
            &session.session_id,
            CreateMessageRequest::new("assistant", "Acknowledged."),
        )
        .unwrap();

        let service = HybridSearchService::new(&repo, &config.memory);
        let result = service
            .search_session(&session.session_id, "observatory code")
            .unwrap();

        assert_eq!(result.status.mode, RetrievalSearchMode::Hybrid);
        assert_eq!(result.status.filtered_stale_embeddings, 1);
        assert_eq!(result.status.downranked_embeddings, 1);
        assert!(result
            .hits
            .iter()
            .all(|hit| hit.message_id.as_ref() != Some(&message_id)));
        assert!(result.hits.iter().any(|hit| {
            hit.hit_kind == RetrievalHitKind::NoteMemory
                && hit.source_state == RetrievalSourceState::InactiveMemory
        }));
    }
}
