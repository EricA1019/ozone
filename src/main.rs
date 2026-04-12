mod catalog;
mod hardware;
mod planner;
mod prefs;
mod processes;
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
    }
}
