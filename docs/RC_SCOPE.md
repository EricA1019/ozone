# Ozone RC Scope

Ozone RC ships the active `ozone` CLI/TUI and the developer-facing
`ozone-mcp` automation binary.

## In Scope

- managed llama.cpp launch and monitoring
- GGUF model inventory, import, linking, removal, and inspection
- hardware-aware profiling and launch-profile generation
- benchmark, sweep, analyze, and eval workflows
- native eval artifacts, reports, CSV output, and error summaries
- release smoke for the active release artifacts and CLI/help surface

## Out of Scope

- ozone+ as a shipping end-user binary
- chat, roleplay, character cards, memories, branches, swipes, or transcripts
- KoboldCpp/SillyTavern handoff flows
- cloud-only benchmark workflows
- full SWE-bench or Terminal-Bench by default

## Workspace Policy

- Active workspace members must be built by `cargo build --workspace`, checked
  by CI, and represented in `make release-smoke` when they are release
  artifacts.
- Archived crates may remain in the tree only when they are explicitly excluded
  from the workspace and documented as out of RC scope.
- Release gates must not depend on deprecated ozone+ chat/session behavior.

## MCP Tool Scope

Default `tools/list` must expose only active RC tools. Archived tools may remain
callable only behind an explicit legacy opt-in while old automation is being
retired. Use `OZONE_MCP_ENABLE_LEGACY_TOOLS=1` for that opt-in; active
front-door tools must also hide archived ozone+ targets unless the same opt-in
is set.

| Tool | Scope | Notes |
| --- | --- | --- |
| `workspace_status` | active-rc | inspect repository/workspace state |
| `cargo_tool` | active-rc | run focused Cargo validation |
| `catalog_list` | active-rc | inspect active/sandboxed GGUF catalog |
| `preferences_get` | active-rc | inspect active/sandboxed Ozone prefs |
| `sandbox_tool` | active-rc | create/destroy temp-XDG smoke sandboxes |
| `screen_nav_targets` | active-rc | list active capturable screen targets |
| `mock_user_tool` | active-rc | drive active front-door terminal journeys |
| `screenshot_tool` | active-rc | capture active terminal screens |
| `screen_check_tool` | active-rc | assert terminal capture text/baselines |
| `mock_backend_tool` | legacy-archived | old KoboldCpp-compatible mock backend |
| `launcher_smoke` | legacy-archived | old ozone+ handoff smoke |
| `session_tool` | legacy-archived | ozone+ sessions/transcripts |
| `message_tool` | legacy-archived | ozone+ runtime-backed messages |
| `memory_tool` | legacy-archived | ozone+ pinned/note memories |
| `search_tool` | legacy-archived | ozone+ search/index rebuild |
| `branch_tool` | legacy-archived | ozone+ branch management |
| `swipe_tool` | legacy-archived | ozone+ swipe candidates |
| `export_tool` | legacy-archived | ozone+ session/transcript export |
| `import_card` | legacy-archived | ozone+ character-card import |
