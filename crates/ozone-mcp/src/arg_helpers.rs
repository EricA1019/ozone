//! Argument parsing helpers for MCP tool calls.
//!
//! These functions extract typed values from JSON-RPC arguments,
//! providing consistent error messages and type coercion.

use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

pub fn command_output_data(output: &std::process::Output) -> Value {
    json!({
        "success": output.status.success(),
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr)
    })
}

pub fn required_string(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("missing required string field `{key}`"))
}

pub fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

pub fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

pub(crate) fn optional_object<'a>(args: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    args.get(key).and_then(Value::as_object)
}

pub fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

/// `required_u64` is used by `tools/swipe_tool.rs` behind `#[cfg(feature = "legacy-tools")]`.
/// Dead in default builds.
#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub fn required_u64(args: &Value, key: &str) -> Result<u64> {
    optional_u64(args, key).ok_or_else(|| anyhow!("missing required integer field `{key}`"))
}

pub(crate) fn host_toolchain_dir(name: &str) -> Option<String> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(name).display().to_string())
}

/// `checked_u16` also exists in `testing::screen` — this copy is unused
/// in default builds but kept for feature-gated callers.
#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub(crate) fn checked_u16(value: u64, key: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| anyhow!("field `{key}` must be <= {}", u16::MAX))
}

/// `checked_usize` also exists in `testing::screen` — this copy is unused
/// in default builds but kept for feature-gated callers.
#[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
pub(crate) fn checked_usize(value: u64, key: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("field `{key}` is too large"))
}

pub fn optional_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

pub fn optional_string_array(args: &Value, key: &str) -> Result<Vec<String>> {
    match args.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| anyhow!("field `{key}` must contain only strings"))
            })
            .collect(),
        Some(_) => bail!("field `{key}` must be an array of strings"),
    }
}
