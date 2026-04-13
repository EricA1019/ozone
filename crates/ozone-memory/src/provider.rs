use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{source_text_hash, EmbeddingContent};

const DEFAULT_EMBEDDING_MODEL: &str = "BAAI/bge-small-en-v1.5";
const DEFAULT_EMBEDDING_DIMENSIONS: usize = 384;
const DEFAULT_EMBEDDING_BATCH_SIZE: usize = 256;
const DEFAULT_QUERY_PREFIX: &str = "query: ";
const DEFAULT_PASSAGE_PREFIX: &str = "passage: ";
const DEFAULT_MOCK_SEED: u64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingProviderKind {
    #[default]
    Disabled,
    Mock,
    Fastembed,
}

impl EmbeddingProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Mock => "mock",
            Self::Fastembed => "fastembed",
        }
    }
}

impl fmt::Display for EmbeddingProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingProviderConfig {
    #[serde(default)]
    pub provider: EmbeddingProviderKind,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_expected_dimensions")]
    pub expected_dimensions: Option<usize>,
    #[serde(default = "default_embedding_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_query_prefix")]
    pub query_prefix: String,
    #[serde(default = "default_passage_prefix")]
    pub passage_prefix: String,
    #[serde(default = "default_mock_seed")]
    pub mock_seed: u64,
    #[serde(default)]
    pub show_download_progress: bool,
}

impl EmbeddingProviderConfig {
    pub fn resolved_dimensions(&self) -> usize {
        self.expected_dimensions
            .unwrap_or(DEFAULT_EMBEDDING_DIMENSIONS)
    }

    pub fn metadata(&self) -> EmbeddingProviderMetadata {
        EmbeddingProviderMetadata {
            provider: self.provider,
            model: self.model.clone(),
            dimensions: self.resolved_dimensions(),
        }
    }
}

impl Default for EmbeddingProviderConfig {
    fn default() -> Self {
        Self {
            provider: EmbeddingProviderKind::Disabled,
            model: default_embedding_model(),
            expected_dimensions: default_expected_dimensions(),
            batch_size: default_embedding_batch_size(),
            query_prefix: default_query_prefix(),
            passage_prefix: default_passage_prefix(),
            mock_seed: default_mock_seed(),
            show_download_progress: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingProviderMetadata {
    pub provider: EmbeddingProviderKind,
    pub model: String,
    pub dimensions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPurpose {
    Document,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub text: String,
    pub purpose: EmbeddingPurpose,
}

impl EmbeddingRequest {
    pub fn new(text: impl Into<String>, purpose: EmbeddingPurpose) -> Self {
        Self {
            text: text.into(),
            purpose,
        }
    }

    pub fn document(text: impl Into<String>) -> Self {
        Self::new(text, EmbeddingPurpose::Document)
    }

    pub fn query(text: impl Into<String>) -> Self {
        Self::new(text, EmbeddingPurpose::Query)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedEmbedding {
    pub request: EmbeddingRequest,
    pub content: EmbeddingContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingBatch {
    pub provider: EmbeddingProviderMetadata,
    pub embeddings: Vec<GeneratedEmbedding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EmbeddingAvailability {
    Ready,
    Disabled { reason: String },
    Unavailable { reason: String },
}

pub trait EmbeddingProvider: Send + Sync {
    fn metadata(&self) -> EmbeddingProviderMetadata;
    fn availability(&self) -> EmbeddingAvailability;
    fn embed(
        &self,
        requests: &[EmbeddingRequest],
    ) -> Result<EmbeddingBatch, EmbeddingProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingProviderError {
    Disabled { reason: String },
    Unavailable { reason: String },
    Backend { reason: String },
}

impl EmbeddingProviderError {
    fn disabled(reason: impl Into<String>) -> Self {
        Self::Disabled {
            reason: reason.into(),
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn backend(reason: impl Into<String>) -> Self {
        Self::Backend {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for EmbeddingProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled { reason } => write!(f, "embeddings are disabled: {reason}"),
            Self::Unavailable { reason } => write!(f, "embeddings are unavailable: {reason}"),
            Self::Backend { reason } => write!(f, "embedding backend failed: {reason}"),
        }
    }
}

impl Error for EmbeddingProviderError {}

pub fn build_embedding_provider(config: EmbeddingProviderConfig) -> Box<dyn EmbeddingProvider> {
    match config.provider {
        EmbeddingProviderKind::Disabled => Box::new(DisabledEmbeddingProvider::new(config)),
        EmbeddingProviderKind::Mock => Box::new(MockEmbeddingProvider::new(config)),
        EmbeddingProviderKind::Fastembed => build_fastembed_provider(config),
    }
}

#[derive(Debug, Clone)]
struct DisabledEmbeddingProvider {
    metadata: EmbeddingProviderMetadata,
    reason: String,
}

impl DisabledEmbeddingProvider {
    fn new(mut config: EmbeddingProviderConfig) -> Self {
        config.provider = EmbeddingProviderKind::Disabled;
        Self {
            metadata: config.metadata(),
            reason: "memory.embedding.provider is disabled".to_owned(),
        }
    }
}

impl EmbeddingProvider for DisabledEmbeddingProvider {
    fn metadata(&self) -> EmbeddingProviderMetadata {
        self.metadata.clone()
    }

    fn availability(&self) -> EmbeddingAvailability {
        EmbeddingAvailability::Disabled {
            reason: self.reason.clone(),
        }
    }

    fn embed(
        &self,
        _requests: &[EmbeddingRequest],
    ) -> Result<EmbeddingBatch, EmbeddingProviderError> {
        Err(EmbeddingProviderError::disabled(self.reason.clone()))
    }
}

#[cfg_attr(feature = "fastembed", allow(dead_code))]
#[derive(Debug, Clone)]
struct UnavailableEmbeddingProvider {
    metadata: EmbeddingProviderMetadata,
    reason: String,
}

#[cfg_attr(feature = "fastembed", allow(dead_code))]
impl UnavailableEmbeddingProvider {
    fn new(metadata: EmbeddingProviderMetadata, reason: impl Into<String>) -> Self {
        Self {
            metadata,
            reason: reason.into(),
        }
    }
}

impl EmbeddingProvider for UnavailableEmbeddingProvider {
    fn metadata(&self) -> EmbeddingProviderMetadata {
        self.metadata.clone()
    }

    fn availability(&self) -> EmbeddingAvailability {
        EmbeddingAvailability::Unavailable {
            reason: self.reason.clone(),
        }
    }

    fn embed(
        &self,
        _requests: &[EmbeddingRequest],
    ) -> Result<EmbeddingBatch, EmbeddingProviderError> {
        Err(EmbeddingProviderError::unavailable(self.reason.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct MockEmbeddingProvider {
    config: EmbeddingProviderConfig,
    metadata: EmbeddingProviderMetadata,
}

impl MockEmbeddingProvider {
    pub fn new(mut config: EmbeddingProviderConfig) -> Self {
        config.provider = EmbeddingProviderKind::Mock;
        let metadata = config.metadata();
        Self { config, metadata }
    }
}

impl EmbeddingProvider for MockEmbeddingProvider {
    fn metadata(&self) -> EmbeddingProviderMetadata {
        self.metadata.clone()
    }

    fn availability(&self) -> EmbeddingAvailability {
        EmbeddingAvailability::Ready
    }

    fn embed(
        &self,
        requests: &[EmbeddingRequest],
    ) -> Result<EmbeddingBatch, EmbeddingProviderError> {
        let embeddings = requests
            .iter()
            .cloned()
            .map(|request| GeneratedEmbedding {
                content: EmbeddingContent::new(
                    mock_embedding(
                        &request.text,
                        request.purpose,
                        self.metadata.dimensions,
                        self.config.mock_seed,
                    ),
                    source_text_hash(&request.text),
                ),
                request,
            })
            .collect();

        Ok(EmbeddingBatch {
            provider: self.metadata(),
            embeddings,
        })
    }
}

#[cfg(feature = "fastembed")]
pub use fastembed_backend::FastembedEmbeddingProvider;

#[cfg(feature = "fastembed")]
fn build_fastembed_provider(config: EmbeddingProviderConfig) -> Box<dyn EmbeddingProvider> {
    Box::new(FastembedEmbeddingProvider::new(config))
}

#[cfg(not(feature = "fastembed"))]
fn build_fastembed_provider(config: EmbeddingProviderConfig) -> Box<dyn EmbeddingProvider> {
    let mut metadata = config.metadata();
    metadata.provider = EmbeddingProviderKind::Fastembed;
    Box::new(UnavailableEmbeddingProvider::new(
        metadata,
        "ozone-memory was built without the `fastembed` feature",
    ))
}

fn default_embedding_model() -> String {
    DEFAULT_EMBEDDING_MODEL.to_owned()
}

fn default_expected_dimensions() -> Option<usize> {
    Some(DEFAULT_EMBEDDING_DIMENSIONS)
}

fn default_embedding_batch_size() -> usize {
    DEFAULT_EMBEDDING_BATCH_SIZE
}

fn default_query_prefix() -> String {
    DEFAULT_QUERY_PREFIX.to_owned()
}

fn default_passage_prefix() -> String {
    DEFAULT_PASSAGE_PREFIX.to_owned()
}

fn default_mock_seed() -> u64 {
    DEFAULT_MOCK_SEED
}

fn mock_embedding(text: &str, purpose: EmbeddingPurpose, dimensions: usize, seed: u64) -> Vec<f32> {
    let mut state = source_text_hash(text)
        ^ seed
        ^ match purpose {
            EmbeddingPurpose::Document => 0x9E37_79B9_7F4A_7C15,
            EmbeddingPurpose::Query => 0xA076_1D64_78BD_642F,
        };
    let mut vector = Vec::with_capacity(dimensions);

    for idx in 0..dimensions {
        state = splitmix64(state ^ idx as u64);
        let mantissa = ((state >> 40) & 0x00FF_FFFF) as u32;
        let value = (mantissa as f32 / 16_777_215.0) * 2.0 - 1.0;
        vector.push(value);
    }

    normalize_l2(&mut vector);
    vector
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn normalize_l2(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        return;
    }

    for value in vector {
        *value /= norm;
    }
}

#[cfg(feature = "fastembed")]
mod fastembed_backend {
    use std::sync::Mutex;

    use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

    use super::{
        default_embedding_model, render_fastembed_input, EmbeddingAvailability, EmbeddingBatch,
        EmbeddingProvider, EmbeddingProviderConfig, EmbeddingProviderError, EmbeddingProviderKind,
        EmbeddingProviderMetadata, EmbeddingRequest, GeneratedEmbedding,
    };
    use crate::{source_text_hash, EmbeddingContent};

    enum FastembedState {
        Uninitialized { model: EmbeddingModel },
        Ready(TextEmbedding),
        Failed(String),
    }

    pub struct FastembedEmbeddingProvider {
        config: EmbeddingProviderConfig,
        metadata: EmbeddingProviderMetadata,
        state: Mutex<FastembedState>,
    }

    impl FastembedEmbeddingProvider {
        pub fn new(mut config: EmbeddingProviderConfig) -> Self {
            config.provider = EmbeddingProviderKind::Fastembed;

            let (metadata, state) = match preflight_fastembed(&config) {
                Ok((model, dimensions)) => (
                    EmbeddingProviderMetadata {
                        provider: EmbeddingProviderKind::Fastembed,
                        model: config.model.clone(),
                        dimensions,
                    },
                    FastembedState::Uninitialized { model },
                ),
                Err(reason) => (
                    EmbeddingProviderMetadata {
                        provider: EmbeddingProviderKind::Fastembed,
                        model: config.model.clone(),
                        dimensions: config.resolved_dimensions(),
                    },
                    FastembedState::Failed(reason),
                ),
            };

            Self {
                config,
                metadata,
                state: Mutex::new(state),
            }
        }

        fn with_model<R>(
            &self,
            op: impl FnOnce(&mut TextEmbedding) -> Result<R, fastembed::Error>,
        ) -> Result<R, EmbeddingProviderError> {
            let mut state = self.state.lock().map_err(|_| {
                EmbeddingProviderError::unavailable("fastembed provider lock poisoned")
            })?;

            if let FastembedState::Uninitialized { model } = &*state {
                let initialized =
                    init_fastembed_model(model.clone(), self.config.show_download_progress)
                        .map_err(|reason| {
                            *state = FastembedState::Failed(reason.clone());
                            EmbeddingProviderError::unavailable(reason)
                        })?;
                *state = FastembedState::Ready(initialized);
            }

            match &mut *state {
                FastembedState::Ready(model) => {
                    op(model).map_err(|error| EmbeddingProviderError::backend(error.to_string()))
                }
                FastembedState::Failed(reason) => {
                    Err(EmbeddingProviderError::unavailable(reason.clone()))
                }
                FastembedState::Uninitialized { .. } => unreachable!("provider should initialize"),
            }
        }
    }

    impl EmbeddingProvider for FastembedEmbeddingProvider {
        fn metadata(&self) -> EmbeddingProviderMetadata {
            self.metadata.clone()
        }

        fn availability(&self) -> EmbeddingAvailability {
            match self.state.lock() {
                Ok(state) => match &*state {
                    FastembedState::Failed(reason) => EmbeddingAvailability::Unavailable {
                        reason: reason.clone(),
                    },
                    FastembedState::Ready(_) | FastembedState::Uninitialized { .. } => {
                        EmbeddingAvailability::Ready
                    }
                },
                Err(_) => EmbeddingAvailability::Unavailable {
                    reason: "fastembed provider lock poisoned".to_owned(),
                },
            }
        }

        fn embed(
            &self,
            requests: &[EmbeddingRequest],
        ) -> Result<EmbeddingBatch, EmbeddingProviderError> {
            if requests.is_empty() {
                return Ok(EmbeddingBatch {
                    provider: self.metadata(),
                    embeddings: Vec::new(),
                });
            }

            let rendered_inputs: Vec<_> = requests
                .iter()
                .map(|request| render_fastembed_input(&self.config, request))
                .collect();
            let batch_size = Some(self.config.batch_size);
            let vectors = self.with_model(|model| model.embed(rendered_inputs, batch_size))?;

            let embeddings = requests
                .iter()
                .cloned()
                .zip(vectors.into_iter())
                .map(|(request, vector)| GeneratedEmbedding {
                    content: EmbeddingContent::new(vector, source_text_hash(&request.text)),
                    request,
                })
                .collect();

            Ok(EmbeddingBatch {
                provider: self.metadata(),
                embeddings,
            })
        }
    }

    fn init_fastembed_model(
        model: EmbeddingModel,
        show_download_progress: bool,
    ) -> Result<TextEmbedding, String> {
        let options = InitOptions::new(model).with_show_download_progress(show_download_progress);
        TextEmbedding::try_new(options).map_err(|error| error.to_string())
    }

    fn preflight_fastembed(
        config: &EmbeddingProviderConfig,
    ) -> Result<(EmbeddingModel, usize), String> {
        let model_name = if config.model.trim().is_empty() {
            default_embedding_model()
        } else {
            config.model.clone()
        };
        let (model, dimensions) = resolve_fastembed_model(&model_name)?;

        if let Some(expected_dimensions) = config.expected_dimensions {
            if expected_dimensions != dimensions {
                return Err(format!(
                    "embedding dimension mismatch for model `{model_name}`: expected {expected_dimensions}, actual {dimensions}"
                ));
            }
        }

        Ok((model, dimensions))
    }

    fn resolve_fastembed_model(model: &str) -> Result<(EmbeddingModel, usize), String> {
        let parsed = model
            .parse::<EmbeddingModel>()
            .or_else(|_| alias_model(model))
            .map_err(|_| format!("unsupported fastembed model `{model}`"))?;
        let dimensions = TextEmbedding::list_supported_models()
            .into_iter()
            .find(|info| info.model == parsed)
            .map(|info| info.dim)
            .ok_or_else(|| format!("missing dimension metadata for fastembed model `{model}`"))?;
        Ok((parsed, dimensions))
    }

    fn alias_model(model: &str) -> Result<EmbeddingModel, ()> {
        let normalized = model.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "all-minilm-l6-v2"
            | "sentence-transformers/all-minilm-l6-v2"
            | "sentence-transformers/all-minilm-l6-v2-onnx"
            | "allminilml6v2" => Ok(EmbeddingModel::AllMiniLML6V2),
            "bge-small-en-v1.5" | "baai/bge-small-en-v1.5" | "bgesmallenv15" => {
                Ok(EmbeddingModel::BGESmallENV15)
            }
            "bge-base-en-v1.5" | "baai/bge-base-en-v1.5" | "bgebaseenv15" => {
                Ok(EmbeddingModel::BGEBaseENV15)
            }
            _ => Err(()),
        }
    }
}

#[cfg(feature = "fastembed")]
fn render_fastembed_input(config: &EmbeddingProviderConfig, request: &EmbeddingRequest) -> String {
    match request.purpose {
        EmbeddingPurpose::Document => format!("{}{}", config.passage_prefix, request.text),
        EmbeddingPurpose::Query => format!("{}{}", config.query_prefix, request.text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_config() -> EmbeddingProviderConfig {
        EmbeddingProviderConfig {
            provider: EmbeddingProviderKind::Mock,
            model: "mock/stable".to_owned(),
            expected_dimensions: Some(8),
            batch_size: 8,
            query_prefix: "query: ".to_owned(),
            passage_prefix: "passage: ".to_owned(),
            mock_seed: 42,
            show_download_progress: false,
        }
    }

    #[test]
    fn disabled_provider_surfaces_fallback_cleanly() {
        let provider = build_embedding_provider(EmbeddingProviderConfig::default());

        assert_eq!(
            provider.availability(),
            EmbeddingAvailability::Disabled {
                reason: "memory.embedding.provider is disabled".to_owned(),
            }
        );

        let err = provider
            .embed(&[EmbeddingRequest::document("remember the key")])
            .expect_err("disabled provider should not generate embeddings");
        assert!(matches!(err, EmbeddingProviderError::Disabled { .. }));
    }

    #[test]
    fn mock_provider_is_deterministic() {
        let provider = MockEmbeddingProvider::new(mock_config());
        let requests = vec![
            EmbeddingRequest::document("Remember the observatory key."),
            EmbeddingRequest::query("Where is the observatory key?"),
        ];

        let first = provider
            .embed(&requests)
            .expect("mock embeddings should work");
        let second = provider
            .embed(&requests)
            .expect("mock embeddings should work");

        assert_eq!(first, second);
        assert_eq!(first.provider.dimensions, 8);
        assert_eq!(
            first.embeddings[0].content.source_text_hash,
            source_text_hash("Remember the observatory key.")
        );
    }

    #[cfg(not(feature = "fastembed"))]
    #[test]
    fn fastembed_requests_degrade_to_unavailable_when_feature_is_missing() {
        let provider = build_embedding_provider(EmbeddingProviderConfig {
            provider: EmbeddingProviderKind::Fastembed,
            ..EmbeddingProviderConfig::default()
        });

        assert_eq!(
            provider.availability(),
            EmbeddingAvailability::Unavailable {
                reason: "ozone-memory was built without the `fastembed` feature".to_owned(),
            }
        );
    }
}
