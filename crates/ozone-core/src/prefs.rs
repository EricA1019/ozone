use std::io::ErrorKind;
use std::fs;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::paths;

/// Read the shared preferences file as raw JSON.
///
/// Returns `Ok(None)` when the preferences file is not present. Returns an
/// error when the file cannot be read or the JSON is invalid.
pub fn read_preferences_json() -> Result<Option<Value>> {
    let Some(path) = paths::preferences_path() else {
        return Ok(None);
    };

    match fs::read_to_string(&path) {
        Ok(text) => {
            let value: Value = serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse preferences file {}", path.display()))?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to read preferences file {}", path.display())),
    }
}

/// Write a JSON value to the shared preferences file (pretty-printed).
pub fn write_preferences_json(value: &Value) -> Result<()> {
    if let Some(path) = paths::preferences_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(value)?;
        fs::write(&path, format!("{text}\n"))?;
    }
    Ok(())
}
