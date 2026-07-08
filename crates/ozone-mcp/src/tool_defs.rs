//! Tool definitions — schema entries for all MCP tools.
//!
//! Defines the `ToolDefinition` struct and the `tool_definitions()` function
//! that returns all tool schemas the MCP server advertises.
//!
//! Tools are categorized by `ToolScope`: `ActiveRc` for current-scope tools,
//! `LegacyArchived` for deprecated ozone+ era tools.

use serde::Serialize;
use serde_json::{json, Value};

use super::ToolScope;

#[derive(Debug, Serialize)]
pub(crate) struct ToolDefinition {
    #[serde(skip)]
    pub(crate) scope: ToolScope,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    #[serde(rename = "inputSchema")]
    pub(crate) input_schema: Value,
}

pub(crate) fn tool_definitions() -> Vec<ToolDefinition> {
    scoped_tool_definitions(super::legacy_tools_enabled())
}

pub(crate) fn scoped_tool_definitions(include_legacy: bool) -> Vec<ToolDefinition> {
    all_tool_definitions(include_legacy)
        .into_iter()
        .filter(|tool| include_legacy || tool.scope == ToolScope::ActiveRc)
        .collect()
}

fn all_tool_definitions(include_legacy: bool) -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "workspace_status",
            description: "Inspect Ozone workspace roots, members, and default paths.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "cargo_tool",
            description: "Run focused cargo build/test/check/clippy commands inside the Ozone workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["check", "test", "build", "clippy"] },
                    "package": { "type": "string" },
                    "release": { "type": "boolean" },
                    "quiet": { "type": "boolean" },
                    "extraArgs": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "catalog_list",
            description: "List GGUF files and broken symlinks in the active or sandboxed models directory.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "preferences_get",
            description: "Read the active or sandboxed Ozone preferences.json file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "sandbox_tool",
            description: "Create or destroy a temp-XDG sandbox for Ozone smoke tests.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "destroy"] },
                    "sandboxId": { "type": "string" },
                    "namePrefix": { "type": "string" },
                    "models": { "type": "array", "items": { "type": "string" } },
                    "preferences": { "type": "object" },
                    "createLauncherStub": { "type": "boolean" },
                    "launcherExitCode": { "type": "integer" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "mock_backend_tool",
            description: "Start or stop a mock KoboldCpp-compatible backend inside a sandbox.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start", "stop"] },
                    "sandboxId": { "type": "string" },
                    "port": { "type": "integer" },
                    "modelName": { "type": "string" }
                },
                "required": ["action", "sandboxId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "session_tool",
            description: "Create, list, inspect metadata, or load transcripts for ozone+ sessions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "list", "metadata", "transcript"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "name": { "type": "string" },
                    "characterName": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "branchId": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "message_tool",
            description: "Send a runtime-backed message through ozone-plus.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "content": { "type": "string" },
                    "author": { "type": "string" },
                    "authorName": { "type": "string" }
                },
                "required": ["action", "sessionId", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "memory_tool",
            description: "Create note memories, pin message memories, or list pinned memories.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["note", "pin", "list"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "content": { "type": "string" },
                    "messageId": { "type": "string" },
                    "expiresAfterTurns": { "type": "integer" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "search_tool",
            description: "Run ozone-plus session/global search or trigger index rebuild with structured command results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["session", "global", "index_rebuild"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "branch_tool",
            description: "Create, list, or activate ozone+ branches.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "list", "activate"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "name": { "type": "string" },
                    "fromMessageId": { "type": "string" },
                    "branchId": { "type": "string" },
                    "activate": { "type": "boolean" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "swipe_tool",
            description: "Add, list, or activate ozone+ swipe candidates.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "list", "activate"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "parentMessageId": { "type": "string" },
                    "content": { "type": "string" },
                    "contextMessageId": { "type": "string" },
                    "swipeGroupId": { "type": "string" },
                    "ordinal": { "type": "integer" },
                    "author": { "type": "string" },
                    "authorName": { "type": "string" },
                    "state": { "type": "string", "enum": ["active", "discarded", "failed_mid_stream"] }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "export_tool",
            description: "Export ozone+ sessions or transcripts, optionally writing files.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["session", "transcript"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "branchId": { "type": "string" },
                    "format": { "type": "string", "enum": ["json", "text"] },
                    "outputPath": { "type": "string" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "import_card",
            description: "Import a character card into ozone+ from a file path or JSON string.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "path": { "type": "string" },
                    "cardJson": { "type": "string" },
                    "sessionName": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "provenance": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::LegacyArchived,
            name: "launcher_smoke",
            description: "Drive the base ozone launcher in a PTY and report whether it handed off into a launcher-managed ozone+ session.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "liveRefreshModelName": { "type": "string" },
                    "enterCount": { "type": "integer" }
                },
                "required": ["sandboxId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "screen_nav_targets",
            description: "List centralized cold-start navigation targets for active capturable Ozone screens.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "enum": super::scoped_capture_targets(include_legacy)
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "mock_user_tool",
            description: "Play through active front-door terminal journeys in real Ozone binaries using PTY input only. Omitting sandboxId auto-prepares the recommended temp-XDG sandbox for the requested target or journey.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "journey": {
                        "type": "string",
                        "enum": super::mock_user_journey_names(include_legacy)
                    },
                    "target": {
                        "type": "string",
                        "enum": super::scoped_capture_targets(include_legacy)
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    },
                    "prompt": { "type": "string" },
                    "captureScreenshots": { "type": "boolean", "default": false },
                    "outputDir": { "type": "string" },
                    "rows": { "type": "integer", "minimum": 1 },
                    "columns": { "type": "integer", "minimum": 1 },
                    "fontSize": { "type": "integer", "minimum": 1 },
                    "tailChars": { "type": "integer", "minimum": 1 }
                },
                "anyOf": [
                    { "required": ["journey"] },
                    { "required": ["target"] }
                ],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "screenshot_tool",
            description: "Navigate to an active capturable Ozone screen target and save a PNG plus JSON terminal snapshot. Omitting sandboxId auto-prepares the target's recommended temp-XDG sandbox.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "target": {
                        "type": "string",
                        "enum": super::scoped_capture_targets(include_legacy)
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    },
                    "outputDir": { "type": "string" },
                    "filename": { "type": "string" },
                    "dimensions": {
                        "type": "object",
                        "properties": {
                            "rows": { "type": "integer", "minimum": 1 },
                            "columns": { "type": "integer", "minimum": 1 }
                        },
                        "additionalProperties": false
                    },
                    "rows": { "type": "integer", "minimum": 1 },
                    "columns": { "type": "integer", "minimum": 1 },
                    "fontSize": { "type": "integer", "minimum": 1 },
                    "tailChars": { "type": "integer", "minimum": 1 }
                },
                "required": ["target", "outputDir"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            scope: ToolScope::ActiveRc,
            name: "screen_check_tool",
            description: "Run structured grid-based assertions against a screenshot JSON sidecar or matching PNG artifact path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artifactPath": { "type": "string" },
                    "path": { "type": "string" },
                    "sidecarPath": { "type": "string" },
                    "checks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "enum": [
                                        "text_present",
                                        "text_absent",
                                        "color_at",
                                        "border_intact",
                                        "layout_columns",
                                        "no_overlap",
                                        "baseline_compare"
                                    ]
                                },
                                "name": { "type": "string" },
                                "text": { "type": "string" },
                                "baselinePath": { "type": "string" },
                                "baselineSidecarPath": { "type": "string" },
                                "caseSensitive": { "type": "boolean", "default": false },
                                "minOccurrences": { "type": "integer", "minimum": 1 },
                                "row": { "type": "integer", "minimum": 0 },
                                "column": { "type": "integer", "minimum": 0 },
                                "count": { "type": "integer", "minimum": 1 },
                                "minGap": { "type": "integer", "minimum": 1 },
                                "fg": {
                                    "oneOf": [
                                        { "type": "string" },
                                        {
                                            "type": "array",
                                            "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                                            "minItems": 3,
                                            "maxItems": 3
                                        }
                                    ]
                                },
                                "bg": {
                                    "oneOf": [
                                        { "type": "string" },
                                        {
                                            "type": "array",
                                            "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                                            "minItems": 3,
                                            "maxItems": 3
                                        }
                                    ]
                                },
                                "region": {
                                    "type": "object",
                                    "properties": {
                                        "top": { "type": "integer", "minimum": 0 },
                                        "left": { "type": "integer", "minimum": 0 },
                                        "bottom": { "type": "integer", "minimum": 0 },
                                        "right": { "type": "integer", "minimum": 0 }
                                    },
                                    "additionalProperties": false
                                },
                                "regions": {
                                    "type": "array",
                                    "minItems": 2,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "name": { "type": "string" },
                                            "top": { "type": "integer", "minimum": 0 },
                                            "left": { "type": "integer", "minimum": 0 },
                                            "bottom": { "type": "integer", "minimum": 0 },
                                            "right": { "type": "integer", "minimum": 0 }
                                        },
                                        "required": ["top", "left", "bottom", "right"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["type"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["checks"],
                "anyOf": [
                    { "required": ["artifactPath"] },
                    { "required": ["path"] },
                    { "required": ["sidecarPath"] }
                ],
                "additionalProperties": false
            }),
        },
    ]
}




