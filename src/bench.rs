use crate::db::{self, BenchmarkRow};
use crate::hardware;
use crate::processes;
use anyhow::{anyhow, Result};
use ozone_core::paths;
use std::time::{Duration, Instant};

/// Which inference backend to run the benchmark against.
#[derive(Debug, Clone)]
pub enum BenchBackend {
    LlamaCpp { server_path: std::path::PathBuf },
}

impl BenchBackend {
    pub fn display_name(&self) -> &'static str {
        "llama.cpp"
    }
}

/// Fixed benchmark prompt — long enough to test throughput, short enough to be fast.
/// Roughly 200 input tokens, requests 100 output tokens.
const BENCH_PROMPT: &str = "\
You are a knowledgeable assistant. Explain the following concept in detail, \
providing examples and practical applications:\n\n\
The relationship between computational complexity theory and real-world \
software engineering. Cover topics including Big O notation, amortized \
analysis, space-time tradeoffs, NP-completeness, and how these theoretical \
concepts influence everyday programming decisions such as algorithm selection, \
data structure choice, and system design. Include at least three concrete \
examples where understanding complexity theory led to measurably better \
software. Discuss both the benefits and limitations of theoretical analysis \
when applied to production systems with real hardware constraints, caching \
effects, and concurrent workloads.";

const BENCH_MAX_TOKENS: u32 = 100;
const API_TIMEOUT_SECS: u64 = 600;

/// Read the CPU scaling governor. Returns None if unavailable (non-Linux).
fn read_cpu_governor() -> Option<String> {
    let path = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor";
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

#[derive(Debug, Clone)]
pub struct BenchProgress {
    pub message: String,
}

/// Controls the precision/speed trade-off for benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchMode {
    /// Full precision: warm-up run + dual measured runs + 3s settle.
    /// Best for single-benchmark validation where absolute accuracy matters.
    Precise,
    /// Fast sweep mode: single measured run, minimal settle, no warm-up.
    /// Model load (20-30s GPU work) provides enough thermal stabilisation.
    /// Best for sweeps where relative comparison across configs is what matters.
    Sweep,
}

/// Result of a single benchmark run.
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub tokens_per_sec: f64,
    pub time_to_first_token_ms: u32,
    pub vram_peak_mb: u32,
    pub ram_peak_mb: u32,
    pub total_tokens: u32,
    pub total_time_ms: u32,
    pub status: String,
    /// Human-readable detail when status is not "ok"
    pub error_detail: Option<String>,
    /// Thread count used for this benchmark (None = default/auto)
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct BenchmarkStoreRequest<'a> {
    pub model_name: &'a str,
    pub model_size_gb: f64,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u32,
    pub quant_v: u32,
    pub threads: u32,
    pub launch_profile_name: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct BenchmarkRunRequest<'a> {
    pub model_name: &'a str,
    pub model_path: &'a std::path::Path,
    pub backend: &'a BenchBackend,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub threads: Option<u32>,
    pub mode: BenchMode,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchThreadSweepRequest<'a> {
    pub model_name: &'a str,
    pub model_path: &'a std::path::Path,
    pub backend: &'a BenchBackend,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub base_threads: u32,
}

fn build_llamacpp_bench_args(
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    threads: Option<u32>,
) -> Vec<String> {
    let mut args = vec![
        "--host".into(),
        paths::DEFAULT_LOCALHOST.into(),
        "--port".into(),
        paths::DEFAULT_LLAMACPP_PORT.to_string(),
        "--n-gpu-layers".into(),
        gpu_layers.to_string(),
        "--ctx-size".into(),
        context_size.to_string(),
        "--threads".into(),
        threads.unwrap_or(8).to_string(),
        "--parallel".into(),
        "1".into(),
    ];
    args.extend(crate::processes::kv_cache_args(quant_k, quant_v));
    args
}

/// Run a single benchmark: clear → launch → generate → measure → kill → store.
#[tracing::instrument(skip(request))]
pub async fn run_benchmark(request: BenchmarkRunRequest<'_>) -> Result<BenchResult> {
    run_benchmark_with_progress(
        request.model_name,
        request.model_path,
        request.backend,
        request.gpu_layers,
        request.context_size,
        request.quant_k,
        request.quant_v,
        request.threads,
        request.mode,
        |progress| eprintln!("  ⬡ {}", progress.message),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_benchmark_with_progress<F>(
    _model_name: &str,
    model_path: &std::path::Path,
    backend: &BenchBackend,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    threads: Option<u32>,
    mode: BenchMode,
    mut on_progress: F,
) -> Result<BenchResult>
where
    F: FnMut(BenchProgress),
{
    let is_sweep = mode == BenchMode::Sweep;

    // Step 1: Clear existing backends
    on_progress(BenchProgress {
        message: "Clearing GPU backends…".into(),
    });
    processes::clear_gpu_backends().await?;

    // Sweep mode: clear_gpu_backends already sleeps 600ms — enough.
    // Precise mode: add extra settle for a clean start.
    if !is_sweep {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Step 1b: Check CPU governor
    if let Some(gov) = read_cpu_governor() {
        if gov != "performance" {
            on_progress(BenchProgress {
                message: format!(
                    "⚠ CPU governor is '{gov}', not 'performance' — benchmark may vary ±20%. Run: echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor"
                ),
            });
        }
    }

    // Step 2: Launch the selected backend
    on_progress(BenchProgress {
        message: format!("Launching {}…", backend.display_name()),
    });

    match backend {
        BenchBackend::LlamaCpp { server_path } => {
            let args =
                build_llamacpp_bench_args(gpu_layers, context_size, quant_k, quant_v, threads);
            processes::start_llamacpp(server_path, &model_path.to_string_lossy(), &args)
                .await
                .map_err(|e| anyhow!("Launch failed: {e}"))?;
        }
    }

    // Step 3: Confirm model is loaded
    let loaded_model = match backend {
        BenchBackend::LlamaCpp { .. } => processes::get_llamacpp_model()
            .await
            .ok_or_else(|| anyhow!("llama.cpp launched but model not available via API"))?,
    };
    on_progress(BenchProgress {
        message: format!("Model loaded: {loaded_model}"),
    });

    // Step 4: GPU settle — longer for precise mode, short for sweep
    let settle_secs = if is_sweep { 1 } else { 3 };
    on_progress(BenchProgress {
        message: format!("Waiting {settle_secs}s for GPU clocks to stabilise…"),
    });
    tokio::time::sleep(Duration::from_secs(settle_secs)).await;

    // Step 5: Warm-up (precise mode only). Sweep skips — model load already warmed GPU.
    if !is_sweep {
        on_progress(BenchProgress {
            message: "Running warm-up generation (discarded)…".into(),
        });
        let _ = run_llamacpp_generation(false).await;
    }

    // Step 6: Snapshot VRAM
    let vram_pre = hardware::query_gpu_memory();

    // Step 7: Measured generation(s)
    let gen_result = if is_sweep {
        // Sweep: single measured run at temp=0
        on_progress(BenchProgress {
            message: format!("Running generation benchmark ({BENCH_MAX_TOKENS} tokens)…"),
        });
        match backend {
            BenchBackend::LlamaCpp { .. } => run_llamacpp_generation(true).await,
        }
    } else {
        // Precise: dual measured runs, averaged
        on_progress(BenchProgress {
            message: format!("Running measured generation ({BENCH_MAX_TOKENS} tokens)…"),
        });
        let run1 = run_llamacpp_generation(true).await;
        on_progress(BenchProgress {
            message: "Running validation run…".into(),
        });
        let run2 = run_llamacpp_generation(true).await;

        match (run1, run2) {
            (Ok(g1), Ok(g2)) => {
                let variance = if g1.tokens_per_sec > 0.0 {
                    ((g2.tokens_per_sec - g1.tokens_per_sec).abs() / g1.tokens_per_sec) * 100.0
                } else {
                    0.0
                };
                if variance > 20.0 {
                    on_progress(BenchProgress {
                        message: format!(
                            "⚠ Runs diverged by {variance:.0}% — possible thermal throttling. Using average."
                        ),
                    });
                }
                Ok(GenerationResult {
                    tokens_per_sec: (g1.tokens_per_sec + g2.tokens_per_sec) / 2.0,
                    ttft_ms: (g1.ttft_ms + g2.ttft_ms) / 2,
                    token_count: (g1.token_count + g2.token_count) / 2,
                    total_ms: (g1.total_ms + g2.total_ms) / 2,
                    content: g1.content,
                })
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    };

    // Step 8: Snapshot VRAM after measured runs
    let vram_post = hardware::query_gpu_memory();
    let vram_peak_mb = vram_post
        .as_ref()
        .map(|v| v.used_mb as u32)
        .or_else(|| vram_pre.as_ref().map(|v| v.used_mb as u32))
        .unwrap_or(0);

    // Step 9: Get RAM usage
    let hw = hardware::load_hardware();
    let ram_peak_mb = hw.ram_used_mb as u32;

    // Step 10: Kill backend
    on_progress(BenchProgress {
        message: format!("Stopping {}…", backend.display_name()),
    });
    processes::clear_gpu_backends().await?;

    match gen_result {
        Ok(gen) => {
            let status = if output_is_garbage(&gen.content) {
                "garbage"
            } else {
                "ok"
            };
            Ok(BenchResult {
                tokens_per_sec: gen.tokens_per_sec,
                time_to_first_token_ms: gen.ttft_ms,
                vram_peak_mb,
                ram_peak_mb,
                total_tokens: gen.token_count,
                total_time_ms: gen.total_ms,
                status: status.into(),
                error_detail: if status == "garbage" {
                    Some("Output failed garbage detection".into())
                } else {
                    None
                },
                threads: None,
            })
        }
        Err(e) => {
            let detail = e.to_string();
            let status = if detail.contains("OOM") || detail.contains("out of memory") {
                "oom"
            } else if detail.contains("timeout") || detail.contains("Timeout") {
                "timeout"
            } else {
                "error"
            };
            Ok(BenchResult {
                tokens_per_sec: 0.0,
                time_to_first_token_ms: 0,
                vram_peak_mb,
                ram_peak_mb,
                total_tokens: 0,
                total_time_ms: 0,
                status: status.into(),
                error_detail: Some(detail),
                threads: None,
            })
        }
    }
}

struct GenerationResult {
    tokens_per_sec: f64,
    ttft_ms: u32,
    token_count: u32,
    total_ms: u32,
    content: String,
}

/// Run a generation against the llama.cpp /completion endpoint.
/// `measured`: when true, uses temperature=0 for deterministic, reproducible output.
/// When false (warm-up), uses temperature=0.7 to exercise realistic paths.
async fn run_llamacpp_generation(measured: bool) -> Result<GenerationResult> {
    #[derive(serde::Deserialize)]
    struct BenchTimings {
        predicted_n: u32,
        predicted_ms: f64,
        #[serde(default)]
        prompt_ms: f64,
    }
    #[derive(serde::Deserialize)]
    struct BenchResponse {
        content: Option<String>,
        timings: Option<BenchTimings>,
    }

    let client = ozone_core::http::client_with_timeout(API_TIMEOUT_SECS)?;

    let temperature = if measured { 0.0 } else { 0.7 };
    let max_tokens = if measured { BENCH_MAX_TOKENS } else { 10 };

    let payload = serde_json::json!({
        "prompt": BENCH_PROMPT,
        "n_predict": max_tokens,
        "temperature": temperature,
        "stream": false,
    });

    let url = format!("{}/completion", ozone_core::paths::llamacpp_base_url());

    let start = Instant::now();
    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| anyhow!("Generation request failed: {e}"))?;

    let total_elapsed = start.elapsed();

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Generation failed (HTTP {status}): {body}"));
    }

    let data: BenchResponse = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse llama.cpp completion response: {e}"))?;
    let generated_text = data.content.clone().unwrap_or_default();

    let total_ms = total_elapsed.as_millis() as u32;

    if let Some(timings) = data.timings {
        // Use server-reported timings for accuracy
        let tokens_per_sec = if timings.predicted_ms > 0.0 {
            timings.predicted_n as f64 / timings.predicted_ms * 1000.0
        } else {
            0.0
        };
        // Estimate TTFT from prompt eval time when available
        let ttft_ms = if timings.prompt_ms > 0.0 {
            timings.prompt_ms as u32
        } else {
            (total_ms as f64 * 0.15) as u32
        };
        Ok(GenerationResult {
            tokens_per_sec,
            ttft_ms,
            token_count: timings.predicted_n,
            total_ms,
            content: generated_text.clone(),
        })
    } else {
        // Fallback: estimate from wall clock and text length
        let text = data.content.unwrap_or_default();
        let token_count = (text.len() as f64 / 4.0).round().max(1.0) as u32;
        let tps = if total_ms > 0 {
            token_count as f64 / (total_ms as f64 / 1000.0)
        } else {
            0.0
        };
        let ttft_ms = (total_ms as f64 * 0.15) as u32;
        Ok(GenerationResult {
            tokens_per_sec: tps,
            ttft_ms,
            token_count,
            total_ms,
            content: text,
        })
    }
}

/// Store a benchmark result in the database.
pub fn store_result(request: BenchmarkStoreRequest<'_>, result: &BenchResult) -> Result<i64> {
    store_result_with_profile(request, result)
}

pub fn store_result_with_profile(
    request: BenchmarkStoreRequest<'_>,
    result: &BenchResult,
) -> Result<i64> {
    let conn = db::open()?;
    let hw = hardware::load_hardware();
    let gpu_name = get_gpu_name().unwrap_or_else(|| "unknown".into());
    let gpu_vram_mb = hw.gpu.as_ref().map(|g| g.total_mb as u32).unwrap_or(0);

    let row = BenchmarkRow {
        model_name: request.model_name.to_string(),
        model_size_gb: request.model_size_gb,
        gpu_layers: request.gpu_layers,
        context_size: request.context_size,
        quant_k: request.quant_k,
        quant_v: request.quant_v,
        threads: request.threads,
        tokens_per_sec: result.tokens_per_sec,
        time_to_first_token_ms: result.time_to_first_token_ms,
        vram_peak_mb: result.vram_peak_mb,
        ram_peak_mb: result.ram_peak_mb,
        total_tokens: result.total_tokens,
        total_time_ms: result.total_time_ms,
        status: result.status.clone(),
        gpu_name,
        gpu_vram_mb,
        ram_total_mb: hw.ram_total_mb as u32,
        timestamp: chrono::Local::now().to_rfc3339(),
        notes: String::new(),
        launch_profile_name: request.launch_profile_name.map(str::to_string),
    };
    db::insert_benchmark(&conn, &row)
}

fn get_gpu_name() -> Option<String> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=gpu_name", "--format=csv,noheader"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.trim().to_string())
}

/// Print benchmark results to stdout.
pub fn print_result(
    model_name: &str,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
    result: &BenchResult,
) {
    println!();
    println!("  ⬡ Benchmark Results");
    println!("  ─────────────────────────────────────────────────");
    println!("  Model:       {model_name}");
    println!("  GPU Layers:  {gpu_layers}");
    println!("  Context:     {context_size}");
    println!("  Quant K:     {quant_k}");
    println!("  Quant V:     {quant_v}");
    println!("  Status:      {}", result.status);
    println!("  ─────────────────────────────────────────────────");
    if result.status == "ok" {
        println!("  Tokens/sec:  {:.2}", result.tokens_per_sec);
        println!("  TTFT:        {} ms", result.time_to_first_token_ms);
        println!("  VRAM Peak:   {} MB", result.vram_peak_mb);
        println!("  RAM Peak:    {} MB", result.ram_peak_mb);
        println!("  Tokens:      {}", result.total_tokens);
        println!("  Total time:  {} ms", result.total_time_ms);
    } else {
        println!("  Benchmark failed: {}", result.status);
        if let Some(ref detail) = result.error_detail {
            println!("  Detail: {detail}");
        }
    }
    println!();
}

/// Detect garbage output from a struggling model.
/// Returns true if the output looks like random/looping tokens rather
/// than coherent text. Used to flag (not stop) further testing on broken configs.
pub fn output_is_garbage(text: &str) -> bool {
    if text.is_empty() || text.len() < 10 {
        return true;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 3 {
        return true;
    }
    // Character-level loop check: if any single char dominates, it's gibberish
    let total_chars = text.chars().count() as f64;
    if total_chars > 0.0 {
        use std::collections::HashMap;
        let mut char_counts: HashMap<char, usize> = HashMap::new();
        for c in text.chars() {
            *char_counts.entry(c).or_insert(0) += 1;
        }
        let max_char_ratio = char_counts
            .values()
            .max()
            .map(|&c| c as f64 / total_chars)
            .unwrap_or(0.0);
        if max_char_ratio > 0.5 {
            return true; // e.g. "aaaaaaaaaaaaaaaaaaaaaaaa"
        }
    }
    // N-gram repetition: check for excessive token-level looping (>90% duplicates)
    use std::collections::HashSet;
    let unique: HashSet<_> = tokens.iter().collect();
    let unique_ratio = unique.len() as f64 / tokens.len() as f64;
    if unique_ratio < 0.1 {
        return true; // >90% duplicate tokens is near-certain looping
    }
    false
}

/// Thread count sweep: tests different thread counts at fixed context/quant,
/// storing each result with a launch_profile_name marker.
/// Uses Sweep mode for speed (single run, no warm-up).
pub async fn run_thread_sweep(
    model_name: &str,
    model_path: &std::path::Path,
    backend: &BenchBackend,
    gpu_layers: i32,
    context_size: u32,
    quant_k: u8,
    quant_v: u8,
) -> Result<Vec<BenchResult>> {
    let thread_counts: &[u32] = &[1, 2, 4, 6, 8, 12];
    let mut results = Vec::new();

    for &threads in thread_counts {
        let mut result = run_benchmark_with_progress(
            model_name,
            model_path,
            backend,
            gpu_layers,
            context_size,
            quant_k,
            quant_v,
            Some(threads),
            BenchMode::Sweep,
            |progress| eprintln!("  ⬡ [threads={threads}] {}", progress.message),
        )
        .await?;
        result.threads = Some(threads);

        // Store with profile marker so it's filterable
        let _ = store_result_with_profile(
            BenchmarkStoreRequest {
                model_name,
                model_size_gb: 0.0,
                gpu_layers,
                context_size,
                quant_k: quant_k as u32,
                quant_v: quant_v as u32,
                threads,
                launch_profile_name: Some("thread-sweep"),
            },
            &result,
        );

        results.push(result);
    }

    Ok(results)
}

/// Batch thread count sweep: tests different batch thread counts independently.
/// Batch threads handle prompt processing; useful for tuning prompt eval speed.
pub async fn run_batch_thread_sweep(
    request: BatchThreadSweepRequest<'_>,
) -> Result<Vec<BenchResult>> {
    let batch_counts: &[u32] = &[1, 2, 4, 6, 8];
    let mut results = Vec::new();

    for &batch in batch_counts {
        // Build custom args with --threads-batch
        let mut args = vec![
            "--host".into(),
            paths::DEFAULT_LOCALHOST.into(),
            "--port".into(),
            paths::DEFAULT_LLAMACPP_PORT.to_string(),
            "--n-gpu-layers".into(),
            request.gpu_layers.to_string(),
            "--ctx-size".into(),
            request.context_size.to_string(),
            "--threads".into(),
            request.base_threads.to_string(),
            "--threads-batch".into(),
            batch.to_string(),
            "--parallel".into(),
            "1".into(),
        ];
        args.extend(crate::processes::kv_cache_args(
            request.quant_k,
            request.quant_v,
        ));

        // Launch + benchmark manually since we need custom args
        processes::clear_gpu_backends().await?;
        processes::start_llamacpp(
            backend_server_path(request.backend)?,
            &request.model_path.to_string_lossy(),
            &args,
        )
        .await?;

        eprintln!("  ⬡ [batch={batch}] Running generation…");
        let gen = run_llamacpp_generation(true).await;

        processes::clear_gpu_backends().await?;

        match gen {
            Ok(g) => {
                let result = BenchResult {
                    tokens_per_sec: g.tokens_per_sec,
                    time_to_first_token_ms: g.ttft_ms,
                    vram_peak_mb: 0,
                    ram_peak_mb: 0,
                    total_tokens: g.token_count,
                    total_time_ms: g.total_ms,
                    status: if output_is_garbage(&g.content) {
                        "garbage".into()
                    } else {
                        "ok".into()
                    },
                    error_detail: None,
                    threads: None,
                };
                let _ = store_result_with_profile(
                    BenchmarkStoreRequest {
                        model_name: request.model_name,
                        model_size_gb: 0.0,
                        gpu_layers: request.gpu_layers,
                        context_size: request.context_size,
                        quant_k: request.quant_k as u32,
                        quant_v: request.quant_v as u32,
                        threads: batch, // store batch threads in threads field for now
                        launch_profile_name: Some("batch-sweep"),
                    },
                    &result,
                );
                results.push(result);
            }
            Err(e) => {
                eprintln!("  ⬡ [batch={batch}] Failed: {e}");
            }
        }
    }

    Ok(results)
}

/// Print a thread sweep summary table.
pub fn print_thread_sweep_summary(thread_results: &[BenchResult]) {
    println!();
    println!("  ⬡ Thread Sweep Results");
    println!("  ─────────────────────────────────────────────────");
    println!(
        "  {:<10} {:<12} {:<10} {:<10}",
        "Threads", "Tok/s", "TTFT ms", "Status"
    );
    println!("  ─────────────────────────────────────────────────");
    let thread_counts = [1, 2, 4, 6, 8, 12];
    for (i, result) in thread_results.iter().enumerate() {
        let t = thread_counts.get(i).unwrap_or(&0);
        println!(
            "  {:<10} {:<12.2} {:<10} {:<10}",
            t, result.tokens_per_sec, result.time_to_first_token_ms, result.status,
        );
    }
    println!();
}

fn backend_server_path(backend: &BenchBackend) -> Result<&std::path::Path> {
    match backend {
        BenchBackend::LlamaCpp { server_path } => Ok(server_path.as_path()),
    }
}

#[cfg(test)]
mod garbage_tests {
    use super::output_is_garbage;

    #[test]
    fn empty_text_is_garbage() {
        assert!(output_is_garbage(""));
    }

    #[test]
    fn short_text_is_garbage() {
        assert!(output_is_garbage("a"));
    }

    #[test]
    fn repetitive_looping_is_garbage() {
        let looping =
            "the the the the the the the the the the the the the the the the the the the the";
        assert!(output_is_garbage(looping));
    }

    #[test]
    fn char_loop_is_garbage() {
        let char_loop = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(output_is_garbage(char_loop));
    }

    #[test]
    fn coherent_text_is_not_garbage() {
        let coherent = "The relationship between computational complexity and software engineering is a fascinating topic. Big O notation helps programmers understand algorithm scaling.";
        assert!(!output_is_garbage(coherent));
    }

    #[test]
    fn short_coherent_phrase_is_not_garbage() {
        // Short but coherent benchmark output should NOT be flagged
        let short = "Model loaded successfully.";
        assert!(!output_is_garbage(short));
    }
}
