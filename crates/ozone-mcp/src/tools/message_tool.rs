use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use crate::required_string;
use crate::optional_string;
use crate::OZONE_PLUS_PACKAGE;

pub fn message_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    if action != "send" {
        return Ok(ToolReply::error(
            "Message action failed".to_owned(),
            json!({ "error": format!("unsupported message action `{action}`") }),
        ));
    }

    let session_id = required_string(args, "sessionId")?;
    let content = required_string(args, "content")?;
    let sandbox_id = optional_string(args, "sandboxId");
    let author_kind = optional_string(args, "author").unwrap_or_else(|| "user".to_owned());
    let author_name = optional_string(args, "authorName");
    let mut command = vec![
        "run".to_owned(),
        "-p".to_owned(),
        OZONE_PLUS_PACKAGE.to_owned(),
        "--quiet".to_owned(),
        "--".to_owned(),
        "send".to_owned(),
        session_id.clone(),
        content.clone(),
    ];
    if author_kind != "user" {
        command.push("--author".to_owned());
        command.push(author_kind);
    }
    if let Some(author_name) = author_name {
        command.push("--author-name".to_owned());
        command.push(author_name);
    }
    let parent_message_id = optional_string(args, "parentMessageId");
    let output = server.run_workspace_command("cargo", &command, sandbox_id.as_deref())?;
    let message_ids = output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  message id      "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let user_message_id = message_ids.first().cloned();
    let assistant_message_id = message_ids.get(1).cloned();
    let data = json!({
        "command": output.command,
        "ok": output.success,
        "userMessageId": user_message_id,
        "assistantMessageId": assistant_message_id,
        "parentMessageId": parent_message_id,
        "stdout": output.stdout,
        "stderr": output.stderr,
        "exitCode": output.exit_code
    });
    Ok(if output.success {
        ToolReply::success("Completed runtime-backed send".to_owned(), data)
    } else {
        ToolReply::error("Runtime-backed send failed".to_owned(), data)
    })
}
