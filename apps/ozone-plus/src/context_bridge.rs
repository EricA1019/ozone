use std::fmt::Write as _;

use crate::inference_adapter::{InferenceAdapter, TranscriptRole, TranscriptTurn};
use ozone_core::engine::ConversationMessage;
use ozone_core::session::UnixTimestamp;
use ozone_engine::context::ContextLayerKind;
use ozone_memory::RetrievalResultSet;
use ozone_persist::{AuthorId, PinnedMemoryView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPlanSource {
    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    EnginePlan,
    TranscriptFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTokenBudgetPreview {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPlanPreview {
    pub source: ContextPlanSource,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudgetPreview>,
}

impl ContextPlanPreview {
    fn status_line_preview_text(&self) -> String {
        let origin = match self.source {
            ContextPlanSource::EnginePlan => "engine plan",
            ContextPlanSource::TranscriptFallback => "transcript fallback",
        };
        let mut text = format!("context {origin}: {}", self.summary);
        if let Some(token_budget) = self.token_budget.as_ref() {
            text.push_str(&format!(
                " · {} / {} tokens",
                token_budget.used_tokens, token_budget.max_tokens
            ));
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBuildResult {
    pub prompt: String,
    pub preview: ContextPlanPreview,
    pub is_dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunContextBuild {
    pub built_at: UnixTimestamp,
    pub result: ContextBuildResult,
}

/// Reserved for engine-plan integration path.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineContextPlanOutput {
    pub prompt: String,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudgetPreview>,
    pub is_dry_run: bool,
}

#[derive(Debug, Default)]
pub struct AppContextBridge {
    latest_plan_preview: Option<ContextPlanPreview>,
    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    latest_dry_run: Option<DryRunContextBuild>,
}

impl AppContextBridge {
    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    pub fn latest_plan_preview(&self) -> Option<&ContextPlanPreview> {
        self.latest_plan_preview.as_ref()
    }

    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    pub fn latest_dry_run(&self) -> Option<&DryRunContextBuild> {
        self.latest_dry_run.as_ref()
    }

    pub fn status_line_preview_text(&self) -> String {
        self.latest_plan_preview
            .as_ref()
            .map(ContextPlanPreview::status_line_preview_text)
            .unwrap_or_else(|| "context plan pending".to_string())
    }

    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    pub fn apply_engine_plan_output(
        &mut self,
        output: EngineContextPlanOutput,
    ) -> ContextBuildResult {
        let preview = ContextPlanPreview {
            source: ContextPlanSource::EnginePlan,
            summary: output.summary,
            lines: output.lines,
            selected_items: output.selected_items,
            omitted_items: output.omitted_items,
            token_budget: output.token_budget,
        };
        let result = ContextBuildResult {
            prompt: output.prompt,
            preview: preview.clone(),
            is_dry_run: output.is_dry_run,
        };
        self.latest_plan_preview = Some(preview);
        if result.is_dry_run {
            self.latest_dry_run = Some(DryRunContextBuild {
                built_at: crate::now_timestamp_ms(),
                result: result.clone(),
            });
        }
        result
    }

    pub fn build_from_transcript(
        &mut self,
        transcript: &[ConversationMessage],
        pinned_memories: &[PinnedMemoryView],
        retrieved_memories: Option<&RetrievalResultSet>,
        session_synopsis: Option<&str>,
        inference: &InferenceAdapter,
    ) -> Result<ContextBuildResult, String> {
        self.build_from_transcript_internal(
            transcript,
            pinned_memories,
            retrieved_memories,
            session_synopsis,
            inference,
            false,
        )
    }

    /// Reserved for engine-plan integration path.
    #[allow(dead_code)]
    pub fn dry_run_from_transcript(
        &mut self,
        transcript: &[ConversationMessage],
        pinned_memories: &[PinnedMemoryView],
        retrieved_memories: Option<&RetrievalResultSet>,
        session_synopsis: Option<&str>,
        inference: &InferenceAdapter,
    ) -> Result<DryRunContextBuild, String> {
        let result = self.build_from_transcript_internal(
            transcript,
            pinned_memories,
            retrieved_memories,
            session_synopsis,
            inference,
            true,
        )?;
        let dry_run = DryRunContextBuild {
            built_at: crate::now_timestamp_ms(),
            result,
        };
        self.latest_dry_run = Some(dry_run.clone());
        Ok(dry_run)
    }

    fn build_from_transcript_internal(
        &mut self,
        transcript: &[ConversationMessage],
        pinned_memories: &[PinnedMemoryView],
        retrieved_memories: Option<&RetrievalResultSet>,
        session_synopsis: Option<&str>,
        inference: &InferenceAdapter,
        is_dry_run: bool,
    ) -> Result<ContextBuildResult, String> {
        let active_pinned_memories = pinned_memories
            .iter()
            .filter(|memory| memory.is_active)
            .collect::<Vec<_>>();
        let mut turns = transcript
            .iter()
            .map(|message| {
                TranscriptTurn::from_author_kind(&message.author_kind, message.content.clone())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        inject_active_pinned_memories(&mut turns, &active_pinned_memories);
        inject_retrieved_memories(&mut turns, retrieved_memories);
        if let Some(synopsis) = session_synopsis {
            inject_session_synopsis(&mut turns, synopsis);
        }
        let prompt = inference
            .render_prompt(&turns)
            .map_err(|error| error.to_string())?;

        let preview = ContextPlanPreview {
            source: ContextPlanSource::TranscriptFallback,
            summary: context_preview_summary(
                transcript.len(),
                active_pinned_memories.len(),
                retrieved_memories,
                session_synopsis,
                inference.selected_template(),
            ),
            lines: preview_lines(
                transcript,
                &active_pinned_memories,
                retrieved_memories,
                session_synopsis,
            ),
            selected_items: Some(
                transcript.len()
                    + active_pinned_memories.len()
                    + retrieved_memories
                        .map(|result| result.hits.len())
                        .unwrap_or_default(),
            ),
            omitted_items: Some(0),
            token_budget: Some(ContextTokenBudgetPreview {
                used_tokens: 0,
                max_tokens: u32::try_from(inference.config().context.max_tokens)
                    .unwrap_or(u32::MAX),
            }),
        };

        self.latest_plan_preview = Some(preview.clone());

        Ok(ContextBuildResult {
            prompt,
            preview,
            is_dry_run,
        })
    }
}

fn preview_lines(
    transcript: &[ConversationMessage],
    active_pinned_memories: &[&PinnedMemoryView],
    retrieved_memories: Option<&RetrievalResultSet>,
    session_synopsis: Option<&str>,
) -> Vec<String> {
    let mut lines = pinned_memory_preview_lines(active_pinned_memories);
    lines.extend(retrieved_memory_preview_lines(retrieved_memories));
    lines.extend(session_synopsis_preview_lines(session_synopsis));
    lines.extend(transcript_preview_lines(transcript));
    lines
}

fn transcript_preview_lines(transcript: &[ConversationMessage]) -> Vec<String> {
    const MAX_LINES: usize = 5;
    transcript
        .iter()
        .rev()
        .take(MAX_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| {
            let content = message.content.replace('\n', " ");
            let snippet: String = content.chars().take(80).collect();
            if content.chars().count() > 80 {
                format!("{}: {}…", message.author_kind, snippet)
            } else {
                format!("{}: {}", message.author_kind, snippet)
            }
        })
        .collect()
}

fn pinned_memory_preview_lines(active_pinned_memories: &[&PinnedMemoryView]) -> Vec<String> {
    const MAX_LINES: usize = 3;

    let layer_label = context_layer_label(ContextLayerKind::PinnedMemory);
    let mut lines = active_pinned_memories
        .iter()
        .take(MAX_LINES)
        .map(|memory| {
            format!(
                "{layer_label} · {} · {} · {}",
                memory.record.provenance,
                pinned_memory_source(memory),
                compact_single_line(&memory.record.content.text, 72)
            )
        })
        .collect::<Vec<_>>();

    let omitted = active_pinned_memories.len().saturating_sub(MAX_LINES);
    if omitted > 0 {
        lines.push(format!("{layer_label} · +{omitted} more active"));
    }

    lines
}

fn inject_active_pinned_memories(
    turns: &mut Vec<TranscriptTurn>,
    active_pinned_memories: &[&PinnedMemoryView],
) {
    let Some(memory_block) = pinned_memory_prompt_block(active_pinned_memories) else {
        return;
    };

    append_system_block(turns, memory_block);
}

fn inject_retrieved_memories(
    turns: &mut Vec<TranscriptTurn>,
    retrieved_memories: Option<&RetrievalResultSet>,
) {
    let Some(memory_block) = retrieved_memory_prompt_block(retrieved_memories) else {
        return;
    };

    append_system_block(turns, memory_block);
}

fn inject_session_synopsis(turns: &mut Vec<TranscriptTurn>, synopsis: &str) {
    if synopsis.is_empty() {
        return;
    }
    let layer_label = context_layer_label(ContextLayerKind::SessionSynopsis);
    let synopsis_turn = TranscriptTurn::new(
        TranscriptRole::System,
        format!("[{layer_label}] {synopsis}"),
    );
    // Insert at position 1 (after system prompt if present, before conversation)
    let insert_pos = if turns
        .first()
        .is_some_and(|t| t.role == TranscriptRole::System)
    {
        1
    } else {
        0
    };
    turns.insert(insert_pos, synopsis_turn);
}

fn session_synopsis_preview_lines(session_synopsis: Option<&str>) -> Vec<String> {
    let Some(synopsis) = session_synopsis else {
        return Vec::new();
    };
    if synopsis.is_empty() {
        return Vec::new();
    }
    let layer_label = context_layer_label(ContextLayerKind::SessionSynopsis);
    vec![format!(
        "{layer_label} · {}",
        compact_single_line(synopsis, 72)
    )]
}

fn append_system_block(turns: &mut Vec<TranscriptTurn>, block: String) {
    if let Some(system_turn) = turns
        .iter_mut()
        .find(|turn| turn.role == TranscriptRole::System)
    {
        if system_turn.content.trim().is_empty() {
            system_turn.content = block;
        } else {
            system_turn.content.push_str("\n\n");
            system_turn.content.push_str(&block);
        }
        return;
    }

    turns.insert(0, TranscriptTurn::new(TranscriptRole::System, block));
}

fn pinned_memory_prompt_block(active_pinned_memories: &[&PinnedMemoryView]) -> Option<String> {
    if active_pinned_memories.is_empty() {
        return None;
    }

    let mut block = String::new();
    let layer_label = context_layer_label(ContextLayerKind::PinnedMemory);
    let _ = writeln!(block, "{layer_label} hard context:");

    for memory in active_pinned_memories {
        let _ = writeln!(
            block,
            "- provenance={} pinned_by={} {} {}",
            memory.record.provenance,
            author_id_label(&memory.record.content.pinned_by),
            pinned_memory_source(memory),
            pinned_memory_remaining(memory)
        );
        let _ = writeln!(
            block,
            "  {}",
            compact_single_line(&memory.record.content.text, 240)
        );
    }

    Some(block.trim_end().to_owned())
}

fn context_preview_summary(
    transcript_len: usize,
    active_pinned_count: usize,
    retrieved_memories: Option<&RetrievalResultSet>,
    session_synopsis: Option<&str>,
    template_name: &str,
) -> String {
    let mut summary = format!(
        "{} turn{}",
        transcript_len,
        if transcript_len == 1 { "" } else { "s" }
    );
    if active_pinned_count > 0 {
        summary.push_str(&format!(" + {} active pinned", active_pinned_count));
    }
    if let Some(retrieved) = retrieved_memories {
        summary.push_str(&format!(
            " + {} retrieved ({})",
            retrieved.hits.len(),
            retrieved.status.mode
        ));
    }
    if session_synopsis.is_some_and(|s| !s.is_empty()) {
        summary.push_str(" + synopsis");
    }
    summary.push_str(&format!(" via template {template_name}"));
    summary
}

fn pinned_memory_source(memory: &PinnedMemoryView) -> String {
    match memory.record.source_message_id.as_ref() {
        Some(message_id) => format!("source_message={message_id}"),
        None => "source_message=note".to_owned(),
    }
}

fn pinned_memory_remaining(memory: &PinnedMemoryView) -> String {
    match memory.remaining_turns {
        Some(remaining) => format!("remaining_turns={remaining}"),
        None => "remaining_turns=∞".to_owned(),
    }
}

fn retrieved_memory_preview_lines(retrieved_memories: Option<&RetrievalResultSet>) -> Vec<String> {
    const MAX_LINES: usize = 3;

    let Some(result) = retrieved_memories else {
        return Vec::new();
    };
    let layer_label = context_layer_label(ContextLayerKind::RetrievedMemory);
    let mut lines = vec![format!("{layer_label} · {}", result.status.summary_line())];
    lines.extend(result.hits.iter().take(MAX_LINES).map(|hit| {
        format!(
            "{layer_label} · {} · {} · score={:.2} · {}",
            hit.hit_kind,
            hit.provenance,
            hit.overall_score(),
            compact_single_line(&hit.text, 72)
        )
    }));

    let omitted = result.hits.len().saturating_sub(MAX_LINES);
    if omitted > 0 {
        lines.push(format!("{layer_label} · +{omitted} more retrieved"));
    }

    lines
}

fn retrieved_memory_prompt_block(
    retrieved_memories: Option<&RetrievalResultSet>,
) -> Option<String> {
    let result = retrieved_memories?;
    if result.hits.is_empty() {
        return None;
    }

    let mut block = String::new();
    let layer_label = context_layer_label(ContextLayerKind::RetrievedMemory);
    let _ = writeln!(block, "{layer_label} recall ({}):", result.status.mode);
    for hit in &result.hits {
        let source = hit
            .message_id
            .as_ref()
            .map(|message_id| format!("message={message_id}"))
            .or_else(|| {
                hit.artifact_id
                    .as_ref()
                    .map(|artifact_id| format!("artifact={artifact_id}"))
            })
            .unwrap_or_else(|| "source=unknown".to_owned());
        let _ = writeln!(
            block,
            "- kind={} provenance={} state={} score={:.3} {}",
            hit.hit_kind,
            hit.provenance,
            hit.source_state,
            hit.overall_score(),
            source
        );
        let _ = writeln!(block, "  {}", compact_single_line(&hit.text, 240));
    }

    Some(block.trim_end().to_owned())
}

fn author_id_label(author: &AuthorId) -> &str {
    match author {
        AuthorId::User => "user",
        AuthorId::Character(_) => "character",
        AuthorId::System => "system",
        AuthorId::Narrator => "narrator",
    }
}

fn context_layer_label(kind: ContextLayerKind) -> &'static str {
    match kind {
        ContextLayerKind::PinnedMemory => "pinned_memory",
        ContextLayerKind::SystemPrompt => "system_prompt",
        ContextLayerKind::CharacterCard => "character_card",
        ContextLayerKind::RecentMessages => "recent_messages",
        ContextLayerKind::RetrievedMemory => "retrieved_memory",
        ContextLayerKind::LorebookEntries => "lorebook_entries",
        ContextLayerKind::ThinkingSummary => "thinking_summary",
        ContextLayerKind::SessionSynopsis => "session_synopsis",
    }
}

fn compact_single_line(content: &str, max_chars: usize) -> String {
    let flattened = content.replace('\n', " ");
    let snippet: String = flattened.chars().take(max_chars).collect();
    if flattened.chars().count() > max_chars {
        format!("{snippet}…")
    } else {
        snippet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_core::engine::MessageId;
    use ozone_core::session::SessionId;
    use ozone_persist::{
        AuthorId, MemoryArtifactId, PinnedMemoryContent, PinnedMemoryRecord, PinnedMemoryView,
        Provenance,
    };
    use std::path::PathBuf;

    fn inference_adapter() -> InferenceAdapter {
        InferenceAdapter::load(crate::inference_adapter::InferenceAdapterInit {
            global_config_path: Some(PathBuf::from("/nonexistent/ozone-plus-config.toml")),
            ..Default::default()
        })
        .expect("adapter should load defaults")
    }

    fn message(author_kind: &str, content: &str, ordinal: u8) -> ConversationMessage {
        let session_id =
            SessionId::parse("123e4567-e89b-12d3-a456-426614174000").expect("valid session id");
        let message_id =
            MessageId::parse(format!("123e4567-e89b-12d3-a456-4266141740{:02}", ordinal))
                .expect("valid message id");
        ConversationMessage::new(
            session_id,
            message_id,
            author_kind.to_string(),
            content.to_string(),
            1_700_000_000_000,
        )
    }

    fn pinned_memory(
        text: &str,
        ordinal: u8,
        remaining_turns: Option<u32>,
        is_active: bool,
    ) -> PinnedMemoryView {
        let artifact_id =
            MemoryArtifactId::parse(format!("223e4567-e89b-12d3-a456-4266141740{ordinal:02}"))
                .expect("valid artifact id");
        let record = PinnedMemoryRecord {
            artifact_id,
            session_id: SessionId::parse("323e4567-e89b-12d3-a456-426614174000")
                .expect("valid session id"),
            content: PinnedMemoryContent {
                text: text.to_owned(),
                pinned_by: AuthorId::User,
                expires_after_turns: remaining_turns.map(|remaining| {
                    if is_active {
                        remaining.max(1)
                    } else {
                        1
                    }
                }),
            },
            source_message_id: Some(
                MessageId::parse(format!("423e4567-e89b-12d3-a456-4266141740{ordinal:02}"))
                    .expect("valid message id"),
            ),
            provenance: Provenance::UserAuthored,
            created_at: 1_700_000_000_000 + i64::from(ordinal),
            snapshot_version: 1,
        };

        PinnedMemoryView {
            record,
            turns_elapsed: if is_active { 0 } else { 1 },
            remaining_turns,
            is_active,
        }
    }

    #[test]
    fn transcript_fallback_build_updates_preview_and_status_text() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![
            message("system", "You are concise.", 1),
            message("user", "Hello", 2),
        ];
        let result = bridge
            .build_from_transcript(&transcript, &[], None, None, &adapter)
            .expect("fallback build should succeed");

        assert!(!result.prompt.is_empty());
        assert!(!result.is_dry_run);
        assert_eq!(result.preview.source, ContextPlanSource::TranscriptFallback);
        assert!(bridge
            .status_line_preview_text()
            .contains("context transcript fallback:"));
        assert!(bridge.latest_plan_preview().is_some());
    }

    #[test]
    fn applies_engine_plan_and_tracks_dry_run() {
        let mut bridge = AppContextBridge::default();
        let result = bridge.apply_engine_plan_output(EngineContextPlanOutput {
            prompt: "<prompt>".to_string(),
            summary: "selected 9, omitted 2".to_string(),
            lines: vec!["layer: recency".to_string()],
            selected_items: Some(9),
            omitted_items: Some(2),
            token_budget: Some(ContextTokenBudgetPreview {
                used_tokens: 900,
                max_tokens: 2048,
            }),
            is_dry_run: true,
        });

        assert_eq!(result.preview.source, ContextPlanSource::EnginePlan);
        assert!(bridge
            .status_line_preview_text()
            .contains("selected 9, omitted 2"));
        assert!(bridge.latest_dry_run().is_some());
    }

    #[test]
    fn active_pinned_memories_are_included_but_expired_memories_are_excluded() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![message("user", "Hello", 1)];
        let pinned = vec![
            pinned_memory("Remember the observatory key.", 1, Some(3), true),
            pinned_memory("Expired fallback from last scene.", 2, Some(0), false),
        ];

        let result = bridge
            .build_from_transcript(&transcript, &pinned, None, None, &adapter)
            .expect("fallback build should succeed");

        assert!(result.prompt.contains("Remember the observatory key."));
        assert!(!result.prompt.contains("Expired fallback from last scene."));
        assert!(result.preview.summary.contains("active pinned"));
        assert!(result
            .preview
            .lines
            .iter()
            .any(|line| line.contains("Remember the observatory key.")));
        assert!(!result
            .preview
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback from last scene.")));
    }

    #[test]
    fn retrieved_memories_are_previewed_and_injected_into_context() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![message("user", "Where did I stash the lens?", 1)];
        let retrieved = ozone_memory::RetrievalResultSet {
            query: "stash lens".to_owned(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 0,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("523e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Retrieved".to_owned(),
                    character_name: None,
                    tags: vec!["recall".to_owned()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::NoteMemory,
                artifact_id: Some(
                    MemoryArtifactId::parse("623e4567-e89b-12d3-a456-426614174000").unwrap(),
                ),
                message_id: None,
                source_message_id: None,
                author_kind: None,
                text: "Pack the spare lens before leaving camp.".to_owned(),
                created_at: 1_700_000_000_100,
                provenance: Provenance::UserAuthored,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: Some(false),
                lifecycle: None,
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: None,
                    text_score: 0.0,
                    vector_similarity: Some(0.9),
                    importance_score: 0.85,
                    recency_score: 0.8,
                    provenance: Provenance::UserAuthored,
                    stale_penalty: 1.0,
                }
                .score(
                    &ozone_memory::RetrievalWeights::default(),
                    &ozone_memory::ProvenanceWeights::default(),
                ),
            }],
        };

        let result = bridge
            .build_from_transcript(&transcript, &[], Some(&retrieved), None, &adapter)
            .expect("context build should succeed");

        assert!(result.prompt.contains("retrieved_memory recall"));
        assert!(result
            .prompt
            .contains("Pack the spare lens before leaving camp."));
        assert!(result.preview.summary.contains("1 retrieved (hybrid)"));
        assert!(result
            .preview
            .lines
            .iter()
            .any(|line| line.contains("retrieved_memory · hybrid")));
    }

    #[test]
    fn retrieved_memory_preview_surfaces_fts_fallback_status() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![message("user", "What was the old code?", 1)];
        let retrieved = ozone_memory::RetrievalResultSet {
            query: "old code".to_owned(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::FtsOnly,
                reason: Some("vector index missing".to_owned()),
                filtered_stale_embeddings: 0,
                downranked_embeddings: 0,
            },
            hits: Vec::new(),
        };

        let result = bridge
            .build_from_transcript(&transcript, &[], Some(&retrieved), None, &adapter)
            .expect("context build should succeed");

        assert!(result.preview.summary.contains("0 retrieved (fts_only)"));
        assert!(result
            .preview
            .lines
            .iter()
            .any(|line| line.contains("vector index missing")));
        assert!(!result.prompt.contains("retrieved_memory recall"));
    }

    #[test]
    fn synopsis_injection_adds_system_turn() {
        let mut turns = vec![
            TranscriptTurn::new(TranscriptRole::System, "You are helpful."),
            TranscriptTurn::new(TranscriptRole::User, "Hello"),
        ];
        inject_session_synopsis(&mut turns, "Previously discussed topic X.");
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[1].role, TranscriptRole::System);
        assert!(turns[1].content.contains("session_synopsis"));
        assert!(turns[1].content.contains("topic X"));
    }

    #[test]
    fn empty_synopsis_is_not_injected() {
        let mut turns = vec![TranscriptTurn::new(TranscriptRole::User, "Hello")];
        inject_session_synopsis(&mut turns, "");
        assert_eq!(turns.len(), 1);
    }

    #[test]
    fn synopsis_inserts_at_position_zero_when_no_system_prompt() {
        let mut turns = vec![TranscriptTurn::new(TranscriptRole::User, "Hello")];
        inject_session_synopsis(&mut turns, "Context from earlier.");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, TranscriptRole::System);
        assert!(turns[0].content.contains("session_synopsis"));
    }

    #[test]
    fn synopsis_appears_in_preview_summary_and_lines() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![message("user", "Hello", 1)];

        let result = bridge
            .build_from_transcript(&transcript, &[], None, Some("Recap of events"), &adapter)
            .expect("build should succeed");

        assert!(result.preview.summary.contains("+ synopsis"));
        assert!(result
            .preview
            .lines
            .iter()
            .any(|line| line.contains("session_synopsis") && line.contains("Recap of events")));
        assert!(result.prompt.contains("session_synopsis"));
        assert!(result.prompt.contains("Recap of events"));
    }
}
