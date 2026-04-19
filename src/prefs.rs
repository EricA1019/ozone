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
        }
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
