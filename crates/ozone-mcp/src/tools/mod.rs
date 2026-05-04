// tools/mod.rs - MCP tool modules for ozone-mcp server

pub mod workspace_status;
pub mod cargo_tool;
pub mod catalog_list;
pub mod preferences_get;
pub mod sandbox_tool;
pub mod mock_backend;

pub use workspace_status::workspace_status_tool;
pub use cargo_tool::cargo_tool;
pub use catalog_list::catalog_list_tool;
pub use preferences_get::preferences_get_tool;
pub use sandbox_tool::sandbox_tool;
pub use mock_backend::mock_backend_tool;

// Helper functions used by tool modules
pub(super) fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required string argument `{key}`"))
}

pub(super) fn optional_string<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(serde_json::Value::as_str)
}

pub(super) fn optional_bool(args: &serde_json::Value, key: &str) -> Option<bool> {
    args.get(key).and_then(serde_json::Value::as_bool)
}

pub(super) fn optional_string_array<'a>(
    args: &'a serde_json::Value,
    key: &str,
) -> anyhow::Result<Vec<&'a str>> {
    match args.get(key) {
        Some(serde_json::Value::Array(arr)) => {
            Ok(arr.iter().map(|v| v.as_str().unwrap_or("")).collect())
        }
        Some(_) => Err(anyhow::anyhow!("argument `{key}` must be an array")),
        None => Ok(vec![]),
    }
}

pub(super) fn optional_u64(args: &serde_json::Value, key: &str) -> Option<u64> {
    args.get(key).and_then(serde_json::Value::as_u64)
}

pub(super) fn required_u64(args: &serde_json::Value, key: &str) -> anyhow::Result<u64> {
    optional_u64(args, key).ok_or_else(|| anyhow::anyhow!("missing required integer field `{key}`"))
}
