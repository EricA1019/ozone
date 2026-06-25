# Ozone — Ozone+ Chat Deprecation & Eval/Bench Focus Plan

**Date:** 2026-06-25
**Current:** v0.4.8-alpha
**Goal:** Deprecate ozone+ chat/roleplay, keep TUI, sharpen eval/benchmark, add MTP

---

## Current State Audit

### Ozone+ Chat — to deprecate

| Location | Lines | Description |
|----------|-------|-------------|
| `apps/ozone-plus/` | ~5,000+ | Standalone ozone+ binary (CLI + chat engine) |
| `src/prefs.rs` | ~10 lines | `Tier::Plus`, `FrontendPreference::OzonePlus` |
| `src/theme.rs` | 1 line | Ozone+ accent color |
| `src/ui/launcher.rs` | ~3 lines | Ozone+ launch references |
| `ozone+/` docs | 4 files | Product family guide, design docs |
| `README.md` | ~20 lines | Ozone+ in product tiers |
| **Total** | **~5,100+ lines** | |

### TUI — to keep and repurpose for eval/bench

| Location | Lines | Status |
|----------|-------|--------|
| `crates/ozone-tui/` | ~2,000+ | ✅ Keep. Chat-specific screens removed, eval/bench screens added |
| `src/ui/` | ~3,000+ | ✅ Keep. Launcher, monitor, profiling UI stays |
| `src/theme.rs` | ~80 lines | ✅ Keep. Theme system stays |

### Eval/Benchmark — to sharpen

| File | Lines | Status |
|------|-------|--------|
| `src/eval.rs` | 589 | 8 presets, lm-eval + EvalPlus runners |
| `src/eval_report.rs` | 314 | Markdown/JSON report generation |
| `src/bench.rs` | 744 | Backend-agnostic benchmarking, Precise/Sweep modes |
| `src/sweep.rs` | 693 | Context stepping, quant KV, VRAM budget |
| `src/db.rs` | ~200 | SQLite benchmark results |
| `src/hardware.rs` | ~150 | GPU/CPU detection |
| `src/planner.rs` | ~200 | Sweep planning |
| `src/processes.rs` | ~400 | Managed llama.cpp lifecycle |
| **Total** | **~3,290 lines** | |

### MTP Support — to add

| Feature | Current | Target |
|---------|---------|--------|
| MTP model detection | None | Detect `MTP` in filename |
| `--draft-max` flag | None | Add to launch config |
| `--spec-type` flag | None | Add to launch config |
| MTP benchmark A/B | None | `BenchMode::MtpCompare` |
| MTP sweep dimension | None | `--mtp` flag to sweeps |
| MTP model catalog | None | Tag MTP-capable models |

---

## Phase 1: Deprecate Ozone+ Chat (Code)

### 1.1 — Remove `Tier::Plus` and `FrontendPreference::OzonePlus`
**Files:** `src/prefs.rs`, `src/main.rs`, `src/ui/launcher.rs`

- Remove `Tier::Plus` from enum
- Remove `FrontendPreference::OzonePlus` from enum
- Remove `--mode plus` and `--frontend ozone-plus` CLI options
- Tier options: only Lite and Base

### 1.2 — Remove ozone-plus app
**Files:** `apps/ozone-plus/`

- Remove entire directory
- Remove any build scripts referencing it

### 1.3 — Remove chat-only TUI screens
**Files:** `crates/ozone-tui/src/`

- Remove chat screens (session, conversation, roleplay)
- Remove ozone+ accent color constant from `src/theme.rs`
- Keep: layout engine, theme system, input dispatch, widget infrastructure
- Update `crates/ozone-tui/Cargo.toml` description to reflect eval focus

### 1.4 — Update CLI help
**Files:** `src/main.rs`

- Remove ozone+ from `after_help`
- Update binary-name detection (remove `ozone-plus`, `oz+` detection)

---

## Phase 2: Deprecate Ozone+ Chat (Docs)

### 2.1 — Update README.md
- Remove ozone+ from product tiers table
- Remove `ozone --mode=plus` from quick start
- Remove chat/roleplay from product philosophy section
- Focus: "backend management + eval/benchmark"

### 2.2 — Archive ozone+/ docs
- Move `ozone+/` → `docs/archive/ozone-plus/`
- Add deprecation notice at top of each file

### 2.3 — Update CHANGELOG + CLAUDE.md

---

## Phase 3: Sharpen Eval/Benchmark

### 3.1 — Make eval/bench always-available features
**Files:** `Cargo.toml`, `src/main.rs`

- Remove `#[cfg(feature = "bench")]` gates → bench always available
- Remove `#[cfg(feature = "sweep")]` gates → sweep always available
- Remove `#[cfg(feature = "analyze")]` gates → analyze always available
- Remove feature flag complexity — bench/sweep/analyze are core

### 3.2 — Add `eval` CLI subcommand
**Files:** `src/main.rs`, `src/eval.rs`

```rust
oz eval --preset gsm8k --model my-model --report   # single preset
oz eval-all --model my-model                        # all 8 presets
oz eval --preset humaneval --model X --json         # machine-readable
```

### 3.3 — Add eval TUI screen
**Files:** `crates/ozone-tui/src/`, `src/ui/`

- New screen: `EvalResultsScreen` — shows results table, progress bars
- New screen: `BenchLiveScreen` — real-time benchmark progress
- Reuse existing TUI infrastructure (layout, theme, input)

### 3.4 — Add JSON output to bench
**Files:** `src/bench.rs`

- `oz bench --model X --json` outputs machine-readable JSON
- Include: model, prompt_tok/s, gen_tok/s, GPU%, VRAM, timestamp

---

## Phase 4: Add MTP Support

### 4.1 — MTP model detection
**Files:** `src/catalog.rs`

- Scan filename for `MTP` suffix
- Add `supports_mtp: bool` to model metadata
- Show `[MTP]` tag in `oz list` output

### 4.2 — MTP launch flags
**Files:** `src/processes.rs`

- Auto-detect MTP models at launch
- `--spec-type ngram-simple` for MTP
- `--draft-max N` (default 16, configurable)
- `--draft-p-min P` (default 0.75)
- Store MTP config in `ManagedLlamaCppLaunchState`

### 4.3 — MTP benchmark comparison
**Files:** `src/bench.rs`

- `BenchMode::MtpCompare` — runs same prompt with/without MTP
- Outputs speedup ratio and per-token comparison
- Store in DB with `mtp_enabled: bool`

### 4.4 — MTP sweep dimension
**Files:** `src/sweep.rs`

- `--mtp` flag to `oz sweep`
- Sweep draft-max values: 4, 8, 16, 32
- Generate MTP speedup curves

---

## Phase 5: Polish & Docs

- `docs/eval-guide.md` — eval workflow guide
- `docs/mtp-setup.md` — MTP setup and tuning
- `docs/ozone-v0.5-plan.md` — this document
- Update CHANGELOG with v0.5.0-alpha entries
- `cargo fmt && cargo clippy && cargo test`

---

## What Stays

| Component | Stays? | New Role |
|-----------|--------|----------|
| `crates/ozone-tui/` | ✅ | Repurposed for eval/bench TUI screens |
| `src/ui/` (launcher, monitor, profiling) | ✅ | Unchanged |
| `src/theme.rs` | ✅ | Kept (minus ozone+ accent) |
| `apps/ozone-plus/` | ❌ | Removed (chat binary) |
| `Tier::Plus` | ❌ | Removed |
| `FrontendPreference::OzonePlus` | ❌ | Removed |
| `ozone+/` docs | ❌ | Archived |

## Summary

| Phase | Files | Removed | Added | Risk |
|-------|-------|---------|-------|------|
| 1 | ~7 | ~5,200 | ~20 | Low — pure removal |
| 2 | ~5 | ~150 | ~50 | Low — documentation |
| 3 | ~6 | ~50 | ~250 | Low — feature gates + TUI screen |
| 4 | ~4 | ~0 | ~300 | Medium — new MTP plumbing |
| 5 | ~5 | ~0 | ~100 | Low — documentation |
| **Total** | **~27** | **~5,400** | **~720** | **Net: -4,680 lines** |

**Execution order:** Phase 1 → 2 → 3 → 4 → 5
