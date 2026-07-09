//! CLI handler for `oz bench` — benchmark a model with specific settings.
//! Extracted from `src/lib.rs` inline dispatch.

use anyhow::Result;

#[cfg(feature = "bench")]
#[allow(clippy::too_many_arguments)]
pub async fn cmd_bench(
    model: String,
    gpu_layers: i32,
    context: u32,
    quant_k: u8,
    quant_v: Option<u8>,
    quant_kv: Option<u8>,
    threads: Option<u32>,
    save_profile: Option<String>,
) -> Result<()> {
    let model_dir = ozone_core::paths::models_dir();
    let model_path = model_dir.join(&model);
    let server_path = crate::llamacpp::resolved_llamacpp_server_path()?;
    let backend = crate::bench::BenchBackend::LlamaCpp { server_path };

    if !model_path.exists() {
        ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
        std::process::exit(1);
    }

    // Resolve quant_k and quant_v: --quant-kv sets both, --quant-v overrides V only
    let effective_k = quant_kv.unwrap_or(quant_k);
    let effective_v = quant_v.or(quant_kv).unwrap_or(quant_k);

    // Get model size for storage
    let model_size_gb = std::fs::metadata(&model_path)
        .map(|m| m.len() as f64 / 1_073_741_824.0)
        .unwrap_or(0.0);

    ozone_core::cli::header("oz Bench");
    ozone_core::cli::field("Model:", &model);
    ozone_core::cli::field("GPU Layers:", &gpu_layers);
    ozone_core::cli::field("Context:", &context);
    ozone_core::cli::field("Quant K:", &effective_k);
    ozone_core::cli::field("Quant V:", &effective_v);
    if let Some(t) = threads {
        ozone_core::cli::field("Threads:", &t);
    }
    ozone_core::cli::spacer();

    let result = crate::bench::run_benchmark(crate::bench::BenchmarkRunRequest {
        model_name: &model,
        model_path: &model_path,
        backend: &backend,
        gpu_layers,
        context_size: context,
        quant_k: effective_k,
        quant_v: effective_v,
        threads,
        mode: crate::bench::BenchMode::Precise,
    })
    .await?;

    crate::bench::print_result(
        &model,
        gpu_layers,
        context,
        effective_k,
        effective_v,
        &result,
    );

    // Store result
    let thread_count = threads.unwrap_or(0);
    match crate::bench::store_result(
        crate::bench::BenchmarkStoreRequest {
            model_name: &model,
            model_size_gb,
            gpu_layers,
            context_size: context,
            quant_k: effective_k as u32,
            quant_v: effective_v as u32,
            threads: thread_count,
            launch_profile_name: None,
        },
        &result,
    ) {
        Ok(id) => ozone_core::cli::success(&format!("Stored as benchmark #{id}")),
        Err(e) => ozone_core::cli::warn(&format!("Failed to store result: {e}")),
    }

    // Save config as a named launch profile if requested
    if let Some(ref profile_name) = save_profile {
        let mut prefs = crate::prefs::load_prefs().await?;
        prefs.upsert_saved_launch_profile(
            &model,
            crate::prefs::SavedLaunchProfile {
                profile_name: profile_name.clone(),
                context_size: context,
                gpu_layers,
                quant_k: effective_k,
                quant_v: effective_v,
                threads,
            },
        );
        crate::prefs::save_prefs(&prefs).await?;
        ozone_core::cli::success(&format!("Saved profile '{profile_name}' for {model}"));
    }
    Ok(())
}
