     1|/// MCP tool: search tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn search_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    let output = match action.as_str() {
    11|        "session" => {
    12|            let session_id = required_string(args, "sessionId")?;
    13|            let query = required_string(args, "query")?;
    14|            server.run_workspace_command(
    15|                "cargo",
    16|                &[
    17|                    "run".to_owned(),
    18|                    "-p".to_owned(),
    19|                    OZONE_PLUS_PACKAGE.to_owned(),
    20|                    "--quiet".to_owned(),
    21|                    "--".to_owned(),
    22|                    "search".to_owned(),
    23|                    "session".to_owned(),
    24|                    session_id,
    25|                    query,
    26|                ],
    27|                sandbox_id.as_deref(),
    28|            )?
    29|        }
    30|        "global" => {
    31|            let query = required_string(args, "query")?;
    32|            server.run_workspace_command(
    33|                "cargo",
    34|                &[
    35|                    "run".to_owned(),
    36|                    "-p".to_owned(),
    37|                    OZONE_PLUS_PACKAGE.to_owned(),
    38|                    "--quiet".to_owned(),
    39|                    "--".to_owned(),
    40|                    "search".to_owned(),
    41|                    "global".to_owned(),
    42|                    query,
    43|                ],
    44|                sandbox_id.as_deref(),
    45|            )?
    46|        }
    47|        "index_rebuild" => server.run_workspace_command(
    48|            "cargo",
    49|            &[
    50|                "run".to_owned(),
    51|                "-p".to_owned(),
    52|                OZONE_PLUS_PACKAGE.to_owned(),
    53|                "--quiet".to_owned(),
    54|                "--".to_owned(),
    55|                "index".to_owned(),
    56|                "rebuild".to_owned(),
    57|            ],
    58|            sandbox_id.as_deref(),
    59|        )?,
    60|        other => {
    61|            return Ok(ToolReply::error(
    62|                "Search action failed".to_owned(),
    63|                json!({ "error": format!("unsupported search action `{other}`") }),
    64|            ));
    65|        }
    66|    };
    67|
    68|    let mode = parse_prefixed_field(&output.stdout, "  mode            ");
    69|    let hits = parse_prefixed_field(&output.stdout, "  hits            ")
    70|        .and_then(|value| value.parse::<u64>().ok());
    71|    let status = parse_prefixed_field(&output.stdout, "  status          ");
    72|    // Replace cryptic embedding disabled message with user-friendly FTS fallback note
    73|    let status = status.map(|s| {
    74|        if s.contains("embedding.provider is disabled") {
    75|            "FTS mode — configure embedding provider for vector search".to_owned()
    76|        } else {
    77|            s
    78|        }
    79|    });
    80|    let data = json!({
    81|        "command": output.command,
    82|        "ok": output.success,
    83|        "mode": mode,
    84|        "status": status,
    85|        "hits": hits,
    86|        "stdout": output.stdout,
    87|        "stderr": output.stderr,
    88|        "exitCode": output.exit_code
    89|    });
    90|    Ok(if output.success {
    91|        ToolReply::success("Completed search/index command".to_owned(), data)
    92|    } else {
    93|        ToolReply::error("Search/index command failed".to_owned(), data)
    94|    })
    95|}
    96|
    97|