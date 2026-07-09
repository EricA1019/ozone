//! CLI command dispatch helpers — extracted from `lib.rs`.
//!
//! Each `cmd_*` function handles one CLI command by calling into the
//! appropriate crate module. `lib.rs::run()` calls these directly.

use anyhow::Result;

// ── Handler modules (one per CLI command, extracted from lib.rs inline dispatch) ──

#[cfg(feature = "bench")]
mod cmd_bench;
#[cfg(feature = "sweep")]
mod cmd_sweep;
#[cfg(feature = "bench")]
mod cmd_thread_sweep;
#[cfg(feature = "eval")]
mod cmd_eval_run;
#[cfg(feature = "eval")]
mod cmd_creative_write;
#[cfg(feature = "model-mgmt")]
mod cmd_model;

// Re-exports — each matches a `Commands::*` variant.
#[cfg(feature = "bench")]
pub use cmd_bench::cmd_bench;
#[cfg(feature = "sweep")]
pub use cmd_sweep::cmd_sweep;
#[cfg(feature = "bench")]
pub use cmd_thread_sweep::cmd_thread_sweep;
#[cfg(feature = "eval")]
pub use cmd_eval_run::cmd_eval_run;
#[cfg(feature = "eval")]
pub use cmd_creative_write::cmd_creative_write;
#[cfg(feature = "model-mgmt")]
pub use cmd_model::cmd_model;

pub async fn cmd_clear() -> Result<()> {
    let killed = crate::llamacpp::clear_gpu_backends().await?;
    if killed.is_empty() {
        ozone_core::cli::info("No GPU backends running.");
    } else {
        for k in &killed {
            ozone_core::cli::success(&format!("Stopped: {k}"));
        }
    }
    Ok(())
}

pub async fn cmd_purge_last_model() -> Result<()> {
    let killed = crate::llamacpp::purge_last_model().await?;
    if killed.is_empty() {
        ozone_core::cli::info("No managed llama.cpp model was running.");
    } else {
        for pid in killed {
            ozone_core::cli::success(&format!("Stopped managed llama.cpp pid {pid}"));
        }
    }
    Ok(())
}

pub async fn cmd_import_specs() -> Result<()> {
    ozone_core::cli::header("Import System Specs");
    ozone_core::cli::info("Capturing GPU, CPU, RAM, and CUDA info\u{2026}");
    let profile = crate::hardware::import_system_specs();
    if let Some(ref name) = profile.gpu_name {
        ozone_core::cli::field("GPU:", name);
    }
    if let Some(ref gpu) = profile.gpu {
        ozone_core::cli::field("VRAM:", &format!("{} MB", gpu.total_mb));
    }
    ozone_core::cli::field("CUDA:", &if profile.cuda_available { "\u{2713}" } else { "\u{2717}" });
    if let Some(ref cap) = profile.compute_capability {
        ozone_core::cli::field("Compute Cap:", cap);
    }
    ozone_core::cli::field("Flash Attn:", &if profile.flash_attn_supported { "\u{2713}" } else { "\u{2717}" });
    ozone_core::cli::field("CPU:", &format!("{} logical / {} physical", profile.cpu_logical, profile.cpu_physical));
    ozone_core::cli::field("RAM:", &format!("{} MB total", profile.ram_total_mb));
    ozone_core::cli::success("Saved to system-profile.json");
    Ok(())
}

pub async fn cmd_profiles() -> Result<()> {
    let prefs = crate::prefs::load_prefs().await?;
    let profs = &prefs.saved_launch_profiles;
    if profs.is_empty() {
        ozone_core::cli::info("No saved launch profiles found.");
    } else {
        println!("Saved launch profiles:");
        for (model, profiles) in profs {
            for p in profiles {
                let default_marker = prefs
                    .default_saved_launch_profile_name_for(model)
                    .filter(|d| d == &p.profile_name)
                    .map(|_| " [default]")
                    .unwrap_or("");
                println!(
                    "  {:<20}  {:>7} ctx  {:>3} gpu  K=q{} V=q{}  threads={}{}",
                    p.profile_name,
                    p.context_size,
                    p.gpu_layers,
                    p.quant_k,
                    p.quant_v,
                    p.threads.map(|t| t.to_string()).unwrap_or_else(|| "auto".into()),
                    default_marker,
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "eval")]
pub async fn cmd_eval_list() -> Result<()> {
    println!("{:<20} {:<50} KIND", "NAME", "DESCRIPTION");
    for task in crate::eval::EVAL_TASKS {
        let kind_label = match task.kind {
            crate::eval::EvalTaskKind::LmEval { .. } => "lm-eval",
            crate::eval::EvalTaskKind::EvalPlus { .. } => "evalplus",
            crate::eval::EvalTaskKind::CreativeWriting => "creative-writing",
        };
        println!("{:<20} {:<50} {}", task.cli_name, task.description, kind_label);
    }
    Ok(())
}

pub async fn cmd_list(json: bool) -> Result<()> {
    let model_dir = ozone_core::paths::models_dir();
    let preset_file = ozone_core::paths::catalog_preset_path();
    let bench_file = model_dir.join("bench-results.txt");
    let report = crate::catalog::load_catalog_report(&model_dir, &preset_file, &bench_file).await?;
    for issue in &report.issues {
        eprintln!("catalog {}: {}", issue.level.label(), issue.message);
    }
    let records = report.records;
    if json {
        let rows: Vec<_> = records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "model": r.model_name,
                    "size_gb": r.model_size_gb,
                    "source": r.recommendation.source.label(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        #[cfg(feature = "model-mgmt")]
        {
            eprintln!("  hint: `oz list` is deprecated — use `oz model list` instead.");
            eprintln!();
        }
        #[cfg(not(feature = "model-mgmt"))]
        {
            eprintln!("  note: this build exposes the lightweight `oz list` catalog only.");
            eprintln!("        for `oz model ...`, install via `./contrib/sync-local-install.sh`");
            eprintln!("        or build `cargo build --release -p ozone --features full`.");
            eprintln!();
        }
        println!("  {:<6}  {:>8}  MODEL", "SOURCE", "SIZE");
        if records.is_empty() {
            println!();
            println!("  no models found in {}", model_dir.display());
            println!();
            #[cfg(feature = "model-mgmt")]
            {
                println!("  next: add one with `oz model add --hf <repo> [filename.gguf]`");
                println!("        or symlink an existing `.gguf` into `~/models/`.");
            }
            #[cfg(not(feature = "model-mgmt"))]
            {
                println!("  next: place a `.gguf` file or symlink in {},", model_dir.display());
                println!("        then rerun `oz list` or use the installed full base build.");
            }
        } else {
            for r in &records {
                let size = if r.model_size_gb <= 0.0 {
                    "⚠ broken".to_string()
                } else {
                    format!("{:.1} GB", r.model_size_gb)
                };
                println!(
                    "  [{:5}]  {:>8}  {}",
                    r.recommendation.source.label(),
                    size,
                    r.model_name
                );
            }
        }
    }
    Ok(())
}

pub async fn cmd_export_server(
    model: String,
    output: Option<String>,
    port: u16,
) -> Result<()> {
    let model_dir = ozone_core::paths::models_dir();
    let model_path = model_dir.join(&model);
    if !model_path.exists() {
        anyhow::bail!("Model not found: {}", model_path.display());
    }

    let server_path = crate::llamacpp::resolved_llamacpp_server_path()?;

    let plan = {
        let report = crate::catalog::load_catalog_report(
            &model_dir,
            &ozone_core::paths::catalog_preset_path(),
            &model_dir.join("bench-results.txt"),
        )
        .await?;
        let record = report
            .records
            .iter()
            .find(|r| r.model_name == model)
            .ok_or_else(|| anyhow::anyhow!("Model '{}' not found in catalog", model))?;
        crate::launch_config::plan_launch(record, &Default::default())
    };

    let output_path = output.as_deref().map(std::path::PathBuf::from).unwrap_or_default();
    let written = crate::export_server::generate_serve_script(
        &plan, &model_path, &server_path, port, &output_path,
    )?;
    ozone_core::cli::success(&format!("Server script written to {}", written.display()));
    Ok(())
}

#[cfg(feature = "eval")]
pub async fn cmd_eval(
    model: String,
    preset: crate::eval::EvalPreset,
    limit: u32,
    base_url: String,
    temperature: f64,
    compare: bool,
    tokenizer: Option<String>,
) -> Result<()> {
    if compare {
        crate::eval::print_comparison(preset.cli_name())?;
        return Ok(());
    }
    crate::eval::run_eval(&model, preset, limit, &base_url, temperature, tokenizer.as_deref()).await
}

#[cfg(feature = "analyze")]
pub async fn cmd_analyze(
    model: Option<String>,
    all: bool,
    generate: bool,
    profiles: bool,
    export: bool,
) -> Result<()> {
    if export {
        let conf_path = ozone_core::paths::runtime_profiles_path();
        crate::analyze::export_presets_conf(&conf_path, model.as_deref())?;
    } else if profiles {
        crate::analyze::show_profiles(model.as_deref())?;
    } else if generate {
        match &model {
            Some(m) => {
                crate::analyze::generate_profiles(m)?;
                crate::analyze::show_profiles(Some(m))?;
            }
            None => {
                ozone_core::cli::error("--generate requires a model name.");
                std::process::exit(1);
            }
        }
    } else if let Some(ref m) = model {
        let count = crate::analyze::show_benchmarks(Some(m))?;
        if count > 0 {
            crate::analyze::show_pareto(m)?;
        }
    } else {
        let _ = all;
        crate::analyze::show_benchmarks(None)?;
    }
    Ok(())
}
