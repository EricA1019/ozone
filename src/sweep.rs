use std::path::PathBuf;

use anyhow::Result;

use crate::bench;
use crate::planner;

pub struct SweepConfig {
    pub model_name: String,
    pub model_path: PathBuf,
    pub launcher_path: PathBuf,
    pub model_size_gb: f64,
    pub context_sizes: Vec<u32>,
    pub quant_kv_levels: Vec<u8>,
    pub gpu_vram_budget_mb: u32,
    #[allow(dead_code)]
    pub ram_total_mb: u32,
}

pub struct SweepResult {
    pub configs_tested: u32,
    pub configs_skipped: u32,
    pub configs_failed: u32,
    pub best_speed: Option<bench::BenchResult>,
    pub best_context: Option<bench::BenchResult>,
    pub pareto_frontier: Vec<ParetoPoint>,
}

#[derive(Debug, Clone)]
pub struct ParetoPoint {
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_kv: u8,
    pub tokens_per_sec: f64,
    pub vram_peak_mb: u32,
}

/// Find the maximum gpu_layers that fits within the VRAM budget via binary search.
fn find_max_layers(
    context_size: u32,
    size_gb: f64,
    quant_kv: u8,
    total_layers: u32,
    budget_mb: u32,
) -> Option<i32> {
    let mut lo: i32 = 0;
    let mut hi: i32 = total_layers as i32;
    let mut best: Option<i32> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let est = planner::estimate_vram_mb(context_size, mid, size_gb, quant_kv, total_layers);
        if est <= budget_mb {
            best = Some(mid);
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    best
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
    let total_layers = planner::estimate_total_layers(config.model_size_gb);
    let total_combos = config.context_sizes.len() * config.quant_kv_levels.len();

    println!();
    println!("  ⬡ Ozone Sweep — {}", config.model_name);
    println!("  ─────────────────────────────────────────────────");
    println!("  VRAM budget: {} MB | Model: {:.1} GB | Layers: {}",
        config.gpu_vram_budget_mb, config.model_size_gb, total_layers);
    println!();

    let mut result = SweepResult {
        configs_tested: 0,
        configs_skipped: 0,
        configs_failed: 0,
        best_speed: None,
        best_context: None,
        pareto_frontier: Vec::new(),
    };

    let mut step = 0u32;

    for &ctx in &config.context_sizes {
        // Early stopping: track whether the smallest quant_kv already OOMs at this context
        let mut all_oom_at_ctx = true;

        for &qkv in &config.quant_kv_levels {
            step += 1;

            // Binary search for max layers that fit VRAM budget
            let max_layers = find_max_layers(
                ctx,
                config.model_size_gb,
                qkv,
                total_layers,
                config.gpu_vram_budget_mb,
            );

            let layers = match max_layers {
                Some(l) => l,
                None => {
                    // Even 0 layers exceeds budget — skip
                    println!(
                        "  [{}/{}] ctx={} qkv={} ... skipped (exceeds VRAM budget)",
                        step, total_combos, ctx, qkv,
                    );
                    result.configs_skipped += 1;
                    continue;
                }
            };

            all_oom_at_ctx = false;

            // Check if this config would be dominated by an existing Pareto point
            // before spending time benchmarking
            // Only skip if context is dominated and layers are strictly fewer
            let dominated_hint = result.pareto_frontier.iter().any(|p| {
                p.context_size >= ctx && p.gpu_layers >= layers && p.context_size > ctx
            });
            if dominated_hint {
                println!(
                    "  [{}/{}] ctx={} qkv={} layers={} ... skipped (dominated)",
                    step, total_combos, ctx, qkv, layers,
                );
                result.configs_skipped += 1;
                continue;
            }

            // Run the actual benchmark
            print!(
                "  [{}/{}] ctx={} qkv={} layers={} ... ",
                step, total_combos, ctx, qkv, layers,
            );

            let bench_result = bench::run_benchmark(
                &config.model_name,
                &config.model_path,
                &config.launcher_path,
                layers,
                ctx,
                qkv,
                None,
            )
            .await?;

            if bench_result.status != "ok" {
                // Retry with fewer layers on OOM/timeout
                if (bench_result.status == "oom" || bench_result.status == "timeout") && layers > 0 {
                    let retry_layers = (layers - 1).max(0);
                    print!("{} — retrying with {} layers ... ", bench_result.status, retry_layers);

                    let retry = bench::run_benchmark(
                        &config.model_name,
                        &config.model_path,
                        &config.launcher_path,
                        retry_layers,
                        ctx,
                        qkv,
                        None,
                    )
                    .await?;

                    if retry.status == "ok" {
                        println!("{:.1} t/s ✓", retry.tokens_per_sec);
                        result.configs_tested += 1;
                        update_bests(&mut result, &retry, ctx);
                        maybe_add_pareto(&mut result.pareto_frontier, retry_layers, ctx, qkv, &retry);
                        store_quietly(&config, retry_layers, ctx, qkv, &retry);
                        continue;
                    }
                }
                println!("{} ✗", bench_result.status);
                result.configs_failed += 1;
                continue;
            }

            println!("{:.1} t/s ✓", bench_result.tokens_per_sec);
            result.configs_tested += 1;
            update_bests(&mut result, &bench_result, ctx);
            maybe_add_pareto(&mut result.pareto_frontier, layers, ctx, qkv, &bench_result);
            store_quietly(&config, layers, ctx, qkv, &bench_result);
        }

        // Early stopping: if all quant_kv levels OOMed at this context, skip larger contexts
        if all_oom_at_ctx {
            let remaining = config.context_sizes.iter().filter(|&&c| c > ctx).count();
            if remaining > 0 {
                let skip_count = remaining * config.quant_kv_levels.len();
                println!(
                    "  → ctx={} exhausted VRAM budget — skipping {} larger configs",
                    ctx, skip_count,
                );
                result.configs_skipped += skip_count as u32;
            }
            break;
        }
    }

    // Sort Pareto frontier by context size ascending
    result.pareto_frontier.sort_by_key(|p| p.context_size);

    // Print summary
    println!();
    println!(
        "  ⬡ Sweep Complete — {} tested, {} skipped, {} failed",
        result.configs_tested, result.configs_skipped, result.configs_failed,
    );

    if !result.pareto_frontier.is_empty() {
        println!();
        println!("  Pareto Frontier (speed vs context):");
        println!("  ┌──────────┬────────┬─────┬──────────┬──────────┐");
        println!("  │ Context  │ Layers │ QKV │ Tokens/s │ VRAM MB  │");
        println!("  ├──────────┼────────┼─────┼──────────┼──────────┤");
        for p in &result.pareto_frontier {
            println!(
                "  │ {:<8} │ {:<6} │ {:<3} │ {:<8.1} │ {:<8} │",
                p.context_size, p.gpu_layers, p.quant_kv, p.tokens_per_sec, p.vram_peak_mb,
            );
        }
        println!("  └──────────┴────────┴─────┴──────────┴──────────┘");
    }

    println!();
    Ok(result)
}

fn update_bests(result: &mut SweepResult, bench: &bench::BenchResult, context_size: u32) {
    // Best speed
    if result
        .best_speed
        .as_ref()
        .map_or(true, |b| bench.tokens_per_sec > b.tokens_per_sec)
    {
        result.best_speed = Some(bench.clone());
    }
    // Best context — largest context with ok status
    if result.best_context.as_ref().map_or(true, |_| {
        result
            .best_context
            .as_ref()
            .map_or(true, |_b| context_size > 0)
    }) {
        // We track best_context simply as the result at the largest working context
        let dominated = result.best_context.as_ref().is_some_and(|b| {
            b.total_tokens > 0 && bench.tokens_per_sec < b.tokens_per_sec && context_size <= b.vram_peak_mb.max(context_size)
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
    quant_kv: u8,
    bench: &bench::BenchResult,
) {
    let candidate = ParetoPoint {
        gpu_layers,
        context_size,
        quant_kv,
        tokens_per_sec: bench.tokens_per_sec,
        vram_peak_mb: bench.vram_peak_mb,
    };
    if !is_dominated(frontier, &candidate) {
        prune_dominated(frontier, &candidate);
        frontier.push(candidate);
    }
}

fn store_quietly(config: &SweepConfig, gpu_layers: i32, context_size: u32, quant_kv: u8, bench: &bench::BenchResult) {
    match bench::store_result(
        &config.model_name,
        config.model_size_gb,
        gpu_layers,
        context_size,
        quant_kv as u32,
        0,
        bench,
    ) {
        Ok(_) => {}
        Err(e) => eprintln!("  Warning: failed to store result: {e}"),
    }
}
