---
name: github-actions-release
description: Debugging or updating this repo's GitHub Actions CI and release automation
triggers:
  - "github actions"
  - "ci failure"
  - "release workflow"
  - "push failed"
edges:
  - target: "context/conventions.md"
    condition: when a CI failure points back to Rust code or command usage
  - target: "context/setup.md"
    condition: when release packaging or environment assumptions need to be checked
last_updated: 2026-04-12
---

# GitHub Actions CI and Release Workflow

## Context

- CI lives in `.github/workflows/ci.yml`.
- Release automation lives in `.github/workflows/release.yml`.
- CI uses GitHub-hosted Rust stable, so it may catch newer clippy lints than an older local toolchain.
- The release flow is split on purpose:
  - pushes to `main` (or manual dispatch) ensure the version tag exists
  - the actual packaged release build runs from the `v*` tag push

## Steps

1. Inspect the latest workflow runs and read the failing job logs first.
2. If CI fails, reproduce with the repo commands locally:
   - `cargo check --release`
   - `cargo clippy --release -- -D warnings`
   - `cargo test --release`
3. Fix the code issue before weakening the lint rules.
4. If release automation is the problem, confirm:
   - how the tag is resolved
   - whether the referenced packaged files actually exist
   - whether the workflow is triggered from the intended event
5. Keep the release workflow idempotent where practical (`overwrite_files`, no duplicate tag creation paths).

## Gotchas

- A normal push to `main` is not the same as a tag push; if the workflow expects a tag, make sure the main-push path creates one.
- Do not let a workflow both push a tag and build the release in the same path if that would race the tag-triggered run.
- Do not silence clippy warnings just because CI is newer than the local toolchain; prefer fixing the code.
- Packaging steps must only reference files that actually exist in the repo.

## Verify

- `cargo check --release` passes.
- `cargo clippy --release -- -D warnings` passes.
- `cargo test --release` passes.
- `.github/workflows/release.yml` has a clear split between tag creation and tag-based release building.
- The archive step references real files in the repository.

## Debug

- If CI fails only on GitHub, compare the local toolchain with GitHub's Rust stable.
- If release automation "does nothing," check whether the workflow is waiting for a tag event rather than a branch push.
- If a release rerun fails on an existing asset, prefer an overwrite-capable release step instead of forcing manual cleanup.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if CI/release behavior changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
