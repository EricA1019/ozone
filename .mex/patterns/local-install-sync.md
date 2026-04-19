---
name: local-install-sync
description: Build and refresh the local Ozone-family installs without copying stale artifacts over current binaries.
triggers:
  - "sync local install"
  - "update local install"
  - "installed binary is stale"
  - "checksum install"
  - "refresh ~/.local/bin"
edges:
  - target: "context/setup.md"
    condition: when the task is about local installation, PATH layout, or daily run commands
  - target: "context/conventions.md"
    condition: when adding repo scripts or changing developer workflow helpers
last_updated: 2026-04-18
---

# Local Install Sync

## Context

- Raw `cp target/release/... ~/.local/bin/` drifts easily and can leave live tests
  pointed at stale binaries.
- `ozone-plus` is especially easy to leave stale if only part of the workspace was
  rebuilt.
- The canonical install-sync path is `./contrib/sync-local-install.sh`.
- Installed `ozone` / `ozone-plus` binaries can also prompt at startup when a
  newer local `target/release` build exists for the synced repo.

## Steps

1. Build the installable binaries explicitly:
   - `cargo build --release -p ozone -p ozone-plus -p ozone-mcp-app`
2. Compare each built artifact against both install locations:
   - `~/.cargo/bin`
   - `~/.local/bin`
3. Update a destination only when the SHA-256 checksum differs or the binary is
   missing.
4. Verify the installed versions/checksums after syncing when the task is
   install-focused or release-focused.
5. If the task touches the startup prompt path, test both branches:
   - decline once with `N`
   - accept once with `Y` and confirm the binary relaunches cleanly

## Gotchas

- `ozone-mcp` may not print a friendly `--version`; checksum equality is the more
  reliable install-sync proof there.
- Do not assume `cargo build --release` alone refreshed the exact binaries you
  care about; build the installable packages explicitly.
- Keep the helper idempotent so rerunning it on a current install is a safe no-op.
- Interactive CI/automation may need `OZONE_SKIP_INSTALL_UPDATE_PROMPT=1` to
  avoid hanging on the `Y/n` question.

## Verify

- `bash -n ./contrib/sync-local-install.sh`
- `./contrib/sync-local-install.sh --no-build`
- `ozone --version`
- `ozone-plus --version`
