//! llama.cpp backend implementation.
//!
//! Covers:
//! - Health probing via `/health`
//! - Model probing via `/v1/models`
//! - Native completion streaming via `/completion`

use std::time::Duration;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::error::InferenceError;

use super::{BackendDescriptor, BackendModelInfo};

#[derive(Debug, Clone, Serialize)]
pub struct LlamaCppCompletionRequest {
    pub prompt: String,
    pub n_predict: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    pub stream: bool,
}

#[derive(Debug, Deserialize)]
struct LlamaModelsResponse {
    data: Vec<LlamaModelEntry>,
}

#[derive(Debug, Deserialize)]
struct LlamaModelEntry {
    id: String,
    meta: Option<LlamaModelMeta>,
}

#[derive(Debug, Deserialize)]
struct LlamaModelMeta {
    n_ctx_train: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct LlamaCppClient {
    base_url: String,
    http: reqwest::Client,
    descriptor: BackendDescriptor,
}

impl LlamaCppClient {
    pub fn new(base_url: impl Into<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build reqwest client")?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            descriptor: BackendDescriptor::llamacpp(),
        })
    }

    pub fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

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

    pub async fn probe_model_info(&self) -> anyhow::Result<BackendModelInfo> {
        let url = format!("{}/v1/models", self.base_url);
        let response: LlamaModelsResponse = self
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
        let entry = response.data.into_iter().next().ok_or_else(|| {
            InferenceError::GenerationFailed("llama.cpp returned no loaded model".into())
        })?;
        Ok(BackendModelInfo {
            model_name: entry.id,
            context_length: entry.meta.and_then(|meta| meta.n_ctx_train).unwrap_or(0),
        })
    }

    pub async fn probe_max_context_length(&self) -> Option<usize> {
        self.probe_model_info()
            .await
            .ok()
            .map(|info| info.context_length)
            .filter(|value| *value > 0)
    }

    pub fn streaming_url(&self) -> String {
        format!("{}/completion", self.base_url)
    }
}

impl super::Backend for LlamaCppClient {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_without_network() {
        let client = LlamaCppClient::new("http://localhost:8080").unwrap();
        assert_eq!(client.descriptor().id.as_str(), "llamacpp");
        assert!(client.descriptor().supports_streaming);
    }

    #[test]
    fn streaming_url_is_correct() {
        let client = LlamaCppClient::new("http://localhost:8080").unwrap();
        assert_eq!(client.streaming_url(), "http://localhost:8080/completion");
    }

    #[test]
    fn trailing_slash_stripped_from_base_url() {
        let client = LlamaCppClient::new("http://localhost:8080/").unwrap();
        assert_eq!(client.streaming_url(), "http://localhost:8080/completion");
    }
}
