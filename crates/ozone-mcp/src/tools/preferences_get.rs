     1|/// MCP tool: preferences get.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn preferences_get_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let sandbox_id = optional_string(args, "sandboxId");
     9|    let preferences_path =
    10|        server.with_sandbox_env(sandbox_id.as_deref(), || Ok(paths::preferences_path()))?;
    11|    let data = match preferences_path {
    12|        Some(path) if path.exists() => {
    13|            let text = fs::read_to_string(&path)
    14|                .with_context(|| format!("failed to read {}", path.display()))?;
    15|            let parsed = serde_json::from_str::<Value>(&text).ok();
    16|            json!({
    17|                "path": path,
    18|                "exists": true,
    19|                "raw": text,
    20|                "parsed": parsed
    21|            })
    22|        }
    23|        Some(path) => json!({
    24|            "path": path,
    25|            "exists": false,
    26|            "raw": null,
    27|            "parsed": null
    28|        }),
    29|        None => json!({
    30|            "path": null,
    31|            "exists": false,
    32|            "raw": null,
    33|            "parsed": null
    34|        }),
    35|    };
    36|
    37|    Ok(ToolReply::success(
    38|        "Loaded preferences file".to_owned(),
    39|        data,
    40|    ))
    41|}
    42|
    43|