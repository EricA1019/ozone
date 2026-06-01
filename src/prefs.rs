use std::collections::BTreeMap;
use std::io::ErrorKind;

use anyhow::{Context, Result};
use ozone_core::paths;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::ui::BackendMode;

/// Product tier for the ozone family
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Lite,
    Base,
    Plus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontendPreference {
    SillyTavern,
    OzonePlus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub last_model_name: String,
    pub last_context_size: Option<u32>,
    pub last_gpu_layers: Option<i32>,
    pub last_quant_kv: Option<u8>,
    pub last_threads: Option<u32>,
    pub last_blas_threads: Option<u32>,
    pub no_browser: bool,
    #[serde(default)]
    pub preferred_backend: Option<BackendMode>,
    #[serde(default)]
    pub preferred_frontend: Option<FrontendPreference>,
    #[serde(default)]
    pub preferred_tier: Option<Tier>,
    /// Legacy toggle for the old side-by-side monitor launch path.
    #[serde(default)]
    pub side_by_side_monitor: bool,
    #[serde(default)]
    pub llamacpp_gpu_layers: Option<i32>,
    #[serde(default)]
    pub llamacpp_context_size: Option<u32>,
    #[serde(default)]
    pub llamacpp_threads: Option<u32>,
    #[serde(default)]
    pub model_launch_overrides: BTreeMap<String, ModelLaunchOverride>,
    #[serde(default)]
    pub saved_launch_profiles: BTreeMap<String, Vec<SavedLaunchProfile>>,
    #[serde(default)]
    pub default_launch_profiles: BTreeMap<String, String>,
    /// Serialised `ThemePreset` string (e.g. `"dark-mint"`, `"ozone-dark"`, `"high-contrast"`).
    /// Converted to the TUI enum at startup; unknown values fall back to `DarkMint`.
    #[serde(default = "default_theme_preset")]
    pub theme_preset: String,
    /// Whether the inspector pane is shown when the chat shell first opens.
    #[serde(default)]
    pub show_inspector: bool,
    /// How message timestamps are displayed: `"relative"`, `"absolute"`, or `"off"`.
    #[serde(default = "default_timestamp_style")]
    pub timestamp_style: String,
    /// Message list density: `"comfortable"` or `"compact"`.
    #[serde(default = "default_message_density")]
    pub message_density: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLaunchOverride {
    #[serde(default)]
    pub context_size: Option<u32>,
    #[serde(default)]
    pub gpu_layers: Option<i32>,
    #[serde(default)]
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLaunchProfile {
    pub profile_name: String,
    pub context_size: u32,
    pub gpu_layers: i32,
    #[serde(default = "default_quant_kv")]
    pub quant_kv: u8,
    #[serde(default)]
    pub threads: Option<u32>,
}

fn coerce_supported_tier(tier: Option<Tier>) -> Option<Tier> {
    match tier {
        Some(Tier::Plus) => Some(Tier::Base),
        other => other,
    }
}

fn coerce_supported_frontend(
    frontend: Option<FrontendPreference>,
) -> Option<FrontendPreference> {
    match frontend {
        Some(FrontendPreference::OzonePlus) => None,
        other => other,
    }
}

fn coerce_supported_backend(_backend: Option<BackendMode>) -> Option<BackendMode> {
    Some(BackendMode::LlamaCpp)
}

impl ModelLaunchOverride {
    pub fn is_empty(&self) -> bool {
        self.context_size.is_none() && self.gpu_layers.is_none() && self.threads.is_none()
    }
}

fn default_quant_kv() -> u8 {
    1
}

fn default_theme_preset() -> String {
    "dark-mint".to_string()
}

fn default_timestamp_style() -> String {
    "relative".to_string()
}

fn default_message_density() -> String {
    "comfortable".to_string()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            last_model_name: String::new(),
            last_context_size: None,
            last_gpu_layers: None,
            last_quant_kv: None,
            last_threads: None,
            last_blas_threads: None,
            no_browser: false,
            preferred_backend: Some(BackendMode::LlamaCpp),
            preferred_frontend: None,
            preferred_tier: None,
            side_by_side_monitor: false,
            llamacpp_gpu_layers: None,
            llamacpp_context_size: None,
            llamacpp_threads: None,
            model_launch_overrides: BTreeMap::new(),
            saved_launch_profiles: BTreeMap::new(),
            default_launch_profiles: BTreeMap::new(),
            theme_preset: default_theme_preset(),
            show_inspector: false,
            timestamp_style: default_timestamp_style(),
            message_density: default_message_density(),
        }
    }
}

impl Preferences {
    pub fn launch_override_for(&self, model_name: &str) -> Option<ModelLaunchOverride> {
        self.model_launch_overrides
            .get(model_name)
            .cloned()
            .or_else(|| self.legacy_launch_override_for(model_name))
            .filter(|override_state| !override_state.is_empty())
    }

    pub fn set_model_launch_override(
        &mut self,
        model_name: impl Into<String>,
        override_state: ModelLaunchOverride,
    ) {
        let model_name = model_name.into();
        if override_state.is_empty() {
            self.model_launch_overrides.remove(&model_name);
        } else {
            self.model_launch_overrides
                .insert(model_name, override_state);
        }
    }

    pub fn saved_launch_profiles_for(&self, model_name: &str) -> Vec<SavedLaunchProfile> {
        self.saved_launch_profiles
            .get(model_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn saved_launch_profile(
        &self,
        model_name: &str,
        profile_name: &str,
    ) -> Option<SavedLaunchProfile> {
        self.saved_launch_profiles
            .get(model_name)
            .and_then(|profiles| {
                profiles
                    .iter()
                    .find(|profile| profile.profile_name == profile_name)
            })
            .cloned()
    }

    pub fn upsert_saved_launch_profile(
        &mut self,
        model_name: impl Into<String>,
        profile: SavedLaunchProfile,
    ) {
        let model_name = model_name.into();
        let profiles = self.saved_launch_profiles.entry(model_name).or_default();
        if let Some(existing) = profiles
            .iter_mut()
            .find(|existing| existing.profile_name == profile.profile_name)
        {
            *existing = profile;
        } else {
            profiles.push(profile);
            profiles.sort_by(|left, right| left.profile_name.cmp(&right.profile_name));
        }
    }

    pub fn remove_saved_launch_profile(&mut self, model_name: &str, profile_name: &str) -> bool {
        let mut removed = false;
        let mut remove_model_entry = false;
        if let Some(profiles) = self.saved_launch_profiles.get_mut(model_name) {
            let original_len = profiles.len();
            profiles.retain(|profile| profile.profile_name != profile_name);
            removed = profiles.len() != original_len;
            remove_model_entry = profiles.is_empty();
        }
        if remove_model_entry {
            self.saved_launch_profiles.remove(model_name);
        }
        if self.default_saved_launch_profile_name_for(model_name) == Some(profile_name) {
            self.default_launch_profiles.remove(model_name);
        }
        removed
    }

    pub fn default_saved_launch_profile_name_for(&self, model_name: &str) -> Option<&str> {
        self.default_launch_profiles
            .get(model_name)
            .map(String::as_str)
            .filter(|name| self.saved_launch_profile(model_name, name).is_some())
    }

    pub fn set_default_saved_launch_profile(
        &mut self,
        model_name: impl Into<String>,
        profile_name: impl Into<String>,
    ) {
        self.default_launch_profiles
            .insert(model_name.into(), profile_name.into());
    }

    fn legacy_launch_override_for(&self, model_name: &str) -> Option<ModelLaunchOverride> {
        if self.last_model_name != model_name {
            return None;
        }

        let override_state = ModelLaunchOverride {
            context_size: self.llamacpp_context_size,
            gpu_layers: self.llamacpp_gpu_layers,
            threads: self.llamacpp_threads,
        };
        (!override_state.is_empty()).then_some(override_state)
    }
}

pub async fn load_prefs() -> Result<Preferences> {
    let path = paths::preferences_path().context("Could not determine preferences path")?;
    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str::<Preferences>(&text)
            .map(normalize_loaded_prefs)
            .with_context(|| format!("Failed to parse preferences file {}", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Preferences::default()),
        Err(error) => Err(error)
            .with_context(|| format!("Failed to read preferences file {}", path.display())),
    }
}

fn normalize_loaded_prefs(mut prefs: Preferences) -> Preferences {
    prefs.preferred_backend = coerce_supported_backend(prefs.preferred_backend);
    prefs.preferred_tier = coerce_supported_tier(prefs.preferred_tier);
    prefs.preferred_frontend = coerce_supported_frontend(prefs.preferred_frontend);
    prefs
}

pub async fn save_prefs(prefs: &Preferences) -> Result<()> {
    let Some(path) = paths::preferences_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let text = serde_json::to_string_pretty(prefs)?;
    fs::write(&path, format!("{text}\n")).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ModelLaunchOverride, Preferences, SavedLaunchProfile, Tier};
    use crate::test_support::env_lock;
    use crate::ui::BackendMode;
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-prefs-tests-{prefix}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            if root.exists() {
                std::fs::remove_dir_all(&root).unwrap();
            }
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn xdg_data_home(&self) -> PathBuf {
            self.root.join("xdg-data")
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    struct ScopedEnvVar {
        key: &'static str,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &'static str, value: impl AsRef<Path>) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value.as_ref());
            Self { key, previous }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn per_model_launch_override_round_trips() {
        let mut prefs = Preferences::default();
        prefs.set_model_launch_override(
            "model-a.gguf",
            ModelLaunchOverride {
                context_size: Some(16384),
                gpu_layers: Some(28),
                threads: None,
            },
        );

        assert_eq!(
            prefs.launch_override_for("model-a.gguf"),
            Some(ModelLaunchOverride {
                context_size: Some(16384),
                gpu_layers: Some(28),
                threads: None,
            })
        );
    }

    #[test]
    fn legacy_llamacpp_profile_is_used_as_fallback_override() {
        let prefs = Preferences {
            last_model_name: "legacy.gguf".into(),
            llamacpp_gpu_layers: Some(18),
            llamacpp_context_size: Some(8192),
            llamacpp_threads: Some(6),
            ..Preferences::default()
        };

        assert_eq!(
            prefs.launch_override_for("legacy.gguf"),
            Some(ModelLaunchOverride {
                context_size: Some(8192),
                gpu_layers: Some(18),
                threads: Some(6),
            })
        );
    }

    #[test]
    fn saved_launch_profiles_round_trip_and_track_default() {
        let mut prefs = Preferences::default();
        prefs.upsert_saved_launch_profile(
            "model-a.gguf",
            SavedLaunchProfile {
                profile_name: "custom-1".into(),
                context_size: 16384,
                gpu_layers: 20,
                quant_kv: 1,
                threads: Some(8),
            },
        );
        prefs.set_default_saved_launch_profile("model-a.gguf", "custom-1");

        assert_eq!(
            prefs.default_saved_launch_profile_name_for("model-a.gguf"),
            Some("custom-1")
        );
        assert_eq!(
            prefs.saved_launch_profile("model-a.gguf", "custom-1"),
            Some(SavedLaunchProfile {
                profile_name: "custom-1".into(),
                context_size: 16384,
                gpu_layers: 20,
                quant_kv: 1,
                threads: Some(8),
            })
        );
    }

    #[test]
    fn load_prefs_missing_file_returns_defaults() {
        let _env_guard = env_lock();
        let sandbox = TestSandbox::new("missing-file");
        std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let prefs = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::load_prefs())
            .expect("missing prefs file should fall back to defaults");

        assert_eq!(prefs.preferred_backend, Some(BackendMode::LlamaCpp));
        assert_eq!(prefs.theme_preset, "dark-mint");
        assert_eq!(prefs.timestamp_style, "relative");
    }

    #[test]
    fn load_prefs_invalid_json_returns_error() {
        let _env_guard = env_lock();
        let sandbox = TestSandbox::new("invalid-json");
        std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let path = ozone_core::paths::preferences_path().expect("prefs path should resolve");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{invalid json").unwrap();

        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::load_prefs())
            .expect_err("invalid prefs JSON should return an error");

        let message = error.to_string();
        assert!(message.contains("Failed to parse preferences file"));
        assert!(message.contains(&path.display().to_string()));
    }

        #[test]
        fn load_prefs_coerces_plus_state_to_supported_surface() {
            let _env_guard = env_lock();
                let sandbox = TestSandbox::new("coerce-plus-state");
                std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
                let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
                let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

                let path = ozone_core::paths::preferences_path().expect("prefs path should resolve");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(
                        &path,
                        r#"{
    "version": 1,
    "last_model_name": "legacy.gguf",
    "no_browser": true,
    "preferred_frontend": "ozone-plus",
    "preferred_tier": "plus",
    "saved_launch_profiles": {
        "legacy.gguf": [
            {
                "profile_name": "custom-1",
                "context_size": 16384,
                "gpu_layers": 22,
                "quant_kv": 1,
                "threads": 6
            }
        ]
    },
    "default_launch_profiles": {
        "legacy.gguf": "custom-1"
    }
}
"#,
                )
                .unwrap();

                let prefs = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(super::load_prefs())
                        .expect("legacy plus prefs should migrate");

                assert_eq!(prefs.preferred_tier, Some(Tier::Base));
                assert_eq!(prefs.preferred_frontend, None);
                assert_eq!(
                        prefs.default_saved_launch_profile_name_for("legacy.gguf"),
                        Some("custom-1")
                );
        }

        #[test]
        fn load_prefs_preserves_lite_preference() {
            let _env_guard = env_lock();
                let sandbox = TestSandbox::new("preserve-lite-state");
                std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
                let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
                let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

                let path = ozone_core::paths::preferences_path().expect("prefs path should resolve");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(
                        &path,
                        r#"{
    "version": 1,
    "last_model_name": "lite.gguf",
    "preferred_tier": "lite"
}
"#,
                )
                .unwrap();

                let prefs = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(super::load_prefs())
                        .expect("lite prefs should migrate");

                assert_eq!(prefs.preferred_tier, Some(Tier::Lite));
        }

    #[test]
    fn load_prefs_coerces_legacy_backend_to_llamacpp() {
        let _env_guard = env_lock();
        let sandbox = TestSandbox::new("coerce-legacy-backend");
        std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let path = ozone_core::paths::preferences_path().expect("prefs path should resolve");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
    "version": 1,
    "last_model_name": "legacy.gguf",
    "preferred_backend": "ollama"
}
"#,
        )
        .unwrap();

        let prefs = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::load_prefs())
            .expect("legacy backend prefs should migrate");

        assert_eq!(prefs.preferred_backend, Some(BackendMode::LlamaCpp));
    }

    #[test]
    fn load_prefs_coerces_legacy_kobold_backend_to_llamacpp() {
        let _env_guard = env_lock();
        let sandbox = TestSandbox::new("coerce-legacy-kobold-backend");
        std::fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

        let path = ozone_core::paths::preferences_path().expect("prefs path should resolve");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
    "version": 1,
    "last_model_name": "legacy.gguf",
    "preferred_backend": "kobold-cpp"
}
"#,
        )
        .unwrap();

        let prefs = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::load_prefs())
            .expect("legacy kobold backend prefs should migrate");

        assert_eq!(prefs.preferred_backend, Some(BackendMode::LlamaCpp));
    }

    #[test]
    fn removing_saved_launch_profile_clears_default_marker() {
        let mut prefs = Preferences::default();
        prefs.upsert_saved_launch_profile(
            "model-a.gguf",
            SavedLaunchProfile {
                profile_name: "custom-1".into(),
                context_size: 8192,
                gpu_layers: 16,
                quant_kv: 1,
                threads: None,
            },
        );
        prefs.set_default_saved_launch_profile("model-a.gguf", "custom-1");

        assert!(prefs.remove_saved_launch_profile("model-a.gguf", "custom-1"));
        assert!(prefs.saved_launch_profiles_for("model-a.gguf").is_empty());
        assert_eq!(
            prefs.default_saved_launch_profile_name_for("model-a.gguf"),
            None
        );
    }
}
