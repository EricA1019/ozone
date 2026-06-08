use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use std::fs;
use anyhow::bail;
use crate::required_string;
use crate::optional_string;
use crate::parse_session_id;
use crate::parse_branch_id;
use crate::render_transcript_text;

pub fn export_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    match action.as_str() {
        "session" => server.with_repo(sandbox_id.as_deref(), |repo| {
            let export = repo.export_session(&session_id)?;
            if let Some(output_path) = optional_string(args, "outputPath") {
                let text = serde_json::to_string_pretty(&export)?;
                fs::write(&output_path, format!("{text}\n"))?;
            }
            Ok(ToolReply::success(
                "Exported session".to_owned(),
                json!({ "export": export }),
            ))
        }),
        "transcript" => {
            let branch_id = optional_string(args, "branchId")
                .map(|value| parse_branch_id(&value))
                .transpose()?;
            let format = optional_string(args, "format").unwrap_or_else(|| "json".to_owned());
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let export = repo.export_transcript(&session_id, branch_id.as_ref())?;
                if let Some(output_path) = optional_string(args, "outputPath") {
                    match format.as_str() {
                        "json" => {
                            let text = serde_json::to_string_pretty(&export)?;
                            fs::write(&output_path, format!("{text}\n"))?;
                        }
                        "text" => {
                            fs::write(&output_path, render_transcript_text(&export))?;
                        }
                        other => bail!("unsupported transcript export format `{other}`"),
                    }
                }
                Ok(ToolReply::success(
                    "Exported transcript".to_owned(),
                    json!({ "export": export, "format": format }),
                ))
            })
        }
        other => Ok(ToolReply::error(
            "Export action failed".to_owned(),
            json!({ "error": format!("unsupported export action `{other}`") }),
        )),
    }
}
