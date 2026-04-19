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
last_updated: 2026-04-16
---

# Architecture

## System Overview

Ozone is now a Rust Cargo workspace that preserves the current `ozone` app while
opening room for ozone+.

User runs ozone -> splash -> launcher dashboard -> either:

- pick model -> planner computes settings -> confirm -> launches KoboldCpp -> polls API -> opens SillyTavern
- or profile/recommend model -> advisory screen -> confirm -> profiling task runs in background -> success/failure report -> optional generate/export/launch follow-up

Monitor mode shows live VRAM/RAM/CPU/services with 2s refresh.

## Key Components

- Cargo root package `ozone`: the current middle-tier Rust TUI app in `src/`.
- apps/ozone-mcp: developer-facing stdio MCP binary that exposes repo-dev commands, ozone+ testing helpers, temp-XDG sandbox controls, and launcher smoke orchestration without changing the end-user CLIs.
- apps/ozone-plus: ozone+ app that now combines the persisted session CLI surfaces with the default `open` TUI shell, an app-side inference adapter, manual and hybrid recall commands, explicit `index rebuild`, human-readable metadata surfaces, and a real async session-runtime bridge that streams assistant turns on top of the engine/persistence path.
- crates/ozone-core: shared product metadata and ozone data/log path helpers.
- crates/ozone-engine: trait-first single-writer conversation engine surface with command processing, broadcast events, snapshots, an in-memory store used by engine tests, and the first explicit Phase 1E context-assembly module (`ContextPlan` / `ContextAssembler`).
- crates/ozone-inference: layered config loader, prompt-template registry, streaming decoder, backend descriptors, and the first KoboldCpp gateway implementation for ozone+.
- crates/ozone-mcp: MCP server/library crate that owns stdio framing, JSON-RPC request routing, temp-XDG sandbox state, mock backend control, structured tool schemas, and the first automation surface for repo-dev plus ozone+ smoke work.
- crates/ozone-memory: Phase 2B retrieval domain crate for pinned-memory content, optional embedding providers, retrieval scoring/result types, expiry views, and the disk-backed `usearch` vector index manager that later 2C layers can build on.
- crates/ozone-persist: ozone+ persistence layer with schema setup, migrations, advisory locks, durable transcript/branch/swipe repository APIs, Phase 2A pinned-memory plus cross-session search helpers, and Phase 2B embedding-artifact persistence built on `memory_artifacts` and `session_search`.
- crates/ozone-tui: ozone+ shell crate with session state, key/input handling, responsive layout models, ratatui rendering, the generic terminal event loop used by `apps/ozone-plus`, plus the generic context preview / dry-run / inspector surface introduced in Phase 1E and the recall browser / `Ctrl+K` affordances that now surface hybrid-retrieval results without adding a new pane.
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

- KoboldCpp: LLM inference server at localhost:5001.
- llama.cpp: local `llama-server` runtime at localhost:8080.
- SillyTavern: Chat frontend at localhost:8000.
- nvidia-smi: GPU monitoring.
- ~/models/: .gguf files, koboldcpp-presets.conf, bench-results.txt.

## What Does NOT Exist Here

- No direct inference.
- No web UI (terminal only).
- No Windows support (Linux only).
- No direct Ollama-backed ozone+ runtime path yet; base ozone still intentionally rejects guided `Ollama + ozone+` launch while the ozone+ runtime now supports KoboldCpp and llama.cpp only.
- The new MCP server is a developer/testing control plane, not an end-user product tier.
- Front-door mock-user journeys now live inside the MCP server as PTY-driven terminal scripts; they still rely on recent-screen text markers rather than any real DOM/pixel automation layer.
