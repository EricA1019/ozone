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
last_updated: 2026-04-12
---

# Architecture

## System Overview

Ozone is a single-binary Rust TUI that manages local LLM backends.

User runs ozone -> splash -> launcher dashboard -> either:

- pick model -> planner computes settings -> confirm -> launches KoboldCpp -> polls API -> opens SillyTavern
- or profile/recommend model -> advisory screen -> confirm -> profiling task runs in background -> success/failure report -> optional generate/export/launch follow-up

Monitor mode shows live VRAM/RAM/CPU/services with 2s refresh.

## Key Components

- catalog.rs: Reads model files, parses presets/benchmarks. Produces CatalogRecord.
- planner.rs: Mixed-memory launch planner. estimate_vram_mb() predicts VRAM usage.
- profiling.rs: TUI-facing advisory/orchestration layer. Validates model/launcher paths, recommends next actions, runs profiling workflows, and classifies failures into actionable reports.
- hardware.rs: Queries nvidia-smi for GPU, /proc for RAM, CPU cores.
- processes.rs: Process management, disk I/O from /proc/diskstats.
- prefs.rs: JSON preferences at ~/.local/share/ozone/preferences.json.
- db.rs: SQLite wrapper for benchmarks + profiles.
- ui/: ratatui TUI (mod.rs, launcher.rs, monitor.rs, splash.rs). `ui/mod.rs` owns event/state flow; `ui/launcher.rs` now renders the advisory/confirm/running/success/failure profiling screens too.
- theme.rs: Color palette, style helpers, ASCII wordmark, HEX_CURSOR.

## External Dependencies

- KoboldCpp: LLM inference server at localhost:5001.
- SillyTavern: Chat frontend at localhost:8000.
- nvidia-smi: GPU monitoring.
- ~/models/: .gguf files, koboldcpp-presets.conf, bench-results.txt.

## What Does NOT Exist Here

- No model downloading.
- No direct inference.
- No web UI (terminal only).
- No Windows support (Linux only).
