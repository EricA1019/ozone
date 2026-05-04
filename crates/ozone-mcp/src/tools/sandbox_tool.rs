/// MCP tool: sandbox tool.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;
use super::required_string;

pub fn sandbox_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    match required_string(args, "action")?.as_str() {
        "create" => server.create_sandbox(args),
        "destroy" => server.destroy_sandbox(args),
        other => Ok(ToolReply::error(
            "Sandbox action failed".to_owned(),
            json!({ "error": format!("unsupported sandbox action `{other}`") }),
        )),
    }
}