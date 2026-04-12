mod analyze;
mod bench;
mod catalog;
mod db;
mod hardware;
mod planner;
mod prefs;
mod processes;
mod sweep;
mod theme;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ozone", about = "⬡ Ozone — local AI stack operator", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, help = "Skip browser launch")]
    no_browser: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List available models
    List {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    /// Clear GPU backends (KoboldCpp, Ollama)
    Clear,
    /// Live monitor dashboard
    Monitor,
    /// Benchmark a model with specific settings
    Bench {
        /// Model filename (e.g. mn-12b-mag-mell-r1.gguf)
        model: String,
        #[arg(long, default_value = "-1", allow_hyphen_values = true, help = "GPU layers (-1 = all)")]
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => ui::run_launcher(cli.no_browser).await,
        Some(Commands::Clear) => {
            let killed = processes::clear_gpu_backends().await?;
            if killed.is_empty() {
                println!("No GPU backends running.");
            } else {
                for k in &killed { println!("  Stopped: {k}"); }
            }
            Ok(())
        }
        Some(Commands::Monitor) => ui::run_monitor().await,
        Some(Commands::List { json }) => {
            let home = std::env::var("HOME").unwrap_or_default();
            let model_dir = std::path::PathBuf::from(&home).join("models");
            let preset_file = model_dir.join("koboldcpp-presets.conf");
            let bench_file = model_dir.join("bench-results.txt");
            let records = catalog::load_catalog(&model_dir, &preset_file, &bench_file).await.unwrap_or_default();
            if json {
                println!("[");
                for (i, r) in records.iter().enumerate() {
                    let comma = if i + 1 < records.len() { "," } else { "" };
                    println!("  {{\"model\": \"{}\", \"size_gb\": {}, \"source\": \"{}\"}}{comma}",
                        r.model_name, r.model_size_gb, r.recommendation.source.label());
                }
                println!("]");
            } else {
                for r in &records {
                    println!("  [{:5}]  {:.1} GB  {}", r.recommendation.source.label(), r.model_size_gb, r.model_name);
                }
            }
            Ok(())
        }
        Some(Commands::Bench { model, gpu_layers, context, quant_kv, threads }) => {
            let home = std::env::var("HOME").unwrap_or_default();
            let model_dir = std::path::PathBuf::from(&home).join("models");
            let model_path = model_dir.join(&model);
            let launcher_path = model_dir.join("launch-koboldcpp.sh");

            if !model_path.exists() {
                eprintln!("Model not found: {}", model_path.display());
                std::process::exit(1);
            }

            // Get model size for storage
            let model_size_gb = std::fs::metadata(&model_path)
                .map(|m| m.len() as f64 / 1_073_741_824.0)
                .unwrap_or(0.0);

            println!();
            println!("  ⬡ Ozone Bench");
            println!("  ─────────────────────────────────────────────────");
            println!("  Model:       {model}");
            println!("  GPU Layers:  {gpu_layers}");
            println!("  Context:     {context}");
            println!("  Quant KV:    {quant_kv}");
            if let Some(t) = threads {
                println!("  Threads:     {t}");
            }
            println!();

            let result = bench::run_benchmark(
                &model, &model_path, &launcher_path,
                gpu_layers, context, quant_kv, threads,
            ).await?;

            bench::print_result(&model, gpu_layers, context, quant_kv, &result);

            // Store result
            let thread_count = threads.unwrap_or(0);
            match bench::store_result(&model, model_size_gb, gpu_layers, context, quant_kv as u32, thread_count, &result) {
                Ok(id) => eprintln!("  Stored as benchmark #{id}"),
                Err(e) => eprintln!("  Warning: failed to store result: {e}"),
            }
            Ok(())
        }
        Some(Commands::Sweep { model, max_context, quick }) => {
            let home = std::env::var("HOME").unwrap_or_default();
            let model_dir = std::path::PathBuf::from(&home).join("models");
            let model_path = model_dir.join(&model);
            let launcher_path = model_dir.join("launch-koboldcpp.sh");

            if !model_path.exists() {
                eprintln!("Model not found: {}", model_path.display());
                std::process::exit(1);
            }

            let model_size_gb = std::fs::metadata(&model_path)
                .map(|m| m.len() as f64 / 1_073_741_824.0)
                .unwrap_or(0.0);

            let hw = hardware::load_hardware();
            let gpu_vram_budget_mb = hw.gpu.as_ref()
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
                model_path,
                launcher_path,
                model_size_gb,
                context_sizes,
                quant_kv_levels,
                gpu_vram_budget_mb,
                ram_total_mb,
            };

            sweep::run_sweep(sweep_config).await?;
            Ok(())
        }
        Some(Commands::Analyze { model, all, generate, profiles }) => {
            if profiles {
                analyze::show_profiles(model.as_deref())?;
            } else if generate {
                match &model {
                    Some(m) => {
                        analyze::generate_profiles(m)?;
                        analyze::show_profiles(Some(m))?;
                    }
                    None => {
                        eprintln!("  --generate requires a model name.");
                        std::process::exit(1);
                    }
                }
            } else if let Some(ref m) = model {
                analyze::show_benchmarks(Some(m))?;
                analyze::show_pareto(m)?;
            } else {
                // --all or no model
                let _ = all; // acknowledged
                analyze::show_benchmarks(None)?;
            }
            Ok(())
        }
    }
}
