//! Unified evaluation result type.
//!
//! Produced by both native eval suites (`suites.rs` → `runner.rs`) and external
//! benchmarks (`eval.rs`). Enables shared reporting, comparison, and storage
//! across all eval paths.
//!
//! # Extension points
//!
//! - `From<(&EvalTask, &ScoredResult)>` converts a native task + its scored
//!   result into `EvalResult` (see `src/eval_result.rs`).
//! - `From<&serde_json::Value>` parses lm-eval / EvalPlus JSON output.
//!
//! # Design decisions
//!
//! Reuses the existing `EvalStatus` enum from `eval_types.rs` instead of defining
//! a parallel enum. See `docs/adr/eval-unification.md` for the full ADR.

use crate::eval_types::EvalStatus;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unified eval result — produced by both native suites and external benchmarks.
/// Phase 3.3 wires this into the runner; suppress dead_code until then.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    /// Model name or identifier.
    pub model_name: String,
    /// Task key (e.g. `"health_001_short_answer"` or `"gsm8k"`).
    pub task_key: String,
    /// Suite or benchmark name.
    pub suite: String,
    /// Lane for gate-based promotion (`None` for external benchmarks).
    pub lane: Option<String>,
    /// Normalized score (0.0–1.0).
    pub score: f64,
    /// Whether the task passed its threshold.
    pub passed: bool,
    /// Outcome status — reuses `EvalStatus` from `eval_types.rs`.
    pub status: EvalStatus,
    /// Elapsed time in milliseconds.
    pub duration_ms: u64,
    /// Error message if `status` is an error variant.
    pub error_message: Option<String>,
    /// Paths to generated artifacts (CSV, JSON, Markdown).
    pub artifact_paths: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// From<(&EvalTask, &ScoredResult)> — converts native task + scored result
// ---------------------------------------------------------------------------

use crate::scorers::ScoredResult;
use crate::suites::EvalTask;

impl From<(&EvalTask, &ScoredResult)> for EvalResult {
    fn from((task, scored): (&EvalTask, &ScoredResult)) -> Self {
        let status = if scored.passed {
            EvalStatus::Passed
        } else {
            // Map FailureType variants to EvalStatus
            use crate::eval_types::FailureType;
            match scored.failure {
                FailureType::Timeout => EvalStatus::Timeout,
                FailureType::RuntimeError => EvalStatus::Crashed,
                _ => EvalStatus::Failed,
            }
        };
        Self {
            // model_name is filled by the caller after converting
            model_name: String::new(),
            task_key: task.key.to_string(),
            suite: task.suite.to_string(),
            lane: task.lane.map(|s| s.to_string()),
            score: scored.score,
            passed: scored.passed,
            status,
            // duration_ms is filled by the caller
            duration_ms: 0,
            error_message: if scored.passed {
                None
            } else {
                Some(scored.detail.clone())
            },
            artifact_paths: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// From<&serde_json::Value> — parses lm-eval / EvalPlus JSON output
// ---------------------------------------------------------------------------

impl TryFrom<&serde_json::Value> for EvalResult {
    type Error = String;

    /// Parse an lm-eval or EvalPlus JSON result into an `EvalResult`.
    ///
    /// Expected JSON structure:
    /// ```json
    /// {
    ///   "results": {
    ///     "task_name": {
    ///       "acc,none": 0.85,
    ///       "acc_norm,none": 0.82
    ///     }
    ///   },
    ///   "config": { "model": "model-name" }
    /// }
    /// ```
    fn try_from(json: &serde_json::Value) -> Result<Self, Self::Error> {
        let model_name = json
            .get("config")
            .and_then(|c| c.get("model"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        let results = json
            .get("results")
            .and_then(|r| r.as_object())
            .ok_or_else(|| "missing 'results' object in lm-eval JSON".to_string())?;

        // Pick the first task result (caller should iterate for multi-task)
        let (task_key, task_result) = results.iter().next().ok_or_else(|| {
            "empty 'results' object in lm-eval JSON".to_string()
        })?;

        // Try acc_norm first, fall back to acc
        let score_value = task_result
            .get("acc_norm,none")
            .or_else(|| task_result.get("acc,none"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                format!("no 'acc_norm,none' or 'acc,none' in task '{}'", task_key)
            })?;

        Ok(Self {
            model_name,
            task_key: task_key.clone(),
            suite: "lm-eval".to_string(),
            lane: None,
            score: score_value,
            passed: score_value >= 0.5, // common default threshold
            status: EvalStatus::Passed,
            duration_ms: 0,
            error_message: None,
            artifact_paths: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Into<Vec<EvalResult>> helper — converts runner::TaskResult into Vec<EvalResult>
// ---------------------------------------------------------------------------

use crate::runner::TaskResult;

impl From<&TaskResult> for EvalResult {
    fn from(task: &TaskResult) -> Self {
        // Map string status to EvalStatus
        let status = match task.status.as_str() {
            "passed" => EvalStatus::Passed,
            "unstable" => EvalStatus::Unstable,
            "gate_skipped" => EvalStatus::SkippedGate,
            "budget_skipped" => EvalStatus::SkippedBudget,
            "timeout" => EvalStatus::Timeout,
            "adapter_error" => EvalStatus::AdapterError,
            _ => EvalStatus::Failed,
        };
        Self {
            model_name: String::new(), // filled by caller
            task_key: task.task_key.clone(),
            suite: task.suite.clone(),
            lane: task.lane.clone(),
            score: task.avg_score,
            passed: task.passed,
            status,
            duration_ms: 0,
            error_message: if task.passed { None } else { Some(task.failure.clone()) },
            artifact_paths: vec![],
        }
    }
}

/// Build a unified report from runner results, then return the directory path.
pub fn write_unified_report_from_runner(
    model_name: &str,
    tasks: &[TaskResult],
) -> Result<std::path::PathBuf, anyhow::Error> {
    let results: Vec<EvalResult> = tasks.iter().map(|t| {
        let mut r = EvalResult::from(t);
        r.model_name = model_name.to_string();
        r
    }).collect();
    crate::eval_report::build_unified_report(&results, model_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval_types::{EvalStatus, FailureType};
    use crate::scorers::ScoredResult;
    use crate::suites::EvalTask;

    #[test]
    fn eval_result_serializes_to_json() {
        let r = EvalResult {
            model_name: "test-model".into(),
            task_key: "health_001".into(),
            suite: "health".into(),
            lane: None,
            score: 1.0,
            passed: true,
            status: EvalStatus::Passed,
            duration_ms: 42,
            error_message: None,
            artifact_paths: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: EvalResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model_name, "test-model");
        assert!(parsed.passed);
        assert_eq!(parsed.duration_ms, 42);
    }

    #[test]
    fn eval_result_handles_error_status() {
        let r = EvalResult {
            model_name: "crashy".into(),
            task_key: "bad_task".into(),
            suite: "test".into(),
            lane: None,
            score: 0.0,
            passed: false,
            status: EvalStatus::Crashed,
            duration_ms: 0,
            error_message: Some("SIGSEGV".into()),
            artifact_paths: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("Crashed"));
        assert!(json.contains("SIGSEGV"));
    }

    #[test]
    fn eval_result_round_trips_all_variants() {
        let variants = [
            EvalStatus::Passed,
            EvalStatus::Failed,
            EvalStatus::SkippedGate,
            EvalStatus::Timeout,
            EvalStatus::Crashed,
        ];
        for status in &variants {
            let r = EvalResult {
                model_name: "m".into(),
                task_key: "t".into(),
                suite: "s".into(),
                lane: None,
                score: 0.5,
                passed: true,
                status: *status,
                duration_ms: 0,
                error_message: None,
                artifact_paths: vec![],
            };
            let json = serde_json::to_string(&r).unwrap();
            let parsed: EvalResult = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.status, *status);
        }
    }

    #[test]
    fn from_native_task_and_scored_result_passed() {
        let task = &EvalTask {
            key: "health_001_short_answer",
            suite: "health",
            lane: None,
            difficulty: "easy",
            language: "text",
            size_class: crate::eval_types::SizeClass::Tiny,
            min_context: 1024,
            prompt: "2+2",
            max_output_tokens: 8,
            scorer: "exact",
            expected_answer: Some("4"),
            no_thinking: false,
        };
        let scored = &ScoredResult {
            score: 1.0,
            passed: true,
            failure: FailureType::None,
            detail: "exact match".into(),
        };
        let result = EvalResult::from((task, scored));
        assert!(result.passed);
        assert_eq!(result.task_key, "health_001_short_answer");
        assert_eq!(result.suite, "health");
        assert_eq!(result.score, 1.0);
    }

    #[test]
    fn from_native_task_and_scored_result_failed() {
        let task = &EvalTask {
            key: "health_001_short_answer",
            suite: "health",
            lane: None,
            difficulty: "easy",
            language: "text",
            size_class: crate::eval_types::SizeClass::Tiny,
            min_context: 1024,
            prompt: "2+2",
            max_output_tokens: 8,
            scorer: "exact",
            expected_answer: Some("4"),
            no_thinking: false,
        };
        let scored = &ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::WrongAnswer,
            detail: "expected '4', got '5'".into(),
        };
        let result = EvalResult::from((task, scored));
        assert!(!result.passed);
        assert_eq!(result.status, EvalStatus::Failed);
        assert_eq!(
            result.error_message.as_deref(),
            Some("expected '4', got '5'")
        );
    }

    #[test]
    fn from_lm_eval_json_parses_basic_result() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "results": {
                    "gsm8k": {
                        "acc,none": 0.72
                    }
                },
                "config": {
                    "model": "test-model"
                }
            }"#,
        )
        .unwrap();

        let result = EvalResult::try_from(&json).unwrap();
        assert_eq!(result.model_name, "test-model");
        assert_eq!(result.task_key, "gsm8k");
        assert_eq!(result.suite, "lm-eval");
        assert!((result.score - 0.72).abs() < 1e-6);
    }

    #[test]
    fn from_lm_eval_json_missing_results_returns_error() {
        let json: serde_json::Value = serde_json::from_str(r#"{"config": {}}"#).unwrap();
        let result = EvalResult::try_from(&json);
        assert!(result.is_err());
    }

    #[test]
    fn from_lm_eval_json_prefers_acc_norm_over_acc() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "results": {
                    "mmlu": {
                        "acc,none": 0.50,
                        "acc_norm,none": 0.65
                    }
                },
                "config": {
                    "model": "m"
                }
            }"#,
        )
        .unwrap();

        let result = EvalResult::try_from(&json).unwrap();
        assert!((result.score - 0.65).abs() < 1e-6);
    }
}
