---
name: setup
description: Dev environment setup and commands. Load when setting up the project for the first time or when environment issues arise.
triggers:
  - "setup"
  - "install"
  - "environment"
  - "getting started"
  - "how do I run"
  - "local development"
edges:
  - target: context/stack.md
    condition: when specific technology versions or library details are needed
  - target: context/architecture.md
    condition: when understanding how components connect during setup
last_updated: 2026-05-12
---

# Setup

## Prerequisites
- Stable Rust toolchain with `cargo`
- `~/.cargo/bin` and/or `~/.local/bin` on your `PATH`
- Python 3.10+ plus `uv` or `pipx` if you want the optional Graphify code-graph workflow
- At least one local inference backend for live runtime work:
  - KoboldCpp, or
  - llama.cpp (`llama-cli` / `llama-server`), or
  - Ollama for the base launcher's ST path

## First-time Setup
1. `git clone https://github.com/EricA1019/ozone.git`
2. `cd ozone`
3. `./contrib/sync-local-install.sh`
4. `make setup-hooks` — install git hooks so local binaries auto-sync after every commit/merge and any existing Graphify graph gets a code-only refresh
5. `ozone`

## Environment Variables
- `OZONE__BACKEND__TYPE` (conditionally required) — force `ozone-plus` to use a specific runtime backend such as `koboldcpp` or `llamacpp`
- `OZONE__BACKEND__URL` (conditionally required) — point `ozone-plus` at a running backend URL
- `OZONE_KOBOLDCPP_LAUNCHER` (optional) — override the KoboldCpp launcher path
- `OZONE_LLAMACPP_CLI` (optional) — override the `llama-cli` path used by `ozone model add --hf`
- `OZONE_LLAMACPP_SERVER` (optional) — override the `llama-server` path used by the launcher/runtime
- `OZONE_SKIP_INSTALL_UPDATE_PROMPT` (optional) — suppress the stale-installed-binary `Y/n` update prompt for automation or scripted runs

## Common Commands
- `cargo build --workspace --release` — build the whole workspace release outputs
- `cargo build -p ozone -p ozone-plus -p ozone-mcp-app` — build the live-test binaries in debug mode
- `cargo build --release -p ozone --features full` — build the installable base ozone artifact with profiling and model-management commands
- `cargo build --release -p ozone-plus -p ozone-mcp-app` — build the other installable release binaries explicitly
- `./contrib/sync-local-install.sh` — rebuild and refresh `~/.cargo/bin` + `~/.local/bin` only when checksums changed
- `./contrib/sync-local-install.sh --verify-only` — fail if installed binaries do not match the current release artifacts
- `make sync` — same as above (preferred shorthand)
- `make verify-install-parity` — shorthand for the non-mutating installed-vs-release parity check
- `make release-smoke` — run the shipped-artifact temp-XDG smoke checks for the fresh base launcher path and the ozone+ existing-user data path
- `make release-gates` — run workspace preflight, install-parity verification, and release smoke in one command
- `make setup-hooks` — one-time: install git hooks so commits/merges auto-sync the local install
- `uv tool install --upgrade --force graphifyy && graphify install --platform copilot` — optional: install Graphify and its user-level Copilot skill
- `make graphify-refresh` — optional: run a safe code-only Graphify refresh when `graphify-out/graph.json` already exists
- `make graphify-scope SCOPE=ozone-tui-core` — optional: build an isolated production-only TUI core graph under `tmp/graphify-scopes/ozone-tui-core/`
- `cargo clippy --workspace --all-targets` — lint the workspace
- `cargo test --workspace` — run the full test suite
- `ozone --version && ozone-plus --version` — verify the installed launcher/runtime version pair

## Common Issues
- **Installed binary is stale:** run `make sync` (or `./contrib/sync-local-install.sh`); for permanent fix, run `make setup-hooks` once
- **Need a failing check instead of an auto-fix for install drift:** run `make verify-install-parity` (or `./contrib/sync-local-install.sh --verify-only`)
- **`make release-gates` fails right after ozone-mcp changes:** this usually means the installed `ozone-mcp` binary is stale by design; run `make sync` once, then rerun `make release-gates`
- **Stale install after a commit:** run `make setup-hooks` to install git hooks — after that, every local commit auto-syncs the installed binaries from the current `target/release` build
- **Base `ozone` release artifact is missing profiling/model commands:** build `cargo build --release -p ozone --features full` or use `./contrib/sync-local-install.sh`; the default feature set is intentionally leaner than the installable base artifact
- **`ozone-plus` release artifact is stale after a partial build:** build explicit packages (`cargo build -p ozone-plus --release` or use `./contrib/sync-local-install.sh`) instead of relying on an older root release artifact
- **PTY smoke tools are launching stale debug binaries:** rebuild the real app targets (`cargo build -p ozone -p ozone-plus -p ozone-mcp-app`) or just run `cargo build --workspace` before `mock_user_tool` / `screenshot_tool`
- **Interactive automation should not stop for a `Y/n` update question:** set `OZONE_SKIP_INSTALL_UPDATE_PROMPT=1`
- **llama.cpp backend commands fail with "not found":** set `OZONE_LLAMACPP_CLI` / `OZONE_LLAMACPP_SERVER` to your local llama.cpp install paths
- **Graphify warns that the local Copilot skill is older than the package:** run `graphify install --platform copilot`; if the CLI itself is stale or broken, rerun `uv tool install --upgrade --force graphifyy`
- **Need to refresh the code graph after refactors:** run `make graphify-refresh`; for markdown/doc/image changes, rerun `/graphify . --update` from Copilot Chat instead of relying on the code-only refresh
- **Need a cleaner Graphify view for one architecture slice:** run `./contrib/graphify-scope.sh --list` and then `make graphify-scope SCOPE=<scope>`; for ozone-tui event-loop/layout/render questions, start with `ozone-tui-core`
