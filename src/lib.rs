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
    #[cfg(feature = "eval")]
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
    #[cfg(feature = "eval")]
    EvalList,
    /// Run creative writing evaluation probe (multi-temperature diversity scoring)
    #[cfg(feature = "eval")]
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
    #[cfg(feature = "eval")]
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
        }) => commands::cmd_bench(model, gpu_layers, context, quant_k, quant_v, quant_kv, threads, save_profile).await,
        #[cfg(feature = "sweep")]
        Some(Commands::Sweep {
            model,
            max_context,
            quick,
            context_sweep,
            quant_kv,
            sweep_quant,
        }) => commands::cmd_sweep(model, max_context, quick, context_sweep, quant_kv, sweep_quant).await,
        #[cfg(feature = "bench")]
        Some(Commands::ThreadSweep {
            model,
            gpu_layers,
            context,
            quant_k,
            quant_v,
            batch,
        }) => commands::cmd_thread_sweep(model, gpu_layers, context, quant_k, quant_v, batch).await,
        #[cfg(feature = "analyze")]
        Some(Commands::Analyze { model, all, generate, profiles, export }) => commands::cmd_analyze(model, all, generate, profiles, export).await,
        #[cfg(feature = "eval")]
        Some(Commands::Eval { model, preset, limit, base_url, temperature, compare, tokenizer }) => commands::cmd_eval(model, preset, limit, base_url, temperature, compare, tokenizer).await,
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
        }) => commands::cmd_eval_run(
            model_path, backend, base_url, context_length,
            skip_warmup, skip_health_gate,
            quick, standard,
            attempts, gate_attempts,
            no_manage_server, allow_below_min_context,
            gpu_layers, threads, cli_server_path,
            cache_type_k, cache_type_v, cache_type_kv,
            flash_attn, no_thinking,
        ).await,
        #[cfg(feature = "eval")]
        Some(Commands::EvalList) => commands::cmd_eval_list().await,
        #[cfg(feature = "eval")]
        Some(Commands::CreativeWrite {
            model,
            base_url,
            prompts: _prompts,
        }) => commands::cmd_creative_write(model, base_url).await,
        #[cfg(feature = "model-mgmt")]
        Some(Commands::Model { command }) => commands::cmd_model(command).await,
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
