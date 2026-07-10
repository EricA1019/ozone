# ozone-plus-archive-reference

## Status: Reference

Ozone+ (chat, roleplay, character cards, memories, branches, swipes,
KoboldCpp/SillyTavern handoff, transcript UX) was deprecated in 2026-Q2.

## Where Archived Docs Live

- Design docs: `docs/archive/ozone-plus/`
- Archived patterns: `.mex/patterns/ozoneplus-*.md` (kept for historical reference)
- Archived crates: `crates/ozone-engine`, `crates/ozone-inference`, `crates/ozone-tui` (excluded from workspace)

## Active RC Scope

The active RC surface is the `ozone` binary and `ozone-mcp` automation.
See `docs/RC_SCOPE.md` for current scope boundaries.

## Pattern Files Still Referencing ozone+

These patterns mention ozone+ features but may contain reusable infrastructure
knowledge (e.g., TUI patterns, backend integration, launch diagnostics):

- `koboldcpp-launch-diagnostics.md`
- `llamacpp-backend-integration.md`
- `ozone-launcher-normalization.md`
- `startup-failure-hardening.md`
- `tui-*.md` (TUI shell patterns — reusable for current TUI work)
- `textarea-command-surfaces.md`

The following are purely ozone+ and should only be consulted for historical
context:

- `ozoneplus-context-inspector.md`
- `ozoneplus-conversation-engine.md`
- `ozoneplus-message-editor.md`
- `ozoneplus-persistence-bootstrap.md`
- `ozoneplus-phase1f-import-export.md`
- `ozoneplus-phase1g-launcher-onramp.md`
- `ozoneplus-phase2a-memory-foundations.md`
- `ozoneplus-phase2b-hybrid-retrieval.md`
- `ozoneplus-reroll-swipes.md`
- `ozoneplus-roadmap-planning.md`
- `ozoneplus-runtime-decomposition.md`
- `ozoneplus-streaming-backend-runtime.md`
- `ozoneplus-tui-shell.md`
- `ozoneplus-workspace-bootstrap.md`
- `product-family-docs.md`
