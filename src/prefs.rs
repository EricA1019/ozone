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
            theme_preset: default_theme_preset(),
            show_inspector: false,
            timestamp_style: default_timestamp_style(),
            message_density: default_message_density(),
        }
    }
}

impl Preferences {
    pub fn has_llamacpp_profile(&self) -> bool {
        self.llamacpp_gpu_layers.is_some()
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


