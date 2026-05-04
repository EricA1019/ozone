use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::json;

/// Returns workspace status including repo root, server version, workspace members, and default paths.
pub fn workspace_status_tool(server: &OzoneMcpServer) -> Result<ToolReply> {
    let preferences_path = ozone_core::paths::preferences_path();
    let data_dir = ozone_core::paths::data_dir();
    let models_dir = ozone_core::paths::models_dir();
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
                "presetsPath": ozone_core::paths::presets_path(),
                "launcherPath": ozone_core::paths::launcher_path()
            }
        }),
    ))
}
