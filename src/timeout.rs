//! Dynamic timeout computation for evaluation tasks.
//!
//! Timeouts are calculated from the model's measured speed (from calibration)
//! and the task's expected output size. This ensures slower local models get
//! enough time while preventing broken runs from hanging forever.

#[derive(Debug, Clone, Copy)]
pub struct TimeoutEstimate {
    prompt_tokens: u32,
    max_output_tokens: u32,
    prompt_tok_per_sec: f64,
    decode_tok_per_sec: f64,
    first_token_ms: u64,
    min_seconds: u64,
    multiplier: f64,
    hard_cap_seconds: u64,
}

impl TimeoutEstimate {
    pub fn new(
        prompt_tokens: u32,
        max_output_tokens: u32,
        prompt_tok_per_sec: f64,
        decode_tok_per_sec: f64,
        first_token_ms: u64,
    ) -> Self {
        Self {
            prompt_tokens,
            max_output_tokens,
            prompt_tok_per_sec,
            decode_tok_per_sec,
            first_token_ms,
            min_seconds: MIN_TIMEOUT_SECS,
            multiplier: DEFAULT_MULTIPLIER,
            hard_cap_seconds: HARD_CAP_SECS,
        }
    }

    #[cfg(test)]
    pub fn with_limits(mut self, min_seconds: u64, multiplier: f64, hard_cap_seconds: u64) -> Self {
        self.min_seconds = min_seconds;
        self.multiplier = multiplier;
        self.hard_cap_seconds = hard_cap_seconds;
        self
    }
}

/// Compute a dynamic timeout in seconds for an evaluation task.
pub fn compute_timeout(estimate: TimeoutEstimate) -> u64 {
    // Avoid division by zero
    let prompt_tps = if estimate.prompt_tok_per_sec > 0.0 {
        estimate.prompt_tok_per_sec
    } else {
        1.0
    };
    let decode_tps = if estimate.decode_tok_per_sec > 0.0 {
        estimate.decode_tok_per_sec
    } else {
        1.0
    };

    let prompt_time = estimate.prompt_tokens as f64 / prompt_tps;
    let decode_time = estimate.max_output_tokens as f64 / decode_tps;
    let first_token_sec = estimate.first_token_ms as f64 / 1000.0;
    let overhead_sec = 1.0; // fixed overhead for HTTP, parsing, etc.

    let estimated = prompt_time + decode_time + first_token_sec + overhead_sec;
    let with_margin = estimated * estimate.multiplier;

    (with_margin as u64)
        .max(estimate.min_seconds)
        .min(estimate.hard_cap_seconds)
}

/// Default multiplier applied to estimated runtime for safety margin.
pub const DEFAULT_MULTIPLIER: f64 = 2.5;

/// Minimum timeout in seconds for any task.
pub const MIN_TIMEOUT_SECS: u64 = 20;

/// Hard cap on timeout in seconds for any task.
pub const HARD_CAP_SECS: u64 = 180;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_timeout_basic() {
        // Fast model, small task: 100 prompt + 50 output
        let t = compute_timeout(TimeoutEstimate::new(100, 50, 500.0, 50.0, 100));
        // Estimated: 100/500 + 50/50 + 0.1 + 1.0 = 0.2 + 1.0 + 0.1 + 1.0 = 2.3
        // With margin: 2.3 * 2.5 = 5.75, clamped min 20
        assert_eq!(t, 20);
    }

    #[test]
    fn test_compute_timeout_slow_model() {
        // Slow model, large task: 500 prompt + 500 output
        let t = compute_timeout(TimeoutEstimate::new(500, 500, 20.0, 8.0, 500));
        // Estimated: 500/20 + 500/8 + 0.5 + 1.0 = 25 + 62.5 + 0.5 + 1.0 = 89.0
        // With margin: 89.0 * 2.5 = 222.5, clamped max 180
        assert_eq!(t, 180);
    }

    #[test]
    fn test_compute_timeout_honors_hard_cap() {
        let t = compute_timeout(
            TimeoutEstimate::new(10000, 10000, 5.0, 2.0, 2000).with_limits(20, 2.5, 120),
        );
        // Should hit the hard cap
        assert_eq!(t, 120);
    }

    #[test]
    fn test_compute_timeout_zero_speed_fallback() {
        // If speed is 0, fallback to 1.0 to avoid div by zero
        let t = compute_timeout(TimeoutEstimate::new(100, 100, 0.0, 0.0, 0));
        assert_eq!(t, 180);
    }

    #[test]
    fn test_constants_are_sane() {
        assert_eq!(DEFAULT_MULTIPLIER, 2.5);
        assert_eq!(MIN_TIMEOUT_SECS, 20);
        assert_eq!(HARD_CAP_SECS, 180);
    }
}
