//! Eval runner pipeline — wires warmup, calibration, gates, suites, and
//! scoring into a single configurable execution flow.

use anyhow::Result;

use crate::artifacts::{self};
use crate::calibration::{run_calibration, CalibrationResult};
use crate::eval_types::EvalStatus;

use crate::gate::{
    check_health_gate, check_lane_gate, promotion_threshold, should_promote, GateDecision,
};
use crate::hash::{hash_model_file, hash_run_config, RunConfigIdentity};
use crate::policy::{check_task_allowed, ContextPolicy};
use crate::preflight::check_context_fit;
use crate::scorers;
use crate::suites::{EvalTask, CANARY_SUITE, CODE_MICRO, FORMAT_MICRO, HEALTH_SUITE, MATH_MICRO};
use crate::timeout::{compute_timeout, TimeoutEstimate, HARD_CAP_SECS};
use crate::warmup::{reset_backend_session, run_warmup};

fn expected_answer(task: &EvalTask) -> &'static str {
    match task.key {
        "health_001_short_answer" => "4",
        "health_007_basic_math" => "105",
        "health_008_instruction_conflict" => "hello",
        "health_009_context_echo" => "42",
        "canary_003_math_basic" => "60",
        "canary_005_long_context_basic" => "Paris",
        "math_001_arithmetic" => "96",
        "math_002_percent" => "50",
        "math_003_two_step_word" => "37",
        _ => "",
    }
}

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
            Self::Full => vec![
                HEALTH_SUITE,
                CANARY_SUITE,
                CODE_MICRO,
                FORMAT_MICRO,
                MATH_MICRO,
            ],
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
    let config_hash = hash_run_config(&RunConfigIdentity {
        model_hash: &model_hash,
        backend: &config.backend,
        quant: "auto",
        kv_quant: "auto",
        context_length: config.context_length,
        batch_size: 512,
        gpu_layers: 35,
        threads: 12,
        sampler_profile: "default",
        seed: 42,
    });

    // ---- Step 2: Warm-up ----
    if !config.skip_warmup {
        let warmup = run_warmup(&config.base_url, 30).await;
        result.warmup_passed = warmup.success;
        println!(
            "Sweep level: {} ({} tasks)",
            config.sweep_level.label(),
            config.sweep_level.task_count()
        );
        println!(
            "Warm-up: {} ({} ms, {} chars)",
            if warmup.success { "ok" } else { "failed" },
            warmup.latency_ms,
            warmup.output.len()
        );
        if let Some(error) = warmup.error.as_deref() {
            println!("Warm-up detail: {error}");
        }
        if warmup.success {
            let _ = reset_backend_session(&config.base_url).await;
        }
    }

    // ---- Step 3: Calibration ----
    let cal = run_calibration(&config.base_url).await;
    result.calibration = Some(cal.clone());
    println!(
        "Speed: {:.1} tok/s (calibration {} ms)",
        cal.decode_tok_per_sec, cal.total_duration_ms
    );

    // ---- Step 4: Health gate ----
    let health = check_health_gate(&cal);
    result.health_gate = Some(health.clone());
    println!(
        "Gate {} score {:.2}: {}",
        health.gate_name, health.score, health.reason
    );
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

    let client = ozone_core::http::client_with_timeout(HARD_CAP_SECS)?;

    let art_dir = artifacts::default_artifact_base().ok();
    let run_dirs = if let Some(ref base) = art_dir {
        artifacts::create_run_dirs(1, base).ok()
    } else {
        None
    };
    if let Some(ref dirs) = run_dirs {
        let _ = artifacts::write_log(
            dirs,
            "run",
            &format!(
                "model={}\nconfig_hash={config_hash}\nartifact_root={}\n",
                config.model_name,
                dirs.run_root.display()
            ),
        );
    }

    for suite in suites {
        for task in suite {
            // Context fit check
            if let Err(error) =
                check_task_allowed(config.context_length, task.min_context, &config.policy)
            {
                println!("  Skip {} — policy: {error}", task.key);
                continue;
            }
            let expected_budget = task
                .size_class
                .max_output_tokens()
                .min(task.max_output_tokens);
            let fit = check_context_fit(
                1024,
                expected_budget,
                config.context_length,
                config.policy.safety_margin_tokens,
            );
            if !fit.fits {
                println!(
                    "  Skip {} — context: {}",
                    task.key,
                    fit.reason.as_deref().unwrap_or("exceeds available context")
                );
                continue;
            }

            // Compute timeout
            let timeout = compute_timeout(TimeoutEstimate::new(
                1024,
                expected_budget,
                cal.prompt_tok_per_sec,
                cal.decode_tok_per_sec,
                cal.first_token_ms,
            ));

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
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(json) => json["choices"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    Err(_) => String::new(),
                },
                Err(_) => String::new(),
            };
            let latency_ms = start.elapsed().as_millis() as u64;

            // Score
            let scored = scorers::score(task.scorer, &response, expected_answer(task));
            result.tasks_run += 1;
            if scored.passed {
                result.tasks_passed += 1;
            }

            // Log
            if let Some(lane) = task.lane {
                if let Some(required) = promotion_threshold(lane) {
                    let decision = check_lane_gate(lane, scored.score, required);
                    let _promoted = should_promote(&decision);
                }
            }

            let status = if scored.passed {
                EvalStatus::Passed
            } else {
                EvalStatus::Failed
            };
            let mark = if scored.passed { "[PASS]" } else { "[FAIL]" };
            println!(
                "  {mark} {} ({:.1}s) score={:.2} {} status={} failure={} lane={} difficulty={} language={} timeout={}s",
                task.key,
                latency_ms as f64 / 1000.0,
                scored.score,
                scored.detail,
                status.as_str(),
                scored.failure.as_str(),
                task.lane.unwrap_or("none"),
                task.difficulty,
                task.language,
                timeout,
            );

            // Store artifact
            if let Some(ref dirs) = run_dirs {
                let _ = artifacts::write_prompt(dirs, task.key, task.prompt);
                let _ = artifacts::write_response(dirs, task.key, &response);
            }
        }
    }

    result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
    result.status = "completed".into();
    println!(
        "Eval run complete ({sweep}): {passed}/{total} passed",
        sweep = config.sweep_level.label(),
        passed = result.tasks_passed,
        total = result.tasks_run
    );

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
    let _config_hash = hash_run_config(&RunConfigIdentity {
        model_hash: &model_hash,
        backend: &config.backend,
        quant: "auto",
        kv_quant: "auto",
        context_length: config.context_length,
        batch_size: 512,
        gpu_layers: 35,
        threads: 12,
        sampler_profile: "default",
        seed: 42,
    });

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
            detail: format!(
                "{} ({} ms, {} chars{})",
                if warmup.success { "ok" } else { "failed" },
                warmup.latency_ms,
                warmup.output.len(),
                warmup
                    .error
                    .as_deref()
                    .map(|error| format!(", {error}"))
                    .unwrap_or_default()
            ),
        });
        if warmup.success {
            let _ = reset_backend_session(&config.base_url).await;
        }
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
        detail: format!(
            "Speed: {:.1} tok/s · calibration {} ms",
            cal.decode_tok_per_sec, cal.total_duration_ms
        ),
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
        detail: format!(
            "{} score {:.2}: {}",
            if health.passed { "passed" } else { "skipped" },
            health.score,
            health.reason
        ),
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

    let client = ozone_core::http::client_with_timeout(HARD_CAP_SECS)?;

    for suite in suites {
        let suite_name = suite.first().map(|t| t.suite).unwrap_or("unknown");
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: format!("Suite: {}", suite_name),
            detail: format!("Running {} tasks...", suite.len()),
        });

        for task in suite {
            // Context fit check
            if let Err(error) =
                check_task_allowed(config.context_length, task.min_context, &config.policy)
            {
                let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::TaskSkipped {
                    task_key: task.key.into(),
                    reason: error.to_string(),
                });
                continue;
            }
            let expected_budget = task
                .size_class
                .max_output_tokens()
                .min(task.max_output_tokens);
            let fit = check_context_fit(
                1024,
                expected_budget,
                config.context_length,
                config.policy.safety_margin_tokens,
            );
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
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(json) => json["choices"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    Err(_) => String::new(),
                },
                Err(_) => String::new(),
            };
            let latency_ms = start.elapsed().as_millis() as u64;

            // Score
            let scored = scorers::score(task.scorer, &response, expected_answer(task));
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
