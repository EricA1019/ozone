     1|/// MCP tool: screenshot tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn screenshot_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let target = required_string(args, "target")?;
     9|    let output_dir = PathBuf::from(required_string(args, "outputDir")?);
    10|    let prepared_sandbox =
    11|        server.prepare_target_sandbox(optional_string(args, "sandboxId"), &target)?;
    12|    let journey = server.build_mock_user_target_journey(&target)?;
    13|    fs::create_dir_all(&output_dir)
    14|        .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;
    15|
    16|    let capture = screenshot_capture_config(args, &output_dir, &target)?;
    17|    let mut data = server.run_mock_user_journey(
    18|        &prepared_sandbox.sandbox_id,
    19|        &journey,
    20|        Some(target.clone()),
    21|        args,
    22|        Some(capture),
    23|    )?;
    24|    server.annotate_prepared_sandbox(&mut data, &prepared_sandbox);
    25|    if let Value::Object(map) = &mut data {
    26|        map.insert(
    27|            "outputDir".to_owned(),
    28|            Value::String(output_dir.display().to_string()),
    29|        );
    30|    }
    31|    let success = data
    32|        .get("success")
    33|        .and_then(Value::as_bool)
    34|        .unwrap_or(false);
    35|    let missing_dependencies = data
    36|        .get("missingModules")
    37|        .and_then(Value::as_array)
    38|        .is_some_and(|value| !value.is_empty());
    39|    let summary = if missing_dependencies {
    40|        format!("Screenshot capture for `{target}` failed: missing Python dependencies")
    41|    } else if success {
    42|        format!("Captured screenshot for `{target}`")
    43|    } else {
    44|        format!("Screenshot capture for `{target}` failed")
    45|    };
    46|    Ok(if success {
    47|        ToolReply::success(summary, data)
    48|    } else {
    49|        ToolReply::error(summary, data)
    50|    })
    51|}
    52|
    53|