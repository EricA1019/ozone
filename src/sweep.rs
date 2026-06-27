use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::bench;
use crate::planner;

/// Generate context steps for a sweep, from 2K up to the model's native max.
/// Fine-grained at low contexts, geometric stepping at high contexts.
pub fn generate_context_steps(native_max: u32) -> Vec<u32> {
    // Low: fine-grained doubling
    let low: &[u32] = &[2048, 4096, 8192, 16384];

    // Mid: ×1.5 steps from 16384 to 65536
    let mut mid = Vec::new();
    let mut current = 16384u32;
    while current < 65536 && current < native_max {
        current = ((current as f64 * 1.5) / 1024.0).round() as u32 * 1024;
        current = current.min(65536);
        if current > 16384 {
            mid.push(current);
        }
    }

    // High: ×1.25 steps from 65536 to native_max
    let mut high = Vec::new();
    let mut current = 65536u32;
    while current < native_max {
        current = ((current as f64 * 1.25) / 1024.0).round() as u32 * 1024;
        current = current.min(native_max);
        if current > 65536 {
            high.push(current);
        }
    }

    // Combine, deduplicate, ensure max is included
    let mut steps: Vec<u32> = low.to_vec();
    steps.extend(mid);
    steps.extend(high);

    if steps.last().copied().unwrap_or(0) < native_max {
        steps.push(native_max);
    }

    let mut seen = std::collections::HashSet::new();
    steps.retain(|c| seen.insert(*c));

    steps
}

pub struct SweepConfig {
    pub model_name: String,
    pub model_path: PathBuf,
    pub backend: crate::bench::BenchBackend,
    pub model_size_gb: f64,
    pub total_layers: u32,
    pub context_sizes: Vec<u32>,
    pub quant_kv_levels: Vec<(u8, u8)>, // (quant_k, quant_v) pairs
    pub gpu_vram_budget_mb: u32,
}

pub struct SweepResult {
    pub configs_tested: u32,
    pub configs_skipped: u32,
    pub configs_failed: u32,
    pub best_speed: Option<bench::BenchResult>,
    pub best_context: Option<bench::BenchResult>,
    pub pareto_frontier: Vec<ParetoPoint>,
    /// Path to the CSV file with all tested configs.
    pub csv_path: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct SweepCsvRow {
    model: String,
    context_size: u32,
    gpu_layers: i32,
    quant_k: u8,
    quant_v: u8,
    tokens_per_sec: f64,
    ttft_ms: u32,
    vram_peak_mb: u32,
    ram_peak_mb: u32,
    total_tokens: u32,
    total_time_ms: u32,
    status: String,
    error_detail: Option<String>,
    timestamp: String,
}

#[derive(Debug, Clone)]
pub struct SweepProgress {
    pub current: u32,
    pub total: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ParetoPoint {
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub tokens_per_sec: f64,
    pub vram_peak_mb: u32,
}

/// Pick the optimal profile from sweep results: the highest context that stays
/// stable at ≥10 tok/s. Falls back to the fastest config if nothing meets the bar.
pub fn pick_optimal_profile(
    _model_name: &str,
    frontier: &[ParetoPoint],
    threads: Option<u32>,
) -> Option<crate::prefs::SavedLaunchProfile> {
    const MIN_TOK_S: f64 = 10.0;

    // Filter to configs meeting the stability threshold
    let stable: Vec<&ParetoPoint> = frontier
        .iter()
        .filter(|p| p.tokens_per_sec >= MIN_TOK_S)
        .collect();

    if stable.is_empty() {
        return None;
    }

    // Pick the one with the highest context; tie-break on speed
    let best = stable.iter().max_by(|a, b| {
        a.context_size.cmp(&b.context_size).then_with(|| {
            a.tokens_per_sec
                .partial_cmp(&b.tokens_per_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    })?;

    Some(crate::prefs::SavedLaunchProfile {
        profile_name: "auto-optimal".into(),
        context_size: best.context_size,
        gpu_layers: best.gpu_layers,
        quant_k: best.quant_k,
        quant_v: best.quant_v,
        threads,
    })
}

/// Check if a candidate point is dominated by any existing Pareto point.
/// A point is dominated if some existing point is >= on BOTH speed AND context.
fn is_dominated(frontier: &[ParetoPoint], candidate: &ParetoPoint) -> bool {
    frontier.iter().any(|p| {
        p.tokens_per_sec >= candidate.tokens_per_sec
            && p.context_size >= candidate.context_size
            && (p.tokens_per_sec > candidate.tokens_per_sec
                || p.context_size > candidate.context_size)
    })
}

/// Remove points from frontier that are dominated by a new candidate.
fn prune_dominated(frontier: &mut Vec<ParetoPoint>, candidate: &ParetoPoint) {
    frontier.retain(|p| {
        !(candidate.tokens_per_sec >= p.tokens_per_sec
            && candidate.context_size >= p.context_size
            && (candidate.tokens_per_sec > p.tokens_per_sec
                || candidate.context_size > p.context_size))
    });
}

pub async fn run_sweep(config: SweepConfig) -> Result<SweepResult> {
    run_sweep_with_progress(config, |progress| {
        println!("  {}", progress.message);
    })
    .await
}

pub async fn run_sweep_with_progress<F>(
    config: SweepConfig,
    mut on_progress: F,
) -> Result<SweepResult>
where
    F: FnMut(SweepProgress),
{
    let total_layers = config.total_layers;
    let total_combos = config.context_sizes.len() * config.quant_kv_levels.len();

    on_progress(SweepProgress {
        current: 0,
        total: total_combos as u32,
        message: format!("⬡ Ozone Sweep — {}", config.model_name),
    });
    on_progress(SweepProgress {
        current: 0,
        total: total_combos as u32,
        message: format!(
            "VRAM budget: {} MB | Model: {:.1} GB | Layers: {}",
            config.gpu_vram_budget_mb, config.model_size_gb, total_layers,
        ),
    });

    // Set up CSV output for all tested configs
    let csv_path = ozone_core::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!(
            "sweep_{}_{}.csv",
            config.model_name,
            chrono::Utc::now().format("%Y%m%dT%H%M%S"),
        ));
    let mut csv_writer = csv::Writer::from_path(&csv_path)?;
    csv_writer.write_record([
        "model",
        "context_size",
        "gpu_layers",
        "quant_k",
        "quant_v",
        "tokens_per_sec",
        "ttft_ms",
        "vram_peak_mb",
        "ram_peak_mb",
        "total_tokens",
        "total_time_ms",
        "status",
        "error_detail",
        "timestamp",
    ])?;

    let mut result = SweepResult {
        configs_tested: 0,
        configs_skipped: 0,
        configs_failed: 0,
        best_speed: None,
        best_context: None,
        pareto_frontier: Vec::new(),
        csv_path: None,
    };

    let mut step = 0u32;

    for &ctx in &config.context_sizes {
        // Early stopping: track whether the smallest quant_kv already OOMs at this context
        let mut all_oom_at_ctx = true;

        for &(qk, qv) in &config.quant_kv_levels {
            step += 1;

            // Binary search for max layers that fit VRAM budget
            let max_layers = planner::fit_gpu_layers_to_budget(
                ctx,
                config.model_size_gb,
                qk,
                qv,
                total_layers,
                config.gpu_vram_budget_mb,
            );

            let layers = match max_layers {
                Some(l) => l,
                None => {
                    // Even 0 layers exceeds budget — skip
                    on_progress(SweepProgress {
                        current: step,
                        total: total_combos as u32,
                        message: format!(
                            "[{}/{}] ctx={} K=q{qk}/V=q{qv} ... skipped (exceeds VRAM budget)",
                            step, total_combos, ctx,
                        ),
                    });
                    result.configs_skipped += 1;
                    continue;
                }
            };

            all_oom_at_ctx = false;

            // Check if this config would be dominated by an existing Pareto point
            // before spending time benchmarking
            // Only skip if context is dominated and layers are strictly fewer
            let dominated_hint = result
                .pareto_frontier
                .iter()
                .any(|p| p.context_size >= ctx && p.gpu_layers >= layers && p.context_size > ctx);
            if dominated_hint {
                on_progress(SweepProgress {
                    current: step,
                    total: total_combos as u32,
                    message: format!(
                        "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... skipped (dominated)",
                        step, total_combos, ctx, layers,
                    ),
                });
                result.configs_skipped += 1;
                continue;
            }

            on_progress(SweepProgress {
                current: step,
                total: total_combos as u32,
                message: format!(
                    "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... running",
                    step, total_combos, ctx, layers,
                ),
            });

            let bench_result = match bench::run_benchmark_with_progress(
                &config.model_name,
                &config.model_path,
                &config.backend.clone(),
                layers,
                ctx,
                qk,
                qv,
                None,
                bench::BenchMode::Sweep,
                |_| {},
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    let detail = e.to_string();
                    let is_timeout = detail.to_lowercase().contains("timeout");
                    on_progress(SweepProgress {
                        current: step,
                        total: total_combos as u32,
                        message: format!(
                            "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {} ✗ — {detail}",
                            step,
                            total_combos,
                            ctx,
                            layers,
                            if is_timeout {
                                "timeout"
                            } else {
                                "launch-error"
                            },
                        ),
                    });
                    result.configs_failed += 1;
                    // For timeout on first config (cold cache), keep going — subsequent loads will be faster
                    if is_timeout && step <= 2 {
                        on_progress(SweepProgress {
                            current: step,
                            total: total_combos as u32,
                            message: "→ First load timeout (cold cache) — continuing, subsequent loads will be page-cached".into(),
                        });
                    }
                    continue;
                }
            };

            if bench_result.status != "ok" {
                // Retry with fewer layers on OOM/timeout
                if (bench_result.status == "oom" || bench_result.status == "timeout") && layers > 0
                {
                    let retry_layers = (layers - 1).max(0);
                    on_progress(SweepProgress {
                        current: step,
                        total: total_combos as u32,
                        message: format!(
                            "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {} — retrying with {} layers",
                            step, total_combos, ctx, layers, bench_result.status, retry_layers,
                        ),
                    });

                    let retry = match bench::run_benchmark_with_progress(
                        &config.model_name,
                        &config.model_path,
                        &config.backend.clone(),
                        retry_layers,
                        ctx,
                        qk,
                        qv,
                        None,
                        bench::BenchMode::Sweep,
                        |_| {},
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            on_progress(SweepProgress {
                                current: step,
                                total: total_combos as u32,
                                message: format!(
                                    "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {} ✗ — {}",
                                    step, total_combos, ctx, layers, bench_result.status, e,
                                ),
                            });
                            result.configs_failed += 1;
                            continue;
                        }
                    };

                    if retry.status == "ok" {
                        on_progress(SweepProgress {
                            current: step,
                            total: total_combos as u32,
                            message: format!(
                                "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {:.1} t/s ✓",
                                step, total_combos, ctx, retry_layers, retry.tokens_per_sec,
                            ),
                        });
                        result.configs_tested += 1;
                        update_bests(&mut result, &retry, ctx);
                        maybe_add_pareto(
                            &mut result.pareto_frontier,
                            retry_layers,
                            ctx,
                            qk,
                            qv,
                            &retry,
                        );
                        store_quietly(&config, retry_layers, ctx, qk, qv, &retry);
                        write_csv_row(&mut csv_writer, &config, retry_layers, ctx, qk, qv, &retry);
                        continue;
                    }
                }
                on_progress(SweepProgress {
                    current: step,
                    total: total_combos as u32,
                    message: format!(
                        "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {} ✗{}",
                        step,
                        total_combos,
                        ctx,
                        layers,
                        bench_result.status,
                        bench_result
                            .error_detail
                            .as_deref()
                            .map(|d| format!(" — {d}"))
                            .unwrap_or_default(),
                    ),
                });
                result.configs_failed += 1;
                continue;
            }

            on_progress(SweepProgress {
                current: step,
                total: total_combos as u32,
                message: format!(
                    "[{}/{}] ctx={} K=q{qk}/V=q{qv} layers={} ... {:.1} t/s ✓",
                    step, total_combos, ctx, layers, bench_result.tokens_per_sec,
                ),
            });
            result.configs_tested += 1;
            update_bests(&mut result, &bench_result, ctx);
            maybe_add_pareto(
                &mut result.pareto_frontier,
                layers,
                ctx,
                qk,
                qv,
                &bench_result,
            );
            store_quietly(&config, layers, ctx, qk, qv, &bench_result);
            write_csv_row(&mut csv_writer, &config, layers, ctx, qk, qv, &bench_result);
        }

        // Early stopping: if all quant_kv levels OOMed at this context, skip larger contexts
        if all_oom_at_ctx {
            let remaining = config.context_sizes.iter().filter(|&&c| c > ctx).count();
            if remaining > 0 {
                let skip_count = remaining * config.quant_kv_levels.len();
                on_progress(SweepProgress {
                    current: step,
                    total: total_combos as u32,
                    message: format!(
                        "→ ctx={} exhausted VRAM budget — skipping {} larger configs",
                        ctx, skip_count,
                    ),
                });
                result.configs_skipped += skip_count as u32;
            }
            break;
        }
    }

    // Sort Pareto frontier by context size ascending
    result.pareto_frontier.sort_by_key(|p| p.context_size);

    on_progress(SweepProgress {
        current: total_combos as u32,
        total: total_combos as u32,
        message: format!(
            "⬡ Sweep Complete — {} tested, {} skipped, {} failed",
            result.configs_tested, result.configs_skipped, result.configs_failed,
        ),
    });

    if result.configs_tested == 0 {
        on_progress(SweepProgress {
            current: total_combos as u32,
            total: total_combos as u32,
            message: "⚠ No configs were successfully tested.".into(),
        });
        let _ = csv_writer.flush();
        result.csv_path = Some(csv_path);
        return Ok(result);
    }

    if !result.pareto_frontier.is_empty() {
        on_progress(SweepProgress {
            current: total_combos as u32,
            total: total_combos as u32,
            message: "Pareto Frontier (speed vs context):".into(),
        });
        for p in &result.pareto_frontier {
            on_progress(SweepProgress {
                current: total_combos as u32,
                total: total_combos as u32,
                message: format!(
                    "ctx={} layers={} K=q{} V=q{} {:.1} t/s {} MB",
                    p.context_size,
                    p.gpu_layers,
                    p.quant_k,
                    p.quant_v,
                    p.tokens_per_sec,
                    p.vram_peak_mb,
                ),
            });
        }
    }
    let _ = csv_writer.flush();
    result.csv_path = Some(csv_path);
    Ok(result)
}

fn update_bests(result: &mut SweepResult, bench: &bench::BenchResult, context_size: u32) {
    // Best speed
    if result
        .best_speed
        .as_ref()
        .is_none_or(|b| bench.tokens_per_sec > b.tokens_per_sec)
    {
        result.best_speed = Some(bench.clone());
    }
    // Best context — largest context with ok status
    if result.best_context.as_ref().is_none_or(|_| {
        result
            .best_context
            .as_ref()
            .is_none_or(|_b| context_size > 0)
    }) {
        // We track best_context simply as the result at the largest working context
        let dominated = result.best_context.as_ref().is_some_and(|b| {
            b.total_tokens > 0
                && bench.tokens_per_sec < b.tokens_per_sec
                && context_size <= b.vram_peak_mb.max(context_size)
        });
        if !dominated {
            result.best_context = Some(bench.clone());
        }
    }
}

fn maybe_add_pareto(
    frontier: &mut Vec<ParetoPoint>,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    bench: &bench::BenchResult,
) {
    let candidate = ParetoPoint {
        gpu_layers,
        context_size,
        quant_k,
        quant_v,
        tokens_per_sec: bench.tokens_per_sec,
        vram_peak_mb: bench.vram_peak_mb,
    };
    if !is_dominated(frontier, &candidate) {
        prune_dominated(frontier, &candidate);
        frontier.push(candidate);
    }
}

fn write_csv_row(
    writer: &mut csv::Writer<std::fs::File>,
    config: &SweepConfig,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    bench: &bench::BenchResult,
) {
    let row = SweepCsvRow {
        model: config.model_name.clone(),
        context_size,
        gpu_layers,
        quant_k,
        quant_v,
        tokens_per_sec: bench.tokens_per_sec,
        ttft_ms: bench.time_to_first_token_ms,
        vram_peak_mb: bench.vram_peak_mb,
        ram_peak_mb: bench.ram_peak_mb,
        total_tokens: bench.total_tokens,
        total_time_ms: bench.total_time_ms,
        status: bench.status.clone(),
        error_detail: bench.error_detail.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    let _ = writer.serialize(row);
    let _ = writer.flush();
}

fn store_quietly(
    config: &SweepConfig,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    bench: &bench::BenchResult,
) {
    match bench::store_result(
        bench::BenchmarkStoreRequest {
            model_name: &config.model_name,
            model_size_gb: config.model_size_gb,
            gpu_layers,
            context_size,
            quant_k: quant_k as u32,
            quant_v: quant_v as u32,
            threads: 0,
            launch_profile_name: None,
        },
        bench,
    ) {
        Ok(_) => {}
        Err(e) => eprintln!("  Warning: failed to store result: {e}"),
    }
}

/// Context sizes to test in a full sweep (filtered against model max).
pub const SWEEP_CONTEXT_STEPS: &[u32] = &[
    512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144,
];

#[derive(Debug, Clone, Copy)]
pub struct ContextSweepRequest<'a> {
    pub model_name: &'a str,
    pub model_path: &'a Path,
    pub server_path: &'a Path,
    pub gpu_layers: i32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub threads: Option<u32>,
    pub quick: bool,
}

/// Run a context sweep: test each context size, stop at OOM.
/// Returns CSV path and sweet-spot context size.
pub async fn run_context_sweep(request: ContextSweepRequest<'_>) -> Result<(PathBuf, u32)> {
    let ContextSweepRequest {
        model_name,
        model_path,
        server_path,
        gpu_layers,
        quant_k,
        quant_v,
        threads,
        quick,
    } = request;
    let max_context = crate::gguf::read_context_length(model_path).unwrap_or_else(|| {
        eprintln!("  ⚠ Could not read context length from GGUF metadata; defaulting to 131072");
        131072
    });

    let steps: Vec<u32> = SWEEP_CONTEXT_STEPS
        .iter()
        .copied()
        .filter(|s| *s <= max_context)
        .collect();

    let steps = if quick {
        steps.iter().step_by(2).copied().collect()
    } else {
        steps
    };

    if steps.is_empty() {
        anyhow::bail!("No valid context steps for model (max_context={max_context})");
    }

    let csv_path = ozone_core::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!(
            "sweep_{model_name}_{}.csv",
            chrono::Utc::now().format("%Y%m%dT%H%M%S")
        ));

    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record([
        "model",
        "context_size",
        "tok_s",
        "ttft_ms",
        "vram_mb",
        "ram_mb",
        "status",
    ])?;

    let mut sweet_spot = 0u32;
    let mut best_tok_s = 0.0f64;
    let speed_threshold = 10.0f64;

    for &ctx in &steps {
        eprintln!("  Testing context={ctx}...");
        let backend = crate::bench::BenchBackend::LlamaCpp {
            server_path: server_path.to_path_buf(),
        };
        let result = bench::run_benchmark(bench::BenchmarkRunRequest {
            model_name,
            model_path,
            backend: &backend,
            gpu_layers,
            context_size: ctx,
            quant_k,
            quant_v,
            threads,
            mode: bench::BenchMode::Sweep,
        })
        .await;

        match result {
            Ok(r) => {
                if r.status == "ok" && r.tokens_per_sec >= speed_threshold && ctx > sweet_spot {
                    sweet_spot = ctx;
                    best_tok_s = r.tokens_per_sec;
                }
                writer.write_record([
                    model_name,
                    &ctx.to_string(),
                    &r.tokens_per_sec.to_string(),
                    &r.time_to_first_token_ms.to_string(),
                    &r.vram_peak_mb.to_string(),
                    &r.ram_peak_mb.to_string(),
                    &r.status,
                ])?;
                if r.status == "oom" {
                    eprintln!("  OOM at context={ctx}, stopping sweep.");
                    break;
                }
                if r.status == "garbage" {
                    eprintln!("  Garbage output at context={ctx} — noting but continuing sweep (may be prompt-specific).");
                    // Don't stop — garbage detection can have false positives
                }
            }
            Err(e) => {
                writer.write_record([
                    model_name,
                    &ctx.to_string(),
                    "0",
                    "0",
                    "0",
                    "0",
                    "launch_failed",
                ])?;
                eprintln!("  Failed at context={ctx}: {e}");
                break;
            }
        }
    }

    writer.flush()?;
    eprintln!("  Sweet spot: context={sweet_spot} ({best_tok_s:.1} tok/s)");
    eprintln!("  CSV: {}", csv_path.display());

    Ok((csv_path, sweet_spot))
}
