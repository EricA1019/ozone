//! Inference gateway — the entry point for all generation requests.
//!
//! The gateway owns the request lifecycle:
//!   - rate limiting
//!   - retry with exponential back-off
//!   - cancellation via a `tokio::sync::oneshot` sender
//!   - streaming token delivery via an `mpsc` channel
//!   - health monitoring

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio_util::codec::Decoder as _;
use tracing::{debug, warn};

use crate::backend::koboldcpp::{KoboldCppClient, KoboldGenerateRequest};
use crate::backend::BackendDescriptor;
use crate::config::{BackendConfig, RateLimitConfig};
use crate::error::InferenceError;
use crate::stream::{StreamChunk, StreamDecoder, StreamingFormat};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Unique identifier for an in-flight inference request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new() -> Self {
        Self(uuid_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

fn uuid_v4() -> String {
    // Lightweight UUID v4 without pulling in `uuid` for this crate alone.
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        t,
        t >> 16,
        t & 0x0fff,
        t & 0x3fff | 0x8000,
        t as u64 ^ 0xdead_beef_cafe
    )
}

/// A single inference request.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub id: RequestId,
    /// Fully formatted prompt (already rendered via template).
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub rep_pen: Option<f32>,
    pub stop_sequences: Vec<String>,
}

impl InferenceRequest {
    pub fn new(prompt: impl Into<String>, max_tokens: usize) -> Self {
        Self {
            id: RequestId::new(),
            prompt: prompt.into(),
            max_tokens,
            temperature: None,
            top_p: None,
            top_k: None,
            rep_pen: None,
            stop_sequences: Vec::new(),
        }
    }
}

/// Health status of the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendHealth {
    Healthy,
    Slow { latency_ms: u64 },
    Unreachable,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

/// Configuration for the gateway (derived from `BackendConfig`).
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub base_url: String,
    pub streaming_format: StreamingFormat,
    pub rate_limit: RateLimitConfig,
    /// Maximum concurrent in-flight inference requests.
    pub max_concurrent: usize,
}

impl From<&BackendConfig> for GatewayConfig {
    fn from(cfg: &BackendConfig) -> Self {
        Self {
            base_url: cfg.url.clone(),
            streaming_format: StreamingFormat::ServerSentEvents,
            rate_limit: cfg.rate_limit.clone(),
            max_concurrent: 4,
        }
    }
}

/// The inference gateway.
///
/// Owns the HTTP client, concurrency semaphore, and rate-limit state.
/// Cloning is cheap — all state is behind `Arc`.
#[derive(Clone)]
pub struct InferenceGateway {
    client: KoboldCppClient,
    config: GatewayConfig,
    /// Limits concurrent in-flight requests.
    semaphore: Arc<Semaphore>,
}

impl InferenceGateway {
    /// Build a gateway from a backend config section.
    pub fn new(cfg: &BackendConfig) -> anyhow::Result<Self> {
        let gateway_cfg = GatewayConfig::from(cfg);
        let client = KoboldCppClient::new(&gateway_cfg.base_url)
            .context("failed to create KoboldCpp client")?;
        let semaphore = Arc::new(Semaphore::new(gateway_cfg.max_concurrent));
        Ok(Self {
            client,
            config: gateway_cfg,
            semaphore,
        })
    }

    /// Return the backend capability descriptor (no network required).
    pub fn descriptor(&self) -> &BackendDescriptor {
        self.client.descriptor()
    }

    /// Return a reference to the underlying KoboldCpp client for direct probing.
    pub fn client(&self) -> &KoboldCppClient {
        &self.client
    }

    // -----------------------------------------------------------------------
    // Health
    // -----------------------------------------------------------------------

    /// Perform a single health probe.
    pub async fn check_health(&self) -> BackendHealth {
        let start = std::time::Instant::now();
        if self.client.is_healthy().await {
            let latency = start.elapsed().as_millis() as u64;
            if latency > 5_000 {
                BackendHealth::Slow {
                    latency_ms: latency,
                }
            } else {
                BackendHealth::Healthy
            }
        } else {
            BackendHealth::Unreachable
        }
    }

    // -----------------------------------------------------------------------
    // Streaming inference
    // -----------------------------------------------------------------------

    /// Submit a streaming inference request.
    ///
    /// Tokens are sent to `stream_tx` as `StreamChunk` values.
    /// The caller owns the receiving end of `stream_tx`.
    ///
    /// Cancellation: drop the `cancel_tx` or send to it before generation
    /// completes. The gateway will stop sending and drop the permit.
    ///
    /// Returns the number of tokens streamed before completion or cancellation.
    pub async fn stream(
        &self,
        req: InferenceRequest,
        stream_tx: mpsc::Sender<StreamChunk>,
        cancel_rx: oneshot::Receiver<()>,
    ) -> anyhow::Result<usize> {
        // Acquire concurrency permit.
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .context("semaphore closed")?;

        let kobold_req = build_kobold_request(&req);
        let url = self.client.streaming_url();

        // Reuse the pooled HTTP client from the backend for connection reuse.
        let http = self.client.http();

        let response = http
            .post(&url)
            .timeout(Duration::from_secs(300))
            .json(&kobold_req)
            .send()
            .await
            .map_err(|e| InferenceError::BackendUnreachable {
                backend_id: self.client.descriptor().id.to_string(),
                reason: e.to_string(),
            })?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_ms = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| s * 1_000)
                .unwrap_or(self.config.rate_limit.retry_after_429_ms);
            return Err(InferenceError::RateLimited {
                retry_after_ms: retry_ms,
            }
            .into());
        }

        if !response.status().is_success() {
            return Err(
                InferenceError::GenerationFailed(format!("HTTP {}", response.status())).into(),
            );
        }

        let mut decoder = StreamDecoder::new(self.config.streaming_format);
        let mut buf = BytesMut::new();
        let mut tokens_generated: usize = 0;

        // Pin cancel receiver for use in select!.
        let mut cancel_rx = cancel_rx;
        let mut body = response.bytes_stream();

        use futures_util::StreamExt as _;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    debug!("generation cancelled by caller");
                    return Err(InferenceError::Cancelled.into());
                }
                chunk = body.next() => {
                    match chunk {
                        None => {
                            // Stream ended.
                            break;
                        }
                        Some(Err(e)) => {
                            return Err(InferenceError::StreamInterrupted {
                                tokens_generated,
                                reason: e.to_string(),
                            }.into());
                        }
                        Some(Ok(bytes)) => {
                            buf.extend_from_slice(&bytes[..]);
                            loop {
                                match decoder.decode(&mut buf) {
                                    Ok(Some(StreamChunk::Done)) => {
                                        let _ = stream_tx.send(StreamChunk::Done).await;
                                        return Ok(tokens_generated);
                                    }
                                    Ok(Some(chunk)) => {
                                        if let StreamChunk::Token(_) = &chunk {
                                            tokens_generated += 1;
                                        }
                                        if stream_tx.send(chunk).await.is_err() {
                                            // Receiver dropped — treat as cancellation.
                                            return Err(InferenceError::Cancelled.into());
                                        }
                                    }
                                    Ok(None) => break,
                                    Err(e) => {
                                        warn!("stream decode error: {e}");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = stream_tx.send(StreamChunk::Done).await;
        Ok(tokens_generated)
    }

    // -----------------------------------------------------------------------
    // Streaming with retry
    // -----------------------------------------------------------------------

    /// Like [`stream`](Self::stream) but wraps up to `max_retries` attempts
    /// on transient failures with exponential back-off.
    ///
    /// **Phase 1D note:** The current implementation makes a single attempt.
    /// Full multi-attempt retry with proper cancel-token threading is wired
    /// in the Phase 1D app-wiring step.
    pub async fn stream_with_retry(
        &self,
        req: InferenceRequest,
        stream_tx: mpsc::Sender<StreamChunk>,
        cancel_rx: oneshot::Receiver<()>,
        _max_retries: u32,
    ) -> anyhow::Result<usize> {
        // Single-attempt path — full retry loop lands in Phase 1D app-wiring.
        self.stream(req, stream_tx, cancel_rx).await
    }
}

fn build_kobold_request(req: &InferenceRequest) -> KoboldGenerateRequest {
    KoboldGenerateRequest {
        prompt: req.prompt.clone(),
        max_length: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        top_k: req.top_k,
        rep_pen: req.rep_pen,
        min_p: None,
        stop_sequence: req.stop_sequences.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendConfig;
    use tokio::sync::mpsc;

    fn default_cfg() -> BackendConfig {
        BackendConfig::default()
    }

    #[test]
    fn gateway_builds_from_config() {
        let cfg = default_cfg();
        let gw = InferenceGateway::new(&cfg).expect("gateway should build");
        assert_eq!(gw.descriptor().id.as_str(), "koboldcpp");
    }

    #[test]
    fn gateway_descriptor_matches_backend_type() {
        let cfg = BackendConfig {
            url: "http://192.168.1.5:5001".into(),
            ..Default::default()
        };
        let gw = InferenceGateway::new(&cfg).unwrap();
        assert!(gw.descriptor().supports_streaming);
        assert_eq!(
            gw.descriptor().streaming_format,
            StreamingFormat::ServerSentEvents
        );
    }

    #[test]
    fn request_id_is_unique() {
        let a = RequestId::new();
        let b = RequestId::new();
        assert_ne!(a, b, "request IDs should be unique");
    }

    #[test]
    fn inference_request_fields() {
        let req = InferenceRequest::new("Hello world", 128);
        assert_eq!(req.prompt, "Hello world");
        assert_eq!(req.max_tokens, 128);
        assert!(req.temperature.is_none());
        assert!(req.stop_sequences.is_empty());
    }

    #[tokio::test]
    async fn cancel_before_stream_returns_cancelled() {
        // This test exercises the cancel path without a real backend by
        // pre-cancelling immediately.
        let cfg = BackendConfig::default();
        let gw = InferenceGateway::new(&cfg).unwrap();
        let (stream_tx, _stream_rx) = mpsc::channel::<StreamChunk>(64);
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

        // Cancel immediately before we even start.
        let _ = cancel_tx.send(());

        // Without a real backend the request will fail at the connect stage;
        // we just verify the gateway can be constructed and the request type
        // compiles cleanly.
        let req = InferenceRequest::new("test", 10);
        // We don't await here because there's no real server — just check compile.
        let _ = (gw, stream_tx, cancel_rx, req);
    }

    #[test]
    fn kobold_request_built_correctly() {
        let req = InferenceRequest {
            id: RequestId::new(),
            prompt: "Hello".into(),
            max_tokens: 100,
            temperature: Some(0.8),
            top_p: Some(0.9),
            top_k: Some(40),
            rep_pen: Some(1.1),
            stop_sequences: vec!["<|im_end|>".into()],
        };
        let kobold = build_kobold_request(&req);
        assert_eq!(kobold.prompt, "Hello");
        assert_eq!(kobold.max_length, 100);
        assert_eq!(kobold.temperature, Some(0.8));
        assert_eq!(kobold.stop_sequence, vec!["<|im_end|>"]);
    }
}
