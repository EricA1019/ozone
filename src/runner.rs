//! Eval runner pipeline — wires warmup, calibration, gates, suites, and
//! scoring into a single configurable execution flow.
//!
//! Supports multi-attempt scoring: each task can be run N times with
//! different seeds, and results are aggregated using majority-rule pass/fail
//! with stability classification.
//!
//! When `manage_server` is set, the runner handles the full server lifecycle:
//! clear → launch → eval → kill.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::artifacts::{self};
use crate::calibration::{run_calibration, CalibrationResult};
use crate::gate::{
    check_health_gate, check_lane_gate_multi, promotion_threshold, GateDecision,
};
use crate::hash::{hash_model_file, hash_run_config, RunConfigIdentity};
use crate::policy::{check_task_allowed, ContextPolicy};
use crate::preflight::check_context_fit;
use crate::processes;
use crate::scorers::{self, aggregate_multi, multi_to_status};
use crate::suites::{EvalTask, CANARY_SUITE, CODE_MICRO, FORMAT_MICRO, HEALTH_SUITE, MATH_MICRO};
use crate::timeout::{compute_timeout, TimeoutEstimate, HARD_CAP_SECS};
use crate::warmup::{reset_backend_session, run_warmup};

/// Sweep depth for eval runs — controls which suites are executed and
/// default attempt counts.
///
/// Quick: health + canary (~17 tasks). 1 attempt each. Fast sanity check.
/// Standard: health + canary + code_micro (~21 tasks). 3 attempts for gate
///           tasks, 1 for others.
/// Full: all 5 suites (~36 tasks). 3 attempts for all tasks.
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

    /// Default gate attempts for this sweep level.
    ///
    /// Gate tasks (canary suite lane tasks) get this many attempts.
    /// Quick=1, Standard=3, Full=3.
    pub fn default_gate_attempts(&self) -> u32 {
        match self {
            Self::Quick => 1,
            Self::Standard => 3,
            Self::Full => 3,
        }
    }

    /// Default regular attempts for this sweep level.
    ///
    /// Non-gate tasks get this many attempts.
    /// Quick=1, Standard=1, Full=3.
    pub fn default_regular_attempts(&self) -> u32 {
        match self {
            Self::Quick => 1,
            Self::Standard => 1,
            Self::Full => 3,
        }
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
    /// Number of attempts for gate tasks (canary suite lane tasks).
    /// If 0, uses the default for the sweep level.
    pub gate_attempts: u32,
    /// Number of attempts for regular (non-gate) tasks.
    /// If 0, uses the default for the sweep level.
    pub regular_attempts: u32,
    /// Whether to manage the server lifecycle (clear → launch → eval → kill).
    /// Set to true for CLI usage; false for TUI (server already running).
    pub manage_server: bool,
    /// Path to llama.cpp server binary. Required when manage_server is true.
    pub server_path: Option<PathBuf>,
    /// GPU layers to offload (for server launch).
    pub gpu_layers: i32,
    /// Thread count for server launch (None = auto).
    pub threads: Option<u32>,
    /// Enable flash attention (None = auto/default).
    pub flash_attn: Option<bool>,
    /// Suppress thinking/reasoning output for models that over-think simple prompts.
    /// When true, adds stop tokens and penalty parameters to the completions request.
    pub no_thinking: bool,
}

impl EvalRunConfig {
    /// Effective gate attempt count, falling back to sweep-level default.
    pub fn effective_gate_attempts(&self) -> u32 {
        if self.gate_attempts > 0 {
            self.gate_attempts
        } else {
            self.sweep_level.default_gate_attempts()
        }
    }

    /// Effective regular attempt count, falling back to sweep-level default.
    pub fn effective_regular_attempts(&self) -> u32 {
        if self.regular_attempts > 0 {
            self.regular_attempts
        } else {
            self.sweep_level.default_regular_attempts()
        }
    }
}

impl Default for EvalRunConfig {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            model_path: String::new(),
            backend: "llama.cpp".into(),
            base_url: ozone_core::paths::DEFAULT_LLAMACPP_BASE_URL.into(),
            context_length: 32768,
            skip_warmup: false,
            skip_health_gate: false,
            policy: ContextPolicy::default(),
            sweep_level: SweepLevel::Full,
            gate_attempts: 0,
            regular_attempts: 0,
            manage_server: false,
            server_path: None,
            gpu_layers: 35,
            threads: None,
            flash_attn: None,
            no_thinking: false,
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
    pub tasks_skipped_gate: usize,
    pub total_duration_ms: u64,
    /// Per-task results for persistence.
    pub tasks: Vec<TaskResult>,
}

/// Per-task result for JSON persistence.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskResult {
    pub task_key: String,
    pub suite: String,
    pub lane: Option<String>,
    pub avg_score: f64,
    pub pass_count: u32,
    pub total_attempts: u32,
    pub passed: bool,
    pub status: String,
    pub failure: String,
    pub stability: String,
    pub latency_ms: u64,
}

/// Build llama.cpp server arguments for eval mode.
///
/// Uses conservative defaults suitable for evaluation: temp=0, deterministic
/// seed, and the configured context length (NOT the model's native max).
fn build_eval_server_args(config: &EvalRunConfig) -> Vec<String> {
    let mut args = vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        "8989".into(),
        "--n-gpu-layers".into(),
        config.gpu_layers.to_string(),
        "--ctx-size".into(),
        config.context_length.to_string(),
        "--threads".into(),
        config.threads.unwrap_or(8).to_string(),
        "--parallel".into(),
        "1".into(),
    ];
    // Flash attention (default on for speed)
    let fa = match config.flash_attn {
        Some(false) => "off",
        _ => "on",
    };
    args.push("--flash-attn".into());
    args.push(fa.into());
    // Use Q8 KV cache by default (safer for GPU memory, matches bench pattern)
    args.extend(processes::kv_cache_args(8, 8));
    args
}

/// Run the full eval pipeline with multi-attempt scoring.
///
/// Each task is run N times (configurable per sweep level) with varied seeds.
/// Gate tasks use a 2-of-3 pass rule; failed gates skip deeper tasks in
/// that lane while other lanes continue.
#[tracing::instrument(skip(config))]
pub async fn run_eval(config: &EvalRunConfig) -> Result<EvalRunResult> {
    let overall_start = std::time::Instant::now();
    let gate_attempts = config.effective_gate_attempts();
    let regular_attempts = config.effective_regular_attempts();


    let mut result = EvalRunResult {
        status: "running".into(),
        warmup_passed: false,
        calibration: None,
        health_gate: None,
        tasks_run: 0,
        tasks_passed: 0,
        tasks_skipped_gate: 0,
        total_duration_ms: 0,
        tasks: Vec::new(),
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

    // ---- Step 2: Server launch (when managed) ----
    if config.manage_server {
        let server_path = config
            .server_path
            .as_deref()
            .context("server_path is required when manage_server is true")?;
        let server_args = build_eval_server_args(config);
        let _stopped = processes::clear_gpu_backends().await?;
        // Wait for GPU memory to fully release before re-launching
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        processes::start_llamacpp(
            server_path,
            &config.model_path,
            &server_args,
        )
        .await
        .context("Failed to launch llama.cpp server for eval")?;
        let _loaded = processes::get_llamacpp_model()
            .await
            .ok_or_else(|| anyhow::anyhow!("Server launched but model not available"))?;
    }

    // ---- Step 3: Warm-up ----
    if !config.skip_warmup {
        let warmup = run_warmup(&config.base_url, 30).await;
        result.warmup_passed = warmup.success;
        tracing::warn!(
            "Sweep level: {} ({} tasks, gate×{} / regular×{})",
            config.sweep_level.label(),
            config.sweep_level.task_count(),
            gate_attempts,
            regular_attempts,
        );
        tracing::info!(
            "Warm-up: {} ({} ms, {} chars)",
            if warmup.success { "ok" } else { "failed" },
            warmup.latency_ms,
            warmup.output.len()
        );
        if let Some(error) = warmup.error.as_deref() {
            tracing::debug!("Warm-up detail: {error}");
        }
        if warmup.success {
            let _ = reset_backend_session(&config.base_url).await;
        }
    }

    // ---- Step 4: Calibration ----
    let cal = run_calibration(&config.base_url).await;
    result.calibration = Some(cal.clone());
    tracing::info!(
        "Speed: {:.1} tok/s (calibration {} ms)",
        cal.decode_tok_per_sec, cal.total_duration_ms
    );

    // ---- Step 5: Health gate ----
    let health = check_health_gate(&cal);
    result.health_gate = Some(health.clone());
    tracing::info!(
        "Gate {} score {:.2}: {}",
        health.gate_name, health.score, health.reason
    );
    if !health.passed && !config.skip_health_gate {
        result.status = format!("blocked: {}", health.reason);
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }

    // ---- Step 6: Context check ----
    if let Err(e) = check_task_allowed(config.context_length, 0, &config.policy) {
        result.status = format!("blocked: {e}");
        result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
        return Ok(result);
    }

    // ---- Step 7: Run suites ----
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

    // Track lane gate results: lane_name → GateDecision
    let mut lane_gates: HashMap<&'static str, GateDecision> = HashMap::new();

    for suite in suites {
        let suite_name = suite.first().map(|t| t.suite).unwrap_or("unknown");
        let is_canary = suite_name == "canary";

        for task in suite {
            // ---- Gate skip check (for non-canary lane tasks) ----
            if !is_canary {
                if let Some(lane) = task.lane {
                    if let Some(gate) = lane_gates.get(lane) {
                        if !gate.passed {
                            tracing::warn!("  [SKIP] {} — gate '{}' failed: {}", task.key, lane, gate.reason);
                            result.tasks_skipped_gate += 1;
                            continue;
                        }
                    }
                }
            }

            // Context fit check
            if let Err(error) =
                check_task_allowed(config.context_length, task.min_context, &config.policy)
            {
                tracing::warn!("  Skip {} — policy: {error}", task.key);
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
                tracing::info!(
                    "  Skip {} — context: {}",
                    task.key,
                    fit.reason.as_deref().unwrap_or("exceeds available context")
                );
                continue;
            }

            // Determine attempt count for this task
            let attempts = if is_canary && task.lane.is_some() {
                gate_attempts
            } else {
                regular_attempts
            };

            // Compute timeout once (reserved for future per-task timeout enforcement)
            let _timeout = compute_timeout(TimeoutEstimate::new(
                1024,
                expected_budget,
                cal.prompt_tok_per_sec,
                cal.decode_tok_per_sec,
                cal.first_token_ms,
            ));

            let url = format!("{}/v1/completions", config.base_url.trim_end_matches('/'));

            // ---- Multi-attempt loop ----
            let mut attempt_results = Vec::with_capacity(attempts as usize);
            let mut total_latency_ms: u64 = 0;

            for attempt_idx in 0..attempts {
                let seed: i64 = 42 + attempt_idx as i64;
                let mut body = serde_json::json!({
                    "prompt": task.prompt,
                    "max_tokens": task.max_output_tokens,
                    "temperature": 0.0,
                    "seed": seed,
                });
                if config.no_thinking || task.no_thinking {
                    body["stop"] = serde_json::json!(["

", "
response"]);
                    body["presence_penalty"] = serde_json::json!(0.2);
                    body["frequency_penalty"] = serde_json::json!(0.1);
                }

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
                total_latency_ms += latency_ms;

                // Score this attempt
                let scored = scorers::score(task.scorer, &response, task.expected_answer.unwrap_or(""));
                attempt_results.push(scored);

                // Store artifact (last attempt only to save space)
                if attempt_idx == attempts.saturating_sub(1) {
                    if let Some(ref dirs) = run_dirs {
                        let _ = artifacts::write_prompt(dirs, task.key, task.prompt);
                        let _ = artifacts::write_response(dirs, task.key, &response);
                    }
                }
            }

            // ---- Aggregate results ----
            let multi = aggregate_multi(&attempt_results);
            let status = multi_to_status(&multi);
            result.tasks_run += 1;
            if multi.passed {
                result.tasks_passed += 1;
            }

            // ---- Gate check for canary lane tasks ----
            if is_canary {
                if let Some(lane) = task.lane {
                    if let Some(required) = promotion_threshold(lane) {
                        let decision = check_lane_gate_multi(lane, &multi.scores, required);
                        lane_gates.insert(lane, decision);
                    }
                }
            }

            // ---- Print result ----
            let mark = match (multi.passed, multi.stability) {
                (true, scorers::Stability::Clean) => "[PASS]",
                (true, _) => "[PASS*]",
                (false, scorers::Stability::Unstable) => "[FAIL*]",
                (false, _) => "[FAIL]",
            };
            let attempts_str = if attempts > 1 {
                format!(" {}/{})", multi.pass_count, multi.total_attempts)
            } else {
                String::new()
            };
            tracing::info!(
                "  {mark} {} ({:.1}s) avg={:.2}{attempts_str} status={} failure={} stability={} lane={}",
                task.key,
                total_latency_ms as f64 / 1000.0,
                multi.avg_score,
                status.as_str(),
                multi.failure.as_str(),
                multi.stability.label(),
                task.lane.unwrap_or("none"),
            );

            // Collect for persistence
            result.tasks.push(TaskResult {
                task_key: task.key.to_string(),
                suite: task.suite.to_string(),
                lane: task.lane.map(|s| s.to_string()),
                avg_score: multi.avg_score,
                pass_count: multi.pass_count,
                total_attempts: multi.total_attempts,
                passed: multi.passed,
                status: status.as_str().to_string(),
                failure: multi.failure.as_str().to_string(),
                stability: multi.stability.label().to_string(),
                latency_ms: total_latency_ms,
            });
        }
    }

    result.total_duration_ms = overall_start.elapsed().as_millis() as u64;
    result.status = "completed".into();
    tracing::info!(
        "Eval run complete ({sweep}): {passed}/{total} passed ({skipped} skipped by gate)",
        sweep = config.sweep_level.label(),
        passed = result.tasks_passed,
        total = result.tasks_run,
        skipped = result.tasks_skipped_gate,
    );

    // ---- Cleanup: kill managed server ----
    if config.manage_server {
        processes::clear_gpu_backends().await?;
    }

    // ---- Save results ----
    if let Some(path) = save_eval_results(config, &result) {
        tracing::info!("  Results saved: {}", path.display());
    }
    if let Some(path) = save_eval_csv(config, &result) {
        tracing::info!("  CSV saved: {}", path.display());
    }

    // Unified report (additive — existing paths unchanged)
    if !result.tasks.is_empty() {
        let model_name = &config.model_name;
        if let Ok(unified_dir) = crate::eval_result::write_unified_report_from_runner(
            model_name,
            &result.tasks,
        ) {
            tracing::info!("  Unified report: {}", unified_dir.display());
        }
    }

    Ok(result)
}

/// Save eval results as JSON to results/native/{model}_{timestamp}.json.
fn save_eval_results(config: &EvalRunConfig, result: &EvalRunResult) -> Option<std::path::PathBuf> {
    let root = std::path::Path::new("results/native");
    let _ = std::fs::create_dir_all(root);
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let filename = format!("{}_{ts}.json", config.model_name);
    let path = root.join(filename);

    let json = serde_json::json!({
        "model": config.model_name,
        "model_path": config.model_path,
        "sweep_level": config.sweep_level.label(),
        "context_length": config.context_length,
        "timestamp": ts.to_string(),
        "status": result.status,
        "warmup_passed": result.warmup_passed,
        "tasks_run": result.tasks_run,
        "tasks_passed": result.tasks_passed,
        "tasks_skipped_gate": result.tasks_skipped_gate,
        "total_duration_ms": result.total_duration_ms,
        "tasks": serde_json::to_value(&result.tasks).ok()?,
    });

    std::fs::write(&path, serde_json::to_string_pretty(&json).ok()?).ok()?;
    // Also write a stable "latest" copy
    let latest = root.join("latest.json");
    let _ = std::fs::copy(&path, &latest);
    Some(path)
}

/// Save eval results as CSV to results/native/{model}_{timestamp}.csv.
fn save_eval_csv(config: &EvalRunConfig, result: &EvalRunResult) -> Option<std::path::PathBuf> {
    let root = std::path::Path::new("results/native");
    let _ = std::fs::create_dir_all(root);
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let filename = format!("{}_{ts}.csv", config.model_name);
    let path = root.join(filename);

    let mut w = csv::Writer::from_path(path.clone()).ok()?;
    let _ = w.write_record([
        "task_key", "suite", "lane", "avg_score", "pass_count",
        "total_attempts", "passed", "status", "failure", "stability", "latency_ms",
    ]);
    for t in &result.tasks {
        let _ = w.write_record([
            &t.task_key,
            &t.suite,
            t.lane.as_deref().unwrap_or(""),
            &format!("{:.3}", t.avg_score),
            &t.pass_count.to_string(),
            &t.total_attempts.to_string(),
            &t.passed.to_string(),
            &t.status,
            &t.failure,
            &t.stability,
            &t.latency_ms.to_string(),
        ]);
    }
    let _ = w.flush();
    // Also write a stable "latest" copy
    let latest = root.join("latest.csv");
    let _ = std::fs::copy(&path, &latest);
    Some(path)
}

/// Run the eval pipeline with TUI event emission (multi-attempt).
///
/// Same as `run_eval` but sends progress events through an unbounded channel
/// for real-time TUI display. The sender is consumed by the function.
pub async fn run_eval_with_events(
    config: &EvalRunConfig,
    tx: tokio::sync::mpsc::UnboundedSender<crate::ui::eval_run_workflow::EvalRunEvent>,
) -> Result<EvalRunResult> {
    let overall_start = std::time::Instant::now();
    let gate_attempts = config.effective_gate_attempts();
    let regular_attempts = config.effective_regular_attempts();

    let mut result = EvalRunResult {
        status: "running".into(),
        warmup_passed: false,
        calibration: None,
        health_gate: None,
        tasks_run: 0,
        tasks_passed: 0,
        tasks_skipped_gate: 0,
        total_duration_ms: 0,
        tasks: Vec::new(),
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

    // ---- Step 2: Server launch (when managed) ----
    if config.manage_server {
        let server_path = config
            .server_path
            .as_deref()
            .context("server_path is required when manage_server is true")?;
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Server".into(),
            detail: "Clearing GPU backends...".into(),
        });
        processes::clear_gpu_backends().await?;
        // Wait for GPU memory to fully release before re-launching
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Server".into(),
            detail: format!("Launching {} with ctx={}...", server_path.display(), config.context_length),
        });
        processes::start_llamacpp(
            server_path,
            &config.model_path,
            &build_eval_server_args(config),
        )
        .await
        .context("Failed to launch llama.cpp server for eval")?;
        let _loaded = processes::get_llamacpp_model()
            .await
            .ok_or_else(|| anyhow::anyhow!("Server launched but model not available"))?;
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Server".into(),
            detail: format!("Model loaded: {_loaded}"),
        });
    }

    // ---- Step 3: Warm-up ----
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

    // Track lane gate results
    let mut lane_gates: HashMap<&'static str, GateDecision> = HashMap::new();

    for suite in suites {
        let suite_name = suite.first().map(|t| t.suite).unwrap_or("unknown");
        let is_canary = suite_name == "canary";
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: format!("Suite: {}", suite_name),
            detail: format!("Running {} tasks...", suite.len()),
        });

        for task in suite {
            // ---- Gate skip check (for non-canary lane tasks) ----
            if !is_canary {
                if let Some(lane) = task.lane {
                    if let Some(gate) = lane_gates.get(lane) {
                        if !gate.passed {
                            let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::TaskSkipped {
                                task_key: task.key.into(),
                                reason: format!("gate '{}' failed: {}", lane, gate.reason),
                            });
                            result.tasks_skipped_gate += 1;
                            continue;
                        }
                    }
                }
            }

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

            // Determine attempt count
            let attempts = if is_canary && task.lane.is_some() {
                gate_attempts
            } else {
                regular_attempts
            };

            let url = format!("{}/v1/completions", config.base_url.trim_end_matches('/'));

            // ---- Multi-attempt loop ----
            let mut attempt_results = Vec::with_capacity(attempts as usize);
            let mut total_latency_ms: u64 = 0;

            for attempt_idx in 0..attempts {
                let seed: i64 = 42 + attempt_idx as i64;
                let mut body = serde_json::json!({
                    "prompt": task.prompt,
                    "max_tokens": task.max_output_tokens,
                    "temperature": 0.0,
                    "seed": seed,
                });
                if config.no_thinking || task.no_thinking {
                    body["stop"] = serde_json::json!(["

", "
response"]);
                    body["presence_penalty"] = serde_json::json!(0.2);
                    body["frequency_penalty"] = serde_json::json!(0.1);
                }

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
                total_latency_ms += latency_ms;

                let scored = scorers::score(task.scorer, &response, task.expected_answer.unwrap_or(""));
                attempt_results.push(scored);
            }

            // ---- Aggregate results ----
            let multi = aggregate_multi(&attempt_results);
            result.tasks_run += 1;
            if multi.passed {
                result.tasks_passed += 1;
            }

            // ---- Gate check for canary lane tasks ----
            if is_canary {
                if let Some(lane) = task.lane {
                    if let Some(required) = promotion_threshold(lane) {
                        let decision = check_lane_gate_multi(lane, &multi.scores, required);
                        lane_gates.insert(lane, decision);
                    }
                }
            }

            // Emit event with multi-attempt detail
            let detail = if attempts > 1 {
                format!("{}/{} {}", multi.pass_count, multi.total_attempts, multi.detail)
            } else {
                multi.detail.clone()
            };
            let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::TaskResult {
                task_key: task.key.into(),
                passed: multi.passed,
                score: multi.avg_score,
                detail,
                latency_ms: total_latency_ms,
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

    // ---- Cleanup: kill managed server ----
    if config.manage_server {
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Cleanup".into(),
            detail: "Shutting down server...".into(),
        });
        processes::clear_gpu_backends().await?;
    }

    // ---- Save results ----
    if let Some(path) = save_eval_results(config, &result) {
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "Results".into(),
            detail: format!("Saved: {}", path.display()),
        });
    }
    if let Some(path) = save_eval_csv(config, &result) {
        let _ = tx.send(crate::ui::eval_run_workflow::EvalRunEvent::Stage {
            name: "CSV".into(),
            detail: format!("Saved: {}", path.display()),
        });
    }

    Ok(result)
}
