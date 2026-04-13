use ozone_core::session::UnixTimestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{PersistError, Result};

pub const CHARACTER_CARD_FORMAT: &str = "ozone-plus.character-card.v1";
pub const SESSION_EXPORT_FORMAT: &str = "ozone-plus.session-export.v1";
pub const TRANSCRIPT_EXPORT_FORMAT: &str = "ozone-plus.transcript-export.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterCard {
    pub format: String,
    pub source_format: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub greeting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_dialogue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_history_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl CharacterCard {
    pub fn from_json_str(contents: &str) -> Result<Self> {
        let value = serde_json::from_str::<Value>(contents).map_err(|error| {
            PersistError::InvalidData(format!("failed to parse character card JSON: {error}"))
        })?;

        let root = value.as_object().ok_or_else(|| {
            PersistError::InvalidData("character card JSON must be an object".to_owned())
        })?;
        let (payload, nested_payload) = payload_object(root)?;
        let fallback = nested_payload.then_some(root);

        let name = string_field(payload, fallback, &["name"], true)?.ok_or_else(|| {
            PersistError::InvalidData(
                "character card field 'name' must be a non-empty string".to_owned(),
            )
        })?;
        let description = string_field(payload, fallback, &["description"], false)?;
        let personality = string_field(payload, fallback, &["personality"], false)?;
        let scenario = string_field(payload, fallback, &["scenario"], false)?;
        let greeting = string_field(payload, fallback, &["greeting", "first_mes"], false)?;
        let example_dialogue = string_field(
            payload,
            fallback,
            &["example_dialogue", "mes_example"],
            false,
        )?;
        let creator = string_field(payload, fallback, &["creator"], false)?;
        let creator_notes = string_field(payload, fallback, &["creator_notes"], false)?;
        let system_prompt = string_field(payload, fallback, &["system_prompt"], false)?;
        let post_history_instructions =
            string_field(payload, fallback, &["post_history_instructions"], false)?;
        let tags = tags_field(payload, fallback)?;

        let has_content = [
            description.as_ref(),
            personality.as_ref(),
            scenario.as_ref(),
            greeting.as_ref(),
            example_dialogue.as_ref(),
            system_prompt.as_ref(),
            post_history_instructions.as_ref(),
        ]
        .iter()
        .any(|field| field.is_some());

        if !has_content {
            return Err(PersistError::InvalidData(
                "character card must include at least one content field: description, personality, scenario, greeting/first_mes, example_dialogue/mes_example, system_prompt, or post_history_instructions"
                    .to_owned(),
            ));
        }

        Ok(Self {
            format: CHARACTER_CARD_FORMAT.to_owned(),
            source_format: infer_source_format(root, nested_payload),
            name,
            description,
            personality,
            scenario,
            greeting,
            example_dialogue,
            creator,
            creator_notes,
            system_prompt,
            post_history_instructions,
            tags,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCharacterCardRequest {
    pub card: CharacterCard,
    pub session_name: Option<String>,
    pub tags: Vec<String>,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCharacterCard {
    pub session: ozone_core::session::SessionRecord,
    pub seeded_branch_id: Option<ozone_core::engine::BranchId>,
    pub seeded_message_id: Option<ozone_core::engine::MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCharacterCard {
    pub imported_at: UnixTimestamp,
    pub provenance: String,
    pub card: CharacterCard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExport {
    pub format: String,
    pub exported_at: UnixTimestamp,
    pub session: SessionExportSummary,
    pub active_branch_id: Option<String>,
    pub character_card: Option<StoredCharacterCard>,
    pub branches: Vec<SessionExportBranch>,
    pub messages: Vec<SessionExportMessage>,
    pub bookmarks: Vec<SessionExportBookmark>,
    pub swipe_groups: Vec<SessionExportSwipeGroup>,
}

impl SessionExport {
    pub fn to_pretty_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            PersistError::InvalidData(format!("failed to serialize session export JSON: {error}"))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportSummary {
    pub session_id: String,
    pub name: String,
    pub character_name: Option<String>,
    pub created_at: UnixTimestamp,
    pub last_opened_at: UnixTimestamp,
    pub message_count: u64,
    pub db_size_bytes: Option<u64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportBranch {
    pub branch_id: String,
    pub name: String,
    pub state: String,
    pub tip_message_id: String,
    pub forked_from_message_id: String,
    pub created_at: UnixTimestamp,
    pub description: Option<String>,
    pub transcript_message_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportMessage {
    pub message_id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub content: String,
    pub created_at: UnixTimestamp,
    pub edited_at: Option<UnixTimestamp>,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportBookmark {
    pub bookmark_id: String,
    pub message_id: String,
    pub note: Option<String>,
    pub created_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportSwipeGroup {
    pub swipe_group_id: String,
    pub parent_message_id: String,
    pub parent_context_message_id: Option<String>,
    pub active_ordinal: u16,
    pub candidates: Vec<SessionExportSwipeCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExportSwipeCandidate {
    pub ordinal: u16,
    pub message_id: String,
    pub state: String,
    pub partial_content: Option<String>,
    pub tokens_generated: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptExport {
    pub format: String,
    pub exported_at: UnixTimestamp,
    pub session: TranscriptExportSession,
    pub branch: Option<TranscriptExportBranch>,
    pub messages: Vec<SessionExportMessage>,
}

impl TranscriptExport {
    pub fn to_pretty_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            PersistError::InvalidData(format!(
                "failed to serialize transcript export JSON: {error}"
            ))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptExportSession {
    pub session_id: String,
    pub name: String,
    pub character_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptExportBranch {
    pub branch_id: String,
    pub name: String,
    pub state: String,
    pub tip_message_id: String,
    pub forked_from_message_id: String,
    pub created_at: UnixTimestamp,
    pub description: Option<String>,
}

fn payload_object(root: &Map<String, Value>) -> Result<(&Map<String, Value>, bool)> {
    match root.get("data") {
        Some(Value::Object(payload)) => Ok((payload, true)),
        Some(_) => Err(PersistError::InvalidData(
            "character card field 'data' must be an object when present".to_owned(),
        )),
        None => Ok((root, false)),
    }
}

fn infer_source_format(root: &Map<String, Value>, nested_payload: bool) -> String {
    if root
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|value| value == CHARACTER_CARD_FORMAT)
    {
        return CHARACTER_CARD_FORMAT.to_owned();
    }

    if let Some(spec) = root.get("spec").and_then(Value::as_str) {
        if let Some(version) = root.get("spec_version").and_then(Value::as_str) {
            return format!("{spec}:{version}");
        }

        return spec.trim().to_owned();
    }

    if nested_payload {
        "nested-json".to_owned()
    } else {
        "flat-json".to_owned()
    }
}

fn string_field(
    primary: &Map<String, Value>,
    fallback: Option<&Map<String, Value>>,
    field_names: &[&str],
    required: bool,
) -> Result<Option<String>> {
    let value = find_field(primary, fallback, field_names);
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                if required {
                    Err(PersistError::InvalidData(format!(
                        "character card field '{}' must be a non-empty string",
                        field_names[0]
                    )))
                } else {
                    Ok(None)
                }
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        Some(_) => Err(PersistError::InvalidData(format!(
            "character card field '{}' must be a string",
            field_names[0]
        ))),
        None => Ok(None),
    }
}

fn tags_field(
    primary: &Map<String, Value>,
    fallback: Option<&Map<String, Value>>,
) -> Result<Vec<String>> {
    let Some(value) = find_field(primary, fallback, &["tags"]) else {
        return Ok(Vec::new());
    };

    match value {
        Value::Array(values) => {
            let mut tags = Vec::new();
            for item in values {
                let text = item.as_str().ok_or_else(|| {
                    PersistError::InvalidData(
                        "character card field 'tags' must contain only strings".to_owned(),
                    )
                })?;
                push_tag(&mut tags, text);
            }
            Ok(tags)
        }
        Value::String(value) => {
            let mut tags = Vec::new();
            for candidate in value.split([',', '\n']) {
                push_tag(&mut tags, candidate);
            }
            Ok(tags)
        }
        _ => Err(PersistError::InvalidData(
            "character card field 'tags' must be an array of strings or a comma-separated string"
                .to_owned(),
        )),
    }
}

fn find_field<'a>(
    primary: &'a Map<String, Value>,
    fallback: Option<&'a Map<String, Value>>,
    field_names: &[&str],
) -> Option<&'a Value> {
    for field_name in field_names {
        if let Some(value) = primary.get(*field_name) {
            return Some(value);
        }
    }

    let fallback = fallback?;

    for field_name in field_names {
        if let Some(value) = fallback.get(*field_name) {
            return Some(value);
        }
    }

    None
}

fn push_tag(tags: &mut Vec<String>, candidate: &str) {
    let trimmed = candidate.trim();
    if trimmed.is_empty() || tags.iter().any(|existing| existing == trimmed) {
        return;
    }

    tags.push(trimmed.to_owned());
}

#[cfg(test)]
mod tests {
    use super::CharacterCard;

    #[test]
    fn character_card_parser_accepts_nested_json_shape() {
        let card = CharacterCard::from_json_str(
            r#"{
                "spec": "chara_card_v2",
                "spec_version": "2.0",
                "data": {
                    "name": "Aster",
                    "description": "An observant astronomer.",
                    "first_mes": "Welcome back to the observatory.",
                    "tags": ["stellar", "mentor"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(card.name, "Aster");
        assert_eq!(
            card.greeting.as_deref(),
            Some("Welcome back to the observatory.")
        );
        assert_eq!(card.tags, vec!["stellar".to_owned(), "mentor".to_owned()]);
        assert_eq!(card.source_format, "chara_card_v2:2.0");
    }

    #[test]
    fn character_card_parser_rejects_missing_name() {
        let error = CharacterCard::from_json_str(r#"{"description":"test"}"#).unwrap_err();

        assert!(error.to_string().contains("field 'name'"));
    }

    #[test]
    fn character_card_parser_requires_content_fields() {
        let error = CharacterCard::from_json_str(r#"{"name":"Aster"}"#).unwrap_err();

        assert!(error
            .to_string()
            .contains("must include at least one content field"));
    }
}
