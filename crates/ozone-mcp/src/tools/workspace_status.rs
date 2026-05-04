     1|/// MCP tool: workspace status.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|fn workspace_status_tool(&self) -> Result<ToolReply> {
     8|    let preferences_path = paths::preferences_path();
     9|    let data_dir = paths::data_dir();
    10|    let models_dir = paths::models_dir();
    11|    let workspace_members = vec![
    12|        "apps/ozone-mcp",
    13|        "apps/ozone-plus",
    14|        "crates/ozone-core",
    15|        "crates/ozone-engine",
    16|        "crates/ozone-inference",
    17|        "crates/ozone-mcp",
    18|        "crates/ozone-memory",
    19|        "crates/ozone-persist",
    20|        "crates/ozone-tui",
    21|    ];
    22|
    23|    Ok(ToolReply::success(
    24|        "Loaded workspace status".to_owned(),
    25|        json!({
    26|            "repoRoot": server.repo_root,
    27|            "serverVersion": env!("CARGO_PKG_VERSION"),
    28|            "workspaceMembers": workspace_members,
    29|            "defaultPaths": {
    30|                "dataDir": data_dir,
    31|                "preferencesPath": preferences_path,
    32|                "modelsDir": models_dir,
    33|                "presetsPath": paths::presets_path(),
    34|                "launcherPath": paths::launcher_path()
    35|            }
    36|        }),
    37|    ))
    38|}
    39|
    40|