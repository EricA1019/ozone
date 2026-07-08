//! Core MCP types — ToolReply, CommandOutput, EnvOverrideGuard.
//!
//! These are the building-block types used across the MCP server:
//! tool response formatting, command execution output, and
//! environment variable override management.

use std::{collections::BTreeMap, env};

use serde_json::{json, Value};

/// Formatted reply sent back to the MCP client after a tool call.
#[derive(Debug)]
pub(crate) struct ToolReply {
    pub(crate) summary: String,
    pub(crate) data: Value,
    pub(crate) is_error: bool,
}

impl ToolReply {
    pub(crate) fn success(summary: String, data: Value) -> Self {
        Self {
            summary,
            data,
            is_error: false,
        }
    }

    pub(crate) fn error(summary: String, data: Value) -> Self {
        Self {
            summary,
            data,
            is_error: true,
        }
    }

    pub(crate) fn into_result(self) -> Value {
        let text = format!(
            "{}\n{}",
            self.summary,
            serde_json::to_string_pretty(&self.data).unwrap_or_else(|_| "{}".to_owned())
        );
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": {
                "summary": self.summary,
                "data": self.data
            },
            "isError": self.is_error
        })
    }
}

/// Captures the output of a shell command.
#[derive(Debug)]
pub(crate) struct CommandOutput {
    pub(crate) command: String,
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Temporarily overrides environment variables, restoring them on drop.
pub(crate) struct EnvOverrideGuard {
    previous: Vec<(String, Option<String>)>,
}

impl EnvOverrideGuard {
    pub(crate) fn new(overrides: BTreeMap<String, String>) -> Self {
        let mut previous = Vec::with_capacity(overrides.len());
        for (key, value) in overrides {
            previous.push((key.clone(), env::var(&key).ok()));
            env::set_var(&key, value);
        }
        Self { previous }
    }
}

impl Drop for EnvOverrideGuard {
    fn drop(&mut self) {
        while let Some((key, value)) = self.previous.pop() {
            match value {
                Some(value) => env::set_var(&key, value),
                None => env::remove_var(&key),
            }
        }
    }
}
