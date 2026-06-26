#[cfg(feature = "analyze")]
mod analyze;
mod artifacts;
#[cfg(feature = "bench")]
mod bench;
mod calibration;
mod catalog;
mod creative_writing;
#[cfg(any(feature = "bench", feature = "analyze", feature = "profiling-ui"))]
mod csv_export;
#[cfg(any(feature = "bench", feature = "analyze", feature = "profiling-ui"))]
mod db;
mod eval;
mod eval_report;
mod eval_types;
mod export_server;
mod gate;
#[cfg(any(feature = "profiling-ui", feature = "sweep"))]
mod gguf;
mod hardware;
mod hash;
mod llama;
#[cfg(feature = "model-mgmt")]
mod model;
mod planner;
mod policy;
mod preflight;
mod prefs;
mod processes;
#[cfg(feature = "profiling-ui")]
mod profiling;
mod runner;
mod scorers;
mod suites;
#[cfg(feature = "sweep")]
mod sweep;
#[cfg(test)]
mod test_support;
mod theme;
mod timeout;
mod ui;
mod warmup;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Product tier for mode selection
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TierArg {
    Lite,
    Base,
}

impl From<TierArg> for prefs::Tier {
    fn from(arg: TierArg) -> Self {
        match arg {
            TierArg::Lite => prefs::Tier::Lite,
            TierArg::Base => prefs::Tier::Base,
        }
    }
}

fn detect_tier_from_binary_name(name: &str) -> Option<prefs::Tier> {
    if name == "oz" {
        Some(prefs::Tier::Base)
    } else {
        None
    }
}

#[derive(Parser)]
#[command(
    name = "oz",
    about = "⬡ oz — local AI stack operator & launcher",
    after_help = "Ozone is focused on local model launch, profiling, benchmarking, and capability evaluation. The old chat shell is deprecated and archived.",
    version = concat!(env!("CARGO_PKG_VERSION"), "+", env!("OZONE_GIT_HASH"))
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, help = "Skip browser launch")]
    no_browser: bool,

    /// Override product tier (lite, base).
    /// Also detectable via binary name (e.g. `oz`).
    #[arg(long, value_enum)]
    mode: Option<TierArg>,

    /// Force the tier picker to appear, ignoring saved preference.
    #[arg(long)]
    pick: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List available models
    List {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    /// Stop the managed llama.cpp backend
    Clear,
    /// Stop the managed llama.cpp model and clear its tracked launch state
    PurgeLastModel,
    /// Capture and save system hardware specs for offline reuse
    ImportSpecs,
    /// Live monitor dashboard
    Monitor,
    /// Benchmark a model with specific settings
    #[cfg(feature = "bench")]
    Bench {
        /// Model filename (e.g. mn-12b-mag-mell-r1.gguf)
        model: String,
        #[arg(
            long,
            default_value = "-1",
            allow_hyphen_values = true,
            help = "GPU layers (-1 = all)"
        )]
        gpu_layers: i32,
        #[arg(long, default_value = "4096", help = "Context size")]
        context: u32,
        #[arg(
            long,
            default_value = "1",
            help = "K-cache quantization: 1=f16, 2=q8_0, 3=q4_0"
        )]
        quant_k: u8,
        #[arg(
            long,
            default_value = "1",
            help = "V-cache quantization: 1=f16, 2=q8_0, 3=q4_0 (defaults to quant-k)"
        )]
        quant_v: Option<u8>,
        #[arg(
            long,
            default_value = "1",
            help = "Shorthand to set both K and V cache quantization at once"
        )]
        quant_kv: Option<u8>,
        #[arg(long, help = "CPU threads (auto if omitted)")]
        threads: Option<u32>,
        #[arg(
            long,
            help = "Save the tested config as a named profile in launcher prefs"
        )]
        save_profile: Option<String>,
    },
    /// Analyze benchmark results and generate profiles
    #[cfg(feature = "analyze")]
    Analyze {
        /// Model name (omit for summary of all models)
        model: Option<String>,
        #[arg(long, help = "Show all models")]
        all: bool,
        #[arg(long, help = "Generate/update profiles from benchmarks")]
        generate: bool,
        #[arg(long, help = "Show stored profiles")]
        profiles: bool,
        #[arg(long, help = "Export the recommended runtime profile set")]
        export: bool,
    },
    /// Smart parameter sweep to find optimal settings
    #[cfg(feature = "sweep")]
    Sweep {
        /// Model filename
        model: String,
        #[arg(long, help = "Max context size to test")]
        max_context: Option<u32>,
        #[arg(long, help = "Quick sweep (fewer configs)")]
        quick: bool,
        #[arg(long, help = "Run context-size sweep instead of parameter sweep")]
        context_sweep: bool,
        #[arg(
            long,
            default_value = "1",
            help = "KV cache quantization: 1=f16, 2=q8_0, 3=q4_0 (sets both K and V)"
        )]
        quant_kv: u8,
        #[arg(
            long,
            help = "When set, sweep across multiple KV cache quant levels (1,2,3) per context"
        )]
        sweep_quant: bool,
    },
    /// Sweep thread counts to find the optimal setting for a model
    #[cfg(feature = "bench")]
    ThreadSweep {
        /// Model filename
        model: String,
        #[arg(
            long,
            default_value = "-1",
            allow_hyphen_values = true,
            help = "GPU layers (-1 = all)"
        )]
        gpu_layers: i32,
        #[arg(long, default_value = "4096", help = "Context size")]
        context: u32,
        #[arg(long, default_value = "1", help = "K-cache quantization")]
        quant_k: u8,
        #[arg(long, default_value = "1", help = "V-cache quantization")]
        quant_v: u8,
        #[arg(long, help = "Sweep batch threads instead of main threads")]
        batch: bool,
    },
    /// Run evaluation probes against a running local server
    Eval {
        /// Model filename reported by the local API
        model: String,
        #[arg(
            long,
            value_enum,
            default_value = "gsm8k",
            help = "Evaluation preset to run"
        )]
        preset: eval::EvalPreset,
        #[arg(long, default_value = "1", help = "Number of samples/examples to run")]
        limit: u32,
        #[arg(
            long,
            default_value = "http://127.0.0.1:8989",
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(
            long,
            default_value = "0.0",
            help = "Temperature for generation (0.0 = deterministic)"
        )]
        temperature: f64,
        #[arg(long, help = "Compare all models with prior results for this preset")]
        compare: bool,
    },
    /// Generate a standalone launch script for a model
    ExportServer {
        /// Model filename
        model: String,
        #[arg(long, help = "Output path (default: ~/models/serve-<model>.sh)")]
        output: Option<String>,
        #[arg(long, default_value = "8989", help = "Port for the server")]
        port: u16,
    },
    /// List available evaluation presets
    EvalList,
    /// Run creative writing evaluation probe (multi-temperature diversity scoring)
    CreativeWrite {
        /// Model filename
        model: String,
        #[arg(
            long,
            default_value = "http://127.0.0.1:8989",
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(
            long,
            default_value = "contrib/evals/prompts/creative_writing.toml",
            help = "Path to prompt bank TOML"
        )]
        prompts: Option<String>,
    },
    /// Manage local model files (list, add, remove, info)
    #[cfg(feature = "model-mgmt")]
    Model {
        #[command(subcommand)]
        command: model::ModelCommand,
    },
    /// Run the native eval pipeline (warmup, calibration, gates, suites)
    EvalRun {
        /// Model file path (for hashing)
        model_path: String,
        #[arg(long, default_value = "llama.cpp", help = "Backend type")]
        backend: String,
        #[arg(
            long,
            default_value = "http://127.0.0.1:8989",
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(long, default_value_t = 16384, help = "Configured context length")]
        context_length: u32,
        #[arg(long, help = "Skip warm-up phase")]
        skip_warmup: bool,
        #[arg(long, help = "Skip health gate (force run suites)")]
        skip_health_gate: bool,
    },
    /// List saved launch profiles from preferences
    Profiles,
}

#[tokio::main]
async fn main() -> Result<()> {
    if ozone_core::install::maybe_prompt_for_local_install_update("oz")? {
        ozone_core::install::relaunch_current_process()?;
    }

    let cli = Cli::parse();

    // Determine tier from --mode, argv[0], or saved preference
    let tier_override = cli.mode.map(prefs::Tier::from).or_else(|| {
        std::env::args().next().and_then(|arg0| {
            let name = std::path::Path::new(&arg0)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            detect_tier_from_binary_name(name)
        })
    });

    match cli.command {
        None => ui::run_launcher(cli.no_browser, tier_override, cli.pick).await,
        Some(Commands::Clear) => {
            let killed = processes::clear_gpu_backends().await?;
            if killed.is_empty() {
                ozone_core::cli::info("No GPU backends running.");
            } else {
                for k in &killed {
                    ozone_core::cli::success(&format!("Stopped: {k}"));
                }
            }
            Ok(())
        }
        Some(Commands::PurgeLastModel) => {
            let killed = processes::purge_last_model().await?;
            if killed.is_empty() {
                ozone_core::cli::info("No managed llama.cpp model was running.");
            } else {
                for pid in killed {
                    ozone_core::cli::success(&format!("Stopped managed llama.cpp pid {pid}"));
                }
            }
            Ok(())
        }
        Some(Commands::ImportSpecs) => {
            ozone_core::cli::header("Import System Specs");
            ozone_core::cli::info("Capturing GPU, CPU, RAM, and CUDA info…");
            let profile = hardware::import_system_specs();
            if let Some(ref name) = profile.gpu_name {
                ozone_core::cli::field("GPU:", name);
            }
            if let Some(ref gpu) = profile.gpu {
                ozone_core::cli::field("VRAM:", &format!("{} MB", gpu.total_mb));
            }
            ozone_core::cli::field(
                "CUDA:",
                &if profile.cuda_available {
                    format!("✓ v{}", profile.cuda_version.as_deref().unwrap_or("?"))
                } else {
                    "✗".to_string()
                },
            );
            if let Some(ref cap) = profile.compute_capability {
                ozone_core::cli::field("Compute Cap:", cap);
            }
            ozone_core::cli::field(
                "Flash Attn:",
                &if profile.flash_attn_supported {
                    "✓".to_string()
                } else {
                    "✗".to_string()
                },
            );
            ozone_core::cli::field(
                "CPU:",
                &format!(
                    "{} logical / {} physical",
                    profile.cpu_logical, profile.cpu_physical
                ),
            );
            ozone_core::cli::field("RAM:", &format!("{} MB total", profile.ram_total_mb));
            ozone_core::cli::success("Saved to system-profile.json");
            Ok(())
        }
        Some(Commands::Monitor) => ui::run_monitor().await,
        Some(Commands::List { json }) => {
            let model_dir = ozone_core::paths::models_dir();
            let preset_file = ozone_core::paths::catalog_preset_path();
            let bench_file = model_dir.join("bench-results.txt");
            let report =
                catalog::load_catalog_report(&model_dir, &preset_file, &bench_file).await?;
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
                    eprintln!(
                        "        for `oz model ...`, install via `./contrib/sync-local-install.sh`"
                    );
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
                        println!(
                            "  next: place a `.gguf` file or symlink in {},",
                            model_dir.display()
                        );
                        println!(
                            "        then rerun `oz list` or use the installed full base build."
                        );
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
        #[cfg(feature = "bench")]
        Some(Commands::Bench {
            model,
            gpu_layers,
            context,
            quant_k,
            quant_v,
            quant_kv,
            threads,
            save_profile,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let server_path = processes::resolved_llamacpp_server_path()?;
            let backend = bench::BenchBackend::LlamaCpp { server_path };

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

            let result = bench::run_benchmark(bench::BenchmarkRunRequest {
                model_name: &model,
                model_path: &model_path,
                backend: &backend,
                gpu_layers,
                context_size: context,
                quant_k: effective_k,
                quant_v: effective_v,
                threads,
                mode: bench::BenchMode::Precise,
            })
            .await?;

            bench::print_result(
                &model,
                gpu_layers,
                context,
                effective_k,
                effective_v,
                &result,
            );

            // Store result
            let thread_count = threads.unwrap_or(0);
            match bench::store_result(
                bench::BenchmarkStoreRequest {
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
        #[cfg(feature = "sweep")]
        Some(Commands::Sweep {
            model,
            max_context,
            quick,
            context_sweep,
            quant_kv,
            sweep_quant,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let server_path = processes::resolved_llamacpp_server_path()?;

            if context_sweep {
                if sweep_quant {
                    // Test each quant level (1=f16, 2=q8_0, 3=q4_0) at each context
                    for &qkv in &[1u8, 2u8, 3u8] {
                        eprintln!("\n  --- Sweep with quant_k={qkv} quant_v={qkv} ---");
                        let _ = sweep::run_context_sweep(sweep::ContextSweepRequest {
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
                let (csv_path, sweet_spot) = sweep::run_context_sweep(sweep::ContextSweepRequest {
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

            let hw = hardware::load_hardware();
            let gpu_vram_budget_mb = hw
                .gpu
                .as_ref()
                .map(|g| (g.total_mb as f64 * 0.9) as u32)
                .unwrap_or(0);

            let (context_sizes, quant_kv_levels) = if quick {
                (vec![4096, 8192], vec![(1u8, 1u8)])
            } else {
                // Read the model's native max context from GGUF metadata
                let native_max = gguf::read_context_length(&model_path).unwrap_or(65536);
                let max = max_context.unwrap_or(native_max).min(native_max);
                let ctxs = sweep::generate_context_steps(max);
                (ctxs, vec![(1u8, 1u8), (2u8, 2u8)])
            };

            let sweep_config = sweep::SweepConfig {
                model_name: model.clone(),
                model_path: model_path.clone(),
                backend: bench::BenchBackend::LlamaCpp { server_path },
                model_size_gb,
                total_layers: gguf::inspect_model_topology(
                    &model_path,
                    planner::estimate_total_layers(model_size_gb),
                )
                .total_layers,
                context_sizes,
                quant_kv_levels,
                gpu_vram_budget_mb,
            };

            let result = sweep::run_sweep(sweep_config).await?;

            // Auto-save the optimal profile for quick loading
            if let Some(optimal) =
                sweep::pick_optimal_profile(&model, &result.pareto_frontier, None)
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
        #[cfg(feature = "bench")]
        Some(Commands::ThreadSweep {
            model,
            gpu_layers,
            context,
            quant_k,
            quant_v,
            batch,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let server_path = processes::resolved_llamacpp_server_path()?;
            let backend = bench::BenchBackend::LlamaCpp { server_path };

            if !model_path.exists() {
                ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
                std::process::exit(1);
            }

            if batch {
                ozone_core::cli::header("oz Batch Thread Sweep");
                ozone_core::cli::field("Model:", &model);
                ozone_core::cli::field("Context:", &context);
                ozone_core::cli::spacer();

                let results = bench::run_batch_thread_sweep(bench::BatchThreadSweepRequest {
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
                bench::print_thread_sweep_summary(&results);
            } else {
                ozone_core::cli::header("oz Thread Sweep");
                ozone_core::cli::field("Model:", &model);
                ozone_core::cli::field("Context:", &context);
                ozone_core::cli::field("Quant K:", &quant_k);
                ozone_core::cli::field("Quant V:", &quant_v);
                ozone_core::cli::spacer();

                let results = bench::run_thread_sweep(
                    &model,
                    &model_path,
                    &backend,
                    gpu_layers,
                    context,
                    quant_k,
                    quant_v,
                )
                .await?;
                bench::print_thread_sweep_summary(&results);
            }
            Ok(())
        }
        #[cfg(feature = "analyze")]
        Some(Commands::Analyze {
            model,
            all,
            generate,
            profiles,
            export,
        }) => {
            if export {
                let conf_path = ozone_core::paths::runtime_profiles_path();
                analyze::export_presets_conf(&conf_path, model.as_deref())?;
            } else if profiles {
                analyze::show_profiles(model.as_deref())?;
            } else if generate {
                match &model {
                    Some(m) => {
                        analyze::generate_profiles(m)?;
                        analyze::show_profiles(Some(m))?;
                    }
                    None => {
                        ozone_core::cli::error("--generate requires a model name.");
                        std::process::exit(1);
                    }
                }
            } else if let Some(ref m) = model {
                let count = analyze::show_benchmarks(Some(m))?;
                if count > 0 {
                    analyze::show_pareto(m)?;
                }
            } else {
                let _ = all;
                analyze::show_benchmarks(None)?;
            }
            Ok(())
        }
        Some(Commands::Eval {
            model,
            preset,
            limit,
            base_url,
            temperature,
            compare,
        }) => {
            if compare {
                eval::print_comparison(preset.cli_name())?;
                return Ok(());
            }
            eval::run_eval(&model, preset, limit, &base_url, temperature).await?;
            Ok(())
        }
        Some(Commands::ExportServer {
            model,
            output,
            port,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            if !model_path.exists() {
                anyhow::bail!("Model not found: {}", model_path.display());
            }

            let server_path = processes::resolved_llamacpp_server_path()?;

            // Use catalog recommendation as the launch plan
            let plan = {
                let report = catalog::load_catalog_report(
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
                crate::planner::plan_launch(record, &Default::default())
            };

            let output_path = output.as_deref().map(PathBuf::from).unwrap_or_default();
            let written = export_server::generate_serve_script(
                &plan,
                &model_path,
                &server_path,
                port,
                &output_path,
            )?;
            ozone_core::cli::success(&format!("Server script written to {}", written.display()));
            Ok(())
        }
        Some(Commands::EvalRun {
            model_path,
            backend,
            base_url,
            context_length,
            skip_warmup,
            skip_health_gate,
        }) => {
            use crate::runner::EvalRunConfig;
            let config = EvalRunConfig {
                model_name: std::path::Path::new(&model_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                model_path,
                backend,
                base_url,
                context_length,
                skip_warmup,
                skip_health_gate,
                ..Default::default()
            };
            let result = runner::run_eval(&config).await?;
            println!(
                "Status: {} ({}/{} passed in {:.1}s)",
                result.status,
                result.tasks_passed,
                result.tasks_run,
                result.total_duration_ms as f64 / 1000.0
            );
            Ok(())
        }
        Some(Commands::EvalList) => {
            println!("{:<20} {:<50} KIND", "NAME", "DESCRIPTION");
            for task in eval::EVAL_TASKS {
                let kind_label = match task.kind {
                    eval::EvalTaskKind::LmEval { .. } => "lm-eval",
                    eval::EvalTaskKind::EvalPlus { .. } => "evalplus",
                    eval::EvalTaskKind::CreativeWriting => "creative-writing",
                };
                println!(
                    "{:<20} {:<50} {}",
                    task.cli_name, task.description, kind_label
                );
            }
            Ok(())
        }
        Some(Commands::CreativeWrite {
            model,
            base_url,
            prompts: _prompts,
        }) => {
            let root = crate::eval::resolve_project_root()?;
            let prompt_bank = creative_writing::load_prompt_bank(&root)?;
            if prompt_bank.is_empty() {
                anyhow::bail!("No prompts found in creative writing prompt bank");
            }

            let artifacts_dir = root
                .join("contrib/evals/artifacts")
                .join("creative_writing");
            let csv_path = creative_writing::run_creative_writing_eval(
                &model,
                &prompt_bank,
                &base_url,
                &artifacts_dir,
            )
            .await?;

            // Build and write markdown report
            let report_md = creative_writing::build_creative_report(&csv_path)?;
            let report_path = csv_path.with_extension("md");
            std::fs::write(&report_path, &report_md)?;

            ozone_core::cli::success(&format!("Creative writing eval complete for '{}'", model));
            ozone_core::cli::field("CSV:", &csv_path.display());
            ozone_core::cli::field("Report:", &report_path.display());
            Ok(())
        }
        #[cfg(feature = "model-mgmt")]
        Some(Commands::Model { command }) => match model::run(command).await {
            Ok(()) => Ok(()),
            Err(e) => {
                ozone_core::cli::error(&format!("{e}"));
                std::process::exit(1);
            }
        },
        Some(Commands::Profiles) => {
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
                            p.threads
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "auto".into()),
                            default_marker,
                        );
                    }
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tier_from_binary_name() {
        assert_eq!(detect_tier_from_binary_name("oz"), Some(prefs::Tier::Base));
        assert_eq!(detect_tier_from_binary_name("ozone"), None);
        assert_eq!(detect_tier_from_binary_name("ozone-lite"), None);
        assert_eq!(detect_tier_from_binary_name("oz+"), None);
    }
}
