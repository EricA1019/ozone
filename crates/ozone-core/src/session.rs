use std::{error::Error, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub type UnixTimestamp = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionIdError;

impl fmt::Display for SessionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("session IDs must be UUID strings in 8-4-4-4-12 format")
    }
}

impl Error for SessionIdError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, SessionIdError> {
        value.as_ref().parse()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SessionId {
    type Err = SessionIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        normalize_uuid_string(value).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSessionRequest {
    pub name: String,
    pub character_name: Option<String>,
    pub tags: Vec<String>,
}

impl CreateSessionRequest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            character_name: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateSessionRequest {
    pub name: Option<String>,
    pub character_name: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub name: String,
    pub character_name: Option<String>,
    pub created_at: UnixTimestamp,
    pub last_opened_at: UnixTimestamp,
    pub message_count: u64,
    pub db_size_bytes: Option<u64>,
    pub tags: Vec<String>,
}

impl SessionSummary {
    pub fn new(session_id: SessionId, name: impl Into<String>, created_at: UnixTimestamp) -> Self {
        Self {
            session_id,
            name: name.into(),
            character_name: None,
            created_at,
            last_opened_at: created_at,
            message_count: 0,
            db_size_bytes: None,
            tags: Vec::new(),
        }
    }

    /// Returns the folder this session belongs to, if any.
    /// Folder membership is stored as a `folder:<name>` tag.
    pub fn folder(&self) -> Option<&str> {
        self.tags.iter()
            .find_map(|tag| tag.strip_prefix("folder:"))
    }

    /// Set or remove the folder for this session.
    /// Removes any existing `folder:*` tag before inserting the new one.
    pub fn set_folder(&mut self, folder: Option<&str>) {
        self.tags.retain(|tag| !tag.starts_with("folder:"));
        if let Some(name) = folder {
            let name = name.trim();
            if !name.is_empty() {
                self.tags.push(format!("folder:{name}"));
            }
        }
    }
}

pub type SessionRecord = SessionSummary;

fn normalize_uuid_string(value: &str) -> Result<String, SessionIdError> {
    const HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

    let bytes = value.as_bytes();

    if bytes.len() != 36 {
        return Err(SessionIdError);
    }

    let mut normalized = String::with_capacity(36);

    for (idx, byte) in bytes.iter().enumerate() {
        if HYPHEN_POSITIONS.contains(&idx) {
            if *byte != b'-' {
                return Err(SessionIdError);
            }

            normalized.push('-');
            continue;
        }

        if !byte.is_ascii_hexdigit() {
            return Err(SessionIdError);
        }

        normalized.push((*byte as char).to_ascii_lowercase());
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{
        CreateSessionRequest, SessionId, SessionRecord, SessionSummary, UpdateSessionRequest,
    };

    #[test]
    fn session_id_parsing_normalizes_uuid_strings() {
        let session_id = SessionId::parse("123E4567-E89B-12D3-A456-426614174000").unwrap();

        assert_eq!(session_id.as_str(), "123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(
            session_id.to_string(),
            "123e4567-e89b-12d3-a456-426614174000"
        );
        assert!(SessionId::parse("not-a-session-id").is_err());
    }

    #[test]
    fn session_metadata_defaults_match_phase1a_foundation() {
        let request = CreateSessionRequest::new("Phase 1A Session");

        assert_eq!(request.name, "Phase 1A Session");
        assert_eq!(request.character_name, None);
        assert!(request.tags.is_empty());

        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let summary = SessionSummary::new(session_id.clone(), request.name.clone(), 1_725_647_200);
        let record: SessionRecord = summary.clone();

        assert_eq!(record, summary);
        assert_eq!(summary.session_id, session_id);
        assert_eq!(summary.name, request.name);
        assert_eq!(summary.character_name, None);
        assert_eq!(summary.created_at, 1_725_647_200);
        assert_eq!(summary.last_opened_at, 1_725_647_200);
        assert_eq!(summary.message_count, 0);
        assert_eq!(summary.db_size_bytes, None);
        assert!(summary.tags.is_empty());
    }

    #[test]
    fn update_session_request_defaults_to_no_changes() {
        let request = UpdateSessionRequest::default();

        assert_eq!(request.name, None);
        assert_eq!(request.character_name, None);
        assert_eq!(request.tags, None);
    }

    #[test]
    fn folder_returns_none_when_no_folder_tag() {
        let s = SessionSummary::new(SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(), "test", 0);
        assert_eq!(s.folder(), None);
    }

    #[test]
    fn set_folder_adds_and_replaces_folder_tag() {
        let mut s = SessionSummary::new(SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(), "test", 0);
        s.set_folder(Some("Work"));
        assert_eq!(s.folder(), Some("Work"));
        s.set_folder(Some("Personal"));
        assert_eq!(s.folder(), Some("Personal"));
        assert_eq!(s.tags.iter().filter(|t| t.starts_with("folder:")).count(), 1);
    }

    #[test]
    fn set_folder_none_removes_folder_tag() {
        let mut s = SessionSummary::new(SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(), "test", 0);
        s.set_folder(Some("Work"));
        s.set_folder(None);
        assert_eq!(s.folder(), None);
        assert!(s.tags.iter().all(|t| !t.starts_with("folder:")));
    }
}
