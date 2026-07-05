---
name: agents
description: Always-loaded project anchor. Read this first. Contains project identity, non-negotiables, commands, and pointer to ROUTER.md for full context.
last_updated: 2026-07-05
---

# Ozone

## What This Is
Ozone is a terminal-native local model operator for llama.cpp-backed launching,
monitoring, profiling, benchmarking, sweeps, GGUF model inventory, and
capability evaluation.

RC scope is the active `ozone` binary plus the developer-facing `ozone-mcp`
automation binary. The former ozone+ chat shell, KoboldCpp/SillyTavern handoff,
roleplay, memories, branches, swipes, and transcript UX are deprecated and
archived under `docs/archive/ozone-plus/`.

## Non-Negotiables
- Never commit secrets or API keys
- All code changes must pass `cargo clippy --workspace --all-targets -- -D warnings` (zero warnings)
- No `unwrap()` in runtime code paths — use `?` or log and continue
- Persistence schema changes require a migration path — never break existing user data
- Feature gating must keep the default RC surface coherent: launch, profiling,
  benchmark, sweep, analyze, eval, and model management

## Commands
- Test: `make test` or `cargo test --workspace`
- Lint: `make lint` or `cargo clippy --workspace --all-targets -- -D warnings`
- Preflight: `make preflight` (lint + test — run before every commit)
- Build: `cargo build --workspace` (debug) or `cargo build --release -p ozone --features full`
- Install: `make install` / `make update-oz`
- Release smoke: `make release-smoke`
- Prune: `make prune-artifacts` (clean up target/ bloat)

## After Every Task
After completing any task: update `.mex/ROUTER.md` project state and any `.mex/` files that are now out of date. If no pattern existed for the task you just completed, create one in `.mex/patterns/`.

## Navigation
At the start of every session, read `.mex/ROUTER.md` before doing anything else.
For full project context, patterns, and task guidance — everything is there.
