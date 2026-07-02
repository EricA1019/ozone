---
name: architecture
description: How the major pieces of this project connect and flow.
triggers:
  - architecture
  - system design
  - flow
edges:
  - target: context/stack.md
    condition: when specific technology details are needed
  - target: context/decisions.md
    condition: when understanding why the architecture is structured this way
last_updated: 2026-06-28
---

# Architecture

## System Overview

Ozone is a Rust Cargo workspace centered on the active `ozone` CLI/TUI and the
developer-facing `ozone-mcp` automation binary. RC scope is local llama.cpp
launching, monitoring, profiling, benchmarking, sweeps, GGUF model management,
and capability evaluation.

User runs ozone -> splash -> launcher dashboard -> either:

- pick model -> planner computes settings -> confirm -> launches managed llama.cpp -> monitor/eval/bench follow-up
- or profile/recommend model -> advisory screen -> confirm -> profiling task runs in background -> success/failure report -> optional generate/export/launch follow-up

Monitor mode shows live VRAM/RAM/CPU/services with 2s refresh.

## Key Components

- Cargo root package `ozone`: the current middle-tier Rust TUI app in `src/`.
- apps/ozone-mcp: developer-facing stdio MCP binary that exposes repo-dev commands, temp-XDG sandbox controls, and active front-door smoke orchestration without changing the end-user CLI; archived ozone+ automation is hidden unless `OZONE_MCP_ENABLE_LEGACY_TOOLS=1` is set.
- crates/ozone-core: shared product metadata and ozone data/log path helpers.
- crates/ozone-engine / ozone-memory / ozone-persist / ozone-inference: legacy chat-era domain crates that remain in the workspace for compatibility and tests, but do not define RC end-user scope.
- crates/ozone-tui: archived ozone+ chat shell crate, explicitly excluded from the RC workspace.
- catalog.rs: Reads model files, parses presets/benchmarks. Produces CatalogRecord.
- planner.rs: Mixed-memory launch planner. estimate_vram_mb() predicts VRAM usage.
- profiling.rs: TUI-facing advisory/orchestration layer. Validates model/launcher paths, recommends next actions, runs profiling workflows, and classifies failures into actionable reports.
- hardware.rs: Queries nvidia-smi for GPU, /proc for RAM, CPU cores.
- processes.rs: Process management, disk I/O from /proc/diskstats.
- prefs.rs: JSON preferences via `ozone_core::paths::preferences_path()`.
- db.rs: SQLite wrapper for benchmarks + profiles via `ozone_core::paths::benchmarks_db_path()`.
- model.rs: local GGUF model-management commands for the base `ozone` app (`model list|add|remove|info`), including HuggingFace downloads and Ollama/blob symlink management.
- src/ui/: ratatui TUI (`src/ui/mod.rs`, `src/ui/launcher.rs`, `src/ui/monitor.rs`, `src/ui/splash.rs`). `src/ui/mod.rs` owns event/state flow; `src/ui/launcher.rs` now renders the advisory/confirm/running/success/failure profiling screens too.
- theme.rs: Color palette, style helpers, ASCII wordmark, HEX_CURSOR.

## External Dependencies

- llama.cpp: local `llama-server` runtime at localhost:8989.
- llama.cpp: local `llama-cli` for Hugging Face GGUF imports.
- nvidia-smi: GPU monitoring.
- ~/models/: GGUF files and symlinks.

## What Does NOT Exist Here

- No direct inference.
- No web UI (terminal only).
- No Windows support (Linux only).
- No shipping ozone+ end-user binary in RC.
- No KoboldCpp/SillyTavern handoff in RC.
- The new MCP server is a developer/testing control plane, not an end-user product tier.
- Front-door mock-user journeys live inside the MCP server as PTY-driven terminal scripts; the default catalog is active-RC only, while ozone+ journeys remain archived for explicit legacy opt-in.
