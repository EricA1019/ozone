use std::path::PathBuf;

use ozone_engine::ConversationBranchRecord;
use ozone_inference::MemoryConfig;
use ozone_memory::RetrievalResultSet;
use ozone_persist::PinnedMemoryView;
use ozone_tui::{
    BranchItem as TuiBranchItem, ContextDryRunPreview as TuiContextDryRunPreview,
    ContextPreview as TuiContextPreview, ContextTokenBudget as TuiContextTokenBudget,
    RecallBrowser as TuiRecallBrowser, TranscriptItem as TuiTranscriptItem,
};

use super::RecentSearchSection;
use crate::{
    cli::print::lifecycle_badges,
    context_bridge::{ContextPlanPreview, DryRunContextBuild},
};
use ozone_core::engine::{BranchState, ConversationMessage};

pub(super) fn tui_branch_from_record(record: ConversationBranchRecord) -> TuiBranchItem {
    TuiBranchItem::new(
        record.branch.branch_id.to_string(),
        record.branch.name,
        record.branch.state == BranchState::Active,
    )
}

pub(super) fn tui_transcript_item_from_message(
    message: ConversationMessage,
    is_bookmarked: bool,
) -> TuiTranscriptItem {
    let author = message
        .author_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| message.author_kind.clone());

    TuiTranscriptItem::persisted(
        message.message_id.to_string(),
        author,
        message.content,
        is_bookmarked,
    )
    .with_timestamp(crate::format_message_time(message.created_at))
}

pub(super) fn tui_context_preview_from_plan(preview: &ContextPlanPreview) -> TuiContextPreview {
    let source = match preview.source {
        crate::context_bridge::ContextPlanSource::EnginePlan => "engine-plan",
        crate::context_bridge::ContextPlanSource::TranscriptFallback => "transcript-fallback",
    };

    let mut inline_status = format!("{} · {}", source, preview.summary);
    if let Some(token_budget) = preview.token_budget.as_ref() {
        inline_status.push_str(&format!(
            " · {} / {} tokens",
            token_budget.used_tokens, token_budget.max_tokens
        ));
    }

    TuiContextPreview {
        source: source.to_string(),
        summary: preview.summary.clone(),
        lines: preview.lines.clone(),
        selected_items: preview.selected_items,
        omitted_items: preview.omitted_items,
        token_budget: preview
            .token_budget
            .as_ref()
            .map(|budget| TuiContextTokenBudget {
                used_tokens: budget.used_tokens,
                max_tokens: budget.max_tokens,
            }),
        inline_status,
    }
}

pub(super) fn tui_context_dry_run_from_build(
    dry_run: &DryRunContextBuild,
) -> TuiContextDryRunPreview {
    TuiContextDryRunPreview {
        summary: dry_run.result.preview.summary.clone(),
        built_at: dry_run.built_at,
    }
}

pub(super) fn tui_recall_browser_from_state(
    pinned_memories: &[PinnedMemoryView],
    recent_search: Option<&RecentSearchSection>,
    memory: &MemoryConfig,
) -> TuiRecallBrowser {
    const MAX_SECTION_LINES: usize = 5;

    let active = pinned_memories
        .iter()
        .filter(|memory| memory.is_active)
        .collect::<Vec<_>>();
    let expired = pinned_memories
        .iter()
        .filter(|memory| memory.is_expired())
        .collect::<Vec<_>>();

    let mut lines = vec![format!("active memories {}", active.len())];
    if active.is_empty() {
        lines.push("— none".into());
    } else {
        lines.extend(
            active
                .iter()
                .take(MAX_SECTION_LINES)
                .map(|pinned_memory| format_pinned_memory_browser_line(pinned_memory, memory)),
        );
        let omitted = active.len().saturating_sub(MAX_SECTION_LINES);
        if omitted > 0 {
            lines.push(format!("+{omitted} more active memories"));
        }
    }

    if !expired.is_empty() {
        lines.push(format!("expired memories {}", expired.len()));
        lines.extend(
            expired
                .iter()
                .take(MAX_SECTION_LINES)
                .map(|pinned_memory| format_pinned_memory_browser_line(pinned_memory, memory)),
        );
        let omitted = expired.len().saturating_sub(MAX_SECTION_LINES);
        if omitted > 0 {
            lines.push(format!("+{omitted} more expired memories"));
        }
    }

    if let Some(search) = recent_search {
        lines.push(search.summary.clone());
        if search.lines.is_empty() {
            lines.push("— none".into());
        } else {
            lines.extend(search.lines.iter().take(MAX_SECTION_LINES).cloned());
            let omitted = search.lines.len().saturating_sub(MAX_SECTION_LINES);
            if omitted > 0 {
                lines.push(format!("+{omitted} more search hits"));
            }
        }
    }

    let mut summary_parts = vec![format!("{} active", active.len())];
    if !expired.is_empty() {
        summary_parts.push(format!("{} expired", expired.len()));
    }
    if let Some(search) = recent_search {
        summary_parts.push(format!(
            "{} recent hit{}",
            search.hit_count,
            hit_suffix(search.hit_count)
        ));
    }

    TuiRecallBrowser {
        title: "Recall".into(),
        summary: summary_parts.join(" · "),
        lines,
    }
}

pub(super) fn recent_search_section(
    scope: &str,
    result: &RetrievalResultSet,
    include_session: bool,
) -> RecentSearchSection {
    RecentSearchSection {
        summary: format!(
            "{scope} search \"{}\" · {} · {} hit{}",
            result.query,
            result.status.summary_line(),
            result.hits.len(),
            hit_suffix(result.hits.len())
        ),
        hit_count: result.hits.len(),
        lines: result
            .hits
            .iter()
            .map(|hit| format_retrieval_browser_line(hit, include_session))
            .collect(),
    }
}

fn format_retrieval_browser_line(
    hit: &ozone_memory::RetrievalHit,
    include_session: bool,
) -> String {
    let target = match hit.hit_kind {
        ozone_memory::RetrievalHitKind::Message => format!(
            "msg {}",
            hit.message_id
                .as_ref()
                .map(|message_id| short_id(message_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
        ozone_memory::RetrievalHitKind::PinnedMemory => format!(
            "memory {}",
            hit.artifact_id
                .as_ref()
                .map(|artifact_id| short_id(artifact_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
        ozone_memory::RetrievalHitKind::NoteMemory => format!(
            "note {}",
            hit.artifact_id
                .as_ref()
                .map(|artifact_id| short_id(artifact_id.as_str()))
                .unwrap_or_else(|| "—".to_owned())
        ),
    };
    let session_label = if include_session {
        match hit.session.character_name.as_deref() {
            Some(character_name) if !character_name.is_empty() => format!(
                "{} [{}] / {} · ",
                hit.session.session_name,
                character_name,
                short_id(hit.session.session_id.as_str())
            ),
            _ => format!(
                "{} / {} · ",
                hit.session.session_name,
                short_id(hit.session.session_id.as_str())
            ),
        }
    } else {
        String::new()
    };
    let actor = hit
        .author_kind
        .clone()
        .unwrap_or_else(|| hit.provenance.to_string());
    let state = if hit.source_state == ozone_memory::RetrievalSourceState::Current {
        String::new()
    } else {
        format!(" · {}", hit.source_state)
    };
    let lifecycle = hit
        .lifecycle
        .as_ref()
        .map(|lifecycle| lifecycle_badges(lifecycle, true, true))
        .filter(|badges| !badges.is_empty())
        .map(|badges| format!(" · {}", badges.join(" · ")))
        .unwrap_or_default();

    format!(
        "{}{} · s={:.2} t={:.2} v={:.2} p={:.2} · {}{}{} · {}",
        session_label,
        target,
        hit.overall_score(),
        hit.score.text_contribution,
        hit.score.vector_contribution,
        hit.score.provenance_contribution,
        actor,
        state,
        lifecycle,
        compact_line(&hit.text, 56)
    )
}

fn format_pinned_memory_browser_line(
    pinned_memory: &PinnedMemoryView,
    memory_config: &MemoryConfig,
) -> String {
    let source = pinned_memory
        .record
        .source_message_id
        .as_ref()
        .map(|message_id| format!("src {}", short_id(message_id.as_str())))
        .unwrap_or_else(|| "note".to_owned());
    let expiry = match pinned_memory.remaining_turns {
        Some(remaining) if pinned_memory.is_active => format!("{remaining} turns left"),
        Some(_) => "expired".to_owned(),
        None => "no expiry".to_owned(),
    };
    let lifecycle = crate::cli::print::pinned_memory_lifecycle_summary(memory_config, pinned_memory);
    let lifecycle = lifecycle_badges(&lifecycle, false, false);
    let lifecycle = if lifecycle.is_empty() {
        String::new()
    } else {
        format!(" · {}", lifecycle.join(" · "))
    };

    format!(
        "{} · {} · {} · {}{} · {}",
        short_id(pinned_memory.record.artifact_id.as_str()),
        pinned_memory.record.provenance,
        source,
        expiry,
        lifecycle,
        compact_line(&pinned_memory.record.content.text, 72)
    )
}

fn compact_line(content: &str, max_chars: usize) -> String {
    let flattened = content.replace('\n', " ");
    let snippet: String = flattened.chars().take(max_chars).collect();
    if flattened.chars().count() > max_chars {
        format!("{snippet}…")
    } else {
        snippet
    }
}

fn short_id(value: &str) -> String {
    let snippet: String = value.chars().take(8).collect();
    if value.chars().count() > 8 {
        format!("{snippet}…")
    } else {
        snippet
    }
}

fn hit_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

pub(super) fn repository_template_dir() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join("templates");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}