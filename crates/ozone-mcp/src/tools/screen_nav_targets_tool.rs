use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::json;
use anyhow::anyhow;
use crate::optional_string;
use crate::capturable_screen_journey_builders;

pub fn screen_nav_targets_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let targets = if let Some(target_name) = optional_string(args, "target") {
        let target = capturable_screen_journey_builders()
            .iter()
            .find(|entry| entry.target_screen == target_name)
            .ok_or_else(|| anyhow!("unknown screen navigation target `{target_name}`"))?;
        vec![server.screen_nav_target_data(target)?]
    } else {
        capturable_screen_journey_builders()
            .iter()
            .map(|target| server.screen_nav_target_data(target))
            .collect::<Result<Vec<_>>>()?
    };

    Ok(ToolReply::success(
        "Loaded screen navigation targets".to_owned(),
        json!({ "targets": targets }),
    ))
}
