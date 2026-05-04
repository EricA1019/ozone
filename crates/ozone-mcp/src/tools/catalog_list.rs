use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::{Context, Result};
use serde_json::json;

/// Lists GGUF model files in the models directory with metadata.
pub fn catalog_list_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> Result<ToolReply> {
    let sandbox_id = super::optional_string(args, "sandboxId");
    let (models_dir, prefs_path) = server.with_sandbox_env(sandbox_id.as_deref(), || {
        Ok((ozone_core::paths::models_dir(), ozone_core::paths::preferences_path()))
    })?;

    let mut models = Vec::new();
    if models_dir.exists() {
        for entry in std::fs::read_dir(&models_dir)
            .with_context(|| format!("failed to read models dir {}", models_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let metadata = std::fs::symlink_metadata(&path)?;
            let is_gguf = file_name.ends_with(".gguf");
            let broken_symlink = metadata.file_type().is_symlink() && !path.exists();
            if is_gguf || broken_symlink {
                models.push(json!({
                    "name": file_name,
                    "path": path,
                    "isSymlink": metadata.file_type().is_symlink(),
                    "isBrokenSymlink": broken_symlink,
                    "sizeBytes": if broken_symlink { None } else { std::fs::metadata(&path).ok().map(|value| value.len()) }
                }));
            }
        }
    }
    models.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));

    Ok(ToolReply::success(
        "Listed model catalog files".to_owned(),
        json!({
            "modelsDir": models_dir,
            "preferencesPath": prefs_path,
            "models": models
        }),
    ))
}
