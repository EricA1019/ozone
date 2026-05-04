     1|/// MCP tool: cargo tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn cargo_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let package = optional_string(args, "package");
    10|    let release = optional_bool(args, "release").unwrap_or(false);
    11|    let quiet = optional_bool(args, "quiet").unwrap_or(false);
    12|    let extra_args = optional_string_array(args, "extraArgs")?;
    13|
    14|    let mut command = Command::new("cargo");
    15|    command.current_dir(&server.repo_root);
    16|    match action.as_str() {
    17|        "check" | "test" | "build" | "clippy" => {
    18|            command.arg(&action);
    19|        }
    20|        other => bail!("unsupported cargo action `{other}`"),
    21|    }
    22|    if let Some(package) = package.as_deref() {
    23|        command.arg("-p").arg(package);
    24|    }
    25|    if quiet {
    26|        command.arg("--quiet");
    27|    }
    28|    if release {
    29|        command.arg("--release");
    30|    }
    31|    if action == "clippy" {
    32|        command.arg("--");
    33|        if extra_args.is_empty() {
    34|            command.arg("-D").arg("warnings");
    35|        } else {
    36|            command.args(&extra_args);
    37|        }
    38|    } else {
    39|        command.args(&extra_args);
    40|    }
    41|
    42|    let output = command
    43|        .output()
    44|        .with_context(|| format!("failed to run cargo {action}"))?;
    45|    let data = command_output_data(&output);
    46|    let summary = if output.status.success() {
    47|        format!("cargo {action} succeeded")
    48|    } else {
    49|        format!("cargo {action} failed")
    50|    };
    51|    Ok(if output.status.success() {
    52|        ToolReply::success(summary, data)
    53|    } else {
    54|        ToolReply::error(summary, data)
    55|    })
    56|}
    57|
    58|