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
pub struct GateDecision {
    /// Name of the gate that produced this decision.
    pub gate_name: &'static str,
    /// Score that triggered this decision (0.0–1.0).
    pub score: f64,
    /// Whether the model/config passed this gate.
    pub passed: bool,
    /// Human-readable reason.
    pub reason: String,
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
    }
}

/// Check a lane-specific gate (Level 2).
///
/// Returns a `GateDecision` indicating whether the model/config should
/// be promoted to advanced tests in this lane.
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
    }
}

/// Promotion thresholds for each lane (design doc §16).
///
/// Returns the minimum score required to unlock the next tier of tests.
pub fn promotion_threshold(lane: &str) -> Option<f64> {
    match lane {
        "python_basic" => Some(0.60),
        "rust_basic" => Some(0.50),
        "code_edit_basic" => Some(0.60),
        "math_basic" => Some(0.60),
        "json_tool_basic" => Some(0.75),
        "long_context_basic" => Some(0.50),
        "summarization_basic" => Some(0.50),
        "code_reading_basic" => Some(0.60),
        _ => None,
    }
}

/// Determine whether a lane gate result justifies promotion to the
/// next tier of tests.
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
}
