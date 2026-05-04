use crate::OzoneMcpServer;
use crate::ToolReply;
use crate::command_output_data;
use anyhow::{bail, Context, Result};
use serde_json::Value;

/// Executes cargo commands (check, test, build, clippy) in the repo root.
pub fn cargo_tool(server: &OzoneMcpServer, args: &Value) -> Result<ToolReply> {
        let action = super::required_string(args, "action")?;
        let package = super::optional_string(args, "package");
        let release = super::optional_bool(args, "release").unwrap_or(false);
        let quiet = super::optional_bool(args, "quiet").unwrap_or(false);
        let extra_args = super::optional_string_array(args, "extraArgs")?;

        let mut command = std::process::Command::new("cargo");
        command.current_dir(&server.repo_root);
        match action {
            "check" | "test" | "build" | "clippy" => {
                command.arg(action);
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
