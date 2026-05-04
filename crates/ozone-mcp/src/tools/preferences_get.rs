use super::optional_string;
/// MCP tool: preferences get.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;
use ozone_core::paths;
use std::fs;

pub fn preferences_get_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let sandbox_id = optional_string(args, "sandboxId");
    let preferences_path =
        server.with_sandbox_env(sandbox_id.as_deref(), || Ok(paths::preferences_path()))?;
    let data = match preferences_path {
        Some(path) if path.exists() => {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let parsed = serde_json::from_str::<Value>(&text).ok();
            json!({
                "path": path,
                "exists": true,
                "raw": text,
                "parsed": parsed
            })
        }
        Some(path) => json!({
            "path": path,
            "exists": false,
            "raw": null,
            "parsed": null
        }),
        None => json!({
            "path": null,
            "exists": false,
            "raw": null,
            "parsed": null
        }),
    };

    Ok(ToolReply::success(
        "Loaded preferences file".to_owned(),
        data,
    ))
}

