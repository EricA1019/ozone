//! Scorer functions for native evaluation tasks.
//!
//! Each scorer takes a model response and an optional expected answer,
//! then produces a normalized score (0.0–1.0) and a failure type.

use crate::eval_types::{EvalStatus, FailureType};

/// Scored result from a single eval task.
#[derive(Debug, Clone)]
pub struct ScoredResult {
    /// Normalized score (0.0–1.0).
    pub score: f64,
    /// Whether the task passed the minimum threshold.
    pub passed: bool,
    /// Failure type if not passed.
    pub failure: FailureType,
    /// Human-readable detail.
    pub detail: String,
}

/// Stability classification for multi-attempt results.
///
/// Describes how consistent a model's performance was across attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stability {
    /// All attempts produced identical results.
    Clean,
    /// Some variance but overall result is clear (e.g. 3/3 or 0/3).
    MildVariance,
    /// High variance — mixed pass/fail across attempts (e.g. 2/3 or 1/3).
    Unstable,
}

impl Stability {
    /// Human-readable label for reports.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::MildVariance => "mild variance",
            Self::Unstable => "unstable",
        }
    }
}

/// Aggregated result from multiple attempts of a single eval task.
///
/// Uses a 2-out-of-3 rule for pass/fail when attempts >= 2:
/// - pass_count >= ceil(attempts/2) → passed
/// - If not all attempts agree → unstable
#[derive(Debug, Clone)]
pub struct MultiScoredResult {
    /// Average of all attempt scores.
    pub avg_score: f64,
    /// Number of passed attempts.
    pub pass_count: u32,
    /// Number of failed attempts. Populated but not yet consumed — reserved for score display improvements.
    #[allow(dead_code)]
    pub fail_count: u32,
    /// Total number of attempts.
    pub total_attempts: u32,
    /// Raw scores from each attempt.
    pub scores: Vec<f64>,
    /// Individual attempt results. Populated but not yet consumed — reserved for per-attempt debug views.
    #[allow(dead_code)]
    pub attempts: Vec<ScoredResult>,
    /// Overall pass/fail using majority rule.
    pub passed: bool,
    /// Primary failure type (most common across attempts).
    pub failure: FailureType,
    /// Stability classification.
    pub stability: Stability,
    /// Human-readable summary.
    pub detail: String,
}

/// Aggregate multiple single-attempt results into a `MultiScoredResult`.
///
/// Uses majority-rule pass/fail: if pass_count >= ceil(total/2), the task
/// is considered passed. Stability is classified as:
/// - Clean: all attempts agree
/// - MildVariance: scores vary but pass/fail is unanimous
/// - Unstable: mixed pass/fail
pub fn aggregate_multi(results: &[ScoredResult]) -> MultiScoredResult {
    let total = results.len() as u32;
    let pass_count = results.iter().filter(|r| r.passed).count() as u32;
    let fail_count = total - pass_count;
    let avg_score = if total > 0 {
        results.iter().map(|r| r.score).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let scores: Vec<f64> = results.iter().map(|r| r.score).collect();

    // Majority rule: need > half to pass (ceil division)
    let required_passes = total.div_ceil(2);
    let passed = pass_count >= required_passes;

    // Stability classification
    let stability = if pass_count == 0 || pass_count == total {
        // All same outcome
        let all_same_score = scores.iter().all(|&s| (s - scores[0]).abs() < f64::EPSILON);
        if all_same_score {
            Stability::Clean
        } else {
            Stability::MildVariance
        }
    } else {
        Stability::Unstable
    };

    // Primary failure type: most common non-None failure
    let failure = results
        .iter()
        .filter(|r| r.failure != FailureType::None)
        .fold(None, |acc: Option<(FailureType, usize)>, r| {
            let count = results.iter().filter(|x| x.failure == r.failure).count();
            match acc {
                Some((_, prev_count)) if count > prev_count => Some((r.failure, count)),
                Some(_) => acc,
                None => Some((r.failure, count)),
            }
        })
        .map(|(ft, _)| ft)
        .unwrap_or(FailureType::None);

    let pass_rate_str = format!("{pass_count}/{total}");
    let detail = if total == 1 {
        results[0].detail.clone()
    } else {
        format!(
            "avg={avg_score:.2} pass_rate={pass_rate_str} stability={}",
            stability.label()
        )
    };

    MultiScoredResult {
        avg_score,
        pass_count,
        fail_count,
        total_attempts: total,
        scores,
        attempts: results.to_vec(),
        passed,
        failure,
        stability,
        detail,
    }
}

/// Convert a `MultiScoredResult` into an `EvalStatus`.
///
/// - Passed if all attempts pass
/// - Passed + Unstable if majority passes but not all
/// - Failed if majority fails
/// - Unstable if exactly tied (shouldn't happen with odd attempts)
pub fn multi_to_status(result: &MultiScoredResult) -> EvalStatus {
    if result.pass_count == result.total_attempts {
        EvalStatus::Passed
    } else if result.passed {
        // Majority passed but not all — model can do it, but not reliably
        EvalStatus::Unstable
    } else if result.pass_count == 0 {
        EvalStatus::Failed
    } else {
        // Some passed but majority didn't
        EvalStatus::Unstable
    }
}

/// Exact match scorer — response must equal expected.
pub fn score_exact(response: &str, expected: &str) -> ScoredResult {
    let cleaned = response.trim();
    let passed = cleaned == expected;
    ScoredResult {
        score: if passed { 1.0 } else { 0.0 },
        passed,
        failure: if passed {
            FailureType::None
        } else {
            FailureType::WrongAnswer
        },
        detail: if passed {
            "exact match".into()
        } else {
            format!("expected '{expected}' got '{cleaned}'")
        },
    }
}

/// JSON validity scorer — response must parse as valid JSON.
pub fn score_json(response: &str) -> ScoredResult {
    let trimmed = response.trim();
    // Strip markdown code fences if present
    let cleaned = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim())
        .unwrap_or(trimmed);

    match serde_json::from_str::<serde_json::Value>(cleaned) {
        Ok(_) => ScoredResult {
            score: 1.0,
            passed: true,
            failure: FailureType::None,
            detail: "valid JSON".into(),
        },
        Err(e) => ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::JsonInvalid,
            detail: format!("invalid JSON: {e}"),
        },
    }
}

/// Repetition/aaaa collapse detector.
pub fn score_repetition(response: &str, _expected: &str) -> ScoredResult {
    let text = response.to_lowercase();
    let words: Vec<&str> = text.split_whitespace().collect();

    // Check for aaaa collapse
    let is_aaaa = text.len() > 3 && text.chars().all(|c| c == 'a' || c == ' ' || c == '\n');

    // Check for bigram repetition
    let has_repetition = if words.len() >= 4 {
        let mut seen = std::collections::HashSet::new();
        let mut repeats = 0;
        for pair in words.windows(2) {
            let key = format!("{} {}", pair[0], pair[1]);
            if !seen.insert(key) {
                repeats += 1;
            }
        }
        repeats >= 3
    } else {
        false
    };

    let failed = is_aaaa || has_repetition || response.trim().is_empty();

    ScoredResult {
        score: if failed { 0.0 } else { 1.0 },
        passed: !failed,
        failure: if is_aaaa {
            FailureType::AaaaCollapse
        } else if has_repetition {
            FailureType::RepetitionCollapse
        } else if response.trim().is_empty() {
            FailureType::EmptyOutput
        } else {
            FailureType::None
        },
        detail: if is_aaaa {
            "aaaa collapse detected".into()
        } else if has_repetition {
            "repetition loop detected".into()
        } else if response.trim().is_empty() {
            "empty output".into()
        } else {
            "stable generation".into()
        },
    }
}

/// Python code scorer — checks for valid Rust-like syntax.
/// In Phase D+ this would execute the code; for now checks basic structure.
pub fn score_code_python(response: &str, _expected: &str) -> ScoredResult {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::EmptyOutput,
            detail: "empty output".into(),
        };
    }

    // Check for function definition keyword (def, fn, or function)
    let has_def =
        trimmed.contains("def ") || trimmed.contains("fn ") || trimmed.contains("function ");
    let has_parens = trimmed.contains('(') && trimmed.contains(')');
    let has_return_like = trimmed.contains("return") || trimmed.contains("->");

    let checks_passed = [has_def, has_parens, has_return_like];
    let passed_count = checks_passed.iter().filter(|&&x| x).count();
    let score = passed_count as f64 / checks_passed.len() as f64;

    ScoredResult {
        score,
        passed: score >= 0.5,
        failure: if score == 0.0 {
            FailureType::FormatInvalid
        } else {
            FailureType::None
        },
        detail: format!(
            "{}/{} structure checks passed",
            passed_count,
            checks_passed.len()
        ),
    }
}

/// Latency/speed scorer — always passes (info only).
pub fn score_latency(response: &str, _expected: &str) -> ScoredResult {
    let passed = !response.trim().is_empty();
    ScoredResult {
        score: if passed { 1.0 } else { 0.0 },
        passed,
        failure: if passed {
            FailureType::None
        } else {
            FailureType::EmptyOutput
        },
        detail: if passed {
            "response received".into()
        } else {
            "empty response".into()
        },
    }
}

/// General format scorer — checks for non-empty, non-gibberish output.
pub fn score_format(response: &str, _expected: &str) -> ScoredResult {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::EmptyOutput,
            detail: "empty output".into(),
        };
    }

    let word_count = trimmed.split_whitespace().count();
    if word_count < 3 {
        return ScoredResult {
            score: 0.3,
            passed: false,
            failure: FailureType::Underanswered,
            detail: "too short".into(),
        };
    }

    ScoredResult {
        score: 1.0,
        passed: true,
        failure: FailureType::None,
        detail: format!("{} words", word_count),
    }
}

/// Rust code scorer — checks for basic Rust syntax markers.
pub fn score_code_rust(response: &str, _expected: &str) -> ScoredResult {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::EmptyOutput,
            detail: "empty output".into(),
        };
    }

    let has_fn = trimmed.contains("fn ");
    let has_arrow = trimmed.contains("->");
    let has_braces = trimmed.contains('{') && trimmed.contains('}');
    let has_let_or_return = trimmed.contains("let ") || trimmed.contains("return");

    let checks = [has_fn, has_arrow, has_braces, has_let_or_return];
    let passed_count = checks.iter().filter(|&&x| x).count();
    let score = passed_count as f64 / checks.len() as f64;

    ScoredResult {
        score,
        passed: score >= 0.5,
        failure: if score == 0.0 {
            FailureType::FormatInvalid
        } else {
            FailureType::None
        },
        detail: format!("{}/{} Rust checks passed", passed_count, checks.len()),
    }
}

/// Dispatch to the correct scorer by name.
pub fn score(scorer: &str, response: &str, expected: &str) -> ScoredResult {
    match scorer {
        "exact" => score_exact(response, expected),
        "json" => score_json(response),
        "repetition" => score_repetition(response, expected),
        "code_python" => score_code_python(response, expected),
        "code_rust" => score_code_rust(response, expected),
        "latency" => score_latency(response, expected),
        "format" => score_format(response, expected),
        _ => ScoredResult {
            score: 0.0,
            passed: false,
            failure: FailureType::AdapterError,
            detail: format!("unknown scorer: {scorer}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let r = score_exact("42", "42");
        assert!(r.passed);
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn test_exact_mismatch() {
        let r = score_exact("43", "42");
        assert!(!r.passed);
    }

    #[test]
    fn test_json_valid() {
        let r = score_json("{\"name\": \"test\"}");
        assert!(r.passed);
    }

    #[test]
    fn test_json_invalid() {
        let r = score_json("{invalid}");
        assert!(!r.passed);
    }

    #[test]
    fn test_repetition_detected() {
        let r = score_repetition("hello world hello world hello world", "");
        assert!(!r.passed);
    }

    #[test]
    fn test_repetition_clean() {
        let r = score_repetition("the quick brown fox jumps", "");
        assert!(r.passed);
    }

    #[test]
    fn test_code_python_good() {
        let r = score_code_python("def add(a, b): return a + b", "");
        assert!(r.passed);
    }

    #[test]
    fn test_dispatch() {
        let r = score("exact", "hello", "hello");
        assert!(r.passed);

        let r = score("unknown", "", "");
        assert!(!r.passed);
    }
}
