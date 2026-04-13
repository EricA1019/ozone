use thiserror::Error;

/// Errors produced by the inference subsystem.
#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("backend unreachable: {backend_id}: {reason}")]
    BackendUnreachable { backend_id: String, reason: String },

    #[error("generation failed: {0}")]
    GenerationFailed(String),

    #[error("stream interrupted after {tokens_generated} tokens: {reason}")]
    StreamInterrupted {
        tokens_generated: usize,
        reason: String,
    },

    #[error("rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("prompt template error: {template}: {reason}")]
    PromptTemplate { template: String, reason: String },

    #[error("config invalid: {key}: {reason}")]
    ConfigInvalid { key: String, reason: String },

    #[error("config load error: {0}")]
    ConfigLoad(#[from] config::ConfigError),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("template render error: {0}")]
    TemplateRender(#[from] minijinja::Error),

    #[error("generation cancelled")]
    Cancelled,
}

/// Required by `tokio_util::codec::Decoder` — the error type must implement
/// `From<std::io::Error>` so the default `decode_eof` implementation compiles.
impl From<std::io::Error> for InferenceError {
    fn from(e: std::io::Error) -> Self {
        Self::StreamInterrupted {
            tokens_generated: 0,
            reason: e.to_string(),
        }
    }
}
