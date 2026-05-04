     1|/// MCP tool: message tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn message_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    if action != "send" {
    10|        return Ok(ToolReply::error(
    11|            "Message action failed".to_owned(),
    12|            json!({ "error": format!("unsupported message action `{action}`") }),
    13|        ));
    14|    }
    15|
    16|    let session_id = required_string(args, "sessionId")?;
    17|    let content = required_string(args, "content")?;
    18|    let sandbox_id = optional_string(args, "sandboxId");
    19|    let author_kind = optional_string(args, "author").unwrap_or_else(|| "user".to_owned());
    20|    let author_name = optional_string(args, "authorName");
    21|    let mut command = vec![
    22|        "run".to_owned(),
    23|        "-p".to_owned(),
    24|        OZONE_PLUS_PACKAGE.to_owned(),
    25|        "--quiet".to_owned(),
    26|        "--".to_owned(),
    27|        "send".to_owned(),
    28|        session_id.clone(),
    29|        content.clone(),
    30|    ];
    31|    if author_kind != "user" {
    32|        command.push("--author".to_owned());
    33|        command.push(author_kind);
    34|    }
    35|    if let Some(author_name) = author_name {
    36|        command.push("--author-name".to_owned());
    37|        command.push(author_name);
    38|    }
    39|    let parent_message_id = optional_string(args, "parentMessageId");
    40|    let output = server.run_workspace_command("cargo", &command, sandbox_id.as_deref())?;
    41|    let message_ids = output
    42|        .stdout
    43|        .lines()
    44|        .filter_map(|line| line.strip_prefix("  message id      "))
    45|        .map(str::trim)
    46|        .filter(|value| !value.is_empty())
    47|        .map(ToOwned::to_owned)
    48|        .collect::<Vec<_>>();
    49|    let user_message_id = message_ids.first().cloned();
    50|    let assistant_message_id = message_ids.get(1).cloned();
    51|    let data = json!({
    52|        "command": output.command,
    53|        "ok": output.success,
    54|        "userMessageId": user_message_id,
    55|        "assistantMessageId": assistant_message_id,
    56|        "parentMessageId": parent_message_id,
    57|        "stdout": output.stdout,
    58|        "stderr": output.stderr,
    59|        "exitCode": output.exit_code
    60|    });
    61|    Ok(if output.success {
    62|        ToolReply::success("Completed runtime-backed send".to_owned(), data)
    63|    } else {
    64|        ToolReply::error("Runtime-backed send failed".to_owned(), data)
    65|    })
    66|}
    67|
    68|