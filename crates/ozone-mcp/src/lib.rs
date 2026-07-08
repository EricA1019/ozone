#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use std::{
    env,
    io::{self, BufReader, Write},
};

#[cfg(test)]
use sandbox::Sandbox;

use anyhow::Result;
#[cfg(feature = "legacy-tools")]
use ozone_persist::{
    BranchRecord, PersistError, PinnedMemoryView, PinnedMemoryRecord, SqliteRepository,
    SessionSummary, TranscriptExport,
};

mod jsonrpc;
mod sandbox;
mod testing;
mod tool_dispatch;
mod tools;
mod arg_helpers;
mod persist_helpers;
mod types;
mod tool_defs;
mod server;
pub(crate) use self::server::OzoneMcpServer;

use self::arg_helpers::*;
use self::persist_helpers::*;
use self::tool_defs::*;
use self::types::*;

use self::jsonrpc::{
    read_message, write_message, JsonRpcRequest,
};
use self::testing::screen::{screenshot_capture_config, mock_user_capture_settings};

use testing::{
    capturable_screen_journey_builders,
    CapturableScreenJourneyDefinition, PYTHON_PTY_VTE_HELPER, PYTHON_PTY_VTE_HELPER_TRAILER,
};

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub const OZONE_PLUS_PACKAGE: &str = "ozone-plus";
const DEFAULT_PTY_ROWS: u16 = 40;
const DEFAULT_PTY_COLUMNS: u16 = 120;
const DEFAULT_CAPTURE_TAIL_CHARS: usize = 6000;
const DEFAULT_CAPTURE_FONT_SIZE: u16 = 16;
const ENV_ENABLE_LEGACY_TOOLS: &str = "OZONE_MCP_ENABLE_LEGACY_TOOLS";
const LEGACY_TOOL_NAMES: &[&str] = &[
    "mock_backend_tool",
    "launcher_smoke",
    "session_tool",
    "message_tool",
    "memory_tool",
    "search_tool",
    "branch_tool",
    "swipe_tool",
    "export_tool",
    "import_card",
];
const LEGACY_CAPTURE_TARGETS: &[&str] = &[
    "base_ozone_plus_shell",
    "ozone_plus_main_menu",
    "ozone_plus_sessions",
    "ozone_plus_characters",
    "ozone_plus_character_create",
    "ozone_plus_character_import",
    "ozone_plus_settings",
    "ozone_plus_conversation",
    "ozone_plus_help",
];
const ACTIVE_MOCK_USER_JOURNEYS: &[&str] = &["launcher_monitor_roundtrip"];
const LEGACY_MOCK_USER_JOURNEYS: &[&str] = &[
    "launcher_monitor_roundtrip",
    "launcher_to_ozone_plus",
    "ozone_plus_chat_journey",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolScope {
    ActiveRc,
    LegacyArchived,
}

pub(crate) fn legacy_tools_enabled() -> bool {
    env_flag_enabled(ENV_ENABLE_LEGACY_TOOLS)
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn is_legacy_tool_name(tool_name: &str) -> bool {
    LEGACY_TOOL_NAMES.contains(&tool_name)
}

pub(crate) fn is_legacy_capture_target(target_name: &str) -> bool {
    LEGACY_CAPTURE_TARGETS.contains(&target_name)
}

pub(crate) fn is_legacy_mock_user_journey(journey_name: &str) -> bool {
    LEGACY_MOCK_USER_JOURNEYS.contains(&journey_name)
        && !ACTIVE_MOCK_USER_JOURNEYS.contains(&journey_name)
}

pub(crate) fn scoped_capture_targets(
    include_legacy: bool,
) -> Vec<&'static CapturableScreenJourneyDefinition> {
    capturable_screen_journey_builders()
        .iter()
        .filter(|entry| include_legacy || !is_legacy_capture_target(entry.target_screen))
        .collect()
}

fn mock_user_journey_names(include_legacy: bool) -> &'static [&'static str] {
    if include_legacy {
        LEGACY_MOCK_USER_JOURNEYS
    } else {
        ACTIVE_MOCK_USER_JOURNEYS
    }
}

pub fn run_stdio_server() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut server = OzoneMcpServer::new()?;

    while let Some(request) = read_message(&mut reader)? {
        if let Some(response) = server.handle_request(request) {
            write_message(&mut writer, &response)?;
            writer.flush()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
