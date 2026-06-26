use crate::optional_string;
use crate::required_string;
use crate::screenshot_capture_config;
/// MCP tool: screenshot capture.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Context;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub fn screenshot_tool(
    server: &mut OzoneMcpServer,
    args: &serde_json::Value,
) -> anyhow::Result<ToolReply> {
    let target = required_string(args, "target")?;
    let output_dir = PathBuf::from(required_string(args, "outputDir")?);
    let prepared_sandbox = server.prepare_target_sandbox(
        optional_string(args, "sandboxId").map(|s| s.to_string()),
        &target,
    )?;
    let journey = server.build_mock_user_target_journey(&target)?;
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;

    let capture = screenshot_capture_config(args, &output_dir, &target)?;
    let mut data = server.run_mock_user_journey(
        &prepared_sandbox.sandbox_id,
        &journey,
        Some(target.clone()),
        args,
        Some(capture),
    )?;
    server.annotate_prepared_sandbox(&mut data, &prepared_sandbox);
    if let Value::Object(map) = &mut data {
        map.insert(
            "outputDir".to_owned(),
            Value::String(output_dir.display().to_string()),
        );
    }
    let success = data
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let missing_dependencies = data
        .get("missingModules")
        .and_then(Value::as_array)
        .is_some_and(|value| !value.is_empty());
    let summary = if missing_dependencies {
        format!("Screenshot capture for `{target}` failed: missing Python dependencies")
    } else if success {
        format!("Captured screenshot for `{target}`")
    } else {
        format!("Screenshot capture for `{target}` failed")
    };
    Ok(if success {
        ToolReply::success(summary, data)
    } else {
        ToolReply::error(summary, data)
    })
}
