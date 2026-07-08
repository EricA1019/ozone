//! # Ozone — local AI stack operator
//!
//! This is the library root for the `ozone` crate. It exposes all internal
//! modules for integration testing and reuse.
//!
//! The binary entry point is in `main.rs`, which simply calls `ozone::run()`.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

#[cfg(feature = "analyze")]
pub mod analyze;
#[cfg(feature = "eval")]
pub mod artifacts;
#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "eval")]
pub mod calibration;
pub mod catalog;
pub mod commands;
#[cfg(feature = "eval")]
pub mod creative_writing;
pub mod disk;
#[cfg(any(feature = "eval", feature = "bench", feature = "analyze", feature = "profiling-ui"))]
pub mod db;
#[cfg(feature = "eval")]
pub mod eval;
#[cfg(feature = "eval")]
pub mod eval_report;
#[cfg(feature = "eval")]
pub mod eval_types;
#[cfg(feature = "eval")]
pub mod eval_result;
pub mod export_server;
#[cfg(feature = "eval")]
pub mod gate;
#[cfg(any(feature = "profiling-ui", feature = "sweep"))]
pub mod gguf;
pub mod hardware;
#[cfg(feature = "eval")]
pub mod hash;
pub mod llama;
#[cfg(feature = "model-mgmt")]
pub mod model;
pub mod launch_config;
#[cfg(feature = "eval")]
pub mod policy;
#[cfg(feature = "eval")]
pub mod preflight;
pub mod prefs;
pub mod processes;
#[cfg(feature = "profiling-ui")]
#[cfg(any(feature = "profiling-ui", feature = "bench", feature = "sweep", feature = "analyze"))]
pub mod profiling_actions;
#[cfg(any(feature = "profiling-ui", feature = "bench", feature = "sweep", feature = "analyze"))]
pub mod profiling;
#[cfg(feature = "eval")]
pub mod runner;
#[cfg(feature = "eval")]
pub mod scorers;
#[cfg(feature = "eval")]
pub mod suites;
#[cfg(feature = "sweep")]
pub mod sweep;
#[cfg(test)]
pub mod test_support;
pub mod theme;
#[cfg(feature = "eval")]
pub mod timeout;
pub mod ui;
#[cfg(feature = "eval")]
pub mod warmup;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

// ---------------------------------------------------------------------------
// Named constants — single source of truth for CLI defaults.
// ---------------------------------------------------------------------------

/// Default llama.cpp server URL (mirrors `ozone_core::paths::DEFAULT_LLAMACPP_BASE_URL`).
const DEFAULT_LLAMACPP_URL: &str = ozone_core::paths::DEFAULT_LLAMACPP_BASE_URL;
/// Default server port as string (for clap `default_value`).
const DEFAULT_LLAMACPP_PORT_STR: &str = "8989";
/// Default context size as string (for clap `default_value`).
const DEFAULT_CONTEXT_SIZE_STR: &str = "4096";
/// Default GPU layers sentinel: -1 means "all layers".
const GPU_LAYERS_AUTO_STR: &str = "-1";
/// Default KV cache quantization: 1=f16, 2=q8_0, 3=q4_0.
const DEFAULT_KV_QUANT_STR: &str = "1";
/// Default benchmark preset.
const DEFAULT_BENCH_PRESET: &str = "gsm8k";
/// Default sample count as string.
const DEFAULT_SAMPLES_STR: &str = "1";
/// Default temperature for generation (0.0 = deterministic).
const DEFAULT_TEMPERATURE_STR: &str = "0.0";
/// Default backend identifier.
const DEFAULT_BACKEND_STR: &str = "llama.cpp";
/// Default creative writing prompt bank path.
const DEFAULT_CREATIVE_PROMPTS_PATH: &str = "contrib/evals/prompts/creative_writing.toml";

// Non-string constants (for `default_value_t` and struct defaults).
/// Default context length in tokens (32k).
const DEFAULT_CONTEXT_SIZE: u32 = 32768;
/// Default GPU layers for server launch.
const DEFAULT_GPU_LAYERS: i32 = 35;

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
pub enum Commands {
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
            default_value = GPU_LAYERS_AUTO_STR,
            allow_hyphen_values = true,
            help = "GPU layers (-1 = all)"
        )]
        gpu_layers: i32,
        #[arg(long, default_value = DEFAULT_CONTEXT_SIZE_STR, help = "Context size")]
        context: u32,
        #[arg(
            long,
            default_value = DEFAULT_KV_QUANT_STR,
            help = "K-cache quantization: 1=f16, 2=q8_0, 3=q4_0"
        )]
        quant_k: u8,
        #[arg(
            long,
            default_value = DEFAULT_KV_QUANT_STR,
            help = "V-cache quantization: 1=f16, 2=q8_0, 3=q4_0 (defaults to quant-k)"
        )]
        quant_v: Option<u8>,
        #[arg(
            long,
            default_value = DEFAULT_KV_QUANT_STR,
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
            default_value = DEFAULT_KV_QUANT_STR,
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
            default_value = GPU_LAYERS_AUTO_STR,
            allow_hyphen_values = true,
            help = "GPU layers (-1 = all)"
        )]
        gpu_layers: i32,
        #[arg(long, default_value = DEFAULT_CONTEXT_SIZE_STR, help = "Context size")]
        context: u32,
        #[arg(long, default_value = DEFAULT_KV_QUANT_STR, help = "K-cache quantization")]
        quant_k: u8,
        #[arg(long, default_value = DEFAULT_KV_QUANT_STR, help = "V-cache quantization")]
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
            default_value = DEFAULT_BENCH_PRESET,
            help = "Evaluation preset to run"
        )]
        preset: eval::EvalPreset,
        #[arg(long, default_value = DEFAULT_SAMPLES_STR, help = "Number of samples/examples to run")]
        limit: u32,
        #[arg(
            long,
            default_value = DEFAULT_LLAMACPP_URL,
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(
            long,
            default_value = DEFAULT_TEMPERATURE_STR,
            help = "Temperature for generation (0.0 = deterministic)"
        )]
        temperature: f64,
        #[arg(long, help = "Compare all models with prior results for this preset")]
        compare: bool,
        #[arg(long, help = "HuggingFace tokenizer for loglikelihood tasks (MMLU, HellaSwag). e.g. Qwen/Qwen2.5-7B-Instruct")]
        tokenizer: Option<String>,
    },
    /// Generate a standalone launch script for a model
    ExportServer {
        /// Model filename
        model: String,
        #[arg(long, help = "Output path (default: ~/models/serve-<model>.sh)")]
        output: Option<String>,
        #[arg(long, default_value = DEFAULT_LLAMACPP_PORT_STR, help = "Port for the server")]
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
            default_value = DEFAULT_LLAMACPP_URL,
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(
            long,
            default_value = DEFAULT_CREATIVE_PROMPTS_PATH,
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
        #[arg(long, default_value = DEFAULT_BACKEND_STR, help = "Backend type")]
        backend: String,
        #[arg(
            long,
            default_value = DEFAULT_LLAMACPP_URL,
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(long, default_value_t = DEFAULT_CONTEXT_SIZE, help = "Configured context length (default 32k)")]
        context_length: u32,
        #[arg(long, help = "Skip warm-up phase")]
        skip_warmup: bool,
        #[arg(long, help = "Skip health gate (force run suites)")]
        skip_health_gate: bool,
        #[arg(long, help = "Quick sweep (health + canary, 1 attempt each)")]
        quick: bool,
        #[arg(long, help = "Standard sweep (health + canary + code_micro, 3 attempts for gates)")]
        standard: bool,
        #[arg(long, help = "Full sweep (all 5 suites, 3 attempts for all tasks)")]
        full: bool,
        #[arg(long, help = "Number of attempts per task (overrides sweep-level default)")]
        attempts: Option<u32>,
        #[arg(long, help = "Number of attempts for gate tasks (overrides sweep-level default)")]
        gate_attempts: Option<u32>,
        #[arg(long, help = "Skip server management — connect to already-running server")]
        no_manage_server: bool,
        #[arg(long, help = "Allow eval below 16k min quality context threshold")]
        allow_below_min_context: bool,
        #[arg(long, default_value_t = DEFAULT_GPU_LAYERS, help = "GPU layers to offload (for server launch)")]
        gpu_layers: i32,
        #[arg(long, help = "CPU threads (auto if omitted)")]
        threads: Option<u32>,
        #[arg(long, help = "Path to llama-server binary (auto-discover if not set)")]
        server_path: Option<String>,
        #[arg(long, help = "Enable flash attention (default on, set --flash-attn off to disable)")]
        flash_attn: Option<bool>,
        #[arg(
            long,
            help = "K-cache quantization: 1=f16, 2=q8_0, 3=q4_0 (default: f16)"
        )]
        cache_type_k: Option<u8>,
        #[arg(
            long,
            help = "V-cache quantization: 1=f16, 2=q8_0, 3=q4_0 (defaults to cache-type-k if omitted)"
        )]
        cache_type_v: Option<u8>,
        #[arg(
            long,
            help = "Shorthand to set both K and V cache quantization at once"
        )]
        cache_type_kv: Option<u8>,
        #[arg(long, help = "Suppress thinking/reasoning output (adds stop tokens + penalties)")]
        no_thinking: bool,
    },
    /// List saved launch profiles from preferences
    Profiles,
}

#[tokio::main]
#[tracing::instrument(skip_all)]
pub async fn run() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
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
        Some(Commands::Clear) => commands::cmd_clear().await,
        Some(Commands::PurgeLastModel) => commands::cmd_purge_last_model().await,
        Some(Commands::ImportSpecs) => commands::cmd_import_specs().await,
        Some(Commands::Monitor) => ui::run_monitor().await,
        Some(Commands::List { json }) => commands::cmd_list(json).await,
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
                    crate::launch_config::estimate_total_layers(model_size_gb),
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
        #[cfg(feature = "eval")]
        Some(Commands::Eval {
            model,
            preset,
            limit,
            base_url,
            temperature,
            compare,
            tokenizer,
        }) => {
            if compare {
                eval::print_comparison(preset.cli_name())?;
                return Ok(());
            }
            eval::run_eval(&model, preset, limit, &base_url, temperature, tokenizer.as_deref()).await?;
            Ok(())
        }
        #[cfg(not(feature = "eval"))]
        Some(Commands::Eval { .. }) => {
            anyhow::bail!("eval command requires the 'eval' feature. Build with --features full or --features eval.")
        }
        Some(Commands::ExportServer { model, output, port }) => commands::cmd_export_server(model, output, port).await,
        #[cfg(feature = "eval")]
        Some(Commands::EvalRun {
            model_path,
            backend,
            base_url,
            context_length,
            skip_warmup,
            skip_health_gate,
            quick,
            standard,
            full: _full,
            attempts,
            gate_attempts,
            no_manage_server,
            allow_below_min_context,
            gpu_layers,
            threads,
            server_path: cli_server_path,
            cache_type_k,
            cache_type_v,
            cache_type_kv,
            flash_attn,
            no_thinking,
        }) => {
            use crate::runner::{EvalRunConfig, SweepLevel};
            let sweep_level = if quick {
                SweepLevel::Quick
            } else if standard {
                SweepLevel::Standard
            } else {
                SweepLevel::Full // default (also when --full is set)
            };
            // Resolve cache type with env var fallback
            let resolve_cache_type = |cli_val: Option<u8>, env_var: &str, default: u8| -> u8 {
                cli_val.unwrap_or_else(|| {
                    std::env::var(env_var)
                        .ok()
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(default)
                })
            };
            let effective_cache_k = resolve_cache_type(cache_type_k, "OZONE_QUANT_K", 1);
            let effective_cache_v = resolve_cache_type(
                cache_type_v.or(cache_type_kv),
                "OZONE_QUANT_V",
                effective_cache_k,
            );

            let mut policy = crate::policy::ContextPolicy::default();
            if allow_below_min_context {
                policy.allow_below_min_context = true;
            }
            let config = if no_manage_server {
                EvalRunConfig {
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
                    policy,
                    sweep_level,
                    gate_attempts: gate_attempts.unwrap_or(0),
                    regular_attempts: attempts.unwrap_or(0),
                    gpu_layers,
                    threads,
                    manage_server: false,
                    server_path: None,
                    cache_type_k: effective_cache_k,
                    cache_type_v: effective_cache_v,
                    flash_attn,
                    no_thinking,
                }
            } else {
                let resolved_server_path = if let Some(ref p) = cli_server_path {
                    std::path::PathBuf::from(p)
                } else {
                    processes::resolved_llamacpp_server_path()?
                };
                EvalRunConfig {
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
                    policy,
                    sweep_level,
                    gate_attempts: gate_attempts.unwrap_or(0),
                    regular_attempts: attempts.unwrap_or(0),
                    gpu_layers,
                    threads,
                    manage_server: true,
                    server_path: Some(resolved_server_path),
                    cache_type_k: effective_cache_k,
                    cache_type_v: effective_cache_v,
                    flash_attn,
                    no_thinking,
                }
            };
            let result = runner::run_eval(&config).await?;
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
        #[cfg(not(feature = "eval"))]
        Some(Commands::EvalRun { .. }) => {
            anyhow::bail!("eval-run requires the 'eval' feature. Build with --features full or --features eval.")
        }
        #[cfg(feature = "eval")]
        Some(Commands::EvalList) => commands::cmd_eval_list().await,
        #[cfg(not(feature = "eval"))]
        Some(Commands::EvalList) => anyhow::bail!("eval-list requires the 'eval' feature."),
        #[cfg(feature = "eval")]
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

            let artifacts_dir = root.join("results").join("creative_writing");
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
        #[cfg(not(feature = "eval"))]
        Some(Commands::CreativeWrite { .. }) => {
            anyhow::bail!("creative-write requires the 'eval' feature. Build with --features full or --features eval.")
        }
        #[cfg(feature = "model-mgmt")]
        Some(Commands::Model { command }) => match model::run(command).await {
            Ok(()) => Ok(()),
            Err(e) => {
                ozone_core::cli::error(&format!("{e}"));
                std::process::exit(1);
            }
        },
        Some(Commands::Profiles) => commands::cmd_profiles().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_tier_oz_binary_maps_to_base() {
        // The \`oz\` binary name is the shorthand alias for the Base tier.
        assert_eq!(detect_tier_from_binary_name("oz"), Some(prefs::Tier::Base));
    }

    #[test]
    fn detect_tier_unrecognized_binary_names_return_none() {
        // Only \`oz\` is recognized. All other names return None.
        assert_eq!(detect_tier_from_binary_name("ozone"), None);
        assert_eq!(detect_tier_from_binary_name("ozone-lite"), None);
        assert_eq!(detect_tier_from_binary_name("oz+"), None);
        assert_eq!(detect_tier_from_binary_name("foo"), None);
        assert_eq!(detect_tier_from_binary_name("ozonelite"), None);
        assert_eq!(detect_tier_from_binary_name(""), None);
    }

    #[test]
    fn detect_tier_is_case_sensitive() {
        // Binary name detection is case-sensitive. "OZ" is not "oz".
        assert_eq!(detect_tier_from_binary_name("OZ"), None);
        assert_eq!(detect_tier_from_binary_name("Oz"), None);
    }

    // -- Pre-snapshot CLI command parsing tests --
    // These verify Cli::parse_from produces the correct command variants.
    // They serve as regression tests for CLI dispatch extraction.

    #[test]
    fn cli_parse_list_command() {
        let cli = Cli::parse_from(["oz", "list"]).command;
        assert!(matches!(cli, Some(Commands::List { .. })));
    }

    #[test]
    fn cli_parse_list_json_flag() {
        let cli = Cli::parse_from(["oz", "list", "--json"]).command;
        match cli {
            Some(Commands::List { json }) => assert!(json),
            _ => panic!("expected List command"),
        }
    }

    #[test]
    fn cli_parse_clear_command() {
        let cli = Cli::parse_from(["oz", "clear"]).command;
        assert!(matches!(cli, Some(Commands::Clear)));
    }

    #[test]
    fn cli_parse_monitor_command() {
        let cli = Cli::parse_from(["oz", "monitor"]).command;
        assert!(matches!(cli, Some(Commands::Monitor)));
    }

    #[test]
    fn cli_parse_eval_command() {
        let cli = Cli::parse_from(["oz", "eval", "model.gguf"]).command;
        match cli {
            Some(Commands::Eval { model, .. }) => assert_eq!(model, "model.gguf"),
            _ => panic!("expected Eval command"),
        }
    }

    #[test]
    fn cli_parse_profiles_command() {
        let cli = Cli::parse_from(["oz", "profiles"]).command;
        assert!(matches!(cli, Some(Commands::Profiles)));
    }

    #[test]
    fn cli_parse_no_command_selects_launcher() {
        let cli = Cli::parse_from(["oz"]).command;
        assert!(cli.is_none(), "no command should leave cli.command as None (launcher mode)");
    }
}
