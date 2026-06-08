---
name: agents
description: Always-loaded project anchor. Read this first. Contains project identity, non-negotiables, commands, and pointer to ROUTER.md for full context.
last_updated: 2025-07-14
---

# Ozone

## What This Is
A local-first AI backend management and conversation toolkit for llama.cpp and KoboldCpp, shipping as three tiers: ozonelite (lean backend control), ozone (profiling and tuning), and ozone+ (full conversation UX with sessions, memory, and roleplay).

## Non-Negotiables
- Never commit secrets or API keys
- All code changes must pass `cargo clippy --workspace --all-targets -- -D warnings` (zero warnings)
- No `unwrap()` in runtime code paths — use `?` or log and continue
- Persistence schema changes require a migration path — never break existing user data
- Feature gating: lite ⊂ base ⊂ full — higher tiers include all lower-tier features

## Commands
- Test: `make test` or `cargo test --workspace`
- Lint: `make lint` or `cargo clippy --workspace --all-targets -- -D warnings`
- Preflight: `make preflight` (lint + test — run before every commit)
- Build: `cargo build` (debug) or `cargo build --release`
- Install: `make install` (both binaries) / `make install-lite` / `make install-base` / `make install-plus`
- Prune: `make prune-artifacts` (clean up target/ bloat)

## After Every Task
After completing any task: update `.mex/ROUTER.md` project state and any `.mex/` files that are now out of date. If no pattern existed for the task you just completed, create one in `.mex/patterns/`.

## Navigation
At the start of every session, read `.mex/ROUTER.md` before doing anything else.
For full project context, patterns, and task guidance — everything is there.
