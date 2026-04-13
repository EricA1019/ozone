//! `ozone-inference` — Phase 1D inference gateway, layered config, and prompt
//! templates for the ozone+ product family.
//!
//! # Crate structure
//!
//! - [`config`] — Layered TOML config loading (defaults → global → session → env).
//! - [`templates`] — Prompt-format template registry (ChatML, Alpaca, Llama-3-Instruct).
//! - [`stream`] — Streaming decoder (`tokio_util::codec::Decoder`) for SSE / JSONLines.
//! - [`backend`] — Backend capability descriptors and static metadata.
//! - [`backend::koboldcpp`] — KoboldCpp HTTP client.
//! - [`gateway`] — Inference gateway with streaming, cancellation, retries, health.
//! - [`error`] — `InferenceError` type.

pub mod backend;
pub mod config;
pub mod error;
pub mod gateway;
pub mod stream;
pub mod templates;

// Convenience re-exports for callers.
pub use config::{
    ConfigLoader, GarbageCollectionPolicy, MemoryConfig, MemoryLifecycleConfig, OzoneConfig,
    StaleArtifactPolicy, StorageTierPolicy, SummaryConfig,
};
pub use error::InferenceError;
pub use gateway::{BackendHealth, InferenceGateway, InferenceRequest, RequestId};
pub use stream::{StreamChunk, StreamDecoder, StreamingFormat};
pub use templates::{detect_template, TemplateMessage, TemplateRegistry};
