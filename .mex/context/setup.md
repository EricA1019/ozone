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
last_updated: 2026-04-18
---

# Setup

## Prerequisites
- Stable Rust toolchain with `cargo`
- `~/.cargo/bin` and/or `~/.local/bin` on your `PATH`
- At least one local inference backend for live runtime work:
  - KoboldCpp, or
  - llama.cpp (`llama-cli` / `llama-server`), or
  - Ollama for the base launcher's ST path

## First-time Setup
1. `git clone https://github.com/EricA1019/ozone.git`
2. `cd ozone`
3. `./contrib/sync-local-install.sh`
4. `ozone`

## Environment Variables
- `OZONE__BACKEND__TYPE` (conditionally required) — force `ozone-plus` to use a specific runtime backend such as `koboldcpp` or `llamacpp`
- `OZONE__BACKEND__URL` (conditionally required) — point `ozone-plus` at a running backend URL
- `OZONE_KOBOLDCPP_LAUNCHER` (optional) — override the KoboldCpp launcher path
- `OZONE_LLAMACPP_CLI` (optional) — override the `llama-cli` path used by `ozone model add --hf`
- `OZONE_LLAMACPP_SERVER` (optional) — override the `llama-server` path used by the launcher/runtime
- `OZONE_SKIP_INSTALL_UPDATE_PROMPT` (optional) — suppress the stale-installed-binary `Y/n` update prompt for automation or scripted runs

## Common Commands
- `cargo build --workspace --release` — build the whole workspace
- `cargo build -p ozone -p ozone-plus -p ozone-mcp-app --release` — build the installable binaries explicitly
- `./contrib/sync-local-install.sh` — rebuild and refresh `~/.cargo/bin` + `~/.local/bin` only when checksums changed
- `cargo clippy --workspace --all-targets` — lint the workspace
- `cargo test --workspace` — run the full test suite
- `ozone --version && ozone-plus --version` — verify the installed launcher/runtime version pair

## Common Issues
- **Installed binary is stale:** run `./contrib/sync-local-install.sh` instead of manually copying `target/release/*` into `~/.local/bin`
- **`ozone-plus` release artifact is stale after a partial build:** build explicit packages (`cargo build -p ozone-plus --release` or use `./contrib/sync-local-install.sh`) instead of relying on an older root release artifact
- **Interactive automation should not stop for a `Y/n` update question:** set `OZONE_SKIP_INSTALL_UPDATE_PROMPT=1`
- **llama.cpp backend commands fail with "not found":** set `OZONE_LLAMACPP_CLI` / `OZONE_LLAMACPP_SERVER` to your local llama.cpp install paths
