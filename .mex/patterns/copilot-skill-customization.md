---
name: copilot-skill-customization
description: Create or update reusable local Copilot skills under the user skill library, including scaffold init, lean SKILL.md authoring, reference extraction, and package-script validation.
triggers:
  - "create skill"
  - "update skill"
  - "copilot skill"
  - "SKILL.md"
  - "customization"
edges:
  - target: "context/conventions.md"
    condition: when editing markdown, paths, or portability-sensitive skill docs
  - target: "patterns/mex-scaffold-sync.md"
    condition: when `.mex` files also change or scaffold checks report drift
last_updated: 2026-04-13
---

# Copilot Skill Customization

## Context

- Reusable user skills belong in the user-level Copilot skill library.
- In scaffold docs, prefer descriptive prose over machine-specific paths.
- The existing `skill-creator` user skill includes an init helper for scaffolding and a packaging helper for validation.
- Send packaged output to a session or temp directory so the live skill library stays clean.
- Keep the main skill file lean. Move bulky templates, examples, or matrices into a references directory and link them from the main file.

## Steps

1. Decide placement.
   - If the skill is reusable across repos, create it in the user-level Copilot skill library.
   - If the task is repo-specific and the repo already has a clear Copilot customization convention, follow that convention; otherwise default to the user skill library and note the assumption.
2. Scaffold the skill.
   - Use the init helper from the `skill-creator` user skill to scaffold the new skill.
   - Delete placeholder files you do not need instead of leaving scaffold examples behind.
3. Author the contents.
   - Write concrete frontmatter: tight `name`, trigger-rich `description`.
   - Keep workflow guidance in the main skill file.
   - Put detailed report templates, examples, or domain references in a references directory.
4. Validate before done.
   - Use the packaging helper from the `skill-creator` user skill against the new skill and write the artifact outside the live skill library.
   - Treat validation failures as defects and fix them before stopping.
5. Grow the scaffold.
   - If the task exposed a repeatable workflow, add or update a `.mex` pattern and register it in `patterns/INDEX.md`.
   - Update `.mex/ROUTER.md` only if the project or agent-workflow state materially changed.

## Gotchas

- Placeholder scaffold files are easy to forget; remove them if they are not needed.
- Descriptions like "use for anything involving X" trigger too broadly and will collide with neighboring skills.
- Large examples in the main skill file waste context; move them into references.
- Packaging into the live skill folder creates unnecessary artifacts; use a temp or session directory instead.
- A repo may have `.mex` without any repo-local Copilot customization convention; do not invent one silently.

## Verify

- The skill directory contains only files that the skill actually uses.
- The main skill file frontmatter is concrete and useful as a trigger surface.
- Any files linked from the main skill file exist and are named clearly.
- The packaging helper validates the skill successfully.
- `patterns/INDEX.md` lists this pattern if it was newly created.

## Debug

- If the init helper is unavailable, create the skill directory and main skill file manually.
- If `python` is missing, use `python3`.
- If packaging fails on description quality, make the description more concrete about triggers and outputs.
- If `mex check` reports dead edges, rewrite them as paths relative to `.mex/`.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if the workflow state changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
