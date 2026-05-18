---
name: graphify-integration
description: Install or refresh Graphify for Ozone, align the user-level Copilot skill, and wire the repo-managed refresh flow without fighting existing git hooks.
triggers:
  - "graphify"
  - "knowledge graph"
  - "graphify skill"
  - "graphify-out"
edges:
  - target: "context/setup.md"
    condition: when install commands or optional dev-tool setup need updating
  - target: "patterns/copilot-skill-customization.md"
    condition: when the user-level Copilot skill needs to be created, compared, or refreshed
  - target: "context/conventions.md"
    condition: when editing repo automation, docs, or shell scripts around the Graphify workflow
last_updated: 2026-05-12
---

# Graphify Integration

## Context

- Graphify is optional developer tooling, not part of the shipped Ozone runtime.
- The package name is `graphifyy`; the CLI name is `graphify`.
- The reusable Graphify skill belongs in the user-level Copilot skill library, not under this repo.
- Ozone already owns git-hook installation through `contrib/install-dev-hooks.sh` and `contrib/hooks/*`.

## Steps

1. Read the upstream Graphify install/docs first.
   - Confirm the current package version and supported Copilot install command.
   - Prefer `uv tool install --upgrade --force graphifyy` over ad hoc pip repair.
2. Repair the local CLI before touching repo files.
   - Verify `graphify --version` and `graphify --help`.
   - If `graphify` exists on `PATH` but `import graphify` fails, overwrite the stale shim with the `uv` install above.
3. Refresh the user-level Copilot skill.
   - Run `graphify install --platform copilot`.
   - Compare `~/.copilot/skills/graphify/SKILL.md` with the packaged `skill-copilot.md` from the installed tool environment.
   - If the installer only updates `.graphify_version`, patch `SKILL.md` manually from the packaged template.
4. Align repo files.
   - Keep `.graphifyignore` excluding generated/build noise from the corpus.
   - Do not blanket-ignore `graphify-out/` in `.gitignore`; ignore only local Graphify runtime files so the shareable graph/report can be committed.
   - Document the supported flow: install, `/graphify .`, manual refresh, semantic refresh, and callflow export.
5. Integrate with repo-managed hooks, not Graphify's hook installer.
   - Add a helper script that safely runs `graphify update .` only when `graphify` is on `PATH` and `graphify-out/graph.json` already exists.
   - Call that helper from existing post-commit/post-merge hooks without blocking git operations.
6. Verify.
   - Check CLI version/help, run the helper in the repo, and syntax-check modified shell scripts.

## Gotchas

- `graphify install --platform copilot` can update `.graphify_version` without overwriting a stale `SKILL.md`.
- `graphify update .` is code-only; markdown/docs/papers/images still need `/graphify . --update` from Copilot Chat or a headless `graphify extract ...` run with a configured backend.
- This repo already installs its own git hooks, so `graphify hook install` is redundant and can fight the symlinked hook setup.
- The initial full graph build is best done through `/graphify .` in Copilot Chat because that path uses the assistant model for semantic extraction.

## Verify

- `graphify --version` prints cleanly with no stale-skill warning.
- `graphify --help` lists `update`, `extract`, and `export callflow-html`.
- `./contrib/refresh-graphify.sh` exits cleanly when Graphify or `graphify-out/graph.json` is absent.
- `bash -n` passes on modified hook/helper scripts.
- README and `context/setup.md` describe the same install/refresh flow.

## Debug

- If `graphify` imports fail, rerun `uv tool install --upgrade --force graphifyy`.
- If the skill still looks old, compare against the packaged `skill-copilot.md` inside the uv tool environment.
- If post-commit refresh never runs, confirm `make setup-hooks` installed the symlinked hooks and that `graphify-out/graph.json` exists.
- If a doc-heavy refresh is missing concepts, rerun `/graphify . --update` instead of relying on `graphify update .`.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`
