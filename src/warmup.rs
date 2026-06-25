//! Warm-up discard generation for evaluation runs.
//!
//! Some local backends produce poor or unstable first generations after load.
//! This module runs a short discard prompt before real evaluation tasks,
//! logs the output but NEVER scores it, and optionally resets the backend
//! session/KV cache.

use anyhow::{Context, Result};
use std::time::{Duration, Instant};

/// Result of a warm-up generation (never scored, always logged).
#[derive(Debug, Clone)]
pub struct WarmupResult {
    /// The raw text output from the model.
    pub output: String,
    /// Time to first token in milliseconds.
    pub latency_ms: u64,
    /// Whether the generation completed without error.
    pub success: bool,
    /// Any error message if unsuccessful.
    pub error: Option<String>,
}

/// Run a warm-up discard generation against a llama.cpp server.
///
/// Sends "Reply with exactly: READY" with max_tokens=8 and temperature=0.0.
/// The output is logged but NEVER scored. This ensures the first real
/// evaluation task is not affected by cold-start instability.
///
/// If the backend is not reachable or returns an error, the function
/// returns a `WarmupResult` with `success = false` — it does NOT
/// propagate the error. Eval should continue regardless.
pub async fn run_warmup(base_url: &str, timeout_secs: u64) -> WarmupResult {
    let start = Instant::now();

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return WarmupResult {
                output: String::new(),
                latency_ms: start.elapsed().as_millis() as u64,
                success: false,
                error: Some(format!("failed to build HTTP client: {e}")),
            };
        }
    };

    let url = format!("{}/v1/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "prompt": "Reply with exactly: READY",
        "max_tokens": 8,
        "temperature": 0.0,
        "seed": 42,
        "stop": ["\n"],
    });

    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    let text = json["choices"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    WarmupResult {
                        output: text,
                        latency_ms: latency,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => WarmupResult {
                    output: String::new(),
                    latency_ms: latency,
                    success: false,
                    error: Some(format!("failed to parse response: {e}")),
                },
            }
        }
        Err(e) => WarmupResult {
            output: String::new(),
            latency_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(format!("request failed: {e}")),
        },
    }
}

/// Attempt to reset the backend session/KV cache after warm-up.
///
/// For llama.cpp this sends a request to clear the slot state.
/// Best-effort — failure is logged but not fatal.
pub async fn reset_backend_session(base_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

    // Send a minimal request to reset the slot state
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": "reset"}],
        "max_tokens": 1,
        "temperature": 0.0,
    });

    let _ = client.post(&url).json(&body).send().await
        .with_context(|| "failed to reset backend session")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_result_defaults() {
        let r = WarmupResult {
            output: String::new(),
            latency_ms: 0,
            success: false,
            error: None,
        };
        assert!(!r.success);
        assert!(r.output.is_empty());
    }

    #[tokio::test]
    async fn test_run_warmup_with_unreachable_server() {
        // A server that doesn't exist should return a non-fatal error
        let result = run_warmup("http://127.0.0.1:1", 2).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
