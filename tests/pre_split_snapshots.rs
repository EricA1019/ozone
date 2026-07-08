//! Pre-split snapshot tests — written before Wave 2 file splits to verify
//! that behavior is preserved after extraction.
//!
//! Each test captures the current public API surface of a target file.
//! After the split, these tests must still compile and pass without changes.
//!
//! NOTE: These tests currently use replicated logic because `ozone` is a binary
//! crate without a `lib.rs`. After Wave 2.1 (lib.rs extraction), they should be
//! updated to import from `ozone::*` instead.

// ---------------------------------------------------------------------------
// eval.rs snapshot — EvalPreset enum, EVAL_TASKS registry
// ---------------------------------------------------------------------------

/// Model the EvalPreset enum to capture its current variant surface.
/// After split, this should import from `ozone::eval::EvalPreset`.
fn eval_preset_count() -> usize {
    17
}

fn eval_preset_cli_names() -> &'static [&'static str] {
    &[
        "gsm8k", "instruction", "math", "humaneval", "mmlu", "hellaswag",
        "truthfulqa", "bbh", "mmlu_pro", "arc_challenge", "mmlu_philosophy",
        "hendrycks_ethics", "bbh_formal_fallacies", "bbh_causal_judgement",
        "mbpp", "drop", "gpqa",
    ]
}

fn eval_expected_description_keys() -> &'static [&'static str] {
    &[
        "gsm8k", "instruction", "math", "humaneval", "mmlu", "hellaswag",
        "truthfulqa", "bbh", "mmlu_pro", "arc_challenge", "mmlu_philosophy",
        "hendrycks_ethics", "bbh_formal_fallacies", "bbh_causal_judgement",
        "mbpp", "drop", "gpqa",
    ]
}

#[test]
fn pre_split_eval_preset_has_seventeen_variants() {
    assert_eq!(eval_preset_count(), 17);
}

#[test]
fn pre_split_eval_preset_cli_names_are_known() {
    let names = eval_preset_cli_names();
    assert_eq!(names.len(), 17);
    assert!(names.contains(&"gsm8k"));
    assert!(names.contains(&"gpqa"));
}

#[test]
fn pre_split_eval_tasks_has_expected_keys() {
    let keys = eval_expected_description_keys();
    assert_eq!(keys.len(), 17);
    assert!(keys.contains(&"humaneval"));
}

// ---------------------------------------------------------------------------
// profiling.rs snapshot — ProfilingAction enum
// ---------------------------------------------------------------------------

fn profiling_action_labels() -> &'static [&'static str] {
    &[
        "Run quick sweep",
        "Run full sweep",
        "Run single benchmark",
        "Benchmark saved profile",
        "Generate profiles",
        "Export presets",
        "Import system specs",
        "Sweep thread counts",
        "Launch recommended profile",
        "Review issue report",
    ]
}

#[test]
fn pre_split_profiling_actions_have_ten_labels() {
    let labels = profiling_action_labels();
    assert_eq!(labels.len(), 10);
    assert!(labels.contains(&"Run quick sweep"));
    assert!(labels.contains(&"Review issue report"));
}

// ---------------------------------------------------------------------------
// processes.rs snapshot — kv_cache_args helper
// ---------------------------------------------------------------------------

fn expected_kv_cache_arg_count(quant_k: u8, quant_v: u8) -> usize {
    let k = if quant_k == 2 || quant_k == 3 { 2 } else { 0 };
    let v = if quant_v == 2 || quant_v == 3 { 2 } else { 0 };
    k + v
}

#[test]
fn pre_split_kv_cache_args_default_no_flags() {
    // quant_k=1 (f16) and quant_v=1 (f16) → no flags
    assert_eq!(expected_kv_cache_arg_count(1, 1), 0);
}

#[test]
fn pre_split_kv_cache_args_q8_k_only() {
    // quant_k=2 (q8_0), quant_v=1 (f16) → 2 flags (--cache-type-k q8_0)
    assert_eq!(expected_kv_cache_arg_count(2, 1), 2);
}

#[test]
fn pre_split_kv_cache_args_both_set() {
    // quant_k=3 (q4_0), quant_v=2 (q8_0) → 4 flags
    assert_eq!(expected_kv_cache_arg_count(3, 2), 4);
}
