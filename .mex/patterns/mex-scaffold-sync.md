---
name: mex-scaffold-sync
description: Diagnose and fix drift in the .mex scaffold so mex check and sync tooling match the current repository.
triggers:
  - "mex drift"
  - "mex check"
  - "resync scaffold"
  - "fix mex"
edges:
  - target: "context/conventions.md"
    condition: when editing scaffold markdown, scripts, or path references
  - target: "context/setup.md"
    condition: when checking whether scaffold commands and helper scripts still match the workflow
last_updated: 2026-04-12
---

# Mex Scaffold Sync

## Context

- `mex check` is the fastest way to detect scaffold drift.
- `mex sync --dry-run` shows which files need attention before making edits.
- Pattern/frontmatter edge targets are resolved from the `.mex/` root, not from the pattern file's directory.
- The scaffold docs currently recommend `.mex/setup.sh` and `.mex/sync.sh`, so those wrappers should exist if the docs keep pointing to them.

## Steps

1. Run `mex check --quiet` for a quick pass, then `mex check` for the full report.
2. If the report is non-empty, run `mex sync --dry-run` to confirm the exact files and issues.
3. Fix `.mex` files surgically:
   - normalize edge targets so they resolve from `.mex/`
   - replace shorthand or example paths that do not exist in this repository
   - update `last_updated` on any context/pattern file you change
4. If `SETUP.md` or `SYNC.md` recommend helper scripts, make sure those scripts exist and invoke the real `mex` command from the repo root.
5. Re-run `mex check` until the scaffold is clean.

## Gotchas

- A path that looks fine relative to a pattern file can still be wrong if `mex` resolves it from `.mex/`.
- Example file names in docs can trigger drift if they look like real repo paths but do not exist.
- Fixing only docs while leaving missing helper scripts in place will keep the scaffold misleading even if the checker goes green.

## Verify

- `mex check --quiet` returns a perfect drift score.
- `mex check` reports `0 errors, 0 warnings`.
- `.mex/setup.sh` can print the `mex setup` help text from the repo root.
- `.mex/sync.sh` can run a dry-run sync from the repo root.

## Debug

- If a pattern edge is reported dead, convert it to the path form used by `.mex/ROUTER.md` and other working scaffold files.
- If a docs reference keeps failing path checks, replace it with the real repo path or rewrite it in prose so it is no longer a fake filesystem path.
- If a helper script exists but still fails, make sure it `cd`s to the repo root before invoking `mex`.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if scaffold behavior changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
