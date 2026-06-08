---
name: safe-hygiene-pass
description: Behavior-preserving cleanup passes that remove dead tracked residue, stale comments, and unused plumbing without drifting into structural refactors.
triggers:
  - "cleanup pass"
  - "hygiene debt"
  - "safe cleanup"
  - "dead code cleanup"
edges:
  - target: "context/conventions.md"
    condition: when removing code from load-bearing modules or shared data types
  - target: "patterns/artifact-hygiene.md"
    condition: when the cleanup also touches generated files, ignore rules, or tracked artifacts
  - target: "patterns/github-actions-release.md"
    condition: when the hygiene pass includes workflow diagnostics or release automation fixes
last_updated: 2026-05-18
---

# Safe Hygiene Pass

## Context

- Use this for cleanup that should not change product behavior: dead tracked files,
  stale user-facing copy, stale comments, unused fields/helpers, and junk ignore gaps.
- Start from a concrete candidate file or warning site, not a broad repo sweep.
- Prefer tracked files and wired code paths. Untracked local files are user work until
  proven otherwise; do not delete them in a dirty worktree.

## Steps

1. Pick one low-risk candidate and prove the local control path with targeted search.
2. Classify the cleanup:
   - dead tracked file or backup artifact
   - ignore-rule gap for generated output
   - stale user-facing text or misleading comment
   - unused helper/field/plumbing
3. Before editing, state one falsifiable hypothesis about why the candidate is safe to
   remove or update, and one cheap check that could disconfirm it.
4. If the change touches behavior or user-facing text, add or extend a focused test
   first when practical.
5. Make the smallest behavior-preserving edit that resolves the specific residue.
6. Validate immediately with the narrowest useful executable check, then run the repo
   safety net (`make test` and `cargo clippy --workspace --all-targets -- -D warnings`)
   before calling the pass complete.
7. Update `.mex/ROUTER.md` with what landed and what was intentionally left alone.

## Gotchas

- Dirty-worktree rule: do not delete untracked local files just because they look like
  dead duplicates. Record them as blocked cleanup instead.
- If you run a deletion-safety probe on untracked files, prefer copying them to a
  temp location or explicitly verifying the restore path before trusting cleanup
  traps. A failed restore can temporarily hide user-local work even when the probe
  itself was valid.
- Ignore rules do not retroactively untrack files already committed; pair ignore fixes
  with `git rm --cached` only after confirming the files are disposable.
- `#[allow(dead_code)]` on config/data structs is not automatically junk. Many of these
  fields exist for schema parity, display/reporting surfaces, or future-facing adapters.
  Prefer removing only fields that are written and threaded through constructors but
  never actually read.
- Workflow/editor diagnostics can be real hygiene debt even when CI still passes; use
  targeted diagnostics and fix the invalid context at the source.
- Mega-files like `src/ui/mod.rs` and `apps/ozone-plus/src/runtime.rs` invite cleanup
  creep. Stop when the next candidate becomes structural rather than obviously dead.

## Verify

- `make test`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Confirm any new or updated targeted regression tests pass
- Confirm blocked cleanup items are documented rather than silently ignored

## Debug

- If a supposedly dead field causes compile fallout, search for constructor fixtures and
  test-only builders before assuming the field is still load-bearing.
- If a cleanup candidate is only "unused" because the active code path moved elsewhere,
  prefer documenting it as stale residue or removing the whole dead file rather than
  pruning one field at a time.
- If validation is ambiguous because Cargo test filters match nothing, fall back to the
  project safety-net commands instead of guessing test names repeatedly.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`