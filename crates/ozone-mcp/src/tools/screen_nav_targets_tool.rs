use crate::legacy_tools_enabled;
use crate::optional_string;
use crate::scoped_capture_targets;
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::anyhow;
use anyhow::Result;
use serde_json::json;

pub fn screen_nav_targets_tool(
    server: &OzoneMcpServer,
    args: &serde_json::Value,
) -> anyhow::Result<ToolReply> {
    let targets = if let Some(target_name) = optional_string(args, "target") {
        let target = scoped_capture_targets(legacy_tools_enabled())
            .into_iter()
            .find(|entry| entry.target_screen == target_name)
            .ok_or_else(|| anyhow!("unknown screen navigation target `{target_name}`"))?;
        vec![server.screen_nav_target_data(target)?]
    } else {
        scoped_capture_targets(legacy_tools_enabled())
            .into_iter()
            .map(|target| server.screen_nav_target_data(target))
            .collect::<Result<Vec<_>>>()?
    };

    Ok(ToolReply::success(
        "Loaded screen navigation targets".to_owned(),
        json!({ "targets": targets }),
    ))
}
