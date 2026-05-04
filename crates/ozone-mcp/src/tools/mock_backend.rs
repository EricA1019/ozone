/// MCP tool: mock backend.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;
use super::required_string;

pub fn mock_backend_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    match required_string(args, "action")?.as_str() {
        "start" => server.start_mock_backend(args),
        "stop" => server.stop_mock_backend(args),
        other => Ok(ToolReply::error(
            "Mock backend action failed".to_owned(),
            json!({ "error": format!("unsupported mock backend action `{other}`") }),
        )),
    }
}