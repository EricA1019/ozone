use std::collections::BTreeMap;

use anyhow::Result;
use ozone_core::paths;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::ui::{BackendMode, FrontendMode};

/// Product tier for the ozone family
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Lite,
    Base,
    Plus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub preferred_frontend: Option<FrontendMode>,
    #[serde(default)]
    pub preferred_tier: Option<Tier>,
    /// When true, "Launch ozone+ (side-by-side)" opens ozone+ in a new terminal
    /// window instead of replacing this process via exec().
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
    /// Whether the inspector pane is shown when ozone+ first opens.
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
            preferred_backend: None,
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

pub async fn load_prefs() -> Preferences {
    let Some(path) = paths::preferences_path() else {
        return Preferences::default();
    };
    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Preferences::default(),
    }
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
    use super::{ModelLaunchOverride, Preferences, SavedLaunchProfile};

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
