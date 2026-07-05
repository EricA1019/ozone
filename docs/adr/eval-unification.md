# ADR: Eval Result Unification

**Date**: 2026-07-05
**Status**: Accepted

## Context

Ozone has two parallel evaluation systems:

1. **Native eval** (`src/suites.rs` → `src/runner.rs`): Runs health/canary/code/math
   suites directly against the running backend via the OpenAI-compatible API. Uses
   `EvalTask` structs and produces per-task `ScoredResult` via `src/scorers.rs`.

2. **External eval** (`src/eval.rs`): Wraps `lm-eval-harness` and `EvalPlus` as
   subprocesses. Uses `EvalPreset` enum and parses JSON output files.

Each system has its own result types, its own reporting path, and no shared
interface. This makes it impossible to produce unified reports, compare results
across eval types, or add hybrid eval workflows.

## Decision

### 1. Shared `EvalResult` type

Define a single `EvalResult` struct in a new `src/eval_result.rs` module:

```rust
pub struct EvalResult {
    pub model_name: String,
    pub task_key: String,
    pub suite: String,
    pub lane: Option<String>,
    pub score: f64,
    pub passed: bool,
    pub status: EvalStatus,  // reuses existing enum from eval_types.rs
    pub duration_ms: u64,
    pub error_message: Option<String>,
    pub artifact_paths: Vec<PathBuf>,
}
```

Key design choice: reuse the existing `EvalStatus` enum from `src/eval_types.rs`
(variants: Passed, Failed, SkippedGate, SkippedBudget, SkippedUser, Crashed,
Timeout, Invalid, Unstable, AdapterError, SandboxError, ContextTooSmall, CacheHit)
instead of defining a new enum.

### 2. CLI surface — keep separate commands

| Command | Evaluates | Invocation |
|---------|-----------|------------|
| `oz eval-run` | Native suites | `oz eval-run <model>` — runs against running server |
| `oz eval` | External benchmarks | `oz eval <preset>` — spawns Python subprocess |
| `oz bench eval` | External benchmarks | `oz bench eval` — bench workflow wrapper |

Decision: keep separate commands. They share the result type and reporting
functions but have fundamentally different invocation patterns (live server
API vs. spawned subprocess). Merging them would create a confusing CLI.

### 3. Data source — keep in code

Native eval tasks remain as `const` arrays in `suites.rs`. They are stable,
type-checked at compile time, and well-understood. Externalizing to TOML/YAML
is deferred to a future phase. Rationale:

- No current requirement for user-defined tasks
- Type safety prevents malformed task definitions
- Const arrays are trivially discoverable in the codebase

### 4. Backward compatibility

Existing output paths (`results/lm_eval_*`, `results/evalplus_*`) must continue
to produce identical formats. The unified reporting path is additive — it writes
to `results/unified/{model}/` without touching existing paths.

## Consequences

Positive:
- Single `EvalResult` type for downstream consumers (CSV export, HTML reports, leaderboard generation)
- `From` conversions let both eval systems produce the shared type without modifying existing code
- Additive reporting means zero risk to existing workflows
- Reusing `EvalStatus` avoids a parallel enum that would need constant mapping

Negative:
- Two implementations (2a and 2b) needed for the `From` conversions — one for native, one for external
- External benchmark JSON parsing is fragile (depends on lm-eval output format)
- Separate CLI commands means the two eval paths remain conceptually distinct

Neutral:
- Code stays in `suites.rs` and `eval.rs` unchanged per Open/Closed principle
- New `EvalResult` type is serializable and can be stored in SQLite for historical comparison
