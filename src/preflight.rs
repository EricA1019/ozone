//! Context fit preflight checks for evaluation tasks.
//!
//! Before running a task, verify it fits in the configured context window:
//! prompt_tokens + max_output_tokens + safety_margin <= context_length
//!
//! This prevents false failures caused by impossible context requirements.

/// Result of a context-fit preflight check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitResult {
    /// Whether the task fits in the context window.
    pub fits: bool,
    /// Total tokens required (prompt + output + margin).
    pub required: u32,
    /// Available context window.
    pub available: u32,
    /// Human-readable reason when fits is false.
    pub reason: Option<String>,
}

/// Check whether a task fits in the configured context window.
///
/// The check is:
/// ```text
/// prompt_tokens + max_output_tokens + safety_margin <= context_length
/// ```
///
/// Returns a `FitResult` with the decision and reasoning.
pub fn check_context_fit(
    prompt_tokens: u32,
    max_output_tokens: u32,
    context_length: u32,
    safety_margin: u32,
) -> FitResult {
    let required = prompt_tokens
        .saturating_add(max_output_tokens)
        .saturating_add(safety_margin);

    if required <= context_length {
        FitResult {
            fits: true,
            required,
            available: context_length,
            reason: None,
        }
    } else {
        FitResult {
            fits: false,
            required,
            available: context_length,
            reason: Some(format!(
                "task needs {} tokens ({} prompt + {} output + {} margin) but context is only {}",
                required, prompt_tokens, max_output_tokens, safety_margin, context_length,
            )),
        }
    }
}

/// Default safety margin in tokens for context-fit checks.
pub const DEFAULT_SAFETY_MARGIN: u32 = 512;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fits_within_context() {
        let r = check_context_fit(500, 200, 16384, 512);
        assert!(r.fits);
        assert_eq!(r.required, 1212);
    }

    #[test]
    fn test_exceeds_context() {
        let r = check_context_fit(10000, 5000, 8192, 512);
        assert!(!r.fits);
        assert_eq!(r.required, 15512);
        assert!(r.reason.is_some());
    }

    #[test]
    fn test_exact_fit() {
        let r = check_context_fit(1000, 500, 2012, 512);
        assert!(r.fits);
        assert_eq!(r.required, 2012);
    }

    #[test]
    fn test_zero_safety_margin() {
        let r = check_context_fit(8000, 2000, 10000, 0);
        assert!(r.fits);
    }
}
