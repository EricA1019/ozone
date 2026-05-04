     1|/// MCP tool: mock user tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn mock_user_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let requested_journey = optional_string(args, "journey");
     9|    let requested_target = optional_string(args, "target");
    10|    let prepared_sandbox = server.prepare_mock_user_sandbox(
    11|        optional_string(args, "sandboxId"),
    12|        requested_journey.as_deref(),
    13|        requested_target.as_deref(),
    14|    )?;
    15|    let journey = match (requested_journey.as_deref(), requested_target.as_deref()) {
    16|        (Some(_), Some(_)) => bail!("provide either `journey` or `target`, not both"),
    17|        (Some(journey_name), None) => server.build_mock_user_journey(journey_name, args)?,
    18|        (None, Some(target_name)) => server.build_mock_user_target_journey(target_name)?,
    19|        (None, None) => bail!("mock_user_tool requires either `journey` or `target`"),
    20|    };
    21|    let run_name = journey.name.clone();
    22|    let mut data =
    23|        server.run_mock_user_journey(&prepared_sandbox.sandbox_id, &journey, None, args, None)?;
    24|    server.annotate_prepared_sandbox(&mut data, &prepared_sandbox);
    25|    let success = data
    26|        .get("success")
    27|        .and_then(Value::as_bool)
    28|        .unwrap_or(false);
    29|    Ok(if success {
    30|        ToolReply::success(format!("Completed mock-user journey `{run_name}`"), data)
    31|    } else {
    32|        ToolReply::error(format!("Mock-user journey `{run_name}` failed"), data)
    33|    })
    34|}
    35|
    36|