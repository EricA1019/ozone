---
name: artifact-hygiene
description: Keep Cargo outputs and other generated artifacts from growing without bound while preserving the most useful rollback-friendly binaries.
triggers:
  - "clean build artifacts"
  - "prune target"
  - "repo is too large"
  - "artifact hygiene"
  - "stale outputs"
edges:
  - target: "context/conventions.md"
    condition: when adding or changing maintenance scripts or developer workflow helpers
  - target: "context/setup.md"
    condition: when documenting daily-run or maintenance commands for contributors
last_updated: 2026-04-20
---

# Artifact Hygiene

## Context

- This workspace can accumulate huge `target/` trees, especially after mixed
  debug/release builds, doc builds, alternate `RUSTFLAGS`, and live-test loops.
- Git and external backups already cover historical rollback better than keeping
  every rebuildable object file forever.
- The hygiene goal is to keep **current runnable binaries** handy while pruning
  heavyweight state that Cargo can regenerate.

## Canonical helper

- Use `./contrib/prune-build-artifacts.sh`
- Wrapper targets:
  - `make prune-artifacts`
  - `make prune-artifacts-dry-run`

## Default policy

1. Keep top-level binaries in `target/debug` and `target/release`
2. Remove heavyweight rebuildable state:
   - `deps`
   - `incremental`
   - `build`
   - `.fingerprint`
   - generated docs
   - `release-lite`
3. Prefer `--dry-run` first when auditing space pressure
4. Use `--full` only when you intentionally want an almost-clean build tree

## When to run it

- After large live-test or profiling sessions
- After switching repeatedly between debug/release or custom `RUSTFLAGS`
- When `target/` grows beyond what feels reasonable for the current task
- Before archiving or snapshotting the repo workspace

## Verify

- `bash -n ./contrib/prune-build-artifacts.sh`
- `./contrib/prune-build-artifacts.sh --dry-run`
- `du -sh target`

## Gotchas

- Do not confuse this helper with install syncing; pruning removes rebuildable
  intermediates, while `sync-local-install.sh` refreshes installed binaries.
- If you need every artifact gone, use `--full` or `cargo clean`; the default
  helper intentionally leaves current top-level binaries behind.
