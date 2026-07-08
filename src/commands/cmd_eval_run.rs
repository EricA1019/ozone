//! CLI handler for `oz eval-run` — run a full eval suite against a model.
//! Extracted from `src/lib.rs` inline dispatch.
//!
//! Includes a shared `build_eval_run_config()` helper that eliminates the
//! DRY violation (two nearly-identical EvalRunConfig construction branches).

use std::path::PathBuf;
use anyhow::Result;
use crate::runner::{EvalRunConfig, SweepLevel};

/// Build an `EvalRunConfig` from CLI arguments.
///
/// Shared by both `no_manage_server` and managed-server paths to avoid
/// duplicating the struct construction logic.
#[allow(clippy::too_many_arguments)]
fn build_eval_run_config(
    model_path: &str,
    backend: &str,
    base_url: String,
    context_length: u32,
    skip_warmup: bool,
    skip_health_gate: bool,
    allow_below_min_context: bool,
    sweep_level: SweepLevel,
    gate_attempts: u32,
    regular_attempts: u32,
    gpu_layers: i32,
    threads: Option<u32>,
    manage_server: bool,
    server_path: Option<PathBuf>,
    cache_type_k: u8,
    cache_type_v: u8,
    flash_attn: Option<bool>,
    no_thinking: bool,
) -> EvalRunConfig {
    let mut policy = crate::policy::ContextPolicy::default();
    if allow_below_min_context {
        policy.allow_below_min_context = true;
    }
    EvalRunConfig {
        model_name: std::path::Path::new(model_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        model_path: model_path.to_string(),
        backend: backend.to_string(),
        base_url,
        context_length,
        skip_warmup,
        skip_health_gate,
        policy,
        sweep_level,
        gate_attempts,
        regular_attempts,
        gpu_layers,
        threads,
        manage_server,
        server_path,
        cache_type_k,
        cache_type_v,
        flash_attn,
        no_thinking,
    }
}

/// Resolve cache type with env var fallback.
fn resolve_cache_type(cli_val: Option<u8>, env_var: &str, default: u8) -> u8 {
    cli_val.unwrap_or_else(|| {
        std::env::var(env_var)
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(default)
    })
}

#[cfg(feature = "eval")]
#[allow(clippy::too_many_arguments)]
pub async fn cmd_eval_run(
    model_path: String,
    backend: String,
    base_url: String,
    context_length: u32,
    skip_warmup: bool,
    skip_health_gate: bool,
    quick: bool,
    standard: bool,
    attempts: Option<u32>,
    gate_attempts: Option<u32>,
    no_manage_server: bool,
    allow_below_min_context: bool,
    gpu_layers: i32,
    threads: Option<u32>,
    cli_server_path: Option<String>,
    cache_type_k: Option<u8>,
    cache_type_v: Option<u8>,
    cache_type_kv: Option<u8>,
    flash_attn: Option<bool>,
    no_thinking: bool,
) -> Result<()> {
    let sweep_level = if quick {
        SweepLevel::Quick
    } else if standard {
        SweepLevel::Standard
    } else {
        SweepLevel::Full // default (also when --full is set)
    };

    let effective_cache_k = resolve_cache_type(cache_type_k, "OZONE_QUANT_K", 1);
    let effective_cache_v = resolve_cache_type(
        cache_type_v.or(cache_type_kv),
        "OZONE_QUANT_V",
        effective_cache_k,
    );

    let config = if no_manage_server {
        build_eval_run_config(
            &model_path, &backend, base_url, context_length,
            skip_warmup, skip_health_gate, allow_below_min_context,
            sweep_level,
            gate_attempts.unwrap_or(0), attempts.unwrap_or(0),
            gpu_layers, threads,
            false, None,
            effective_cache_k, effective_cache_v,
            flash_attn, no_thinking,
        )
    } else {
        let resolved_server_path = if let Some(ref p) = cli_server_path {
            PathBuf::from(p)
        } else {
            crate::processes::resolved_llamacpp_server_path()?
        };
        build_eval_run_config(
            &model_path, &backend, base_url, context_length,
            skip_warmup, skip_health_gate, allow_below_min_context,
            sweep_level,
            gate_attempts.unwrap_or(0), attempts.unwrap_or(0),
            gpu_layers, threads,
            true, Some(resolved_server_path),
            effective_cache_k, effective_cache_v,
            flash_attn, no_thinking,
        )
    };

    let result = crate::runner::run_eval(&config).await?;
    println!(
        "Status: {} ({}/{} passed, {} skipped by gate, in {:.1}s)",
        result.status,
        result.tasks_passed,
        result.tasks_run,
        result.tasks_skipped_gate,
        result.total_duration_ms as f64 / 1000.0
    );
    Ok(())
}
