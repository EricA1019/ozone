# Contributing to Ozone

Ozone is being prepared for an RC as a local model/config capability profiler.
Keep contributions aligned with `docs/RC_SCOPE.md`.

## Current Scope

The authoritative RC scope is tracked in `docs/RC_SCOPE.md`. Keep all
contributions aligned with that document.

The old chat work is archived under `docs/archive/ozone-plus/`. Do not add new
features to that path unless the change is an archival correction.

## Before Opening A PR

- Keep one fix or feature per PR.
- Open an issue first for scope changes, benchmark policy changes, or new adapters.
- Test hardware/planner/process changes on real hardware when possible.
- Prefer behavior tests over source-string tests.
- Do not reintroduce ozone+ chat, SillyTavern handoff, or active roleplay flows.

## Workspace Structure

```text
src/                        # active ozone launcher, profiling, bench, eval, model management
crates/
  ozone-core/               # shared metadata, paths, hardware, planner/domain helpers
  ozone-mcp/                # developer automation around the active project
  ozone-engine/             # archived/deprecated chat engine support
  ozone-inference/          # legacy inference gateway pieces plus reusable backend code
  ozone-memory/             # archived/deprecated memory domain
  ozone-persist/            # archived/deprecated session persistence plus reusable storage code
docs/archive/ozone-plus/    # deprecated chat design and documentation
```

## Development

```bash
cargo build --workspace
cargo build --release -p ozone --features full
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./contrib/sync-local-install.sh
./contrib/prune-build-artifacts.sh --dry-run
```

Build just the active end-user binary:

```bash
cargo build -p ozone --release --features full
```

The project uses stable Rust. No nightly features.

## Code Style

- Run `make lint` before submitting.
- Run `make preflight` before marking a PR ready.
- No runtime `unwrap()` in fallible production paths.
- Keep `unsafe` out unless there is a concrete reason and a safety comment.
- Use structured JSON serialization instead of manual string formatting.
- Keep constants and policy values named or data-driven.
- Keep UI behavior discoverable from the screen itself; no hidden mode assumptions.

## Evaluation Rules

- New eval tasks must declare the lane, size class, context requirement, max output budget, and scorer.
- Exact-answer tasks must have an explicit expected answer.
- Skips must be recorded with a reason; do not silently omit gated work.
- Warm-up generations are calibration only and must not be scored.
- Public benchmark adapters should normalize results into Ozone's run/task/gate model.

## Commit Messages

Use conventional commits:

```text
<type>(<scope>): short summary in present tense
```

Common scopes: `launcher`, `model`, `profiling`, `bench`, `eval`, `runner`,
`storage`, `tui`, `docs`, `mcp`, `core`.

Examples:

```text
fix(launcher): use configured model directory during launch
feat(eval): record skipped lane gates in CSV export
docs(readme): clarify RC scope and deprecated chat shell
```

## Pull Request Checklist

- Scope matches the RC product direction.
- Docs and implementation agree.
- New behavior has tests at the behavior boundary.
- `cargo fmt`, `make lint`, and `make test` pass or failures are explicitly explained.
- No deprecated chat or ozone+ flows are reactivated.
