use anyhow::{Result, anyhow};
use std::time::{Duration, Instant};
use crate::db::{self, BenchmarkRow};
use crate::hardware;
use crate::processes;

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
const API_TIMEOUT_SECS: u64 = 180;

#[derive(Debug, Clone)]
pub struct BenchProgress {
    pub stage: &'static str,
    pub message: String,
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
}

/// Run a single benchmark: clear → launch → generate → measure → kill → store.
pub async fn run_benchmark(
    model_name: &str,
    _model_path: &std::path::Path,
    launcher_path: &std::path::Path,
    gpu_layers: i32,
    context_size: u32,
    quant_kv: u8,
    threads: Option<u32>,
) -> Result<BenchResult> {
    run_benchmark_with_progress(
        model_name,
        _model_path,
        launcher_path,
        gpu_layers,
        context_size,
        quant_kv,
        threads,
        |progress| eprintln!("  ⬡ {}", progress.message),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_benchmark_with_progress<F>(
    model_name: &str,
    _model_path: &std::path::Path,
    launcher_path: &std::path::Path,
    gpu_layers: i32,
    context_size: u32,
    quant_kv: u8,
    threads: Option<u32>,
    mut on_progress: F,
) -> Result<BenchResult>
where
    F: FnMut(BenchProgress),
{
    // Step 1: Clear existing backends
    on_progress(BenchProgress {
        stage: "clear",
        message: "Clearing GPU backends…".into(),
    });
    processes::clear_gpu_backends().await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Step 2: Build launch args and start KoboldCpp
    on_progress(BenchProgress {
        stage: "launch",
        message: "Launching KoboldCpp…".into(),
    });
    let mut args: Vec<String> = vec![
        format!("--gpulayers={gpu_layers}"),
        format!("--contextsize={context_size}"),
        format!("--quantkv={quant_kv}"),
    ];
    if let Some(t) = threads {
        args.push(format!("--threads={t}"));
        args.push(format!("--blasthreads={t}"));
    }
    processes::start_kobold(launcher_path, model_name, &args).await
        .map_err(|e| anyhow!("Launch failed: {e}"))?;

    // Step 3: Confirm model is loaded
    let loaded_model = processes::get_kobold_model().await
        .ok_or_else(|| anyhow!("KoboldCpp launched but model not available via API"))?;
    on_progress(BenchProgress {
        stage: "ready",
        message: format!("Model loaded: {loaded_model}"),
    });

    // Step 4: Snapshot VRAM after model load (peak during inference will be higher,
    // but this gives a good baseline)
    let vram_pre = hardware::query_gpu_memory();

    // Step 5: Run generation benchmark
    on_progress(BenchProgress {
        stage: "generate",
        message: format!("Running generation benchmark ({BENCH_MAX_TOKENS} tokens)…"),
    });
    let gen_result = run_generation().await;

    // Step 6: Snapshot VRAM during/after generation
    let vram_post = hardware::query_gpu_memory();
    let vram_peak_mb = vram_post.as_ref()
        .map(|v| v.used_mb as u32)
        .or_else(|| vram_pre.as_ref().map(|v| v.used_mb as u32))
        .unwrap_or(0);

    // Step 7: Get RAM usage
    let hw = hardware::load_hardware();
    let ram_peak_mb = hw.ram_used_mb as u32;

    // Step 8: Kill KoboldCpp
    on_progress(BenchProgress {
        stage: "stop",
        message: "Stopping KoboldCpp…".into(),
    });
    processes::clear_gpu_backends().await?;

    match gen_result {
        Ok(gen) => Ok(BenchResult {
            tokens_per_sec: gen.tokens_per_sec,
            time_to_first_token_ms: gen.ttft_ms,
            vram_peak_mb,
            ram_peak_mb,
            total_tokens: gen.token_count,
            total_time_ms: gen.total_ms,
            status: "ok".into(),
        }),
        Err(e) => {
            let status = if e.to_string().contains("OOM") || e.to_string().contains("out of memory") {
                "oom"
            } else if e.to_string().contains("timeout") || e.to_string().contains("Timeout") {
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
            })
        }
    }
}

struct GenerationResult {
    tokens_per_sec: f64,
    ttft_ms: u32,
    token_count: u32,
    total_ms: u32,
}

async fn run_generation() -> Result<GenerationResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(API_TIMEOUT_SECS))
        .build()?;

    let payload = serde_json::json!({
        "prompt": BENCH_PROMPT,
        "max_length": BENCH_MAX_TOKENS,
        "temperature": 0.7,
        "top_p": 0.9,
    });

    let start = Instant::now();
    let resp = client.post("http://127.0.0.1:5001/api/v1/generate")
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

    let data: serde_json::Value = resp.json().await
        .map_err(|e| anyhow!("Failed to parse generation response: {e}"))?;

    // KoboldCpp returns {"results": [{"text": "..."}]}
    let text = data["results"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|r| r["text"].as_str())
        .unwrap_or("");

    // Estimate token count from text length (rough: ~4 chars per token for English)
    let token_count = (text.len() as f64 / 4.0).round().max(1.0) as u32;
    let total_ms = total_elapsed.as_millis() as u32;

    // Try to get more accurate perf data from KoboldCpp's perf endpoint
    let (final_tps, ttft_ms) = if let Some(perf_tps) = processes::get_kobold_perf().await {
        // Perf endpoint gives us accurate tokens/sec
        // TTFT estimation: total_time - (tokens / tps * 1000)
        let gen_time_ms = if perf_tps > 0.0 {
            (token_count as f64 / perf_tps * 1000.0) as u32
        } else { total_ms };
        let ttft = total_ms.saturating_sub(gen_time_ms);
        (perf_tps, ttft)
    } else {
        // Fallback: estimate from wall clock
        let tps = if total_ms > 0 { token_count as f64 / (total_ms as f64 / 1000.0) } else { 0.0 };
        // Rough TTFT: assume first token takes ~20% of total time for small generations
        let ttft = (total_ms as f64 * 0.15) as u32;
        (tps, ttft)
    };

    Ok(GenerationResult {
        tokens_per_sec: final_tps,
        ttft_ms,
        token_count,
        total_ms,
    })
}

/// Store a benchmark result in the database.
pub fn store_result(
    model_name: &str,
    model_size_gb: f64,
    gpu_layers: i32,
    context_size: u32,
    quant_kv: u32,
    threads: u32,
    result: &BenchResult,
) -> Result<i64> {
    let conn = db::open()?;
    let hw = hardware::load_hardware();
    let gpu_name = get_gpu_name().unwrap_or_else(|| "unknown".into());
    let gpu_vram_mb = hw.gpu.as_ref().map(|g| g.total_mb as u32).unwrap_or(0);

    let row = BenchmarkRow {
        id: None,
        model_name: model_name.to_string(),
        model_size_gb,
        gpu_layers,
        context_size,
        quant_kv,
        threads,
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
pub fn print_result(model_name: &str, gpu_layers: i32, context_size: u32, quant_kv: u8, result: &BenchResult) {
    println!();
    println!("  ⬡ Benchmark Results");
    println!("  ─────────────────────────────────────────────────");
    println!("  Model:       {model_name}");
    println!("  GPU Layers:  {gpu_layers}");
    println!("  Context:     {context_size}");
    println!("  Quant KV:    {quant_kv}");
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
    }
    println!();
}
