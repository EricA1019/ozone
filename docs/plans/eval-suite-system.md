# Eval Suite System + HTML Leaderboard

> Status: **PLANNED** | Created: 2026-07-02
> Phases: 5 | Files touched: 8 (7 modified, 1 new)

---

## Phase 1: `ozone_eval.py` — Engine

**File:** `contrib/evals/scripts/ozone_eval.py`

### 1a. PRESETS — add 9 entries

```python
"mmlu_pro":              "mmlu_pro",
"arc_challenge":         "arc_challenge",
"mmlu_philosophy":       "mmlu_philosophy",
"hendrycks_ethics":      "hendrycks_ethics",
"bbh_formal_fallacies":  "bbh_formal_fallacies",
"bbh_causal_judgement":  "bbh_causal_judgement",
"mbpp":                  "mbpp",
"drop":                  "drop",
"gpqa":                  "gpqa_main_zeroshot",
```

### 1b. LOGPROB_TASKS — add 4 entries

```python
"mmlu_pro", "arc_challenge", "gpqa", "hendrycks_ethics"
```

Note: `mmlu_philosophy`, `bbh_formal_fallacies`, `bbh_causal_judgement` already matched by existing `"mmlu"` and `"bbh"` substrings.

### 1c. SUITES dict

```python
SUITES = {
    "baseline":            ["hellaswag", "arc_challenge",
                            "bbh_formal_fallacies", "bbh_causal_judgement"],
    "general":             ["mmlu", "mmlu_pro"],
    "philosophy-ethics":   ["mmlu_philosophy", "hendrycks_ethics",
                            "bbh_formal_fallacies", "bbh_causal_judgement"],
    "reasoning":           ["bbh", "drop"],
    "math":                ["gsm8k", "math"],
    "coding":              ["mbpp"],
    "safety":              ["truthfulqa", "instruction"],
    "hard":                ["gpqa"],
}
```

### 1d. SWEEPS dict

```python
SWEEPS = {
    "baseline":  ["baseline"],
    "quick":     ["general", "philosophy-ethics"],
    "full":      ["general", "philosophy-ethics", "reasoning",
                  "math", "coding", "safety"],
    "code":      ["coding", "math"],
    "all":       ["general", "philosophy-ethics", "reasoning",
                  "math", "coding", "safety", "hard"],
}
```

### 1e. CLI — `--suite` and `--sweep` flags

Resolution logic expands suites/sweeps → presets before task loop. Suite and sweep mutually exclusive with `--presets`.

---

## Phase 2: `src/eval.rs` — Rust Backend

### 2a. EvalPreset enum — 9 new variants

```rust
MmluPro,
ArcChallenge,
MmluPhilosophy,
HendrycksEthics,
BbhFormalFallacies,
BbhCausalJudgement,
Mbpp,
Drop,
Gpqa,
```

### 2b. cli_name() impl — 9 mappings

```rust
Self::MmluPro => "mmlu_pro",
Self::ArcChallenge => "arc_challenge",
Self::MmluPhilosophy => "mmlu_philosophy",
Self::HendrycksEthics => "hendrycks_ethics",
Self::BbhFormalFallacies => "bbh_formal_fallacies",
Self::BbhCausalJudgement => "bbh_causal_judgement",
Self::Mbpp => "mbpp",
Self::Drop => "drop",
Self::Gpqa => "gpqa",
```

### 2c. EVAL_TASKS — 9 entries

| CLI name | Task | Output dir |
|----------|------|-----------|
| `mmlu_pro` | `mmlu_pro` | `lm_eval_mmlu_pro_probe` |
| `arc_challenge` | `arc_challenge` | `lm_eval_arc_challenge_probe` |
| `mmlu_philosophy` | `mmlu_philosophy` | `lm_eval_mmlu_philosophy_probe` |
| `hendrycks_ethics` | `hendrycks_ethics` | `lm_eval_hendrycks_ethics_probe` |
| `bbh_formal_fallacies` | `bbh_formal_fallacies` | `lm_eval_bbh_formal_fallacies_probe` |
| `bbh_causal_judgement` | `bbh_causal_judgement` | `lm_eval_bbh_causal_judgement_probe` |
| `mbpp` | `mbpp` | `lm_eval_mbpp_probe` |
| `drop` | `drop` | `lm_eval_drop_probe` |
| `gpqa` | `gpqa_main_zeroshot` | `lm_eval_gpqa_probe` |

### 2d. CSV export — NO CHANGES NEEDED

`write_eval_csv()` auto-discovers results via `output_dir` pattern.

---

## Phase 3: UI Files — Rust TUI

### 3a. `src/ui/bench_eval.rs`

- 9 `BenchEvalAction` variants
- CLI name → action mappings
- Display format strings
- Command builder strings

### 3b. `src/ui/bench_eval_flow.rs`

- 9 action → `start_eval_with_cli_name` handlers
- CLI name → `EvalPreset` conversions

### 3c. `src/ui/eval_launcher.rs` — category tags

| Presets | Tag |
|---------|-----|
| `mmlu_pro`, `arc_challenge` | `[Knowledge]` |
| `mmlu_philosophy`, `hendrycks_ethics` | `[Ethics]` |
| `bbh_formal_fallacies`, `bbh_causal_judgement` | `[Logic]` |
| `mbpp` | `[Coding]` |
| `drop` | `[Reading]` |
| `gpqa` | `[Hard]` |

---

## Phase 4: Documentation

### 4a. `docs/eval-result-ranges.md`

Add score range documentation for all 9 new presets.

### 4b. `.mex/ROUTER.md`

Updated project state after commit.

---

## Phase 5: HTML Leaderboard

**New file:** `contrib/evals/scripts/generate_leaderboard.py` (~430 lines)

### 5a. Data gathering
- Scan `results/` directory tree for all eval CSVs
- Parse model name, quant, size from ozone catalog + GGUF metadata
- Collect score per eval preset per model
- Get token speed from profiling CSVs

### 5b. Output
Single static `results/leaderboard.html` — zero dependencies, opens in any browser.

### 5c. Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  HERO: ASCII OZONE logo + "7 models · 9 suites · 18 tasks"      │
│        + generation timestamp                                    │
├──────────────────────────────────────────────────────────────────┤
│  CONTROLS: [🔍 Filter] [□ Complete only] [Sort ▼] [☀/☾ Theme]  │
├──────────────────────────────────────────────────────────────────┤
│  LEGEND: ████ ≥80%  ████ ≥50%  ████ ≥20%  ████ <20%  ── none  │
├──────────────────────────────────────────────────────────────────┤
│  TABLE (sticky header)                                           │
│  ┌──────────┬───────┬──────┬───────┬──────────────────────┐      │
│  │ Model    │ Quant  │ Size │ tok/s │ base  │ general │ ... │      │
│  ├──────────┼───────┼──────┼───────┼───────┼─────────┼─────┤      │
│  │ SpeedDem │ IQ4_XS │ 8B   │  85   │ 30 26 │ 32 ─    │ ... │      │
│  └──────────┴───────┴──────┴───────┴───────┴─────────┴─────┘      │
├──────────────────────────────────────────────────────────────────┤
│  EXPORT: [📋 Copy CSV] [📥 Download CSV] [🖨 Print]              │
│  FOOTER: "N of M models evaluated"                               │
└──────────────────────────────────────────────────────────────────┘
```

### 5d. Visual polish

| Feature | Spec |
|---------|------|
| Header | Sticky, dark bg, uppercase |
| Suite headers | Span columns, subtle color tint per suite |
| Row hover | Highlight + left border accent |
| Alternating rows | `#1a1a2e` / `#16162a` (eye strain reduction) |
| Best score | Bold + gold glow shadow |
| Missing (`--`) | Gray `#444`, italic, opacity 0.4 |
| Score cells | Monospace, background fill proportional |
| Score ≥ 80% | Green + ✓ checkmark |
| Quant badges | Color-coded rounded pills |
| Theme toggle | Dark ↔ Light |
| Responsive | Full table → collapsed suite toggles → card view |
| Tooltips | Hover for full task name + metadata |

### 5e. Score color scale

```
≥ 80%  → green, bold, ✓
50-79% → yellow
20-49% → orange
< 20%  → red
none   → gray, italic, --
```

### 5f. Quant badge colors

```
IQ2_XXS → red      IQ3_XXS → orange    IQ4_XS  → yellow
Q4_K_M  → purple   Q5_K_M  → blue      Q6_K    → green
Q8_0    → gold     F16     → gray
```

### 5g. CLI integration (future)

```
oz leaderboard              → generate + open
oz leaderboard --json       → raw JSON for agents
oz leaderboard --model X    → filter to one model
```

---

## Execution Order

| Phase | Depends on | Parallelizable |
|-------|-----------|---------------|
| 1 | None | Yes (Python only) |
| 2 | Phase 1 | After Phase 1 |
| 3 | Phase 2 | After Phase 2 |
| 4 | Phase 2 | After Phase 2 |
| 5 | Phase 2 | After Phase 2 |

---

## Validation Checklist

```
Phase 1:
  □ python3 -c "import py_compile; ..." (syntax check)
  □ --suite baseline runs all 4 tasks
  □ --sweep baseline runs all 4 tasks
  □ Server lifecycle: kill before logprob, start before generate
  □ LOGPROB_TASKS substring matching verified

Phase 2:
  □ cargo check
  □ cargo test --workspace
  □ EvalPreset::cli_name() returns correct strings
  □ find_task() resolves all new presets
  □ write_eval_csv() finds results for new tasks

Phase 3:
  □ cargo check (UI compiles)
  □ bench_eval panel shows new presets
  □ Category tags display correctly

Phase 4:
  □ eval-result-ranges.md documents all new presets
  □ ROUTER.md updated

Phase 5:
  □ python generate_leaderboard.py → valid HTML output
  □ HTML opens without errors in browser
  □ Sort, filter, theme toggle work
  □ No broken links or missing assets
```

---

## Scoring Type Reference

| Suite | Presets | Scoring |
|-------|---------|---------|
| baseline | arc_challenge, hellaswag, bbh_formal_fallacies, bbh_causal_judgement | All loglikelihood |
| general | mmlu, mmlu_pro | All loglikelihood |
| philosophy-ethics | mmlu_philosophy, hendrycks_ethics, bbh_formal_fallacies, bbh_causal_judgement | All loglikelihood |
| reasoning | bbh (loglikelihood), drop (generate) | Mixed |
| math | gsm8k, math | All generate |
| coding | mbpp | generate |
| safety | truthfulqa, instruction | All generate |
| hard | gpqa | loglikelihood |

Server auto-management handles mixed suites seamlessly (logprob first, then start server for generate).

---

## Estimated Runtimes (RTX 3060, Q6_K, --limit 50)

| Suite | Tasks | Time |
|-------|-------|------|
| baseline | 4 | ~6 min |
| philosophy-ethics | 4 | ~5 min |
| general | 2 | ~2h |
| reasoning | 2 | ~2.5h |
| math | 2 | ~30 min |
| coding | 1 | ~10 min |
| safety | 2 | ~15 min |
| hard | 1 | ~10 min |
| **full sweep** | 15 | **~5h** |
