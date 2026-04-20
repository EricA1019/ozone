mod analyze;
mod bench;
mod catalog;
mod db;
mod gguf;
mod hardware;
mod llama;
mod model;
mod planner;
mod prefs;
mod processes;
mod profiling;
mod sweep;
mod theme;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

/// Product tier for mode selection
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TierArg {
    Lite,
    Base,
    Plus,
}

impl From<TierArg> for prefs::Tier {
    fn from(arg: TierArg) -> Self {
        match arg {
            TierArg::Lite => prefs::Tier::Lite,
            TierArg::Base => prefs::Tier::Base,
            TierArg::Plus => prefs::Tier::Plus,
        }
    }
}

fn detect_tier_from_binary_name(name: &str) -> Option<prefs::Tier> {
    if name.contains("lite") || name.contains("ozone-lite") || name.contains("ozonelite") {
        Some(prefs::Tier::Lite)
    } else if name == "oz+"
        || name.contains("ozone+")
        || name.contains("ozoneplus")
        || name.contains("plus")
    {
        Some(prefs::Tier::Plus)
    } else {
        None
    }
}

#[derive(Parser)]
#[command(
    name = "ozone",
    about = "⬡ Ozone — local AI stack operator & launcher",
    version = concat!(env!("CARGO_PKG_VERSION"), "+", env!("OZONE_GIT_HASH"))
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, help = "Skip browser launch")]
    no_browser: bool,

    /// Choose the frontend to open after launching the backend.
    /// Omit to see an interactive choice screen.
    #[arg(long, value_name = "MODE")]
    frontend: Option<ui::FrontendMode>,

    /// Override product tier (lite, base, plus).
    /// Also detectable via binary name (ozone-lite, ozone, ozone+).
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
    /// Live monitor dashboard
    Monitor,
    /// Benchmark a model with specific settings
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
    Analyze {
        /// Model name (omit for summary of all models)
        model: Option<String>,
        #[arg(long, help = "Show all models")]
        all: bool,
        #[arg(long, help = "Generate/update profiles from benchmarks")]
        generate: bool,
        #[arg(long, help = "Show stored profiles")]
        profiles: bool,
        #[arg(long, help = "Export profiles to koboldcpp-presets.conf")]
        export: bool,
    },
    /// Smart parameter sweep to find optimal settings
    Sweep {
        /// Model filename
        model: String,
        #[arg(long, help = "Max context size to test")]
        max_context: Option<u32>,
        #[arg(long, help = "Quick sweep (fewer configs)")]
        quick: bool,
    },
    /// Manage local model files (list, add, remove, info)
    Model {
        #[command(subcommand)]
        command: model::ModelCommand,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    if ozone_core::install::maybe_prompt_for_local_install_update("ozone")? {
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
        None => ui::run_launcher(cli.no_browser, cli.frontend, tier_override, cli.pick).await,
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
        Some(Commands::Monitor) => ui::run_monitor().await,
        Some(Commands::List { json }) => {
            if !json {
                eprintln!("  hint: `ozone list` is deprecated — use `ozone model list` instead.");
                eprintln!();
            }
            let model_dir = ozone_core::paths::models_dir();
            let preset_file = ozone_core::paths::presets_path();
            let bench_file = model_dir.join("bench-results.txt");
            let records = catalog::load_catalog(&model_dir, &preset_file, &bench_file)
                .await
                .unwrap_or_default();
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
                println!("  {:<6}  {:>8}  MODEL", "SOURCE", "SIZE");
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
            Ok(())
        }
        Some(Commands::Bench {
            model,
            gpu_layers,
            context,
            quant_kv,
            threads,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let launcher_path = processes::resolved_kobold_launcher_path();
            let backend = bench::BenchBackend::KoboldCpp { launcher_path };

            if !model_path.exists() {
                ozone_core::cli::error(&format!("Model not found: {}", model_path.display()));
                std::process::exit(1);
            }

            // Get model size for storage
            let model_size_gb = std::fs::metadata(&model_path)
                .map(|m| m.len() as f64 / 1_073_741_824.0)
                .unwrap_or(0.0);

            ozone_core::cli::header("Ozone Bench");
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
                &model,
                model_size_gb,
                gpu_layers,
                context,
                quant_kv as u32,
                thread_count,
                &result,
            ) {
                Ok(id) => ozone_core::cli::success(&format!("Stored as benchmark #{id}")),
                Err(e) => ozone_core::cli::warn(&format!("Failed to store result: {e}")),
            }
            Ok(())
        }
        Some(Commands::Sweep {
            model,
            max_context,
            quick,
        }) => {
            let model_dir = ozone_core::paths::models_dir();
            let model_path = model_dir.join(&model);
            let launcher_path = processes::resolved_kobold_launcher_path();

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
            let ram_total_mb = hw.ram_total_mb as u32;

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
                backend: bench::BenchBackend::KoboldCpp { launcher_path },
                model_size_gb,
                total_layers: gguf::inspect_model_topology(
                    &model_path,
                    planner::estimate_total_layers(model_size_gb),
                )
                .total_layers,
                context_sizes,
                quant_kv_levels,
                gpu_vram_budget_mb,
                ram_total_mb,
            };

            sweep::run_sweep(sweep_config).await?;
            Ok(())
        }
        Some(Commands::Analyze {
            model,
            all,
            generate,
            profiles,
            export,
        }) => {
            if export {
                let conf_path = ozone_core::paths::presets_path();
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
        assert_eq!(
            detect_tier_from_binary_name("ozone-lite"),
            Some(prefs::Tier::Lite)
        );
        assert_eq!(
            detect_tier_from_binary_name("ozonelite"),
            Some(prefs::Tier::Lite)
        );
        assert_eq!(
            detect_tier_from_binary_name("ozone+"),
            Some(prefs::Tier::Plus)
        );
        assert_eq!(
            detect_tier_from_binary_name("ozoneplus"),
            Some(prefs::Tier::Plus)
        );
        assert_eq!(detect_tier_from_binary_name("oz+"), Some(prefs::Tier::Plus));
        assert_eq!(detect_tier_from_binary_name("ozone"), None);
    }
}
