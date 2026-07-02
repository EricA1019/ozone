/// MCP tool: mock user tool.
use crate::is_legacy_capture_target;
use crate::is_legacy_mock_user_journey;
use crate::legacy_tools_enabled;
use crate::optional_string;
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::bail;
use serde_json::Value;

pub fn mock_user_tool(
    server: &mut OzoneMcpServer,
    args: &serde_json::Value,
) -> anyhow::Result<ToolReply> {
    let requested_journey = optional_string(args, "journey");
    let requested_target = optional_string(args, "target");
    if !legacy_tools_enabled() {
        if requested_journey
            .as_deref()
            .is_some_and(is_legacy_mock_user_journey)
        {
            bail!(
                "legacy mock-user journeys are archived; set OZONE_MCP_ENABLE_LEGACY_TOOLS=1 to opt in"
            );
        }
        if requested_target
            .as_deref()
            .is_some_and(is_legacy_capture_target)
        {
            bail!(
                "legacy screen targets are archived; set OZONE_MCP_ENABLE_LEGACY_TOOLS=1 to opt in"
            );
        }
    }
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
