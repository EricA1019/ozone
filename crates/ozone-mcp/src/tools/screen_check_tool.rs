     1|/// MCP tool: screen check tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn screen_check_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let artifact_path = optional_string(args, "artifactPath")
     9|        .or_else(|| optional_string(args, "path"))
    10|        .or_else(|| optional_string(args, "sidecarPath"))
    11|        .ok_or_else(|| {
    12|            anyhow!("screen_check_tool requires `artifactPath` (or `path` / `sidecarPath`)")
    13|        })?;
    14|    let checks = args
    15|        .get("checks")
    16|        .and_then(Value::as_array)
    17|        .ok_or_else(|| anyhow!("field `checks` must be an array of check objects"))?;
    18|    if checks.is_empty() {
    19|        bail!("field `checks` must contain at least one check");
    20|    }
    21|
    22|    let (sidecar_path, capture) = load_screen_capture_sidecar(&artifact_path)?;
    23|    let results = checks
    24|        .iter()
    25|        .enumerate()
    26|        .map(|(index, check)| evaluate_screen_check(index, check, &capture))
    27|        .collect::<Result<Vec<_>>>()?;
    28|    let passed = results.iter().filter(|result| result.passed).count();
    29|    let failed = results.len().saturating_sub(passed);
    30|    let success = failed == 0;
    31|    let summary = if success {
    32|        format!("Screen check passed ({passed}/{passed} checks)")
    33|    } else {
    34|        format!(
    35|            "Screen check failed ({passed}/{} checks passed)",
    36|            results.len()
    37|        )
    38|    };
    39|
    40|    let data = json!({
    41|        "artifactPath": artifact_path,
    42|        "sidecarPath": sidecar_path.display().to_string(),
    43|        "pngPath": capture.png_path,
    44|        "screen": {
    45|            "rows": capture.screen_rows,
    46|            "columns": capture.screen_columns,
    47|            "cursor": capture.cursor,
    48|            "font": capture.font
    49|        },
    50|        "summary": {
    51|            "total": results.len(),
    52|            "passed": passed,
    53|            "failed": failed,
    54|            "success": success
    55|        },
    56|        "checks": results
    57|    });
    58|
    59|    Ok(if success {
    60|        ToolReply::success(summary, data)
    61|    } else {
    62|        ToolReply::error(summary, data)
    63|    })
    64|}
    65|
    66|fn build_mock_user_journey(
    67|    &self,
    68|    journey_name: &str,
    69|    args: &Value,
    70|) -> Result<MockUserJourneySpec> {
    71|    match journey_name {
    72|        "launcher_monitor_roundtrip" => {
    73|            let mut journey =
    74|                server.build_capturable_screen_journey("base_monitor", args, journey_name)?;
    75|            journey.steps.push(MockUserJourneyStep::text(
    76|                "return to launcher",
    77|                "r",
    78|                1200,
    79|                ["Launch", "Open ozone+", "Settings"],
    80|            ));
    81|            Ok(journey)
    82|        }
    83|        "launcher_to_ozone_plus" => {
    84|            server.build_capturable_screen_journey("base_ozone_plus_shell", args, journey_name)
    85|        }
    86|        "ozone_plus_chat_journey" => {
    87|            let prompt = optional_string(args, "prompt")
    88|                .unwrap_or_else(|| "Check the observatory key".to_owned());
    89|            let mut journey = server.build_capturable_screen_journey(
    90|                "base_ozone_plus_shell",
    91|                args,
    92|                journey_name,
    93|            )?;
    94|            if let Some(step) = journey.steps.last_mut() {
    95|                step.settle_ms = 2500;
    96|            }
    97|            journey.steps.extend([
    98|                MockUserJourneyStep::key(
    99|                    "open new chat",
   100|                    "enter",
   101|                    800,
   102|                    ["Composer", "insert mode", "NOR"],
   103|                ),
   104|                MockUserJourneyStep::text("type prompt", &prompt, 400, []),
   105|                MockUserJourneyStep::key(
   106|                    "send prompt",
   107|                    "enter",
   108|                    8000,
   109|