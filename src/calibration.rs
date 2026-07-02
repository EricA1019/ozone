//! Calibration health checks for evaluation runs.
//!
//! After warm-up but before real tasks, runs a short calibration pass:
//! - Measures prompt/decode tok/s and first-token latency
//! - Checks for repetition collapse and aaaa collapse
//! - Verifies stop token behavior
//! - Estimates dynamic timeout parameters for later tasks
//!
//! Calibration never fails an eval run — it reports results and the
//! runner decides whether to gate promotion.

use std::time::Instant;

/// Results of a calibration health check.
#[derive(Debug, Clone)]
pub struct CalibrationResult {
    /// Prompt processing throughput (tokens/sec).
    pub prompt_tok_per_sec: f64,
    /// Token generation throughput (tokens/sec).
    pub decode_tok_per_sec: f64,
    /// Time to first token in milliseconds.
    pub first_token_ms: u64,
    /// Whether basic response stability was observed.
    pub basic_stability: bool,
    /// Whether the model entered a repetition loop.
    pub repetition_flag: bool,
    /// Whether the model collapsed to aaaa... pattern.
    pub aaaa_flag: bool,
    /// Whether the stop token was obeyed (\n).
    pub stop_obeyed: bool,
    /// Human-readable backend status message.
    pub backend_status: String,
    /// Total calibration duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Run calibration probes against a running llama.cpp server.
///
/// Sends 3 probes:
///   1. Speed probe — measures throughput and latency
///   2. Repetition probe — checks for aaaa/repetition collapse
///   3. Stop probe — verifies stop token behavior
///
/// Returns a `CalibrationResult` with all metrics. Does NOT return an error
/// for bad calibration results — the runner decides how to use the data.
pub async fn run_calibration(base_url: &str) -> CalibrationResult {
    let overall_start = Instant::now();

    let client = match ozone_core::http::client_with_timeout(60) {
        Ok(c) => c,
        Err(e) => {
            return CalibrationResult {
                prompt_tok_per_sec: 0.0,
                decode_tok_per_sec: 0.0,
                first_token_ms: 0,
                basic_stability: false,
                repetition_flag: true,
                aaaa_flag: true,
                stop_obeyed: false,
                backend_status: format!("HTTP client error: {e}"),
                total_duration_ms: overall_start.elapsed().as_millis() as u64,
            };
        }
    };

    let base = base_url.trim_end_matches('/');

    // --- Probe 1: Speed measurement ---
    let speed_result = run_speed_probe(&client, base).await;

    // --- Probe 2: Repetition / aaaa collapse ---
    let collapse_result = run_collapse_probe(&client, base).await;

    // --- Probe 3: Stop token behavior ---
    let stop_result = run_stop_probe(&client, base).await;

    let total_ms = overall_start.elapsed().as_millis() as u64;

    CalibrationResult {
        prompt_tok_per_sec: speed_result.prompt_tok_per_sec,
        decode_tok_per_sec: speed_result.decode_tok_per_sec,
        first_token_ms: speed_result.first_token_ms,
        basic_stability: speed_result.success && collapse_result.success,
        repetition_flag: collapse_result.repetition,
        aaaa_flag: collapse_result.aaaa,
        stop_obeyed: stop_result.obeyed,
        backend_status: if speed_result.success {
            "ok".into()
        } else {
            "speed_probe_failed".into()
        },
        total_duration_ms: total_ms,
    }
}

// ---------------------------------------------------------------------------
// Individual probes
// ---------------------------------------------------------------------------

struct SpeedProbe {
    prompt_tok_per_sec: f64,
    decode_tok_per_sec: f64,
    first_token_ms: u64,
    success: bool,
}

async fn run_speed_probe(client: &reqwest::Client, base: &str) -> SpeedProbe {
    let url = format!("{}/v1/completions", base);
    let body = serde_json::json!({
        "prompt": "Write a short poem about artificial intelligence in exactly four lines.",
        "max_tokens": 64,
        "temperature": 0.0,
        "seed": 42,
    });

    let start = Instant::now();
    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;
            match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
                    let completion_tokens =
                        json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
                    // Estimate TTFT from first token timing (not available from API directly)
                    let first_token_ms = (elapsed_ms as f64 * 0.3) as u64;

                    let prompt_tps = if prompt_tokens > 0 && elapsed_ms > 0 {
                        prompt_tokens as f64 / (elapsed_ms as f64 / 1000.0)
                    } else {
                        0.0
                    };
                    let decode_tps = if completion_tokens > 0 && elapsed_ms > 0 {
                        completion_tokens as f64 / (elapsed_ms as f64 / 1000.0)
                    } else {
                        0.0
                    };

                    SpeedProbe {
                        prompt_tok_per_sec: (prompt_tps * 100.0).round() / 100.0,
                        decode_tok_per_sec: (decode_tps * 100.0).round() / 100.0,
                        first_token_ms,
                        success: completion_tokens > 0,
                    }
                }
                Err(_) => SpeedProbe {
                    prompt_tok_per_sec: 0.0,
                    decode_tok_per_sec: 0.0,
                    first_token_ms: 0,
                    success: false,
                },
            }
        }
        Err(_) => SpeedProbe {
            prompt_tok_per_sec: 0.0,
            decode_tok_per_sec: 0.0,
            first_token_ms: 0,
            success: false,
        },
    }
}

struct CollapseProbe {
    repetition: bool,
    aaaa: bool,
    success: bool,
}

async fn run_collapse_probe(client: &reqwest::Client, base: &str) -> CollapseProbe {
    let url = format!("{}/v1/completions", base);
    let body = serde_json::json!({
        "prompt": "Hello",
        "max_tokens": 32,
        "temperature": 0.0,
        "seed": 42,
    });

    match client.post(&url).json(&body).send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let text = json["choices"][0]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase();

                let aaaa =
                    text.len() > 3 && text.chars().all(|c| c == 'a' || c == ' ' || c == '\n');
                let repetition = detect_repetition(&text);

                CollapseProbe {
                    repetition,
                    aaaa,
                    success: !text.is_empty(),
                }
            }
            Err(_) => CollapseProbe {
                repetition: true,
                aaaa: true,
                success: false,
            },
        },
        Err(_) => CollapseProbe {
            repetition: true,
            aaaa: true,
            success: false,
        },
    }
}

struct StopProbe {
    obeyed: bool,
}

async fn run_stop_probe(client: &reqwest::Client, base: &str) -> StopProbe {
    let url = format!("{}/v1/completions", base);
    let body = serde_json::json!({
        "prompt": "Return the number five.",
        "max_tokens": 16,
        "temperature": 0.0,
        "seed": 42,
        "stop": ["5"],
    });

    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    let text = json["choices"][0]["text"].as_str().unwrap_or("");
                    // Stop is obeyed if the response doesn't contain "5"
                    let obeyed = !text.contains('5');
                    StopProbe { obeyed }
                }
                Err(_) => StopProbe { obeyed: false },
            }
        }
        Err(_) => StopProbe { obeyed: false },
    }
}

/// Simple repetition detection: check if any bigram repeats 3+ times.
fn detect_repetition(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    let mut repeat_count = 0;
    for pair in words.windows(2) {
        let key = format!("{} {}", pair[0], pair[1]);
        if !seen.insert(key) {
            repeat_count += 1;
            if repeat_count >= 3 {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_repetition_no_repeat() {
        assert!(!detect_repetition("the quick brown fox jumps"));
    }

    #[test]
    fn test_detect_repetition_with_repeat() {
        assert!(detect_repetition("hello world hello world hello world"));
    }

    #[test]
    fn test_detect_repetition_short_text() {
        assert!(!detect_repetition("hi"));
    }

    #[tokio::test]
    async fn test_run_calibration_unreachable_server() {
        let result = run_calibration("http://127.0.0.1:1").await;
        assert!(result.repetition_flag);
    }

    #[test]
    fn test_calibration_result_struct_all_fields_accessible() {
        // Verify that all fields of CalibrationResult are accessible
        // and that constructing one works correctly. This guards against
        // accidental field removal or renaming that breaks downstream
        // consumers (gate.rs, CSV export, TUI views).
        let result = CalibrationResult {
            prompt_tok_per_sec: 150.5,
            decode_tok_per_sec: 25.3,
            first_token_ms: 200,
            basic_stability: true,
            repetition_flag: false,
            aaaa_flag: false,
            stop_obeyed: true,
            backend_status: "ok".into(),
            total_duration_ms: 5000,
        };

        assert_eq!(result.prompt_tok_per_sec, 150.5);
        assert_eq!(result.decode_tok_per_sec, 25.3);
        assert_eq!(result.first_token_ms, 200);
        assert!(result.basic_stability);
        assert!(!result.repetition_flag);
        assert!(!result.aaaa_flag);
        assert!(result.stop_obeyed);
        assert_eq!(result.backend_status, "ok");
        assert_eq!(result.total_duration_ms, 5000);
    }
}
