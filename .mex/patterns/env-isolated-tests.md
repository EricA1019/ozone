---
name: env-isolated-tests
description: Keep config-sensitive and XDG-sensitive tests deterministic when ambient shell env vars or user config would otherwise leak into the test process.
triggers:
  - "config loader test"
  - "XDG sandbox test"
  - "ambient env leakage"
  - "without_env_overrides"
  - "ScopedEnvVar"
edges:
  - target: "context/conventions.md"
    condition: before adding new test helpers or environment-scoped fixtures
  - target: "ozoneplus-phase2b-hybrid-retrieval.md"
    condition: when testing `index rebuild`, embedding-provider config, or hybrid retrieval flows
last_updated: 2026-05-08
---

# Env-Isolated Tests

## Context

- This repo's tests often run inside a shell that already exports `OZONE__...`
  overrides and XDG paths from live smoke tooling.
- Config-sensitive tests must not silently inherit those values or they stop
  proving baked defaults, file-layer merges, and disabled-provider behavior.
- `ozone-inference::ConfigLoader` now has `without_env_overrides()` for tests
  that need to validate defaults or explicit TOML layers only.

## Steps

1. Identify whether the test is proving config defaults/merges or real
   environment override behavior.
2. For config-loader unit tests, call `ConfigLoader::without_env_overrides()`
   unless the test explicitly exists to verify `OZONE__...` env handling.
3. For CLI or integration tests that resolve repo/config paths through XDG,
   set `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `HOME` to sandbox-local paths.
4. Write fixture config files into the same sandboxed config root the loader
   will actually read, not just `HOME/.config` by convention.
5. Keep env mutation serialized behind the existing test lock when using
   `ScopedEnvVar` or other process-global env changes.

## Gotchas

- Ambient `OZONE__BACKEND__URL` or `OZONE__BACKEND__TYPE` can make default-load
  tests fail even when the loader code is correct.
- Ambient `XDG_CONFIG_HOME` can cause a test to write one config file and load a
  different one, especially around `ozone-plus index rebuild` and embedding
  provider setup.
- Fixing this in one test by mutating global env without the lock can create
  new nondeterministic failures in other tests.

## Verify

- Run the specific previously failing tests first.
- Run the owning crate's full test target.
- Run `make preflight` if the failure blocked workspace validation.

## Debug

- Print `env | rg '^OZONE(__|_)|^XDG_'` from the same shell when failures only
  reproduce locally.
- If a test writes config but the app still loads defaults, compare the fixture
  write path against `directories::ProjectDirs` resolution under the sandboxed
  env.
- If a config test unexpectedly succeeds, check whether a real env override is
  masking the invalid value under test.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if env-isolation changed what validation is green
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new