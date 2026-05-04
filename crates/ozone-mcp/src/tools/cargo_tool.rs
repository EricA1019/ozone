/// MCP tool: cargo tool.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::Value;
use anyhow::bail;
use super::required_string;
use super::optional_string;
use super::optional_bool;
use super::optional_string_array;
use std::process::Command;
use crate::command_output_data;

pub fn cargo_tool(server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let package = optional_string(args, "package");
    let release = optional_bool(args, "release").unwrap_or(false);
    let quiet = optional_bool(args, "quiet").unwrap_or(false);
    let extra_args = optional_string_array(args, "extraArgs")?;

    let mut command = Command::new("cargo");
    command.current_dir(&server.repo_root);
    match action.as_str() {
        "check" | "test" | "build" | "clippy" => {
            command.arg(&action);
        }
        other => bail!("unsupported cargo action `{other}`"),
    }
    if let Some(package) = package.as_deref() {
        command.arg("-p").arg(package);
    }
    if quiet {
        command.arg("--quiet");
    }
    if release {
        command.arg("--release");
    }
    if action == "clippy" {
        command.arg("--");
        if extra_args.is_empty() {
            command.arg("-D").arg("warnings");
        } else {
            command.args(&extra_args);
        }
    } else {
        command.args(&extra_args);
    }

    let output = command
        .output()
        .with_context(|| format!("failed to run cargo {action}"))?;
    let data = command_output_data(&output);
    let summary = if output.status.success() {
        format!("cargo {action} succeeded")
    } else {
        format!("cargo {action} failed")
    };
    Ok(if output.status.success() {
        ToolReply::success(summary, data)
    } else {
        ToolReply::error(summary, data)
    })
}