---
name: release-smoke-gates
description: Add or debug shipped-artifact release smoke for fresh temp-XDG and existing-user paths, especially when `make release-smoke` or `make release-gates` changes.
triggers:
  - "release smoke"
  - "release gates"
  - "fresh temp xdg"
  - "existing-user path"
  - "shipped artifact smoke"
edges:
  - target: "context/setup.md"
    condition: when checking the canonical commands or install-parity workflow
  - target: "patterns/ozone-mcp-automation.md"
    condition: when the smoke uses temp-XDG sandboxes, mock backends, PTY journeys, or ozone-mcp helpers
  - target: "patterns/local-install-sync.md"
    condition: when install parity or stale local binaries are part of the failure
last_updated: 2026-05-12
---

# Release Smoke Gates

## Context

- The canonical release-readiness command is `make release-gates`.
- `make release-gates` depends on three distinct truths:
  - workspace lint/tests are green
  - installed binaries match the current `target/release` artifacts
  - shipped-artifact smoke passes for fresh-user and existing-user paths
- The smoke harness lives in `crates/ozone-mcp`, but the binaries under test must be the shipped ones in `target/release`, not debug or `cargo run` fallbacks.

## Steps

1. Build or verify the release artifacts first:
   - `./contrib/sync-local-install.sh --verify-only` if you need a failing parity check
   - `make sync` if parity is already known to be stale and you want to repair it
2. Keep the base Ozone front-door smoke PTY-driven:
   - run it through the ozone-mcp mock-user runner
   - set `OZONE_MCP_FRONT_DOOR_PROFILE=release` so journeys launch `target/release/ozone`
   - seed at least one mock model in the temp-XDG sandbox if the journey must get past splash and reach launcher
  - when you need stronger launcher-adjacent assertions, attach capture overrides to that same seeded release journey or the neighboring seeded capturable journeys (`base_settings`, `base_confirm_launch`, `base_frontend_choice`) and run `screen_check_tool` against the emitted sidecars instead of switching to the generic `base_launcher` sandbox
  - for base Settings captures, prefer the launcher quick-command overlay path (`/settings`) over repeated arrow navigation when launcher action ordering is not stable under PTY automation
  - for base launch-plan coverage, `base_confirm_launch` is a good next checkpoint because it reuses the same seeded sandbox and can assert stable plan labels such as `Confirm Launch`, `Context:`, and `QuantKV:` before frontend choice or launch handoff begins
  - for one step beyond launch-plan coverage, `base_frontend_choice` is a good checkpoint because it still uses the seeded sandbox but stays before backend startup noise; assert `Choose Frontend`, `SillyTavern`, and `ozone+`, and keep `Launching KoboldCpp` absent
3. Keep ozone+ persistence/data-path smoke on shipped binaries, but prefer CLI assertions over PTY menu navigation:
   - launch `target/release/ozone-plus` inside the same temp-XDG sandbox
   - use `create`, `send`, and `list` to prove fresh-user and existing-user persistence
   - inspect the sandboxed repository directly to verify transcript/session growth
4. Keep the release smoke isolated from normal workspace tests:
   - use ignored `ozone-mcp` tests named with a `release_smoke_gate_` prefix
   - invoke them explicitly from `make release-smoke`
5. Wire the canonical command last:
   - `make release-smoke`
   - `make release-gates`

## Gotchas

- Empty-model fresh temp-XDG sandboxes can stay on the base splash forever because `splash_ready` requires both hardware data and a non-empty catalog.
- The generic `base_launcher` capturable target uses a minimal first-run sandbox with no seeded model; for shipped-artifact launcher smoke, reuse the seeded launch-path journey or your capture can stall on splash before the real launcher renders.
- Launcher action ordering can drift enough that repeated `down` navigation lands on the wrong surface; if a target is reachable from the quick-command overlay, that route is usually more stable than stacked arrow keys.
- The PTY helper only supports a fixed named-key table. Printable characters such as `/` for the quick-command overlay must be sent as text, not as a key token.
- `base_confirm_launch` is usually more stable to assert than `base_frontend_choice` or `base_launching` because it stops before handoff- or backend-dependent behavior starts; prefer it when you want one more planner-adjacent release check without widening the smoke too far.
- `base_frontend_choice` is still a reasonable next checkpoint when you do want one more visual assertion past confirm launch, but stop there before `base_launching` unless you explicitly want backend-launch sensitivity in the smoke gate.
- A passing `cargo test -p ozone-mcp release_smoke_gate ...` does not guarantee `make release-gates` will pass; install parity can still fail if `ozone-mcp` in `~/.cargo/bin` or `~/.local/bin` is stale.
- PTY main-menu navigation in ozone+ is a weaker release gate than direct CLI persistence checks; use PTY where front-door rendering matters, and CLI where persisted user state is the real requirement.
- Treat `make verify-install-parity` failures as signal, not noise. Repair them with `make sync`, then rerun the same gate.

## Verify

- `cargo test -p ozone-mcp release_smoke_gate -- --ignored --nocapture --test-threads=1`
- `make release-smoke`
- `make release-gates`

## Debug

- If the fresh base smoke never leaves splash, inspect whether the sandbox has a mock model file and whether the journey is accidentally matching splash text instead of a later screen.
- If ozone+ fresh/existing-user smoke fails, check the sandbox command env first: `OZONE__BACKEND__TYPE`, `OZONE__BACKEND__URL`, `XDG_DATA_HOME`, and `HOME`.
- If `make release-gates` fails only at install parity, resync local installs and rerun the exact same command before touching code.
- If the smoke passes in debug but fails in release, verify that the test is really launching `target/release/...` and not falling back to `target/debug` or `cargo run`.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`