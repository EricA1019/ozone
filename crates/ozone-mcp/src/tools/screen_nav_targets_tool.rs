     1|/// MCP tool: screen nav targets tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn screen_nav_targets_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let targets = if let Some(target_name) = optional_string(args, "target") {
     9|        let target = capturable_screen_journey_builders()
    10|            .iter()
    11|            .find(|entry| entry.target_screen == target_name)
    12|            .ok_or_else(|| anyhow!("unknown screen navigation target `{target_name}`"))?;
    13|        vec![server.screen_nav_target_data(target)?]
    14|    } else {
    15|        capturable_screen_journey_builders()
    16|            .iter()
    17|            .map(|target| server.screen_nav_target_data(target))
    18|            .collect::<Result<Vec<_>>>()?
    19|    };
    20|
    21|    Ok(ToolReply::success(
    22|        "Loaded screen navigation targets".to_owned(),
    23|        json!({ "targets": targets }),
    24|    ))
    25|}
    26|
    27|