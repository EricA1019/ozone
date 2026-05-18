use ozone_core::paths::preferences_path;
use ozone_tui::ThemePreset;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonePlusPrefs {
    #[serde(default = "default_theme_preset_str")]
    pub theme_preset: String,
    #[serde(default)]
    pub side_by_side_monitor: bool,
    #[serde(default)]
    pub show_inspector: bool,
    #[serde(default = "default_timestamp_style_str")]
    pub timestamp_style: String,
    #[serde(default = "default_message_density_str")]
    pub message_density: String,
}

fn default_theme_preset_str() -> String {
    "dark-mint".to_string()
}
fn default_timestamp_style_str() -> String {
    "relative".to_string()
}
fn default_message_density_str() -> String {
    "comfortable".to_string()
}

impl Default for OzonePlusPrefs {
    fn default() -> Self {
        Self {
            theme_preset: default_theme_preset_str(),
            side_by_side_monitor: false,
            show_inspector: false,
            timestamp_style: default_timestamp_style_str(),
            message_density: default_message_density_str(),
        }
    }
}

pub fn load_theme_preset() -> ThemePreset {
    let Some(path) = preferences_path() else {
        return ThemePreset::default();
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return ThemePreset::default(),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return ThemePreset::default(),
    };
    value
        .get("theme_preset")
        .and_then(|v| v.as_str())
        .map(ThemePreset::from_pref_str)
        .unwrap_or_default()
}

pub fn load_prefs_sync() -> OzonePlusPrefs {
    let Some(path) = preferences_path() else {
        return OzonePlusPrefs::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => OzonePlusPrefs::default(),
    }
}

pub fn save_prefs_sync(prefs: &OzonePlusPrefs) -> Result<(), String> {
    let Some(path) = preferences_path() else {
        return Ok(());
    };
    let existing_text = fs::read_to_string(&path).unwrap_or_default();
    let mut existing: serde_json::Value =
        serde_json::from_str(&existing_text).unwrap_or(serde_json::json!({}));
    if let Some(obj) = existing.as_object_mut() {
        let new_val = serde_json::to_value(prefs).map_err(|e| e.to_string())?;
        if let Some(new_obj) = new_val.as_object() {
            for (k, v) in new_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&existing).map_err(|e| e.to_string())?;
    fs::write(&path, format!("{text}\n")).map_err(|e| e.to_string())?;
    Ok(())
}