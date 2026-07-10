//! Backend capability descriptors and traits.
//!
//! Every backend exposes a `BackendDescriptor` (static metadata) and may
//! optionally probe live capabilities at startup.

pub mod koboldcpp;
pub mod llamacpp;

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

    pub fn llamacpp() -> Self {
        Self("llamacpp".into())
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
            default_url: ozone_core::paths::koboldcpp_base_url(),
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

    /// Return the descriptor for the built-in llama.cpp backend.
    pub fn llamacpp() -> Self {
        Self {
            id: BackendId::llamacpp(),
            display_name: "llama.cpp".into(),
            default_url: ozone_core::paths::llamacpp_base_url(),
            streaming_format: StreamingFormat::ServerSentEvents,
            supports_streaming: true,
            health_endpoint: "/health".into(),
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
            "llamacpp" => Some(Self::llamacpp()),
            _ => None,
        }
    }

    /// All built-in backend descriptors.
    pub fn all() -> Vec<Self> {
        vec![Self::koboldcpp(), Self::llamacpp()]
    }
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Common interface for inference backends.
///
/// Implement this trait to add a new backend (e.g. Ollama, llama-server).
/// The gateway dispatches through this trait, so swapping backends requires
/// no changes to the gateway itself.
pub trait Backend: Send + Sync {
    /// Static capability descriptor (no network required).
    fn descriptor(&self) -> &BackendDescriptor;

    /// Check whether the backend is reachable.
    fn is_healthy(&self) -> impl std::future::Future<Output = bool> + Send;

    /// Query the currently loaded model name and context length.
    fn probe_model_info(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<BackendModelInfo>> + Send;

    /// Build the URL used for streaming generation requests.
    fn streaming_url(&self) -> String;
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
        assert!(BackendDescriptor::for_type("llamacpp").is_some());
        assert!(BackendDescriptor::for_type("unknown").is_none());
    }

    #[test]
    fn all_returns_at_least_one() {
        assert!(!BackendDescriptor::all().is_empty());
    }

    #[test]
    fn backend_id_display() {
        assert_eq!(BackendId::koboldcpp().to_string(), "koboldcpp");
        assert_eq!(BackendId::llamacpp().to_string(), "llamacpp");
    }

    #[test]
    fn koboldcpp_client_implements_backend_trait() {
        use crate::backend::koboldcpp::KoboldCppClient;
        let client = KoboldCppClient::new("http://localhost:5001").unwrap();
        // Access through the trait to verify the impl compiles and works.
        fn assert_backend<T: Backend>(b: &T) -> (&BackendDescriptor, String) {
            (b.descriptor(), b.streaming_url())
        }
        let (desc, url) = assert_backend(&client);
        assert_eq!(desc.id.as_str(), "koboldcpp");
        assert_eq!(url, "http://localhost:5001/api/extra/generate/stream");
    }

    #[test]
    fn llamacpp_descriptor_is_consistent() {
        let desc = BackendDescriptor::llamacpp();
        assert_eq!(desc.id, BackendId::llamacpp());
        assert_eq!(desc.id.as_str(), "llamacpp");
        assert!(desc.supports_streaming);
        assert_eq!(desc.streaming_format, StreamingFormat::ServerSentEvents);
        assert_eq!(desc.health_endpoint, "/health");
    }
}
