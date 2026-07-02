---
name: agents
description: Always-loaded project anchor. Read this first. Contains project identity, non-negotiables, commands, and pointer to ROUTER.md for full context.
last_updated: 2026-06-27
---

# Ozone

## What This Is
Ozone is a terminal-native local model operator focused on llama.cpp-backed
launching, monitoring, profiling, benchmarking, sweeps, model inventory, and
capability evaluation.

RC scope is the active `ozone` binary plus the developer-facing `ozone-mcp`
automation binary. The former ozone+ chat shell, KoboldCpp/SillyTavern handoff,
roleplay, memories, branches, swipes, and transcript UX are deprecated and
archived under `docs/archive/ozone-plus/`.

## Non-Negotiables
See `.mex/AGENTS.md` — this file delegates to the canonical source.

## Commands
- Dev: `cargo build --workspace`
- Test: `make test` or `cargo test --workspace`
- Lint: `make lint` or `cargo clippy --workspace --all-targets -- -D warnings`
- Preflight: `make preflight` (lint + test — run before every commit)
- Install: `make install`
- Prune: `make prune-artifacts`

## After Every Task
After completing any task: update `.mex/ROUTER.md` project state and any
`.mex/` files that are now out of date. If no pattern existed for the task
you just completed, create one in `.mex/patterns/`.

## Navigation
At the start of every session, read `.mex/ROUTER.md` before doing anything else.
For full project context, patterns, and task guidance — everything is there.
