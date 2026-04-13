//! Layered configuration for ozone+.
//!
//! Priority (highest wins):
//!   env overrides  >  session config  >  global config  >  baked defaults
//!
//! Uses the `config` crate for deep merge so that a session config can
//! override a single nested key without replacing the entire table.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use config::{Config, Environment, File, FileFormat};
use ozone_memory::{
    EmbeddingProviderConfig, EmbeddingProviderKind, ProvenanceWeights, RetrievalWeights,
};
use serde::{Deserialize, Serialize};

use crate::error::InferenceError;

// ---------------------------------------------------------------------------
// Default TOML embedded in the binary
// ---------------------------------------------------------------------------

const DEFAULT_CONFIG_TOML: &str = r#"
[meta]
config_version = 1

[backend]
url = "http://localhost:5001"
type = "koboldcpp"
prompt_template = "chatml"

[backend.health]
poll_interval_secs = 30
timeout_secs = 5

[backend.rate_limit]
min_interval_ms = 500
max_pending_requests = 2
retry_after_429_ms = 5000
backoff_multiplier = 1.5
max_backoff_ms = 30000

[context]
max_tokens = 8192
safety_margin_pct = 10

[tasks]
max_concurrent_jobs = 3
max_queue_size = 20
stale_job_timeout_secs = 300

[memory]
hybrid_alpha = 0.5
max_active_embeddings = 10000
archive_after_turns = 1000
compaction_interval_hours = 24

[memory.retrieval_weights]
semantic = 0.35
importance = 0.25
recency = 0.20
provenance = 0.20

[memory.provenance_weights]
user_authored = 1.0
character_card = 0.9
lorebook = 0.85
system_generated = 0.7
utility_model = 0.6
imported_external = 0.5

[memory.embedding]
provider = "disabled"
model = "BAAI/bge-small-en-v1.5"
expected_dimensions = 384
batch_size = 256
query_prefix = "query: "
passage_prefix = "passage: "
mock_seed = 7
show_download_progress = false

[memory.lifecycle.storage_tiers]
reduced_after_messages = 100
minimal_after_messages = 1000

[memory.lifecycle.stale_artifacts]
max_age_messages = 500
max_age_hours = 168

[memory.lifecycle.garbage_collection]
max_active_embeddings = 10000
purge_unreferenced_backlog = true
compaction_interval_hours = 24

[logging]
level = "info"
file = true
stderr = false
"#;

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OzoneConfig {
    #[serde(default)]
    pub meta: MetaConfig,
    #[serde(default)]
    pub backend: BackendConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub tasks: TasksConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MetaConfig {
    #[serde(default = "default_config_version")]
    pub config_version: u32,
}

fn default_config_version() -> u32 {
    1
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self { config_version: 1 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BackendConfig {
    #[serde(default = "default_backend_url")]
    pub url: String,
    #[serde(default = "default_backend_type")]
    pub r#type: String,
    #[serde(default = "default_prompt_template")]
    pub prompt_template: String,
    #[serde(default)]
    pub health: BackendHealthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

fn default_backend_url() -> String {
    "http://localhost:5001".to_string()
}

fn default_backend_type() -> String {
    "koboldcpp".to_string()
}

fn default_prompt_template() -> String {
    "chatml".to_string()
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            url: default_backend_url(),
            r#type: default_backend_type(),
            prompt_template: default_prompt_template(),
            health: BackendHealthConfig::default(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BackendHealthConfig {
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_poll_interval_secs() -> u64 {
    30
}

fn default_timeout_secs() -> u64 {
    5
}

impl Default for BackendHealthConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: default_poll_interval_secs(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RateLimitConfig {
    #[serde(default = "default_min_interval_ms")]
    pub min_interval_ms: u64,
    #[serde(default = "default_max_pending_requests")]
    pub max_pending_requests: usize,
    #[serde(default = "default_retry_after_429_ms")]
    pub retry_after_429_ms: u64,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f32,
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
}

fn default_min_interval_ms() -> u64 {
    500
}
fn default_max_pending_requests() -> usize {
    2
}
fn default_retry_after_429_ms() -> u64 {
    5000
}
fn default_backoff_multiplier() -> f32 {
    1.5
}
fn default_max_backoff_ms() -> u64 {
    30_000
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: default_min_interval_ms(),
            max_pending_requests: default_max_pending_requests(),
            retry_after_429_ms: default_retry_after_429_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ContextConfig {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_safety_margin_pct")]
    pub safety_margin_pct: u8,
}

fn default_max_tokens() -> usize {
    8192
}
fn default_safety_margin_pct() -> u8 {
    10
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            safety_margin_pct: default_safety_margin_pct(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TasksConfig {
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    #[serde(default = "default_stale_job_timeout_secs")]
    pub stale_job_timeout_secs: u64,
}

fn default_max_concurrent_jobs() -> usize {
    3
}
fn default_max_queue_size() -> usize {
    20
}
fn default_stale_job_timeout_secs() -> u64 {
    300
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: default_max_concurrent_jobs(),
            max_queue_size: default_max_queue_size(),
            stale_job_timeout_secs: default_stale_job_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MemoryConfig {
    #[serde(default = "default_hybrid_alpha")]
    pub hybrid_alpha: f32,
    #[serde(default)]
    pub retrieval_weights: RetrievalWeights,
    #[serde(default)]
    pub provenance_weights: ProvenanceWeights,
    #[serde(default)]
    pub embedding: EmbeddingProviderConfig,
    #[serde(default = "default_max_active_embeddings")]
    pub max_active_embeddings: usize,
    #[serde(default = "default_archive_after_turns")]
    pub archive_after_turns: usize,
    #[serde(default = "default_compaction_interval_hours")]
    pub compaction_interval_hours: u64,
    #[serde(default)]
    pub lifecycle: MemoryLifecycleConfig,
    #[serde(default)]
    pub summary: SummaryConfig,
}

fn default_hybrid_alpha() -> f32 {
    0.5
}

fn default_max_active_embeddings() -> usize {
    10_000
}

fn default_archive_after_turns() -> usize {
    1_000
}

fn default_compaction_interval_hours() -> u64 {
    24
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MemoryLifecycleConfig {
    #[serde(default)]
    pub storage_tiers: StorageTierPolicy,
    #[serde(default)]
    pub stale_artifacts: StaleArtifactPolicy,
    #[serde(default)]
    pub garbage_collection: GarbageCollectionPolicy,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StorageTierPolicy {
    #[serde(default = "default_reduced_after_messages")]
    pub reduced_after_messages: usize,
    #[serde(default = "default_minimal_after_messages")]
    pub minimal_after_messages: usize,
}

fn default_reduced_after_messages() -> usize {
    100
}

fn default_minimal_after_messages() -> usize {
    default_archive_after_turns()
}

impl Default for StorageTierPolicy {
    fn default() -> Self {
        Self {
            reduced_after_messages: default_reduced_after_messages(),
            minimal_after_messages: default_minimal_after_messages(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StaleArtifactPolicy {
    #[serde(default = "default_stale_artifact_max_age_messages")]
    pub max_age_messages: usize,
    #[serde(default = "default_stale_artifact_max_age_hours")]
    pub max_age_hours: u64,
}

fn default_stale_artifact_max_age_messages() -> usize {
    500
}

fn default_stale_artifact_max_age_hours() -> u64 {
    168
}

impl Default for StaleArtifactPolicy {
    fn default() -> Self {
        Self {
            max_age_messages: default_stale_artifact_max_age_messages(),
            max_age_hours: default_stale_artifact_max_age_hours(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct GarbageCollectionPolicy {
    #[serde(default = "default_max_active_embeddings")]
    pub max_active_embeddings: usize,
    #[serde(default = "default_purge_unreferenced_backlog")]
    pub purge_unreferenced_backlog: bool,
    #[serde(default = "default_compaction_interval_hours")]
    pub compaction_interval_hours: u64,
}

fn default_purge_unreferenced_backlog() -> bool {
    true
}

impl Default for GarbageCollectionPolicy {
    fn default() -> Self {
        Self {
            max_active_embeddings: default_max_active_embeddings(),
            purge_unreferenced_backlog: default_purge_unreferenced_backlog(),
            compaction_interval_hours: default_compaction_interval_hours(),
        }
    }
}

/// Configuration for deterministic summary generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummaryConfig {
    /// Maximum number of sentences to include in a chunk summary.
    #[serde(default = "default_chunk_max_sentences")]
    pub chunk_max_sentences: usize,

    /// Minimum number of messages before auto-generating a session synopsis.
    #[serde(default = "default_synopsis_min_messages")]
    pub synopsis_min_messages: usize,

    /// Maximum number of sentences in a session synopsis.
    #[serde(default = "default_synopsis_max_sentences")]
    pub synopsis_max_sentences: usize,

    /// Whether to auto-generate a synopsis when a session is closed.
    #[serde(default = "default_auto_synopsis_on_close")]
    pub auto_synopsis_on_close: bool,
}

fn default_chunk_max_sentences() -> usize {
    5
}
fn default_synopsis_min_messages() -> usize {
    6
}
fn default_synopsis_max_sentences() -> usize {
    3
}
fn default_auto_synopsis_on_close() -> bool {
    true
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            chunk_max_sentences: default_chunk_max_sentences(),
            synopsis_min_messages: default_synopsis_min_messages(),
            synopsis_max_sentences: default_synopsis_max_sentences(),
            auto_synopsis_on_close: default_auto_synopsis_on_close(),
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            hybrid_alpha: default_hybrid_alpha(),
            retrieval_weights: RetrievalWeights::default(),
            provenance_weights: ProvenanceWeights::default(),
            embedding: EmbeddingProviderConfig::default(),
            max_active_embeddings: default_max_active_embeddings(),
            archive_after_turns: default_archive_after_turns(),
            compaction_interval_hours: default_compaction_interval_hours(),
            lifecycle: MemoryLifecycleConfig::default(),
            summary: SummaryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_file")]
    pub file: bool,
    #[serde(default)]
    pub stderr: bool,
    #[serde(default)]
    pub subsystem_levels: HashMap<String, String>,
}

fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_file() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
            stderr: false,
            subsystem_levels: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for loading `OzoneConfig` from layered sources.
///
/// ```rust
/// use ozone_inference::config::ConfigLoader;
///
/// let config = ConfigLoader::new()
///     .global_config_path("/home/user/.config/ozone/config.toml")
///     .build()
///     .expect("config load failed");
/// ```
#[derive(Debug, Default)]
pub struct ConfigLoader {
    global_config_path: Option<PathBuf>,
    session_config_path: Option<PathBuf>,
    /// Extra TOML fragments for overriding, used in tests.
    extra_toml: Option<String>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the global config path. If not set, the XDG default is used.
    pub fn global_config_path(mut self, path: impl AsRef<Path>) -> Self {
        self.global_config_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Optional per-session config path. Session values override global values.
    pub fn session_config_path(mut self, path: impl AsRef<Path>) -> Self {
        self.session_config_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Inject an extra TOML fragment on top of all sources. Useful in tests.
    pub fn extra_toml_override(mut self, toml: impl Into<String>) -> Self {
        self.extra_toml = Some(toml.into());
        self
    }

    /// Load and merge all sources, returning a validated `OzoneConfig`.
    pub fn build(self) -> anyhow::Result<OzoneConfig> {
        let mut builder = Config::builder()
            // 1. Baked defaults
            .add_source(File::from_str(DEFAULT_CONFIG_TOML, FileFormat::Toml));

        // 2. Global user config (~/.config/ozone/config.toml or explicit)
        let global_path = self.global_config_path.or_else(default_global_config_path);
        if let Some(path) = global_path {
            builder = builder.add_source(
                File::from(path.as_path())
                    .format(FileFormat::Toml)
                    .required(false),
            );
        }

        // 3. Per-session config overrides
        if let Some(path) = self.session_config_path {
            builder = builder.add_source(
                File::from(path.as_path())
                    .format(FileFormat::Toml)
                    .required(false),
            );
        }

        // 4. Extra TOML fragment (for test injection or character card overrides)
        if let Some(toml) = self.extra_toml {
            builder = builder.add_source(File::from_str(&toml, FileFormat::Toml));
        }

        // 5. Environment variables: OZONE__BACKEND__URL → backend.url
        builder = builder.add_source(
            Environment::with_prefix("OZONE")
                .separator("__")
                .try_parsing(true),
        );

        let cfg: OzoneConfig = builder
            .build()
            .map_err(InferenceError::ConfigLoad)?
            .try_deserialize()
            .map_err(InferenceError::ConfigLoad)?;

        validate_config(&cfg)?;
        Ok(cfg)
    }
}

fn default_global_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "ozone")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

fn validate_config(cfg: &OzoneConfig) -> anyhow::Result<()> {
    if cfg.backend.url.is_empty() {
        return Err(InferenceError::ConfigInvalid {
            key: "backend.url".into(),
            reason: "must not be empty".into(),
        }
        .into());
    }
    if cfg.context.safety_margin_pct > 50 {
        return Err(InferenceError::ConfigInvalid {
            key: "context.safety_margin_pct".into(),
            reason: "must be ≤ 50".into(),
        }
        .into());
    }
    if !cfg.memory.hybrid_alpha.is_finite() || !(0.0..=1.0).contains(&cfg.memory.hybrid_alpha) {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.hybrid_alpha".into(),
            reason: "must be in the range [0.0, 1.0]".into(),
        }
        .into());
    }
    cfg.memory
        .retrieval_weights
        .validate()
        .map_err(|error| InferenceError::ConfigInvalid {
            key: "memory.retrieval_weights".into(),
            reason: error.to_string(),
        })?;
    cfg.memory
        .provenance_weights
        .validate()
        .map_err(|error| InferenceError::ConfigInvalid {
            key: "memory.provenance_weights".into(),
            reason: error.to_string(),
        })?;
    if cfg.memory.max_active_embeddings == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.max_active_embeddings".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.archive_after_turns == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.archive_after_turns".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.compaction_interval_hours == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.compaction_interval_hours".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.lifecycle.storage_tiers.reduced_after_messages == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.storage_tiers.reduced_after_messages".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.lifecycle.storage_tiers.minimal_after_messages == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.storage_tiers.minimal_after_messages".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.lifecycle.storage_tiers.minimal_after_messages
        <= cfg.memory.lifecycle.storage_tiers.reduced_after_messages
    {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.storage_tiers.minimal_after_messages".into(),
            reason: "must be greater than reduced_after_messages".into(),
        }
        .into());
    }
    if cfg.memory.lifecycle.stale_artifacts.max_age_messages == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.stale_artifacts.max_age_messages".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.lifecycle.stale_artifacts.max_age_hours == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.stale_artifacts.max_age_hours".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg
        .memory
        .lifecycle
        .garbage_collection
        .max_active_embeddings
        == 0
    {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.garbage_collection.max_active_embeddings".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg
        .memory
        .lifecycle
        .garbage_collection
        .compaction_interval_hours
        == 0
    {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.lifecycle.garbage_collection.compaction_interval_hours".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if cfg.memory.embedding.batch_size == 0 {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.embedding.batch_size".into(),
            reason: "must be greater than zero".into(),
        }
        .into());
    }
    if matches!(
        cfg.memory.embedding.expected_dimensions,
        Some(expected_dimensions) if expected_dimensions == 0
    ) {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.embedding.expected_dimensions".into(),
            reason: "must be greater than zero when set".into(),
        }
        .into());
    }
    if !matches!(
        cfg.memory.embedding.provider,
        EmbeddingProviderKind::Disabled
    ) && cfg.memory.embedding.model.trim().is_empty()
    {
        return Err(InferenceError::ConfigInvalid {
            key: "memory.embedding.model".into(),
            reason: "must not be empty when embeddings are enabled".into(),
        }
        .into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load_without_files() {
        let cfg = ConfigLoader::new()
            // Point at a nonexistent global path so no real file is read.
            .global_config_path("/nonexistent/path/config.toml")
            .build()
            .expect("defaults should always load");

        assert_eq!(cfg.backend.url, "http://localhost:5001");
        assert_eq!(cfg.backend.r#type, "koboldcpp");
        assert_eq!(cfg.backend.prompt_template, "chatml");
        assert_eq!(cfg.context.max_tokens, 8192);
        assert_eq!(cfg.context.safety_margin_pct, 10);
        assert_eq!(cfg.backend.health.poll_interval_secs, 30);
        assert_eq!(cfg.backend.rate_limit.min_interval_ms, 500);
        assert_eq!(cfg.memory.hybrid_alpha, 0.5);
        assert_eq!(cfg.memory.max_active_embeddings, 10_000);
        assert_eq!(cfg.memory.archive_after_turns, 1_000);
        assert_eq!(cfg.memory.compaction_interval_hours, 24);
        assert_eq!(cfg.memory.lifecycle, MemoryLifecycleConfig::default());
        assert_eq!(cfg.memory.retrieval_weights, RetrievalWeights::default());
        assert_eq!(cfg.memory.provenance_weights, ProvenanceWeights::default());
        assert_eq!(
            cfg.memory.embedding.provider,
            EmbeddingProviderKind::Disabled
        );
        assert_eq!(cfg.memory.embedding.model, "BAAI/bge-small-en-v1.5");
        assert_eq!(cfg.memory.embedding.expected_dimensions, Some(384));
    }

    #[test]
    fn session_override_deep_merges_single_key() {
        // Only override max_tokens — everything else should stay at defaults.
        let session_toml = r#"
[context]
max_tokens = 4096
"#;
        let cfg = ConfigLoader::new()
            .global_config_path("/nonexistent/path/config.toml")
            .extra_toml_override(session_toml)
            .build()
            .expect("session override should load");

        assert_eq!(cfg.context.max_tokens, 4096);
        // Deep merge: safety_margin_pct was NOT in the override, so default wins.
        assert_eq!(cfg.context.safety_margin_pct, 10);
        // Backend section untouched.
        assert_eq!(cfg.backend.url, "http://localhost:5001");
    }

    #[test]
    fn layer_priority_global_then_session() {
        let global_toml = r#"
[backend]
url = "http://192.168.1.10:5001"
"#;
        let session_toml = r#"
[context]
max_tokens = 2048
"#;
        // Simulate global layer via first extra_toml, session via second.
        // We chain two loaders here by stacking two extra fragments.
        // (In real use they'd be separate file paths.)
        let combined = format!("{global_toml}\n{session_toml}");
        let cfg = ConfigLoader::new()
            .global_config_path("/nonexistent/path/config.toml")
            .extra_toml_override(combined)
            .build()
            .expect("layered override should load");

        assert_eq!(cfg.backend.url, "http://192.168.1.10:5001");
        assert_eq!(cfg.context.max_tokens, 2048);
        assert_eq!(cfg.context.safety_margin_pct, 10);
    }

    #[test]
    fn memory_override_parses_nested_embedding_weight_and_lifecycle_config() {
        let override_toml = r#"
[memory]
hybrid_alpha = 0.25
max_active_embeddings = 2048
archive_after_turns = 256
compaction_interval_hours = 12

[memory.retrieval_weights]
semantic = 0.4
importance = 0.2
recency = 0.2
provenance = 0.2

[memory.provenance_weights]
user_authored = 0.95
character_card = 0.9
lorebook = 0.8
system_generated = 0.75
utility_model = 0.65
imported_external = 0.55

[memory.embedding]
provider = "mock"
model = "mock/stable"
expected_dimensions = 16
batch_size = 32
query_prefix = "q: "
passage_prefix = "d: "
mock_seed = 99

[memory.lifecycle.storage_tiers]
reduced_after_messages = 150
minimal_after_messages = 1500

[memory.lifecycle.stale_artifacts]
max_age_messages = 300
max_age_hours = 72

[memory.lifecycle.garbage_collection]
max_active_embeddings = 4096
purge_unreferenced_backlog = false
compaction_interval_hours = 6
"#;

        let cfg = ConfigLoader::new()
            .global_config_path("/nonexistent/path/config.toml")
            .extra_toml_override(override_toml)
            .build()
            .expect("memory override should parse");

        assert_eq!(cfg.memory.hybrid_alpha, 0.25);
        assert_eq!(cfg.memory.max_active_embeddings, 2048);
        assert_eq!(cfg.memory.archive_after_turns, 256);
        assert_eq!(cfg.memory.compaction_interval_hours, 12);
        assert_eq!(
            cfg.memory.lifecycle.storage_tiers,
            StorageTierPolicy {
                reduced_after_messages: 150,
                minimal_after_messages: 1500,
            }
        );
        assert_eq!(
            cfg.memory.lifecycle.stale_artifacts,
            StaleArtifactPolicy {
                max_age_messages: 300,
                max_age_hours: 72,
            }
        );
        assert_eq!(
            cfg.memory.lifecycle.garbage_collection,
            GarbageCollectionPolicy {
                max_active_embeddings: 4096,
                purge_unreferenced_backlog: false,
                compaction_interval_hours: 6,
            }
        );
        assert_eq!(
            cfg.memory.retrieval_weights,
            RetrievalWeights {
                semantic: 0.4,
                importance: 0.2,
                recency: 0.2,
                provenance: 0.2,
            }
        );
        assert_eq!(cfg.memory.embedding.provider, EmbeddingProviderKind::Mock);
        assert_eq!(cfg.memory.embedding.model, "mock/stable");
        assert_eq!(cfg.memory.embedding.expected_dimensions, Some(16));
        assert_eq!(cfg.memory.embedding.batch_size, 32);
        assert_eq!(cfg.memory.embedding.query_prefix, "q: ");
        assert_eq!(cfg.memory.embedding.passage_prefix, "d: ");
        assert_eq!(cfg.memory.embedding.mock_seed, 99);
        assert_eq!(cfg.memory.provenance_weights.user_authored, 0.95);
        assert_eq!(cfg.memory.provenance_weights.imported_external, 0.55);
    }

    #[test]
    fn invalid_config_is_rejected() {
        let bad_toml = r#"
[context]
safety_margin_pct = 99
"#;
        let result = ConfigLoader::new()
            .global_config_path("/nonexistent/path/config.toml")
            .extra_toml_override(bad_toml)
            .build();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("safety_margin_pct"),
            "error mentions the key: {msg}"
        );
    }

    #[test]
    fn invalid_memory_retrieval_weights_are_rejected() {
        let bad_toml = r#"
[memory.retrieval_weights]
semantic = 0.6
importance = 0.3
recency = 0.2
provenance = 0.2
"#;

        let result = ConfigLoader::new()
            .global_config_path("/nonexistent/path/config.toml")
            .extra_toml_override(bad_toml)
            .build();

        let err = result.expect_err("invalid retrieval weights should fail");
        assert!(
            err.to_string().contains("memory.retrieval_weights"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn invalid_memory_lifecycle_config_is_rejected() {
        let cases = [
            (
                r#"
[memory.lifecycle.storage_tiers]
reduced_after_messages = 0
"#,
                "memory.lifecycle.storage_tiers.reduced_after_messages",
            ),
            (
                r#"
[memory.lifecycle.storage_tiers]
minimal_after_messages = 0
"#,
                "memory.lifecycle.storage_tiers.minimal_after_messages",
            ),
            (
                r#"
[memory.lifecycle.storage_tiers]
reduced_after_messages = 100
minimal_after_messages = 100
"#,
                "memory.lifecycle.storage_tiers.minimal_after_messages",
            ),
            (
                r#"
[memory.lifecycle.stale_artifacts]
max_age_messages = 0
"#,
                "memory.lifecycle.stale_artifacts.max_age_messages",
            ),
            (
                r#"
[memory.lifecycle.stale_artifacts]
max_age_hours = 0
"#,
                "memory.lifecycle.stale_artifacts.max_age_hours",
            ),
            (
                r#"
[memory.lifecycle.garbage_collection]
max_active_embeddings = 0
"#,
                "memory.lifecycle.garbage_collection.max_active_embeddings",
            ),
            (
                r#"
[memory.lifecycle.garbage_collection]
compaction_interval_hours = 0
"#,
                "memory.lifecycle.garbage_collection.compaction_interval_hours",
            ),
        ];

        for (bad_toml, expected_key) in cases {
            let result = ConfigLoader::new()
                .global_config_path("/nonexistent/path/config.toml")
                .extra_toml_override(bad_toml)
                .build();

            let err = result.expect_err("invalid lifecycle config should fail");
            assert!(
                err.to_string().contains(expected_key),
                "unexpected error for {expected_key}: {err}"
            );
        }
    }
}
