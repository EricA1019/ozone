//! CLI handler for `oz sweep` — run a sweep over context sizes and quant levels.
//! Extracted from `src/lib.rs` inline dispatch.

use anyhow::Result;

#[cfg(feature = "sweep")]
#[allow(clippy::too_many_arguments)]
pub async fn cmd_sweep(
    model: String,
    max_context: Option<u32>,
    quick: bool,
    context_sweep: bool,
    quant_kv: u8,
    sweep_quant: bool,
) -> Result<()> {
    let model_dir = ozone_core::paths::models_dir();
    let model_path = model_dir.join(&model);
    let server_path = crate::processes::resolved_llamacpp_server_path()?;

    if context_sweep {
        if sweep_quant {
            // Test each quant level (1=f16, 2=q8_0, 3=q4_0) at each context
            for &qkv in &[1u8, 2u8, 3u8] {
                eprintln!("\n  --- Sweep with quant_k={qkv} quant_v={qkv} ---");
                let _ = crate::sweep::run_context_sweep(crate::sweep::ContextSweepRequest {
                    model_name: &model,
                    model_path: &model_path,
                    server_path: &server_path,
                    gpu_layers: -1,
                    quant_k: qkv,
                    quant_v: qkv,
                    threads: None,
                    quick,
                })
                .await;
            }
            return Ok(());
        }
        let (csv_path, sweet_spot) = crate::sweep::run_context_sweep(crate::sweep::ContextSweepRequest {
            model_name: &model,
            model_path: &model_path,
            server_path: &server_path,
            gpu_layers: -1,
            quant_k: quant_kv,
            quant_v: quant_kv,
            threads: None,
            quick,
        })
        .await?;
        ozone_core::cli::success(&format!(
            "Sweep complete. Sweet spot: context={sweet_spot}. CSV: {}",
            csv_path.display()
        ));
        return Ok(());
    }

    if !model_path.exists() {
        ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
        std::process::exit(1);
    }

    let model_size_gb = std::fs::metadata(&model_path)
        .map(|m| m.len() as f64 / 1_073_741_824.0)
        .unwrap_or(0.0);

    let hw = crate::hardware::load_hardware();
    let gpu_vram_budget_mb = hw
        .gpu
        .as_ref()
        .map(|g| (g.total_mb as f64 * 0.9) as u32)
        .unwrap_or(0);

    let (context_sizes, quant_kv_levels) = if quick {
        (vec![4096, 8192], vec![(1u8, 1u8)])
    } else {
        // Read the model's native max context from GGUF metadata
        let native_max = crate::gguf::read_context_length(&model_path).unwrap_or(65536);
        let max = max_context.unwrap_or(native_max).min(native_max);
        let ctxs = crate::sweep::generate_context_steps(max);
        (ctxs, vec![(1u8, 1u8), (2u8, 2u8)])
    };

    let sweep_config = crate::sweep::SweepConfig {
        model_name: model.clone(),
        model_path: model_path.clone(),
        backend: crate::bench::BenchBackend::LlamaCpp { server_path },
        model_size_gb,
        total_layers: crate::gguf::inspect_model_topology(
            &model_path,
            crate::launch_config::estimate_total_layers(model_size_gb),
        )
        .total_layers,
        context_sizes,
        quant_kv_levels,
        gpu_vram_budget_mb,
    };

    let result = crate::sweep::run_sweep(sweep_config).await?;

    // Auto-save the optimal profile for quick loading
    if let Some(optimal) =
        crate::sweep::pick_optimal_profile(&model, &result.pareto_frontier, None)
    {
        let mut prefs = crate::prefs::load_prefs().await?;
        prefs.upsert_saved_launch_profile(&model, optimal.clone());
        prefs.set_default_saved_launch_profile(&model, "auto-optimal");
        crate::prefs::save_prefs(&prefs).await?;
        ozone_core::cli::success(&format!(
            "Auto-saved 'auto-optimal' profile: ctx={}, gpu={}, K=q{}, V=q{}",
            optimal.context_size, optimal.gpu_layers, optimal.quant_k, optimal.quant_v,
        ));
    }

    if let Some(ref csv_path) = result.csv_path {
        ozone_core::cli::info(&format!("CSV: {}", csv_path.display()));
    }

    Ok(())
}
