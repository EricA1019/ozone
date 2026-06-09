#[cfg(feature = "analyze")]
mod analyze;
#[cfg(feature = "bench")]
mod bench;
mod catalog;
mod creative_writing;
mod eval;
mod eval_report;
mod export_server;
#[cfg(any(feature = "bench", feature = "analyze", feature = "profiling-ui"))]
mod db;
#[cfg(any(feature = "profiling-ui", feature = "sweep"))]
mod gguf;
mod hardware;
mod llama;
#[cfg(feature = "model-mgmt")]
mod model;
mod planner;
mod prefs;
mod processes;
#[cfg(feature = "profiling-ui")]
mod profiling;
#[cfg(feature = "sweep")]
mod sweep;
mod theme;
#[cfg(test)]
mod test_support;
mod ui;

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
    after_help = "Source builds keep default features empty. Use `cargo build --release -p ozone --features full` or `./contrib/sync-local-install.sh` for profiling and `oz model ...` commands in the base binary.",
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
    /// Clear GPU backends (KoboldCpp, llama.cpp, Ollama)
    Clear,
    /// Stop the managed llama.cpp model and clear its tracked launch state
    PurgeLastModel,
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
        #[arg(long, default_value = "1", help = "KV cache quantization level")]
        quant_kv: u8,
        #[arg(long, help = "CPU threads (auto if omitted)")]
        threads: Option<u32>,
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
        #[arg(long, default_value = "1", help = "KV cache quantization: 1=f16, 2=q8_0, 3=q4_0")]
        quant_kv: u8,
    },
    /// Run evaluation probes against a running local server
    Eval {
        /// Model filename reported by the local API
        model: String,
        #[arg(long, value_enum, default_value = "gsm8k", help = "Evaluation preset to run")]
        preset: eval::EvalPreset,
        #[arg(long, default_value = "1", help = "Number of samples/examples to run")]
        limit: u32,
        #[arg(
            long,
            default_value = "http://127.0.0.1:8989",
            help = "Base URL for OpenAI-compatible local API"
        )]
        base_url: String,
        #[arg(long, default_value = "0.0", help = "Temperature for generation (0.0 = deterministic)")]
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
        #[arg(long, default_value = "http://127.0.0.1:8989", help = "Base URL for OpenAI-compatible local API")]
        base_url: String,
        #[arg(long, default_value = "contrib/evals/prompts/creative_writing.toml", help = "Path to prompt bank TOML")]
        prompts: Option<String>,
    },
    /// Manage local model files (list, add, remove, info)
    #[cfg(feature = "model-mgmt")]
    Model {
        #[command(subcommand)]
        command: model::ModelCommand,
    },
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
        Some(Commands::Monitor) => ui::run_monitor().await,
        Some(Commands::List { json }) => {
            let model_dir = ozone_core::paths::models_dir();
            let preset_file = ozone_core::paths::catalog_preset_path();
            let bench_file = model_dir.join("bench-results.txt");
            let report = catalog::load_catalog_report(&model_dir, &preset_file, &bench_file)
                .await?;
            for issue in &report.issues {
                eprintln!("catalog {}: {}", issue.level.label(), issue.message);
            }
            let records = report.records;
            if json {
                println!("[");
                for (i, r) in records.iter().enumerate() {
                    let comma = if i + 1 < records.len() { "," } else { "" };
                    println!(
                        "  {{\"model\": \"{}\", \"size_gb\": {}, \"source\": \"{}\"}}{comma}",
                        r.model_name,
                        r.model_size_gb,
                        r.recommendation.source.label()
                    );
                }
                println!("]");
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
                    eprintln!(
                        "        or build `cargo build --release -p ozone --features full`."
                    );
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
                        println!("  next: place a `.gguf` file or symlink in `~/models/`,");
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
        #[cfg(feature = "bench")]
        Some(Commands::Bench {
            model,
            gpu_layers,
            context,
            quant_kv,
            threads,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let server_path = processes::resolved_llamacpp_server_path()?;
            let backend = bench::BenchBackend::LlamaCpp { server_path };

            if !model_path.exists() {
                ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
                std::process::exit(1);
            }

            // Get model size for storage
            let model_size_gb = std::fs::metadata(&model_path)
                .map(|m| m.len() as f64 / 1_073_741_824.0)
                .unwrap_or(0.0);

            ozone_core::cli::header("oz Bench");
            ozone_core::cli::field("Model:", &model);
            ozone_core::cli::field("GPU Layers:", &gpu_layers);
            ozone_core::cli::field("Context:", &context);
            ozone_core::cli::field("Quant KV:", &quant_kv);
            if let Some(t) = threads {
                ozone_core::cli::field("Threads:", &t);
            }
            ozone_core::cli::spacer();

            let result = bench::run_benchmark(
                &model,
                &model_path,
                &backend,
                gpu_layers,
                context,
                quant_kv,
                threads,
            )
            .await?;

            bench::print_result(&model, gpu_layers, context, quant_kv, &result);

            // Store result
            let thread_count = threads.unwrap_or(0);
            match bench::store_result(
                bench::BenchmarkStoreRequest {
                    model_name: &model,
                    model_size_gb,
                    gpu_layers,
                    context_size: context,
                    quant_kv: quant_kv as u32,
                    threads: thread_count,
                    launch_profile_name: None,
                },
                &result,
            ) {
                Ok(id) => ozone_core::cli::success(&format!("Stored as benchmark #{id}")),
                Err(e) => ozone_core::cli::warn(&format!("Failed to store result: {e}")),
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
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let server_path = processes::resolved_llamacpp_server_path()?;

            if context_sweep {
                let (csv_path, sweet_spot) = sweep::run_context_sweep(
                    &model, &model_path, &server_path, -1, quant_kv, None, quick,
                ).await?;
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
                (vec![4096, 8192], vec![1u8])
            } else {
                let mut ctxs = vec![2048, 4096, 8192, 16384];
                if let Some(max) = max_context {
                    ctxs.retain(|&c| c <= max);
                }
                (ctxs, vec![1u8, 2])
            };

            let sweep_config = sweep::SweepConfig {
                model_name: model,
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

            sweep::run_sweep(sweep_config).await?;
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
        Some(Commands::ExportServer { model, output, port }) => {
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
                ).await?;
                let record = report.records.iter()
                    .find(|r| r.model_name == model)
                    .ok_or_else(|| anyhow::anyhow!("Model '{}' not found in catalog", model))?;
                crate::planner::plan_launch(record, &Default::default())
            };

            let output_path = output.as_deref().map(PathBuf::from).unwrap_or_default();
            let written = export_server::generate_serve_script(
                &plan, &model_path, &server_path, port, &output_path,
            )?;
            ozone_core::cli::success(&format!("Server script written to {}", written.display()));
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
                println!("{:<20} {:<50} {}", task.cli_name, task.description, kind_label);
            }
            Ok(())
        }
        Some(Commands::CreativeWrite { model, base_url, prompts: _prompts }) => {
            let root = crate::eval::resolve_project_root()?;
            let prompt_bank = creative_writing::load_prompt_bank(&root)?;
            if prompt_bank.is_empty() {
                anyhow::bail!("No prompts found in creative writing prompt bank");
            }

            let artifacts_dir = root.join("contrib/evals/artifacts").join("creative_writing");
            let csv_path = creative_writing::run_creative_writing_eval(
                &model, &prompt_bank, &base_url, &artifacts_dir,
            ).await?;

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
