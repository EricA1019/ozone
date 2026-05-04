     1|/// MCP tool: export tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn export_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    11|    match action.as_str() {
    12|        "session" => server.with_repo(sandbox_id.as_deref(), |repo| {
    13|            let export = repo.export_session(&session_id)?;
    14|            if let Some(output_path) = optional_string(args, "outputPath") {
    15|                let text = serde_json::to_string_pretty(&export)?;
    16|                fs::write(&output_path, format!("{text}\n"))?;
    17|            }
    18|            Ok(ToolReply::success(
    19|                "Exported session".to_owned(),
    20|                json!({ "export": export }),
    21|            ))
    22|        }),
    23|        "transcript" => {
    24|            let branch_id = optional_string(args, "branchId")
    25|                .map(|value| parse_branch_id(&value))
    26|                .transpose()?;
    27|            let format = optional_string(args, "format").unwrap_or_else(|| "json".to_owned());
    28|            server.with_repo(sandbox_id.as_deref(), |repo| {
    29|                let export = repo.export_transcript(&session_id, branch_id.as_ref())?;
    30|                if let Some(output_path) = optional_string(args, "outputPath") {
    31|                    match format.as_str() {
    32|                        "json" => {
    33|                            let text = serde_json::to_string_pretty(&export)?;
    34|                            fs::write(&output_path, format!("{text}\n"))?;
    35|                        }
    36|                        "text" => {
    37|                            fs::write(&output_path, render_transcript_text(&export))?;
    38|                        }
    39|                        other => bail!("unsupported transcript export format `{other}`"),
    40|                    }
    41|                }
    42|                Ok(ToolReply::success(
    43|                    "Exported transcript".to_owned(),
    44|                    json!({ "export": export, "format": format }),
    45|                ))
    46|            })
    47|        }
    48|        other => Ok(ToolReply::error(
    49|            "Export action failed".to_owned(),
    50|            json!({ "error": format!("unsupported export action `{other}`") }),
    51|        )),
    52|    }
    53|}
    54|
    55|