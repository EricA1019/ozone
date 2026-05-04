     1|/// MCP tool: catalog list.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn catalog_list_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let sandbox_id = optional_string(args, "sandboxId");
     9|    let (models_dir, prefs_path) = server.with_sandbox_env(sandbox_id.as_deref(), || {
    10|        Ok((paths::models_dir(), paths::preferences_path()))
    11|    })?;
    12|
    13|    let mut models = Vec::new();
    14|    if models_dir.exists() {
    15|        for entry in fs::read_dir(&models_dir)
    16|            .with_context(|| format!("failed to read models dir {}", models_dir.display()))?
    17|        {
    18|            let entry = entry?;
    19|            let path = entry.path();
    20|            let file_name = entry.file_name().to_string_lossy().into_owned();
    21|            let metadata = fs::symlink_metadata(&path)?;
    22|            let is_gguf = file_name.ends_with(".gguf");
    23|            let broken_symlink = metadata.file_type().is_symlink() && !path.exists();
    24|            if is_gguf || broken_symlink {
    25|                models.push(json!({
    26|                    "name": file_name,
    27|                    "path": path,
    28|                    "isSymlink": metadata.file_type().is_symlink(),
    29|                    "isBrokenSymlink": broken_symlink,
    30|                    "sizeBytes": if broken_symlink { None } else { fs::metadata(&path).ok().map(|value| value.len()) }
    31|                }));
    32|            }
    33|        }
    34|    }
    35|    models.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    36|
    37|    Ok(ToolReply::success(
    38|        "Listed model catalog files".to_owned(),
    39|        json!({
    40|            "modelsDir": models_dir,
    41|            "preferencesPath": prefs_path,
    42|            "models": models
    43|        }),
    44|    ))
    45|}
    46|
    47|