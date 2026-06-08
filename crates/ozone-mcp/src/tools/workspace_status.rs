/// MCP tool: workspace status.
use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use ozone_core::paths;

pub fn workspace_status_tool(server: &OzoneMcpServer) -> anyhow::Result<ToolReply> {
    let preferences_path = paths::preferences_path();
    let data_dir = paths::data_dir();
    let models_dir = paths::models_dir();
    let workspace_members = vec![
        "apps/ozone-mcp",
        "apps/ozone-plus",
        "crates/ozone-core",
        "crates/ozone-engine",
        "crates/ozone-inference",
        "crates/ozone-mcp",
        "crates/ozone-memory",
        "crates/ozone-persist",
        "crates/ozone-tui",
    ];

    Ok(ToolReply::success(
        "Loaded workspace status".to_owned(),
        json!({
            "repoRoot": server.repo_root,
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "workspaceMembers": workspace_members,
            "defaultPaths": {
                "dataDir": data_dir,
                "preferencesPath": preferences_path,
                "modelsDir": models_dir,
                "presetsPath": paths::presets_path(),
                "launcherPath": paths::launcher_path()
            }
        }),
    ))
}