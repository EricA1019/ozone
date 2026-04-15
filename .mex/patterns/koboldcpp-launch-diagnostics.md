---
name: koboldcpp-launch-diagnostics
description: Diagnose or harden Ozone's KoboldCpp launcher path, startup errors, and fallback override behavior.
triggers:
  - "koboldcpp won't start"
  - "failed to extract koboldcpp"
  - "launch-koboldcpp.sh"
  - "OZONE_KOBOLDCPP_LAUNCHER"
  - "backend startup diagnostics"
edges:
  - target: "context/architecture.md"
    condition: when deciding whether a fix belongs in base ozone launcher/profile flows versus ozone+ runtime code
  - target: "tui-profiling-workflow.md"
    condition: when launcher failures surface through the profiling advisor or benchmark workflow
  - target: "tui-launcher-smoke-test.md"
    condition: when validating launcher or backend startup behavior on the installed binary
last_updated: 2026-04-14
---

# KoboldCpp Launch Diagnostics

## Context

- Base ozone still launches KoboldCpp through a wrapper path instead of embedding backend management logic directly in ozone+.
- The high-value failure modes are:
  - launcher missing / not executable
  - packaged-binary extraction failure (`[PYI-...] Failed to extract ...`)
  - missing bundled `.so` files
  - fast crash / segfault before `:5001` comes up
  - genuine slow-start timeout
- The repo-side goal is not to repair every local install automatically; it is to make failures fast, classified, and actionable.

## Steps

1. Reproduce the failing launcher path directly outside ozone first:
   - run the configured launcher manually
   - run the underlying KoboldCpp binary directly when needed
   - inspect `~/.local/share/ozone/koboldcpp.log`
2. Confirm whether the child exits immediately or merely never reaches `:5001`.
3. Harden `src/processes.rs::start_kobold()` so it:
   - watches for early child exit
   - classifies known log signatures
   - returns actionable remediation hints instead of a generic timeout
4. Keep launcher resolution configurable through `OZONE_KOBOLDCPP_LAUNCHER` so a repaired wrapper can be used without changing ozone again.
5. Wire the same resolved launcher path into CLI, launcher, and profiling entry points so they all behave consistently.
6. Update profiling failure classification and copy so broken installs do not masquerade as generic backend timeouts.

## Gotchas

- A broken packaged KoboldCpp binary can fail before ozone ever has a chance to talk to `:5001`; waiting the full timeout is bad UX.
- `launch-koboldcpp.sh` may still be present and executable even when the actual install behind it is broken.
- Do not add speculative fallback backends here; keep the fallback bounded to an override path or repaired wrapper.

## Verify

- `cargo test -p ozone --quiet`
- `cargo test --workspace --quiet`
- `cargo clippy --workspace --all-targets --quiet -- -D warnings`
- manual smoke:
  - `cargo run -- bench <model> ...` against the broken install should now fail fast with a classified message
  - setting `OZONE_KOBOLDCPP_LAUNCHER` should redirect ozone to an alternate wrapper path

## Debug

- If profiling still shows a generic timeout, inspect whether `build_failure_report()` knows the new error text.
- If an override path seems ignored, trace every caller that previously hardcoded `~/models/launch-koboldcpp.sh`.
