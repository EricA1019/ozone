---
name: audit-triage-planning
description: Turn a full-project audit into an ordered remediation plan with explicit sequencing, scope boundaries, and wave exit gates.
triggers:
  - "audit triage"
  - "triage plan"
  - "fix audit findings"
  - "project audit follow-up"
edges:
  - target: context/architecture.md
    condition: when the audit findings span multiple crates or app boundaries
  - target: context/conventions.md
    condition: when protocol violations or project rules need to be converted into concrete tasks
  - target: ../plans/release-readiness-plan.md
    condition: when a phased repo plan is needed as a style anchor
last_updated: 2026-05-18
---

# Audit Triage Planning

## Context

- Start from confirmed findings, not impressions.
- Validate current repo state before planning: trust present tests, clippy, compile checks, and live file layout over stale scratch docs.
- The output should be an execution plan, not a restatement of the audit.

## Steps

1. Gather the confirmed findings and split them into four buckets:
   - hidden failures and safety issues
   - docs, CI, and release contract drift
   - structural hotspot files
   - protocol debt that increases future change risk
2. Write one concrete goal sentence for the plan and keep scope narrow enough that the executor can preserve the current green build.
3. Define explicit `In`, `Out`, and `Deferred` scope before writing tasks.
4. Sequence the work into waves:
   - diagnostics and safety hardening first
   - contract alignment second
   - structural refactors third
   - protocol cleanup last
5. Give each wave a short objective, concrete tasks, and an exit gate.
6. End with a validation cadence and a `Done When` section.
7. Save the plan under `plans/` with a descriptive name, then update `.mex/ROUTER.md` if the saved plan changes project-state visibility.

## Gotchas

- Do not flatten the audit into one giant unordered checklist.
- Do not schedule monolith decomposition before hidden-failure hardening.
- Do not treat doc drift as cosmetic when it changes install, backend, CI, or release behavior.
- Keep confirmed findings separate from guesses; guesses become deferred work or spikes.

## Verify

- The plan has a concrete goal, scope, prerequisites, waves, exit gates, validation cadence, and done criteria.
- Wave 0 contains safety and hidden-failure fixes, not refactors.
- Contract alignment happens before structural cleanup.
- Each task points at a real file, module, or system.

## Debug

- If the plan keeps expanding, rewrite the `Out` and `Deferred` lists before adding more tasks.
- If the plan starts reading like a changelog, regroup tasks by execution wave instead of by file.
- If uncertain work remains, split it into a spike instead of smuggling open questions into implementation tasks.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `.mex/patterns/INDEX.md`
