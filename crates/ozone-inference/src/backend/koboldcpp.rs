//! KoboldCpp backend implementation.
//!
//! Covers:
//! - Health / model probing via `/api/v1/model`
//! - Generation request construction (streaming and non-streaming)
//! - SSE streaming via the `/api/extra/generate/stream` endpoint
//! - Performance stats via `/api/extra/perf`

use std::time::Duration;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::error::InferenceError;

use super::{BackendDescriptor, BackendModelInfo};

// ---------------------------------------------------------------------------
// API request / response types
// ---------------------------------------------------------------------------

/// Parameters forwarded to KoboldCpp for a generation request.
#[derive(Debug, Clone, Serialize)]
pub struct KoboldGenerateRequest {
    pub prompt: String,
    pub max_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rep_pen: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop_sequence: Vec<String>,
}

impl KoboldGenerateRequest {
    pub fn new(prompt: impl Into<String>, max_length: usize) -> Self {
        Self {
            prompt: prompt.into(),
            max_length,
            temperature: None,
            top_p: None,
            top_k: None,
            rep_pen: None,
            min_p: None,
            stop_sequence: Vec::new(),
        }
    }
}

/// Response from `/api/v1/model`.
#[derive(Debug, Deserialize)]
struct KoboldModelResponse {
    result: String,
}

/// Response from `/api/extra/perf`.
#[derive(Debug, Deserialize)]
struct KoboldPerfResponse {
    last_process_time_ms: Option<f64>,
    last_token_count: Option<f64>,
    /// Deserialized from KoboldCpp perf response; unused in current UI.
    #[allow(dead_code)]
    total_gens: Option<u64>,
}

/// Live performance snapshot from KoboldCpp.
#[derive(Debug, Clone, PartialEq)]
pub struct KoboldPerfSnapshot {
    /// Tokens per second from the last generation, if available.
    pub tokens_per_second: Option<f64>,
}

// ---------------------------------------------------------------------------
// KoboldCppClient
// ---------------------------------------------------------------------------

/// Thin async client for the KoboldCpp API.
///
/// All methods are pure HTTP — no state is stored between calls.
#[derive(Debug, Clone)]
pub struct KoboldCppClient {
    base_url: String,
    http: reqwest::Client,
    descriptor: BackendDescriptor,
}

impl KoboldCppClient {
    /// Create a new client pointing at `base_url` (e.g. `"http://localhost:5001"`).
    pub fn new(base_url: impl Into<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build reqwest client")?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            descriptor: BackendDescriptor::koboldcpp(),
        })
    }

    /// Static capability descriptor (no network required).
    pub fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    /// Borrow the shared HTTP client for connection-pooled requests.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    // -----------------------------------------------------------------------
    // Health / model probing
    // -----------------------------------------------------------------------

    /// Check whether the backend is reachable.
    ///
    /// Returns `Ok(true)` if the health endpoint responds with 2xx.
    pub async fn is_healthy(&self) -> bool {
        let url = format!("{}{}", self.base_url, self.descriptor.health_endpoint);
        self.http
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Query the currently loaded model name and context length.
    ///
    /// Context length is probed from `/api/v1/config/max_context_length`.
    /// Falls back to 0 if the endpoint is unavailable (older KoboldCpp versions).
    pub async fn probe_model_info(&self) -> anyhow::Result<BackendModelInfo> {
        let url = format!("{}/api/v1/model", self.base_url);
        let resp: KoboldModelResponse = self
            .http
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| InferenceError::BackendUnreachable {
                backend_id: self.descriptor.id.to_string(),
                reason: e.to_string(),
            })?
            .json()
            .await
            .map_err(InferenceError::Http)?;

        let context_length = self.probe_max_context_length().await.unwrap_or(0);

        Ok(BackendModelInfo {
            model_name: resp.result,
            context_length,
        })
    }

    /// Probe the backend's max context length via `/api/v1/config/max_context_length`.
    ///
    /// Returns `None` if the endpoint is unavailable.
    pub async fn probe_max_context_length(&self) -> Option<usize> {
        #[derive(Deserialize)]
        struct MaxCtxResponse {
            value: usize,
        }
        let url = format!("{}/api/v1/config/max_context_length", self.base_url);
        let resp: MaxCtxResponse = self
            .http
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        Some(resp.value)
    }

    /// Fetch live performance stats.
    pub async fn perf_snapshot(&self) -> anyhow::Result<KoboldPerfSnapshot> {
        let url = format!("{}/api/extra/perf", self.base_url);
        let resp: KoboldPerfResponse = self
            .http
            .get(&url)
            .timeout(Duration::from_millis(800))
            .send()
            .await
            .map_err(|e| InferenceError::BackendUnreachable {
                backend_id: self.descriptor.id.to_string(),
                reason: e.to_string(),
            })?
            .json()
            .await
            .map_err(InferenceError::Http)?;

        let tps = match (resp.last_process_time_ms, resp.last_token_count) {
            (Some(ms), Some(tok)) if ms > 0.0 && tok > 0.0 => Some(tok / (ms / 1000.0)),
            _ => None,
        };
        Ok(KoboldPerfSnapshot {
            tokens_per_second: tps,
        })
    }

    // -----------------------------------------------------------------------
    // Generation
    // -----------------------------------------------------------------------

    /// Send a non-streaming generation request.
    ///
    /// Returns the completed text.
    pub async fn generate(&self, req: &KoboldGenerateRequest) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct GenerateResponse {
            results: Vec<GenerateResult>,
        }
        #[derive(Deserialize)]
        struct GenerateResult {
            text: String,
        }

        let url = format!("{}/api/v1/generate", self.base_url);
        let resp: GenerateResponse = self
            .http
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| InferenceError::BackendUnreachable {
                backend_id: self.descriptor.id.to_string(),
                reason: e.to_string(),
            })?
            .json()
            .await
            .map_err(InferenceError::Http)?;

        resp.results
            .into_iter()
            .next()
            .map(|r| r.text)
            .ok_or_else(|| InferenceError::GenerationFailed("empty results array".into()).into())
    }

    /// Build the streaming URL for `/api/extra/generate/stream`.
    pub fn streaming_url(&self) -> String {
        format!("{}/api/extra/generate/stream", self.base_url)
    }
}

impl super::Backend for KoboldCppClient {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    async fn is_healthy(&self) -> bool {
        self.is_healthy().await
    }

    async fn probe_model_info(&self) -> anyhow::Result<BackendModelInfo> {
        self.probe_model_info().await
    }

    fn streaming_url(&self) -> String {
        self.streaming_url()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_without_network() {
        let client = KoboldCppClient::new("http://localhost:5001").unwrap();
        assert_eq!(client.descriptor().id.as_str(), "koboldcpp");
        assert!(client.descriptor().supports_streaming);
    }

    #[test]
    fn streaming_url_is_correct() {
        let client = KoboldCppClient::new("http://localhost:5001").unwrap();
        assert_eq!(
            client.streaming_url(),
            "http://localhost:5001/api/extra/generate/stream"
        );
    }

    #[test]
    fn trailing_slash_stripped_from_base_url() {
        let client = KoboldCppClient::new("http://localhost:5001/").unwrap();
        assert_eq!(
            client.streaming_url(),
            "http://localhost:5001/api/extra/generate/stream"
        );
    }

    #[test]
    fn generate_request_serializes_optional_fields() {
        let req = KoboldGenerateRequest {
            prompt: "Hello".into(),
            max_length: 200,
            temperature: Some(0.7),
            top_p: None,
            top_k: None,
            rep_pen: None,
            min_p: None,
            stop_sequence: Vec::new(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["prompt"], "Hello");
        // f32 → f64 round-trip can have tiny precision differences; compare with tolerance.
        let temp = json["temperature"].as_f64().unwrap();
        assert!(
            (temp - 0.7).abs() < 0.001,
            "temperature should be ~0.7, got {temp}"
        );
        // top_p omitted when None.
        assert!(json.get("top_p").is_none());
        // stop_sequence omitted when empty.
        assert!(json.get("stop_sequence").is_none());
    }
}
