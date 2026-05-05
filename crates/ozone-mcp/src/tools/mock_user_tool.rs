/// MCP tool: mock user tool.
use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::Value;
use anyhow::bail;
use crate::optional_string;

pub fn mock_user_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let requested_journey = optional_string(args, "journey");
    let requested_target = optional_string(args, "target");
    let prepared_sandbox = server.prepare_mock_user_sandbox(
        optional_string(args, "sandboxId"),
        requested_journey.as_deref(),
        requested_target.as_deref(),
    )?;
    let journey = match (requested_journey.as_deref(), requested_target.as_deref()) {
        (Some(_), Some(_)) => bail!("provide either `journey` or `target`, not both"),
        (Some(journey_name), None) => server.build_mock_user_journey(journey_name, args)?,
        (None, Some(target_name)) => server.build_mock_user_target_journey(target_name)?,
        (None, None) => bail!("mock_user_tool requires either `journey` or `target`"),
    };
    let run_name = journey.name.clone();
    let mut data =
        server.run_mock_user_journey(&prepared_sandbox.sandbox_id, &journey, None, args, None)?;
    server.annotate_prepared_sandbox(&mut data, &prepared_sandbox);
    let success = data
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(if success {
        ToolReply::success(format!("Completed mock-user journey `{run_name}`"), data)
    } else {
        ToolReply::error(format!("Mock-user journey `{run_name}` failed"), data)
    })
}