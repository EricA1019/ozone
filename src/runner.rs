//! Eval runner pipeline — wires warmup, calibration, gates, suites, and
//! scoring into a single configurable execution flow.

use anyhow::Result;
use std::time::Duration;

use crate::artifacts::{self};
use crate::calibration::{run_calibration, CalibrationResult};

use crate::gate::{check_health_gate, GateDecision};
use crate::hash::{hash_model_file, hash_run_config};
use crate::policy::{check_task_allowed, ContextPolicy};
use crate::preflight::{check_context_fit};
use crate::scorers;
use crate::suites::{EvalTask, CANARY_SUITE, CODE_MICRO, FORMAT_MICRO, HEALTH_SUITE, MATH_MICRO};
use crate::timeout::{compute_timeout, HARD_CAP_SECS, MIN_TIMEOUT_SECS};
use crate::warmup::run_warmup;


/// Sweep depth for eval runs — controls which suites are executed.
///
/// Quick: health + canary (~17 tasks). Good for fast sanity checks.
/// Standard: health + canary + code_micro (~21 tasks). Covers coding quality.
/// Full: all 5 suites (~36 tasks). Comprehensive evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepLevel {
    Quick,
    Standard,
    Full,
}

impl SweepLevel {
    /// Return the slice of suites to run for this sweep level.
    pub fn suites(&self) -> Vec<&'static [EvalTask]> {
        match self {
            Self::Quick => vec![HEALTH_SUITE, CANARY_SUITE],
            Self::Standard => vec![HEALTH_SUITE, CANARY_SUITE, CODE_MICRO],
            Self::Full => vec![HEALTH_SUITE, CANARY_SUITE, CODE_MICRO, FORMAT_MICRO, MATH_MICRO],
        }
    }

    /// Human-readable label for this sweep level.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Quick => "Quick Sweep",
            Self::Standard => "Standard Sweep",
            Self::Full => "Full Sweep",
        }
    }

    /// Estimated task count for display.
    pub fn task_count(&self) -> usize {
        self.suites().iter().map(|s| s.len()).sum()
    }
}

/// Configuration for an eval run.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EvalRunConfig {
    /// Model name.
    pub model_name: String,
    /// Model file path.
    pub model_path: String,
    /// Backend type (e.g. "llama.cpp").
    pub backend: String,
    /// Backend URL (e.g. "http://127.0.0.1:8989").
    pub base_url: String,
    /// Configured context length.
    pub context_length: u32,
    /// Whether to skip warm-up.
    pub skip_warmup: bool,
    /// Whether to skip health gate (force run suites).
    pub skip_health_gate: bool,
    /// Context policy.
    pub policy: ContextPolicy,
    /// Sweep depth (which suites to run).
    pub sweep_level: SweepLevel,
}

impl Default for EvalRunConfig {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            model_path: String::new(),
            backend: "llama.cpp".into(),
            base_url: "http://127.0.0.1:8989".into(),
            context_length: 16384,
            skip_warmup: false,
            skip_health_gate: false,
            policy: ContextPolicy::default(),
            sweep_level: SweepLevel::Full,
        }
    }
}

/// Result of a full eval run.
#[derive(Debug, Clone)]
pub struct EvalRunResult {
    pub status: String,
    pub warmup_passed: bool,
    pub calibration: Option<CalibrationResult>,
    pub health_gate: Option<GateDecision>,
    pub tasks_run: usize,
    pub tasks_passed: usize,
    pub total_duration_ms: u64,
}

/// Run the full eval pipeline.
pub async fn run_eval(config: &EvalRunConfig) -> Result<EvalRunResult> {
    let overall_start = std::time::Instant::now();
    let mut result = EvalRunResult {
        status: "running".into(),
        warmup_passed: false,
        calibration: None,
        health_gate: None,
        tasks_run: 0,
        tasks_passed: 0,
        total_duration_ms: 0,
    };

    // ---- Step 1: Model hash ----
    let model_hash = hash_model_file(std::path::Path::new(&config.model_path))
        .unwrap_or_else(|_| "unknown".into());
    let _config_hash = hash_run_config(
        &model_hash, &config.backend, "auto", "auto",
        config.context_length, 512, 35, 12, "default", 42,
    );

    // ---- Step 2: Warm-up ----
    if !config.skip_warmup {
        let warmup = run_warmup(&config.base_url, 30).await;
        result.warmup_passed = warmup.success;
        println!("Sweep level: {}", config.sweep_level.label());
    println!("Warm-up: {}", if warmup.success { "ok" } else { "failed" });
    }

    // ---- Step 3: Calibration ----
    let cal = run_calibration(&config.base_url).await;
    result.calibration = Some(cal.clone());
    println!("Speed: {:.1} tok/s", cal.decode_tok_per_sec);

    // ---- Step 4: Health gate ----
    let health = check_health_gate(&cal);
    result.health_gate = Some(health.clone());
    if !health.passed && !config.skip_health_gate {
        result.status = format!("blocked: {}", health.reason);
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }

    // ---- Step 5: Context check ----
    if let Err(e) = check_task_allowed(config.context_length, 0, &config.policy) {
        result.status = format!("blocked: {e}");
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }

    // ---- Step 6: Run suites ----
    let suites = config.sweep_level.suites();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HARD_CAP_SECS))
        .build()?;

    let art_dir = artifacts::default_artifact_base().ok();
    let run_dirs = if let Some(ref base) = art_dir {
        artifacts::create_run_dirs(1, base).ok()
    } else {
        None
    };

    for suite in suites {
        for task in suite {
            // Context fit check
            let fit = check_context_fit(1024, task.max_output_tokens, config.context_length, 512);
            if !fit.fits {
                println!("  Skip {} — context: {}", task.key,
                    fit.reason.as_deref().unwrap_or("exceeds available context"));
                continue;
            }

            // Compute timeout
            let _timeout = compute_timeout(
                1024, task.max_output_tokens,
                cal.prompt_tok_per_sec, cal.decode_tok_per_sec,
                cal.first_token_ms,
                MIN_TIMEOUT_SECS, 2.5, HARD_CAP_SECS,
            );

            // Run task
            let url = format!("{}/v1/completions", config.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "prompt": task.prompt,
                "max_tokens": task.max_output_tokens,
                "temperature": 0.0,
                "seed": 42,
            });

            let start = std::time::Instant::now();
            let response = match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => json["choices"][0]["text"].as_str().unwrap_or("").to_string(),
                        Err(_) => String::new(),
                    }
                }
                Err(_) => String::new(),
            };
            let latency_ms = start.elapsed().as_millis() as u64;

            // Score
            let scored = scorers::score(task.scorer, &response, "");
            result.tasks_run += 1;
            if scored.passed {
                result.tasks_passed += 1;
            }

            // Log
            let mark = if scored.passed { "[PASS]" } else { "[FAIL]" };
            println!("  {mark} {} ({:.1}s) score={:.2} {}",
                task.key, latency_ms as f64 / 1000.0, scored.score, scored.detail);

            // Store artifact
            if let Some(ref dirs) = run_dirs {
                let _ = artifacts::write_prompt(dirs, task.key, task.prompt);
                let _ = artifacts::write_response(dirs, task.key, &response);
            }
        }
    }

    result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
    result.status = "completed".into();
    println!("Eval run complete ({sweep}): {passed}/{total} passed", sweep = config.sweep_level.label(), passed = result.tasks_passed, total = result.tasks_run);

    Ok(result)
}

/// Run the eval pipeline with TUI event emission.
///
/// Same as `run_eval` but sends progress events through an unbounded channel
/// for real-time TUI display. The sender is consumed by the function.
pub async fn run_eval_with_events(
    config: &EvalRunConfig,
    tx: tokio::sync::mpsc::UnboundedSender<crate::ui::eval_run_workflow::EvalRunEvent>,
) -> Result<EvalRunResult> {
    let overall_start = std::time::Instant::now();
    let mut result = EvalRunResult {
        status: "running".into(),
        warmup_passed: false,
        calibration: None,
        health_gate: None,
        tasks_run: 0,
        tasks_passed: 0,
        total_duration_ms: 0,
    };

    // ---- Step 1: Model hash ----
    let model_hash = hash_model_file(std::path::Path::new(&config.model_path))
        .unwrap_or_else(|_| "unknown".into());
    let _config_hash = hash_run_config(
        &model_hash, &config.backend, "auto", "auto",
        config.context_length, 512, 35, 12, "default", 42,
    );

    // ---- Step 2: Warm-up ----
    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
        name: "Warm-up".into(),
        detail: "Running discard generation...".into(),
    });
    if !config.skip_warmup {
        let warmup = run_warmup(&config.base_url, 30).await;
        result.warmup_passed = warmup.success;
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Warm-up".into(),
            detail: if warmup.success { "ok".into() } else { "failed".into() },
        });
    }

    // ---- Step 3: Calibration ----
    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
        name: "Calibration".into(),
        detail: "Running speed/repetition/stop probes...".into(),
    });
    let cal = run_calibration(&config.base_url).await;
    result.calibration = Some(cal.clone());
    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
        name: "Calibration".into(),
        detail: format!("Speed: {:.1} tok/s", cal.decode_tok_per_sec),
    });

    // ---- Step 4: Health gate ----
    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
        name: "Health Gate".into(),
        detail: "Checking calibration results...".into(),
    });
    let health = check_health_gate(&cal);
    result.health_gate = Some(health.clone());
    if !health.passed && !config.skip_health_gate {
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Failed {
            message: format!("Health gate blocked: {}", health.reason),
        });
        result.status = format!("blocked: {}", health.reason);
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }
    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
        name: "Health Gate".into(),
        detail: if health.passed { "passed".into() } else { "skipped".into() },
    });

    // ---- Step 5: Context check ----
    if let Err(e) = check_task_allowed(config.context_length, 0, &config.policy) {
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Failed {
            message: format!("Blocked: {e}"),
        });
        result.status = format!("blocked: {e}");
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }

    // ---- Step 6: Run suites ----
    let suites = config.sweep_level.suites();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HARD_CAP_SECS))
        .build()?;

    for suite in suites {
        let suite_name = suite.first().map(|t| t.suite).unwrap_or("unknown");
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: format!("Suite: {}", suite_name),
            detail: format!("Running {} tasks...", suite.len()),
        });

        for task in suite {
            // Context fit check
            let fit = check_context_fit(1024, task.max_output_tokens, config.context_length, 512);
            if !fit.fits {
                let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::TaskSkipped {
                    task_key: task.key.into(),
                    reason: fit.reason.unwrap_or_default(),
                });
                continue;
            }

            // Run task
            let url = format!("{}/v1/completions", config.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "prompt": task.prompt,
                "max_tokens": task.max_output_tokens,
                "temperature": 0.0,
                "seed": 42,
            });

            let start = std::time::Instant::now();
            let response = match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => json["choices"][0]["text"].as_str().unwrap_or("").to_string(),
                        Err(_) => String::new(),
                    }
                }
                Err(_) => String::new(),
            };
            let latency_ms = start.elapsed().as_millis() as u64;

            // Score
            let scored = scorers::score(task.scorer, &response, "");
            result.tasks_run += 1;
            if scored.passed {
                result.tasks_passed += 1;
            }

            // Emit event
            let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::TaskResult {
                task_key: task.key.into(),
                passed: scored.passed,
                score: scored.score,
                detail: scored.detail,
                latency_ms,
            });
        }
    }

    let duration = overall_start.elapsed().as_millis() as u64;
    result.total_duration_ms = duration;
    result.status = "completed".into();

    let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Completed {
        tasks_run: result.tasks_run,
        tasks_passed: result.tasks_passed,
        duration_ms: duration,
    });

    Ok(result)
}

