//! Gate engine and promotion rules for the eval pipeline.
//!
//! Gates determine whether a model/config should be promoted to more
//! expensive test suites based on cheaper screening results.
//!
//! §14 of the design doc defines three gate levels:
//!   Level 1 — Hard stop (config unusable)
//!   Level 2 — Lane skip (weak in one capability)
//!   Level 3 — Adapter skip (native passes but not enough for public suite)

use crate::calibration::CalibrationResult;

/// A gate decision for a lane or suite.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GateDecision {
    /// Name of the gate that produced this decision.
    pub gate_name: &'static str,
    /// Average score across attempts (0.0–1.0).
    pub score: f64,
    /// Whether the model/config passed this gate.
    pub passed: bool,
    /// Human-readable reason.
    pub reason: String,
    /// Number of passed attempts (for multi-attempt gates).
    pub pass_count: u32,
    /// Total number of attempts.
    pub total_attempts: u32,
}

/// Check health gate (Level 1) based on calibration results.
///
/// This is a hard stop — if the model is severely broken, don't run
/// any quality tests.
pub fn check_health_gate(cal: &CalibrationResult) -> GateDecision {
    let mut failures = Vec::new();

    if cal.repetition_flag {
        failures.push("repetition_collapse");
    }
    if cal.aaaa_flag {
        failures.push("aaaa_collapse");
    }
    if !cal.basic_stability {
        failures.push("unstable");
    }
    if !cal.stop_obeyed {
        failures.push("stop_ignored");
    }
    if cal.backend_status != "ok" {
        failures.push("backend_error");
    }

    let passed = failures.is_empty();
    let reason = if passed {
        "health checks passed".into()
    } else {
        format!("health gate failed: {}", failures.join(", "))
    };

    let score = if passed { 1.0 } else { 0.0 };

    GateDecision {
        gate_name: "health",
        score,
        passed,
        reason,
        pass_count: if passed { 1 } else { 0 },
        total_attempts: 1,
    }
}

/// Check a lane-specific gate (Level 2) — single attempt.
///
/// Returns a `GateDecision` indicating whether the model/config should
/// be promoted to advanced tests in this lane.
#[allow(dead_code)]
pub fn check_lane_gate(gate_name: &'static str, score: f64, required_score: f64) -> GateDecision {
    let passed = score >= required_score;
    let reason = if passed {
        format!(
            "{} score {:.2} >= required {:.2}",
            gate_name, score, required_score
        )
    } else {
        format!(
            "{} score {:.2} < required {:.2}",
            gate_name, score, required_score
        )
    };

    GateDecision {
        gate_name,
        score,
        passed,
        reason,
        pass_count: if passed { 1 } else { 0 },
        total_attempts: 1,
    }
}

/// Check a lane-specific gate (Level 2) — multi-attempt using 2-of-3 rule.
///
/// For 3 attempts:
/// - 3/3 passes → passed, clean
/// - 2/3 passes → passed, unstable
/// - 1/3 passes → failed, unstable
/// - 0/3 passes → failed
///
/// For 1 attempt: falls back to simple threshold check.
pub fn check_lane_gate_multi(
    gate_name: &'static str,
    scores: &[f64],
    required_score: f64,
) -> GateDecision {
    let total = scores.len() as u32;
    let avg_score = if total > 0 {
        scores.iter().sum::<f64>() / total as f64
    } else {
        0.0
    };

    let pass_count = scores.iter().filter(|&&s| s >= required_score).count() as u32;
    let _fail_count = total - pass_count;

    // Majority rule: need > half (ceil division)
    let required_passes = total.div_ceil(2);
    let passed = pass_count >= required_passes;

    let reason = if total == 1 {
        if passed {
            format!(
                "{} score {:.2} >= required {:.2}",
                gate_name, avg_score, required_score
            )
        } else {
            format!(
                "{} score {:.2} < required {:.2}",
                gate_name, avg_score, required_score
            )
        }
    } else if pass_count == total {
        format!(
            "{} avg {:.2} ({}/{}) >= required {:.2} — clean",
            gate_name, avg_score, pass_count, total, required_score
        )
    } else if passed {
        format!(
            "{} avg {:.2} ({}/{}) >= required {:.2} — unstable (one failed attempt)",
            gate_name, avg_score, pass_count, total, required_score
        )
    } else if pass_count == 0 {
        format!(
            "{} avg {:.2} ({}/{}) < required {:.2} — all failed",
            gate_name, avg_score, pass_count, total, required_score
        )
    } else {
        format!(
            "{} avg {:.2} ({}/{}) < required {:.2} — majority failed",
            gate_name, avg_score, pass_count, total, required_score
        )
    };

    GateDecision {
        gate_name,
        score: avg_score,
        passed,
        reason,
        pass_count,
        total_attempts: total,
    }
}

/// Promotion thresholds for each lane (design doc §16).
const PROMOTION_THRESHOLDS: &[(&str, f64)] = &[
    ("python_basic", 0.60),
    ("rust_basic", 0.50),
    ("code_edit_basic", 0.60),
    ("math_basic", 0.60),
    ("json_tool_basic", 0.75),
    ("long_context_basic", 0.50),
    ("summarization_basic", 0.50),
    ("code_reading_basic", 0.60),
];

/// Returns the minimum score required to unlock the next tier of tests.
pub fn promotion_threshold(lane: &str) -> Option<f64> {
    PROMOTION_THRESHOLDS
        .iter()
        .find(|(name, _)| *name == lane)
        .map(|(_, threshold)| *threshold)
}

/// Determine whether a lane gate result justifies promotion to the
/// next tier of tests.
#[allow(dead_code)]
pub fn should_promote(decision: &GateDecision) -> bool {
    if !decision.passed {
        return false;
    }
    if let Some(threshold) = promotion_threshold(decision.gate_name) {
        decision.score >= threshold
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_cal(rep: bool, aaaa: bool, stable: bool, stop: bool, ok: bool) -> CalibrationResult {
        CalibrationResult {
            prompt_tok_per_sec: 100.0,
            decode_tok_per_sec: 10.0,
            first_token_ms: 100,
            basic_stability: stable,
            repetition_flag: rep,
            aaaa_flag: aaaa,
            stop_obeyed: stop,
            backend_status: if ok { "ok".into() } else { "error".into() },
            total_duration_ms: 1000,
        }
    }

    #[test]
    fn test_health_gate_passes_clean() {
        let cal = mock_cal(false, false, true, true, true);
        let d = check_health_gate(&cal);
        assert!(d.passed);
        assert_eq!(d.score, 1.0);
    }

    #[test]
    fn test_health_gate_fails_repetition() {
        let cal = mock_cal(true, false, true, true, true);
        let d = check_health_gate(&cal);
        assert!(!d.passed);
        assert_eq!(d.score, 0.0);
    }

    #[test]
    fn test_health_gate_fails_aaaa() {
        let cal = mock_cal(false, true, true, true, true);
        let d = check_health_gate(&cal);
        assert!(!d.passed);
    }

    #[test]
    fn test_lane_gate_passes() {
        let d = check_lane_gate("python_basic", 0.75, 0.60);
        assert!(d.passed);
    }

    #[test]
    fn test_lane_gate_fails() {
        let d = check_lane_gate("python_basic", 0.45, 0.60);
        assert!(!d.passed);
    }

    #[test]
    fn test_promotion_thresholds() {
        assert_eq!(promotion_threshold("python_basic"), Some(0.60));
        assert_eq!(promotion_threshold("rust_basic"), Some(0.50));
        assert_eq!(promotion_threshold("nonexistent"), None);
    }

    #[test]
    fn test_should_promote() {
        let d = check_lane_gate("python_basic", 0.65, 0.60);
        assert!(should_promote(&d));
    }

    #[test]
    fn test_should_not_promote_below_threshold() {
        let d = check_lane_gate("python_basic", 0.55, 0.60);
        assert!(!should_promote(&d));
    }
    #[test]
    fn test_health_gate_fails_backend_error() {
        let cal = mock_cal(false, false, true, true, false);
        let d = check_health_gate(&cal);
        assert!(!d.passed);
        assert!(d.reason.contains("backend_error"));
    }

    #[test]
    fn test_should_promote_at_exact_threshold() {
        let d = check_lane_gate("python_basic", 0.60, 0.60);
        assert!(should_promote(&d));
    }

    #[test]
    fn test_should_promote_unrecognized_lane() {
        let d = check_lane_gate("unknown_lane", 0.90, 0.50);
        assert!(!should_promote(&d));
    }
}
