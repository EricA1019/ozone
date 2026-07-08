//! Persistence and data-format helpers for MCP tools.
//!
//! ID parsing, preferences normalization, JSON conversion
//! for session, branch, message, and memory records.
//!
//! Most functions are gated behind `#[cfg(feature = "legacy-tools")]`
//! since they depend on `ozone_persist` types.

#[cfg(feature = "legacy-tools")]
use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use ozone_core::{
    engine::{BranchId, ConversationMessage, MessageId, SwipeCandidate, SwipeGroup, SwipeGroupId},
    session::SessionId,
};
#[cfg(feature = "legacy-tools")]
use ozone_persist::{
    BranchRecord, PersistError, PinnedMemoryView, SqliteRepository,
    SessionSummary, TranscriptExport,
};
use serde_json::{json, Value};

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn parse_session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn parse_branch_id(value: &str) -> Result<BranchId> {
    BranchId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn parse_message_id(value: &str) -> Result<MessageId> {
    MessageId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn parse_swipe_group_id(value: &str) -> Result<SwipeGroupId> {
    SwipeGroupId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn now_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn parse_prefixed_field(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .map(ToOwned::to_owned)
    })
}

pub(crate) fn normalize_preferences_json(value: &Value) -> Value {
    normalize_preferences_json_for_key(None, value)
}

pub(crate) fn normalize_preferences_json_for_key(key: Option<&str>, value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(nested_key, nested_value)| {
                    let normalized_key = normalize_preferences_key(nested_key);
                    (
                        normalized_key.clone(),
                        normalize_preferences_json_for_key(Some(&normalized_key), nested_value),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| normalize_preferences_json_for_key(key, item))
                .collect(),
        ),
        Value::String(text) if should_normalize_preferences_enum_value(key) => {
            Value::String(normalize_preferences_enum_value(text))
        }
        other => other.clone(),
    }
}

pub(crate) fn normalize_preferences_key(key: &str) -> String {
    let mut normalized = String::with_capacity(key.len() + 4);
    for ch in key.chars() {
        if ch == '-' {
            normalized.push('_');
        } else if ch.is_ascii_uppercase() {
            if !normalized.is_empty() {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

pub(crate) fn should_normalize_preferences_enum_value(key: Option<&str>) -> bool {
    matches!(
        key,
        Some("preferred_backend" | "preferred_frontend" | "preferred_tier")
    )
}

pub(crate) fn normalize_preferences_enum_value(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len() + 4);
    let chars: Vec<char> = value.chars().collect();
    for (index, ch) in chars.iter().copied().enumerate() {
        if matches!(ch, '-' | '_' | ' ') {
            if !normalized.ends_with('-') {
                normalized.push('-');
            }
            continue;
        }

        if ch.is_ascii_uppercase() {
            let prev = index
                .checked_sub(1)
                .and_then(|prev_index| chars.get(prev_index));
            let next = chars.get(index + 1);
            let should_insert_separator = index > 0
                && !normalized.ends_with('-')
                && prev.is_some_and(|prev| prev.is_ascii_lowercase() || prev.is_ascii_digit())
                || index > 0
                    && !normalized.ends_with('-')
                    && prev.is_some_and(|prev| prev.is_ascii_uppercase())
                    && next.is_some_and(|next| next.is_ascii_lowercase());
            if should_insert_separator {
                normalized.push('-');
            }
            normalized.push(ch.to_ascii_lowercase());
            continue;
        }

        normalized.push(ch.to_ascii_lowercase());
    }
    normalized.trim_matches('-').to_owned()
}

pub fn default_preferences_json() -> Value {
    json!({
        "version": 1,
        "last_model_name": "",
        "last_context_size": null,
        "last_gpu_layers": null,
        "last_quant_kv": null,
        "last_threads": null,
        "last_blas_threads": null,
        "no_browser": false,
        "preferred_backend": null,
        "preferred_frontend": null,
        "preferred_tier": null,
        "side_by_side_monitor": false,
        "llamacpp_gpu_layers": null,
        "llamacpp_context_size": null,
        "llamacpp_threads": null,
        "theme_preset": "dark-mint",
        "show_inspector": false,
        "timestamp_style": "relative",
        "message_density": "comfortable"
    })
}

pub fn merge_json_objects(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let merged_value = match base_map.remove(&key) {
                    Some(base_value) => merge_json_objects(base_value, overlay_value),
                    None => overlay_value,
                };
                base_map.insert(key, merged_value);
            }
            Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

pub fn sanitize_prefix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

#[cfg(feature = "legacy-tools")]
pub fn probe_session_lock(repo: &SqliteRepository, session_id: &SessionId) -> Result<Value> {
    let instance_id = format!("ozone-mcp-{}", Uuid::new_v4().simple());
    match repo.acquire_session_lock(session_id, &instance_id) {
        Ok(lock) => {
            let released = repo.release_session_lock(session_id, &instance_id)?;
            Ok(json!({
                "status": "available",
                "instanceId": lock.instance_id,
                "acquiredAt": lock.acquired_at,
                "heartbeatAt": lock.heartbeat_at,
                "released": released
            }))
        }
        Err(PersistError::SessionLocked {
            instance_id,
            acquired_at,
        }) => Ok(json!({
            "status": "locked",
            "instanceId": instance_id,
            "acquiredAt": acquired_at
        })),
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

#[cfg(feature = "legacy-tools")]
pub fn session_summary_json(session: &ozone_persist::SessionSummary) -> Value {
    json!({
        "sessionId": session.session_id,
        "name": session.name,
        "characterName": session.character_name,
        "createdAt": session.created_at,
        "lastOpenedAt": session.last_opened_at,
        "messageCount": session.message_count,
        "dbSizeBytes": session.db_size_bytes,
        "tags": session.tags,
        "lastMessageId": serde_json::Value::Null
    })
}

#[cfg(feature = "legacy-tools")]
pub fn branch_record_json(record: &BranchRecord) -> Value {
    json!({
        "branchId": record.branch.branch_id,
        "sessionId": record.branch.session_id,
        "name": record.branch.name,
        "state": record.branch.state.as_str(),
        "tipMessageId": record.branch.tip_message_id,
        "forkedFromMessageId": record.forked_from,
        "createdAt": record.branch.created_at,
        "description": record.branch.description
    })
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn message_json(message: &ConversationMessage) -> Value {
    json!({
        "messageId": message.message_id,
        "sessionId": message.session_id,
        "parentId": message.parent_id,
        "authorKind": message.author_kind,
        "authorName": message.author_name,
        "content": message.content,
        "createdAt": message.created_at,
        "editedAt": message.edited_at,
        "isHidden": message.is_hidden
    })
}

#[cfg(feature = "legacy-tools")]
pub fn pinned_memory_record_json(record: &ozone_persist::PinnedMemoryRecord) -> Value {
    json!({
        "artifactId": record.artifact_id,
        "sessionId": record.session_id,
        "sourceMessageId": record.source_message_id,
        "provenance": record.provenance.as_str(),
        "createdAt": record.created_at,
        "snapshotVersion": record.snapshot_version,
        "text": record.content.text,
        "pinnedBy": record.content.pinned_by,
        "expiresAfterTurns": record.content.expires_after_turns
    })
}

#[cfg(feature = "legacy-tools")]
pub fn pinned_memory_view_json(view: &PinnedMemoryView) -> Value {
    json!({
        "record": pinned_memory_record_json(&view.record),
        "isActive": view.is_active,
        "turnsElapsed": view.turns_elapsed,
        "remainingTurns": view.remaining_turns
    })
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn swipe_group_json(group: &SwipeGroup) -> Value {
    json!({
        "swipeGroupId": group.swipe_group_id,
        "parentMessageId": group.parent_message_id,
        "parentContextMessageId": group.parent_context_message_id,
        "activeOrdinal": group.active_ordinal
    })
}

#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn swipe_candidate_json(candidate: &SwipeCandidate) -> Value {
    json!({
        "swipeGroupId": candidate.swipe_group_id,
        "ordinal": candidate.ordinal,
        "messageId": candidate.message_id,
        "state": candidate.state.as_str(),
        "partialContent": candidate.partial_content,
        "tokensGenerated": candidate.tokens_generated
    })
}

#[cfg(feature = "legacy-tools")]
pub fn render_transcript_text(export: &ozone_persist::TranscriptExport) -> String {
    let mut lines = vec![
        "ozone+ transcript export".to_owned(),
        format!("session id: {}", export.session.session_id),
        format!("session name: {}", export.session.name),
    ];
    if let Some(branch) = &export.branch {
        lines.push(format!("branch id: {}", branch.branch_id));
        lines.push(format!("branch name: {}", branch.name));
    }
    lines.push(String::new());
    for message in &export.messages {
        let author = message
            .author_name
            .as_deref()
            .unwrap_or(&message.author_kind);
        lines.push(format!("[{}] {}", author, message.content));
        lines.push(String::new());
    }
    lines.join("\n")
}

