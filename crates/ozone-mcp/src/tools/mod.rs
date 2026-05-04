// tools/mod.rs - MCP tool modules for ozone-mcp server

pub mod workspace_status;
pub mod cargo_tool;
pub mod catalog_list;
pub mod preferences_get;
pub mod sandbox_tool;
pub mod mock_backend;
pub mod session_tool;
pub mod message_tool;
pub mod memory_tool;
pub mod search_tool;
pub mod branch_tool;
pub mod swipe_tool;
pub mod export_tool;
pub mod import_card_tool;
pub mod launcher_smoke_tool;
pub mod mock_user_tool;
pub mod screen_nav_targets_tool;
pub mod screenshot_tool;
pub mod screen_check_tool;

pub use workspace_status::workspace_status_tool;
pub use cargo_tool::cargo_tool;
pub use catalog_list::catalog_list_tool;
pub use preferences_get::preferences_get_tool;
pub use sandbox_tool::sandbox_tool;
pub use mock_backend::mock_backend_tool;
pub use session_tool::session_tool;
pub use message_tool::message_tool;
pub use memory_tool::memory_tool;
pub use search_tool::search_tool;
pub use branch_tool::branch_tool;
pub use swipe_tool::swipe_tool;
pub use export_tool::export_tool;
pub use import_card_tool::import_card_tool;
pub use launcher_smoke_tool::launcher_smoke_tool;
pub use mock_user_tool::mock_user_tool;
pub use screen_nav_targets_tool::screen_nav_targets_tool;
pub use screenshot_tool::screenshot_tool;
pub use screen_check_tool::screen_check_tool;

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
