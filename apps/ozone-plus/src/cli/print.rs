use crate::cli::util::now_timestamp_ms;
use ozone_engine::{ConversationBranchRecord, SwipeGroupSnapshot};
use ozone_inference::MemoryConfig;
use ozone_memory::{ArtifactLifecycleSummary, StorageTier, StorageTierPolicy};
use ozone_persist::{
    AuthorId, BranchRecord, GarbageCollectionOutcome, GarbageCollectionPlan,
    GarbageCollectionReason, PersistencePaths, PinnedMemoryView,
    Provenance, SessionId, SessionSummary, TranscriptExport,
};
use ozone_core::engine::ConversationMessage;
use std::fmt::Write as _;
use std::io::Write;

pub fn print_session_details(session: &SessionSummary) {
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

pub fn print_branch_record(record: &BranchRecord, include_description: bool) {
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

pub fn print_branch_record_from_engine(record: &ConversationBranchRecord, include_description: bool) {
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

pub fn print_transcript(messages: &[ConversationMessage]) {
    if messages.is_empty() {
        println!("  no messages yet");
        return;
    }

    for message in messages {
        print_message(message);
        println!();
    }
}

pub fn print_message(message: &ConversationMessage) {
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

pub fn print_pinned_memory_view(memory: &PinnedMemoryView) {
    println!("  artifact id     {}", memory.record.artifact_id);
    println!("  kind            {}", saved_memory_kind(memory));
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

pub fn saved_memory_kind(memory: &PinnedMemoryView) -> &'static str {
    if memory.record.source_message_id.is_some() {
        "pinned memory"
    } else {
        "note memory"
    }
}

pub fn format_search_report(
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

pub fn format_search_status(status: &ozone_memory::RetrievalStatus) -> String {
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

pub fn format_search_hit(hit: &ozone_memory::RetrievalHit, include_session_details: bool) -> String {
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

pub fn artifact_lifecycle_summary(
    memory: &MemoryConfig,
    snapshot_version: u64,
    created_at: i64,
    current_message_count: u64,
    provenance: Provenance,
) -> ArtifactLifecycleSummary {
    let storage_tiers = StorageTierPolicy::new(
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

    ArtifactLifecycleSummary {
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

pub fn pinned_memory_lifecycle_summary(
    memory: &MemoryConfig,
    pinned_memory: &PinnedMemoryView,
) -> ArtifactLifecycleSummary {
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

pub fn lifecycle_badges(
    lifecycle: &ArtifactLifecycleSummary,
    include_full_tier: bool,
    include_provenance: bool,
) -> Vec<String> {
    let mut badges = Vec::new();
    if include_full_tier
        || lifecycle.storage_tier != StorageTier::Full
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

pub fn lifecycle_detail_line(lifecycle: &ArtifactLifecycleSummary) -> String {
    let mut parts = lifecycle_badges(lifecycle, true, true);
    parts.push(format!(
        "age {} msg/{}h",
        lifecycle.age_messages, lifecycle.age_hours
    ));
    parts.join(" · ")
}

pub fn print_swipe_group_snapshot(snapshot: &SwipeGroupSnapshot) {
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

pub fn print_session_paths(paths: &PersistencePaths, session_id: &SessionId) {
    print_resolved_path("data dir", paths.data_dir());
    print_resolved_path("global db", paths.global_db_path());
    print_resolved_path("sessions dir", paths.sessions_dir());
    print_resolved_path("session dir", paths.session_dir(session_id));
    print_resolved_path("session db", paths.session_db_path(session_id));
    print_resolved_path("config", paths.session_config_path(session_id));
    print_resolved_path("draft", paths.session_draft_path(session_id));
}

pub fn print_optional_path(label: &str, path: Option<std::path::PathBuf>) {
    match path {
        Some(path) => println!("  {label:<13} {}", path.display()),
        None => println!("  {label:<13} unavailable on this machine"),
    }
}

pub fn print_resolved_path(label: &str, path: impl AsRef<std::path::Path>) {
    println!("  {label:<13} {}", path.as_ref().display());
}

pub fn print_gc_plan(plan: &GarbageCollectionPlan) {
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

pub fn print_gc_outcome(outcome: &GarbageCollectionOutcome) {
    println!();
    println!("GC applied");
    println!("  deleted         {}", outcome.deleted_count);
    for (session_id, ids) in &outcome.deleted_artifact_ids {
        println!("  session {session_id}  {} artifact(s)", ids.len());
    }
}

pub fn reason_label(reason: GarbageCollectionReason) -> &'static str {
    match reason {
        GarbageCollectionReason::OrphanedSource => "orphaned_source",
        GarbageCollectionReason::MinimalTier => "minimal_tier",
        GarbageCollectionReason::SupersededSynopsis => "superseded_synopsis",
        GarbageCollectionReason::OverEmbeddingLimit => "over_embedding_limit",
    }
}

pub fn render_transcript_text(export: &TranscriptExport) -> String {
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

pub fn format_timestamp(timestamp: i64) -> String {
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

pub fn format_timestamp_short(timestamp: i64) -> String {
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

pub fn format_message_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone, Utc};
    let secs = timestamp / 1000;
    let Some(dt) = Utc.timestamp_opt(secs, 0).single() else {
        return String::new();
    };
    let local = dt.with_timezone(&Local);
    local.format("%-I:%M %p").to_string()
}

pub fn format_author_id(author: &AuthorId) -> String {
    match author {
        AuthorId::User => "user".to_owned(),
        AuthorId::Character(name) => format!("character:{name}"),
        AuthorId::System => "system".to_owned(),
        AuthorId::Narrator => "narrator".to_owned(),
    }
}

pub fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "—".to_owned()
    } else {
        tags.join(", ")
    }
}

pub fn write_output_file(path: &std::path::Path, contents: &str) -> Result<std::path::PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("output path must not be empty".to_owned());
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }

    let mut file = std::fs::OpenOptions::new()
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

// Re-export for internal use
pub use chrono;