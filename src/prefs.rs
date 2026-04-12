use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

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
        }
    }
}

fn prefs_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "ozone")
        .map(|dirs| dirs.data_dir().join("preferences.json"))
}

pub async fn load_prefs() -> Preferences {
    let Some(path) = prefs_path() else { return Preferences::default() };
    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Preferences::default(),
    }
}

pub async fn save_prefs(prefs: &Preferences) -> Result<()> {
    let Some(path) = prefs_path() else { return Ok(()) };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let text = serde_json::to_string_pretty(prefs)?;
    fs::write(&path, format!("{text}\n")).await?;
    Ok(())
}
