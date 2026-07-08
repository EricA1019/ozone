//! CLI handler for `oz thread-sweep` — sweep CPU thread counts for a config.
//! Extracted from `src/lib.rs` inline dispatch.

use anyhow::Result;

#[cfg(feature = "bench")]
#[allow(clippy::too_many_arguments)]
pub async fn cmd_thread_sweep(
    model: String,
    gpu_layers: i32,
    context: u32,
    quant_k: u8,
    quant_v: u8,
    batch: bool,
) -> Result<()> {
    let model_dir = ozone_core::paths::models_dir();
    let model_path = model_dir.join(&model);
    let server_path = crate::processes::resolved_llamacpp_server_path()?;
    let backend = crate::bench::BenchBackend::LlamaCpp { server_path };

    if !model_path.exists() {
        ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
        std::process::exit(1);
    }

    if batch {
        ozone_core::cli::header("oz Batch Thread Sweep");
        ozone_core::cli::field("Model:", &model);
        ozone_core::cli::field("Context:", &context);
        ozone_core::cli::spacer();

        let results = crate::bench::run_batch_thread_sweep(crate::bench::BatchThreadSweepRequest {
            model_name: &model,
            model_path: &model_path,
            backend: &backend,
            gpu_layers,
            context_size: context,
            quant_k,
            quant_v,
            base_threads: 6,
        })
        .await?;
        crate::bench::print_thread_sweep_summary(&results);
    } else {
        ozone_core::cli::header("oz Thread Sweep");
        ozone_core::cli::field("Model:", &model);
        ozone_core::cli::field("Context:", &context);
        ozone_core::cli::field("Quant K:", &quant_k);
        ozone_core::cli::field("Quant V:", &quant_v);
        ozone_core::cli::spacer();

        let results = crate::bench::run_thread_sweep(
            &model,
            &model_path,
            &backend,
            gpu_layers,
            context,
            quant_k,
            quant_v,
        )
        .await?;
        crate::bench::print_thread_sweep_summary(&results);
    }
    Ok(())
}
