use anyhow::Result;
use serde_json::Value;

use crate::{is_legacy_tool_name, legacy_tools_enabled, tools, OzoneMcpServer, ToolReply};

type ToolHandler = fn(&mut OzoneMcpServer, &Value) -> Result<ToolReply>;

struct ToolRoute {
    name: &'static str,
    handler: ToolHandler,
}

const TOOL_ROUTES: &[ToolRoute] = &[
    ToolRoute {
        name: "workspace_status",
        handler: workspace_status_handler,
    },
    ToolRoute {
        name: "cargo_tool",
        handler: cargo_tool_handler,
    },
    ToolRoute {
        name: "catalog_list",
        handler: catalog_list_handler,
    },
    ToolRoute {
        name: "preferences_get",
        handler: preferences_get_handler,
    },
    ToolRoute {
        name: "sandbox_tool",
        handler: tools::sandbox_tool,
    },
    ToolRoute {
        name: "mock_backend_tool",
        handler: tools::mock_backend_tool,
    },
    ToolRoute {
        name: "session_tool",
        handler: tools::session_tool,
    },
    ToolRoute {
        name: "message_tool",
        handler: tools::message_tool,
    },
    ToolRoute {
        name: "memory_tool",
        handler: tools::memory_tool,
    },
    ToolRoute {
        name: "search_tool",
        handler: tools::search_tool,
    },
    ToolRoute {
        name: "branch_tool",
        handler: tools::branch_tool,
    },
    ToolRoute {
        name: "swipe_tool",
        handler: tools::swipe_tool,
    },
    ToolRoute {
        name: "export_tool",
        handler: tools::export_tool,
    },
    ToolRoute {
        name: "import_card",
        handler: tools::import_card_tool,
    },
    ToolRoute {
        name: "launcher_smoke",
        handler: tools::launcher_smoke_tool,
    },
    ToolRoute {
        name: "screen_nav_targets",
        handler: screen_nav_targets_handler,
    },
    ToolRoute {
        name: "mock_user_tool",
        handler: tools::mock_user_tool,
    },
    ToolRoute {
        name: "screenshot_tool",
        handler: tools::screenshot_tool,
    },
    ToolRoute {
        name: "screen_check_tool",
        handler: screen_check_handler,
    },
];

pub fn dispatch_tool_call(
    server: &mut OzoneMcpServer,
    tool_name: &str,
    arguments: &Value,
) -> Result<ToolReply> {
    dispatch_tool_call_with_legacy_mode(server, tool_name, arguments, legacy_tools_enabled())
}

pub(crate) fn dispatch_tool_call_with_legacy_mode(
    server: &mut OzoneMcpServer,
    tool_name: &str,
    arguments: &Value,
    include_legacy: bool,
) -> Result<ToolReply> {
    if let Some(route) = TOOL_ROUTES.iter().find(|route| route.name == tool_name) {
        if !include_legacy && is_legacy_tool_name(tool_name) {
            return Ok(ToolReply::error(
                "Legacy MCP tool is archived".to_owned(),
                serde_json::json!({
                    "error": format!("tool `{tool_name}` is archived and disabled by default"),
                    "enableWith": "OZONE_MCP_ENABLE_LEGACY_TOOLS=1",
                    "scope": "legacy-archived"
                }),
            ));
        }
        return (route.handler)(server, arguments);
    }

    Ok(ToolReply::error(
        "Unknown tool".to_owned(),
        serde_json::json!({ "error": format!("tool `{tool_name}` does not exist") }),
    ))
}

fn workspace_status_handler(server: &mut OzoneMcpServer, _arguments: &Value) -> Result<ToolReply> {
    tools::workspace_status_tool(server)
}

fn cargo_tool_handler(server: &mut OzoneMcpServer, arguments: &Value) -> Result<ToolReply> {
    tools::cargo_tool(server, arguments)
}

fn catalog_list_handler(server: &mut OzoneMcpServer, arguments: &Value) -> Result<ToolReply> {
    tools::catalog_list_tool(server, arguments)
}

fn preferences_get_handler(server: &mut OzoneMcpServer, arguments: &Value) -> Result<ToolReply> {
    tools::preferences_get_tool(server, arguments)
}

fn screen_nav_targets_handler(server: &mut OzoneMcpServer, arguments: &Value) -> Result<ToolReply> {
    tools::screen_nav_targets_tool(server, arguments)
}

fn screen_check_handler(server: &mut OzoneMcpServer, arguments: &Value) -> Result<ToolReply> {
    tools::screen_check_tool(server, arguments)
}
