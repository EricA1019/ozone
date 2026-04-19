---
name: ozone-mcp-automation
description: Build or extend the developer-facing ozone MCP server, including repo-dev tools, ozone+ app-aware automation, temp-XDG sandboxes, and launcher smoke helpers.
triggers:
  - "ozone mcp"
  - "mcp server"
  - "launcher smoke helper"
  - "temp xdg sandbox"
  - "repo automation"
edges:
  - target: "context/architecture.md"
    condition: when deciding whether a capability belongs in the MCP server, an existing crate, or an end-user CLI
  - target: "context/conventions.md"
    condition: before adding subprocess wrappers or new automation seams
last_updated: 2026-04-16
---

# Ozone MCP Automation

## Context

- `crates/ozone-mcp` owns the MCP server logic and tool implementations.
- `apps/ozone-mcp` is the thin stdio entrypoint; keep product logic out of the binary crate.
- Prefer direct crate APIs for ozone+ persistence-heavy flows (`ozone-persist`, `ozone-memory`, etc.) and only shell out when the real behavior still lives in the end-user CLI/runtime.
- Sandbox helpers must isolate HOME/XDG paths without breaking cargo-backed subprocesses; preserve `CARGO_HOME` and `RUSTUP_HOME`.

## Steps

1. Add or update tool schemas in `tool_definitions()` and keep the names/arguments stable and JSON-shaped.
2. Route new tool calls through the main server dispatch instead of embedding ad hoc RPC handling in multiple places.
3. Reuse existing repository/config helpers first:
   - direct repo access for sessions, metadata, memory, branches, swipes, exports, imports
   - subprocess wrappers only for runtime-backed send/search/index rebuild or launcher PTY flows
4. For smoke tools, use temp-XDG sandboxes rather than the real user environment, and return structured findings instead of raw terminal dumps alone.
5. If the task is **front-door user simulation**, add or extend `mock_user_tool` with named journeys instead of inventing another replay surface:
   - use real `target/debug/ozone` / `target/debug/ozone-plus` binaries when available
   - drive them with PTY keys/text only after setup
   - assert against recent-screen terminal markers, not repo internals
6. Validate both layers:
   - cargo compile/tests for the crate
   - at least one real stdio MCP smoke (`initialize`, `tools/list`, one or more `tools/call` flows)
   - when `mock_user_tool` changes, run at least one named journey end-to-end
7. Update docs and `.mex` state once the tool surface or automation boundary changes materially.

## Gotchas

- Embedded helper scripts inside Rust `format!` strings need escaped braces; JSON literals inside those strings are an easy place to break compilation.
- Passing Rust debug formatting (`Some(...)`) into Python helpers will break; serialize optional values as JSON strings/null instead.
- PTY-driven launcher smoke is noisy and partial by nature; report concrete findings like launcher invocation, created sessions, and captured tail text instead of pretending it is a perfect UI oracle.
- A sandboxed HOME can break cargo/rustup if you do not preserve the real toolchain env vars.
- A sandboxed HOME also hides user-site Python modules from the VTE screenshot helper; if `screenshot_tool` reports missing `pyte` or Pillow in temp-XDG runs, export the real site-packages path through `PYTHONPATH` or install those modules system-wide.
- For front-door journeys, matching against the entire accumulated capture can create false positives; prefer a recent-screen window or step-local view so old launcher text does not satisfy a later ozone+ assertion.
- `cargo run` is less reliable than directly launching the built binary for PTY-style mock-user flows; prefer `target/debug/...` when it exists.

## Verify

- `cargo check -p ozone-mcp -p ozone-mcp-app`
- `cargo test -p ozone-mcp`
- real stdio MCP smoke for:
  - `initialize`
  - `tools/list`
  - at least one sandboxed ozone+ flow (`sandbox_tool` + `session_tool` + `message_tool` or `search_tool`)
  - and, when `mock_user_tool` changes, at least one front-door journey such as `launcher_monitor_roundtrip` or `ozone_plus_chat_journey`

## Debug

- If the server compiles but MCP requests fail, check `Content-Length` framing first.
- If sandboxed cargo commands fail unexpectedly, inspect `CARGO_HOME`, `RUSTUP_HOME`, and the temp HOME/XDG paths in the tool output.
- If launcher smoke reports success without real handoff evidence, inspect the structured session list and launcher invocation log instead of trusting PTY capture alone.
- If a direct repo tool and the CLI disagree, prefer the direct repo path for persistence truth and narrow the subprocess wrapper to the specific runtime-owned seam.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if the MCP surface or automation boundary changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
