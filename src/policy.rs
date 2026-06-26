//! Evaluation policy: context minimums, size class gates, and defaults.
//!
//! Enforces the design doc's min_quality_context policy and provides
//! the canonical defaults for evaluation behavior.

use crate::preflight::DEFAULT_SAFETY_MARGIN;
use anyhow::{bail, Result};

/// Context policy configuration matching design doc §7.
///
/// Quality evaluation below `min_quality_context` is generally not useful
/// for local models. This policy allows controlling what happens when
/// the configured context is too small.
#[derive(Debug, Clone)]
pub struct ContextPolicy {
    /// Minimum context length for quality evaluation (tokens).
    pub min_quality_context: u32,
    /// If true, allow quality tests below min_quality_context.
    pub allow_below_min_context: bool,
    /// Safety margin in tokens for context-fit checks.
    pub safety_margin_tokens: u32,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            min_quality_context: 16_384,
            allow_below_min_context: false,
            safety_margin_tokens: DEFAULT_SAFETY_MARGIN,
        }
    }
}

/// Check whether a task is allowed under the current context policy.
///
/// Returns `Ok(())` if allowed, or `Err` with a message explaining why not.
pub fn check_task_allowed(
    context_length: u32,
    task_min_context: u32,
    policy: &ContextPolicy,
) -> Result<()> {
    if context_length < task_min_context {
        bail!(
            "task requires {} context but config has {}",
            task_min_context,
            context_length,
        );
    }

    if !policy.allow_below_min_context && context_length < policy.min_quality_context {
        bail!(
            "context {} is below min quality threshold {} (set allow_below_min_context to override)",
            context_length, policy.min_quality_context,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_has_16k_min() {
        let p = ContextPolicy::default();
        assert_eq!(p.min_quality_context, 16384);
    }

    #[test]
    fn test_allows_above_min() {
        let p = ContextPolicy::default();
        assert!(check_task_allowed(16384, 0, &p).is_ok());
        assert!(check_task_allowed(32768, 1024, &p).is_ok());
    }

    #[test]
    fn test_rejects_below_min_by_default() {
        let p = ContextPolicy::default();
        let r = check_task_allowed(8192, 0, &p);
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("min quality"));
    }

    #[test]
    fn test_allows_below_min_with_override() {
        let p = ContextPolicy {
            min_quality_context: 16384,
            allow_below_min_context: true,
            safety_margin_tokens: 512,
        };
        assert!(check_task_allowed(4096, 0, &p).is_ok());
    }

    #[test]
    fn test_rejects_below_task_minimum() {
        let p = ContextPolicy::default();
        let r = check_task_allowed(8192, 16384, &p);
        assert!(r.is_err());
    }
}
