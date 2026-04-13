//! Backend capability descriptors and traits.
//!
//! Every backend exposes a `BackendDescriptor` (static metadata) and may
//! optionally probe live capabilities at startup.

pub mod koboldcpp;

use serde::{Deserialize, Serialize};

use crate::stream::StreamingFormat;

// ---------------------------------------------------------------------------
// Backend identity
// ---------------------------------------------------------------------------

/// Stable identifier for a backend type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackendId(pub String);

impl BackendId {
    pub fn koboldcpp() -> Self {
        Self("koboldcpp".into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// Capability descriptors
// ---------------------------------------------------------------------------

/// Supported sampling parameters for a backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplingCapabilities {
    pub temperature: bool,
    pub top_p: bool,
    pub top_k: bool,
    pub repetition_penalty: bool,
    pub min_p: bool,
    pub grammar: bool,
}

impl Default for SamplingCapabilities {
    fn default() -> Self {
        Self {
            temperature: true,
            top_p: true,
            top_k: true,
            repetition_penalty: true,
            min_p: false,
            grammar: false,
        }
    }
}

/// Live metadata queried from the backend at startup or health check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendModelInfo {
    /// Model identifier as reported by the backend.
    pub model_name: String,
    /// Maximum context length supported by the loaded model.
    pub context_length: usize,
}

/// Static descriptor for a backend type.
///
/// Created without any network calls; used for configuration validation and
/// UI display before the backend is contacted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendDescriptor {
    pub id: BackendId,
    pub display_name: String,
    pub default_url: String,
    pub streaming_format: StreamingFormat,
    pub supports_streaming: bool,
    pub health_endpoint: String,
    pub sampling: SamplingCapabilities,
}

impl BackendDescriptor {
    /// Return the descriptor for the built-in KoboldCpp backend.
    pub fn koboldcpp() -> Self {
        Self {
            id: BackendId::koboldcpp(),
            display_name: "KoboldCpp".into(),
            default_url: "http://localhost:5001".into(),
            streaming_format: StreamingFormat::ServerSentEvents,
            supports_streaming: true,
            health_endpoint: "/api/v1/model".into(),
            sampling: SamplingCapabilities {
                grammar: true,
                min_p: true,
                ..Default::default()
            },
        }
    }

    /// Look up a descriptor by backend type name (e.g. `"koboldcpp"`).
    pub fn for_type(backend_type: &str) -> Option<Self> {
        match backend_type {
            "koboldcpp" => Some(Self::koboldcpp()),
            _ => None,
        }
    }

    /// All built-in backend descriptors.
    pub fn all() -> Vec<Self> {
        vec![Self::koboldcpp()]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koboldcpp_descriptor_is_consistent() {
        let desc = BackendDescriptor::koboldcpp();
        assert_eq!(desc.id, BackendId::koboldcpp());
        assert_eq!(desc.id.as_str(), "koboldcpp");
        assert!(desc.supports_streaming);
        assert_eq!(desc.streaming_format, StreamingFormat::ServerSentEvents);
        assert!(desc.sampling.grammar, "KoboldCpp supports grammar sampling");
    }

    #[test]
    fn for_type_lookup() {
        assert!(BackendDescriptor::for_type("koboldcpp").is_some());
        assert!(BackendDescriptor::for_type("unknown").is_none());
    }

    #[test]
    fn all_returns_at_least_one() {
        assert!(!BackendDescriptor::all().is_empty());
    }

    #[test]
    fn backend_id_display() {
        assert_eq!(BackendId::koboldcpp().to_string(), "koboldcpp");
    }
}
