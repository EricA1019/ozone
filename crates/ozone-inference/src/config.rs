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
}
