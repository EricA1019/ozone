use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use crate::required_string;
use crate::optional_string;
use crate::OZONE_PLUS_PACKAGE;
use crate::parse_prefixed_field;

pub fn search_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    let output = match action.as_str() {
        "session" => {
            let session_id = required_string(args, "sessionId")?;
            let query = required_string(args, "query")?;
            server.run_workspace_command(
                "cargo",
                &[
                    "run".to_owned(),
                    "-p".to_owned(),
                    OZONE_PLUS_PACKAGE.to_owned(),
                    "--quiet".to_owned(),
                    "--".to_owned(),
                    "search".to_owned(),
                    "session".to_owned(),
                    session_id,
                    query,
                ],
                sandbox_id.as_deref(),
            )?
        }
        "global" => {
            let query = required_string(args, "query")?;
            server.run_workspace_command(
                "cargo",
                &[
                    "run".to_owned(),
                    "-p".to_owned(),
                    OZONE_PLUS_PACKAGE.to_owned(),
                    "--quiet".to_owned(),
                    "--".to_owned(),
                    "search".to_owned(),
                    "global".to_owned(),
                    query,
                ],
                sandbox_id.as_deref(),
            )?
        }
        "index_rebuild" => server.run_workspace_command(
            "cargo",
            &[
                "run".to_owned(),
                "-p".to_owned(),
                OZONE_PLUS_PACKAGE.to_owned(),
                "--quiet".to_owned(),
                "--".to_owned(),
                "index".to_owned(),
                "rebuild".to_owned(),
            ],
            sandbox_id.as_deref(),
        )?,
        other => {
            return Ok(ToolReply::error(
                "Search action failed".to_owned(),
                json!({ "error": format!("unsupported search action `{other}`") }),
            ));
        }
    };

    let mode = parse_prefixed_field(&output.stdout, "  mode            ");
    let hits = parse_prefixed_field(&output.stdout, "  hits            ")
        .and_then(|value| value.parse::<u64>().ok());
    let status = parse_prefixed_field(&output.stdout, "  status          ");
    // Replace cryptic embedding disabled message with user-friendly FTS fallback note
    let status = status.map(|s| {
        if s.contains("embedding.provider is disabled") {
            "FTS mode — configure embedding provider for vector search".to_owned()
        } else {
            s
        }
    });
    let data = json!({
        "command": output.command,
        "ok": output.success,
        "mode": mode,
        "status": status,
        "hits": hits,
        "stdout": output.stdout,
        "stderr": output.stderr,
        "exitCode": output.exit_code
    });
    Ok(if output.success {
        ToolReply::success("Completed search/index command".to_owned(), data)
    } else {
        ToolReply::error("Search/index command failed".to_owned(), data)
    })
}
