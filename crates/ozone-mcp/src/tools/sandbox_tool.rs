/// MCP tool: sandbox operations (create, destroy).
use crate::OzoneMcpServer;
use crate::ToolReply;
use crate::required_string;
use anyhow::anyhow;
use anyhow::Context;
use serde_json::json;

pub fn sandbox_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    match action.as_str() {
        "create" => server.sandbox_tool(args),
        "destroy" => {
            let sandbox_id = required_string(args, "sandboxId")?;
            let mut sandbox = server
                .sandboxes
                .remove(&sandbox_id)
                .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
            sandbox.stop_backend()?;
            if sandbox.root.exists() {
                std::fs::remove_dir_all(&sandbox.root)
                    .with_context(|| format!("failed to remove {}", sandbox.root.display()))?;
            }
            Ok(ToolReply::success(
                "Destroyed sandbox".to_owned(),
                json!({ "sandboxId": sandbox_id }),
            ))
        }
        other => Ok(ToolReply::error(
            "Sandbox action failed".to_owned(),
            json!({ "error": format!("unsupported sandbox action `{other}`") }),
        )),
    }
}
