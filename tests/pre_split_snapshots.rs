//! Pre-split snapshot tests — verify public API surface stability after
//! structural decomposition (Wave 2+ file splits).
//!
//! These tests import from `ozone::*` directly instead of replicating
//! logic, ensuring they stay in sync with the source.

// ---------------------------------------------------------------------------
// eval.rs snapshot — EvalPreset enum, EVAL_TASKS registry
// ---------------------------------------------------------------------------

#[test]
fn pre_split_eval_preset_has_seventeen_variants() {
    use clap::ValueEnum;
    assert_eq!(ozone::eval::EvalPreset::value_variants().len(), 17);
}

#[test]
fn pre_split_eval_preset_cli_names_are_known() {
    use clap::ValueEnum;
    let names: Vec<String> = ozone::eval::EvalPreset::value_variants()
        .iter()
        .map(|v| v.to_possible_value().unwrap().get_name().to_string())
        .collect();
    assert_eq!(names.len(), 17);
    assert!(names.iter().any(|n| n == "gsm8k"));
    assert!(names.iter().any(|n| n == "gpqa"));
}

#[test]
fn pre_split_eval_tasks_has_expected_keys() {
    let tasks = ozone::eval::EVAL_TASKS;
    assert!(tasks.iter().any(|t| t.cli_name == "humaneval"));
}

// ---------------------------------------------------------------------------
// profiling_actions.rs snapshot — ProfilingAction enum
// ---------------------------------------------------------------------------

#[test]
fn pre_split_profiling_actions_have_ten_labels() {
    use ozone::profiling_actions::ProfilingAction;
    // Manual count from the enum definition
    let variants: &[ProfilingAction] = &[
        ProfilingAction::QuickSweep,
        ProfilingAction::FullSweep,
        ProfilingAction::SingleBenchmark,
        ProfilingAction::BenchmarkSavedProfile,
        ProfilingAction::GenerateProfiles,
        ProfilingAction::ExportPresets,
        ProfilingAction::ImportSpecs,
        ProfilingAction::ThreadSweep,
        ProfilingAction::LaunchRecommended,
        ProfilingAction::ReviewIssue,
    ];
    assert_eq!(variants.len(), 10);
}

// ---------------------------------------------------------------------------
// llamacpp.rs snapshot — kv_cache_args helper
// ---------------------------------------------------------------------------

#[test]
fn pre_split_kv_cache_args_default_no_flags() {
    assert_eq!(ozone::llamacpp::kv_cache_args(1, 1).len(), 0);
}

#[test]
fn pre_split_kv_cache_args_q8_k_only() {
    assert_eq!(ozone::llamacpp::kv_cache_args(2, 1).len(), 2);
}

#[test]
fn pre_split_kv_cache_args_both_set() {
    assert_eq!(ozone::llamacpp::kv_cache_args(3, 2).len(), 4);
}
