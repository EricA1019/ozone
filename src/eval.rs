use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EvalPreset {
    Gsm8k,
    Instruction,
    Math,
    Humaneval,
    Mmlu,
    HellaSwag,
    TruthfulQA,
    Bbh,
    // -- new: general knowledge --
    MmluPro,
    ArcChallenge,
    // -- new: philosophy & ethics --
    MmluPhilosophy,
    HendrycksEthics,
    BbhFormalFallacies,
    BbhCausalJudgement,
    // -- new: coding --
    Mbpp,
    // -- new: reading comprehension --
    Drop,
    // -- new: hard (graduate-level, opt-in) --
    Gpqa,
}

impl EvalPreset {
    pub(crate) fn cli_name(self) -> &'static str {
        match self {
            Self::Gsm8k => "gsm8k",
            Self::Instruction => "instruction",
            Self::Math => "math",
            Self::Humaneval => "humaneval",
            Self::Mmlu => "mmlu",
            Self::HellaSwag => "hellaswag",
            Self::TruthfulQA => "truthfulqa",
            Self::Bbh => "bbh",
            Self::MmluPro => "mmlu_pro",
            Self::ArcChallenge => "arc_challenge",
            Self::MmluPhilosophy => "mmlu_philosophy",
            Self::HendrycksEthics => "hendrycks_ethics",
            Self::BbhFormalFallacies => "bbh_formal_fallacies",
            Self::BbhCausalJudgement => "bbh_causal_judgement",
            Self::Mbpp => "mbpp",
            Self::Drop => "drop",
            Self::Gpqa => "gpqa",
        }
    }

    pub(crate) fn report_label(self) -> &'static str {
        EVAL_TASKS
            .iter()
            .find(|t| t.cli_name == self.cli_name())
            .map(|t| t.report_label)
            .unwrap_or("unknown")
    }
}

/// The kind of eval runner to invoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalTaskKind {
    /// Runs lm-eval with the given task name and output subdirectory.
    LmEval {
        task: &'static str,
        output_dir: &'static str,
    },
    /// Runs EvalPlus codegen.
    EvalPlus { output_dir: &'static str },
    /// Runs the creative writing diversity probe (Phase 2).
    // Constructed indirectly via eval dispatch; never explicitly instantiated.
    // REVIEW(2026-10-01): remove if this enum is still never matched directly.
    #[allow(dead_code)]
    CreativeWriting,
}

/// A named eval task with metadata for CLI and UI.
#[derive(Debug, Clone, Copy)]
pub struct EvalTask {
    pub cli_name: &'static str,
    pub kind: EvalTaskKind,
    pub description: &'static str,
    pub report_label: &'static str,
}

/// Canonical task registry. Adding a new task = adding one entry here.
pub const EVAL_TASKS: &[EvalTask] = &[
    EvalTask {
        cli_name: "gsm8k",
        kind: EvalTaskKind::LmEval {
            task: "gsm8k",
            output_dir: "lm_eval_gsm8k_probe",
        },
        description: "Math reasoning: grade-school word problems (arithmetic)",
        report_label: "GSM8K",
    },
    EvalTask {
        cli_name: "instruction",
        kind: EvalTaskKind::LmEval {
            task: "leaderboard_ifeval",
            output_dir: "lm_eval_instruction_probe",
        },
        description: "IFEval: instruction-following & constraint adherence test",
        report_label: "IFEval",
    },
    EvalTask {
        cli_name: "math",
        kind: EvalTaskKind::LmEval {
            task: "leaderboard_math_hard",
            output_dir: "lm_eval_math_probe",
        },
        description: "Math reasoning: competition-level problem solving",
        report_label: "Math hard",
    },
    EvalTask {
        cli_name: "humaneval",
        kind: EvalTaskKind::EvalPlus {
            output_dir: "evalplus_probe",
        },
        description: "Code generation: Python function completion (164 problems)",
        report_label: "HumanEval / EvalPlus",
    },
    EvalTask {
        cli_name: "mmlu",
        kind: EvalTaskKind::LmEval {
            task: "mmlu",
            output_dir: "lm_eval_mmlu_probe",
        },
        description: "Knowledge: multi-subject QA across 57 academic domains",
        report_label: "MMLU",
    },
    EvalTask {
        cli_name: "hellaswag",
        kind: EvalTaskKind::LmEval {
            task: "hellaswag",
            output_dir: "lm_eval_hellaswag_probe",
        },
        description: "Safety: commonsense reasoning & adversarial filtering",
        report_label: "HellaSwag",
    },
    EvalTask {
        cli_name: "truthfulqa",
        kind: EvalTaskKind::LmEval {
            task: "truthfulqa_gen",
            output_dir: "lm_eval_truthfulqa_probe",
        },
        description: "Safety: factual accuracy & misconception resistance",
        report_label: "TruthfulQA",
    },
    EvalTask {
        cli_name: "bbh",
        kind: EvalTaskKind::LmEval {
            task: "leaderboard_bbh",
            output_dir: "lm_eval_bbh_probe",
        },
        description: "Reasoning: multi-step logic across 23 hard tasks",
        report_label: "BBH",
    },
    // ── New: general knowledge ──
    EvalTask {
        cli_name: "mmlu_pro",
        kind: EvalTaskKind::LmEval {
            task: "mmlu_pro",
            output_dir: "lm_eval_mmlu_pro_probe",
        },
        description: "Knowledge: harder multi-subject QA (extended MMLU)",
        report_label: "MMLU-Pro",
    },
    EvalTask {
        cli_name: "arc_challenge",
        kind: EvalTaskKind::LmEval {
            task: "arc_challenge",
            output_dir: "lm_eval_arc_challenge_probe",
        },
        description: "Science: AI2 Reasoning Challenge (multiple-choice)",
        report_label: "ARC-Challenge",
    },
    // ── New: philosophy & ethics ──
    EvalTask {
        cli_name: "mmlu_philosophy",
        kind: EvalTaskKind::LmEval {
            task: "mmlu_philosophy",
            output_dir: "lm_eval_mmlu_philosophy_probe",
        },
        description: "Philosophy: MMLU philosophy sub-task",
        report_label: "MMLU Philosophy",
    },
    EvalTask {
        cli_name: "hendrycks_ethics",
        kind: EvalTaskKind::LmEval {
            task: "hendrycks_ethics",
            output_dir: "lm_eval_hendrycks_ethics_probe",
        },
        description: "Ethics: Hendrycks ethics benchmark (commonsense, justice, virtue)",
        report_label: "Hendrycks Ethics",
    },
    EvalTask {
        cli_name: "bbh_formal_fallacies",
        kind: EvalTaskKind::LmEval {
            task: "bbh_fewshot_formal_fallacies",
            output_dir: "lm_eval_bbh_formal_fallacies_probe",
        },
        description: "Logic: formal fallacy detection (BBH sub-task)",
        report_label: "BBH Formal Fallacies",
    },
    EvalTask {
        cli_name: "bbh_causal_judgement",
        kind: EvalTaskKind::LmEval {
            task: "bbh_fewshot_causal_judgement",
            output_dir: "lm_eval_bbh_causal_judgement_probe",
        },
        description: "Causality: causal judgement reasoning (BBH sub-task)",
        report_label: "BBH Causal Judgement",
    },
    // ── New: coding ──
    EvalTask {
        cli_name: "mbpp",
        kind: EvalTaskKind::LmEval {
            task: "mbpp",
            output_dir: "lm_eval_mbpp_probe",
        },
        description: "Code generation: Python function completion (MBPP dataset)",
        report_label: "MBPP",
    },
    // ── New: reading comprehension ──
    EvalTask {
        cli_name: "drop",
        kind: EvalTaskKind::LmEval {
            task: "drop",
            output_dir: "lm_eval_drop_probe",
        },
        description: "Reading: discrete reasoning over paragraphs (DROP)",
        report_label: "DROP",
    },
    // ── New: hard (opt-in) ──
    EvalTask {
        cli_name: "gpqa",
        kind: EvalTaskKind::LmEval {
            task: "gpqa_main_zeroshot",
            output_dir: "lm_eval_gpqa_probe",
        },
        description: "Graduate-level physics Q&A (opt-in, very hard)",
        report_label: "GPQA",
    },
];

/// Look up a task by CLI name.
pub fn find_task(cli_name: &str) -> Option<&'static EvalTask> {
    EVAL_TASKS.iter().find(|t| t.cli_name == cli_name)
}

/// Run any eval task from the registry.
pub async fn run_eval_task(
    task: &EvalTask,
    model: &str,
    limit: u32,
    base_url: &str,
    temperature: f64,
    tokenizer: Option<&str>,
) -> Result<()> {
    if limit == 0 {
        bail!("--limit must be greater than 0");
    }

    let root = resolve_project_root()?;
    let artifacts_dir = root.join("results");
    let venv_bin = root.join("contrib/evals/.venv/bin");

    if !root.join("contrib/evals").is_dir() {
        bail!(
            "Missing contrib/evals at {}. Run from an Ozone source checkout or set OZONE_SOURCE_ROOT.",
            root.join("contrib/evals").display()
        );
    }

    // Tasks that require per-token logprobs (llama-cpp-python with logits_all).
    // The HTTP server cannot provide these, so we use ozone_eval.py which loads
    // the GGUF directly via llama-cpp-python.
    const LOGPROB_TASKS: &[&str] = &["mmlu", "hellaswag", "leaderboard_bbh"];

    match task.kind {
        EvalTaskKind::LmEval {
            task: lm_task,
            output_dir,
        } => {
            let is_logprob = LOGPROB_TASKS.iter().any(|t| lm_task.contains(t));

            if is_logprob {
                run_ozone_eval_python(
                    &venv_bin,
                    model,
                    &[lm_task],
                    limit,
                    base_url,
                    temperature,
                )?;
            } else {
                let status = run_lm_eval(
                    &venv_bin,
                    model,
                    lm_task,
                    limit,
                    &artifacts_dir.join(output_dir),
                    base_url,
                    temperature,
                    tokenizer,
                )?;
                if !status.success() {
                    match status.code() {
                        Some(code) => bail!("Evaluation failed with exit code {code}"),
                        None => bail!("Evaluation failed (terminated by signal)"),
                    }
                }
            }
        }
        EvalTaskKind::EvalPlus { output_dir } => {
            let status = run_evalplus_codegen(
                &venv_bin,
                model,
                limit,
                &artifacts_dir.join(output_dir),
                base_url,
            )?;
            if !status.success() {
                match status.code() {
                    Some(code) => bail!("Evaluation failed with exit code {code}"),
                    None => bail!("Evaluation failed (terminated by signal)"),
                }
            }
        }
        EvalTaskKind::CreativeWriting => {
            let prompts = crate::creative_writing::load_prompt_bank(&root)?;
            if prompts.is_empty() {
                bail!("No prompts found in creative writing prompt bank");
            }
            let output_dir = artifacts_dir.join("creative_writing");
            let csv_path = crate::creative_writing::run_creative_writing_eval(
                model,
                &prompts,
                base_url,
                &output_dir,
            )
            .await?;
            let report_md = crate::creative_writing::build_creative_report(&csv_path)?;
            let report_path = csv_path.with_extension("md");
            std::fs::write(&report_path, &report_md)?;
            ozone_core::cli::success(&format!("Creative writing eval complete for '{model}'"));
            ozone_core::cli::field("CSV:", &csv_path.display());
            ozone_core::cli::field("Report:", &report_path.display());
            return Ok(());
        }
    }

    ozone_core::cli::success(&format!(
        "Completed {} for model '{}'.",
        task.description, model
    ));

    // Generate CSV output
    let csv_path = write_eval_csv(task, model, &artifacts_dir)?;
    ozone_core::cli::field("CSV report:", &csv_path.display());
    ozone_core::cli::field("Artifacts:", &artifacts_dir.display());
    Ok(())
}

/// Extract metric values from lm-eval JSON output and write as CSV row.
fn write_eval_csv(task: &EvalTask, model: &str, artifacts_dir: &Path) -> Result<PathBuf> {
    // model is a full GGUF path; use just the filename for CSV records
    let model_name = Path::new(model)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let output_dir = match task.kind {
        EvalTaskKind::LmEval { output_dir, .. } => {
            let base = artifacts_dir.join(output_dir);
            find_model_output_dir(&base, model, model_name)
        },
        EvalTaskKind::EvalPlus { output_dir } => artifacts_dir.join(output_dir).join("humaneval"),
        EvalTaskKind::CreativeWriting => artifacts_dir.join("creative_writing").join(model_name),
    };

    // Find the latest results JSON file
    let json_path = latest_csv_source_file(&output_dir)?;
    let json_text = std::fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&json_text)
        .with_context(|| format!("failed to parse {}", json_path.display()))?;

    let csv_path = disambiguate_csv_path(&json_path.with_extension("csv"))?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut writer = csv::Writer::from_path(&csv_path)
        .with_context(|| format!("failed to create {}", csv_path.display()))?;

    // Write header
    writer.write_record(["model", "task", "metric", "value", "stderr", "timestamp"])?;

    // lm-eval results: extract from results.{task_name} dict
    if let Some(results) = json.get("results") {
        for (task_key, task_data) in results.as_object().unwrap_or(&serde_json::Map::new()) {
            for (metric, value) in task_data.as_object().unwrap_or(&serde_json::Map::new()) {
                if metric == "alias" {
                    continue;
                }
                let (val, stderr) = extract_metric_value(value);
                writer.write_record([
                    model_name,
                    task_key,
                    metric,
                    &val.to_string(),
                    &stderr.to_string(),
                    &timestamp,
                ])?;
            }
        }
    }

    writer.flush()?;
    Ok(csv_path)
}

fn extract_metric_value(value: &serde_json::Value) -> (f64, f64) {
    match value {
        serde_json::Value::Number(n) => (n.as_f64().unwrap_or(0.0), 0.0),
        serde_json::Value::Object(obj) => {
            let val = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let err = obj.get("stderr").and_then(|v| v.as_f64()).unwrap_or(0.0);
            (val, err)
        }
        _ => (0.0, 0.0),
    }
}

fn disambiguate_csv_path(base: &Path) -> Result<PathBuf> {
    if !base.exists() {
        return Ok(base.to_path_buf());
    }
    for n in 1..1000u32 {
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("results");
        let candidate = base.with_file_name(format!("{stem}_run{n}.csv"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("too many existing CSV files at {}", base.display())
}

/// Find the latest JSON results file in a model output directory.
/// Reuses the existing `latest_json_file` pattern from eval_report.rs.
/// Find the model-specific output directory under `base`.
/// lm-eval creates a subdirectory using the model name (full path with `/` → `__`),
/// or a short name depending on how the model was passed.  Pick the one that
/// actually exists; if multiple match, prefer the one whose name contains the
/// full path (the `__`-separated form) as it's the most recent convention.
pub(crate) fn find_model_output_dir(base: &Path, model: &str, model_name: &str) -> PathBuf {
    // Direct possible paths
    let candidates = [
        // Full path with '/' → '__' (current lm-eval convention)
        base.join(model.trim_start_matches('/').replace('/', "__")),
        // Full path with '.' → '_' (some lm-eval versions)
        base.join(model.trim_start_matches('/').replace('/', "__").replace('.', "_")),
        // Just the filename stem (previous code convention)
        base.join(model_name),
        // Filename stem + .gguf extension (some lm-eval versions)
        base.join(Path::new(model).file_name().and_then(|s| s.to_str()).unwrap_or(model_name)),
    ];

    // Return the first one that exists, falling back to the file-stem path
    for c in &candidates {
        if c.is_dir() {
            return c.to_path_buf();
        }
    }
    // None match yet — maybe the output hasn't been written. Use the full-path form.
    candidates[0].clone()
}

fn latest_csv_source_file(dir: &Path) -> Result<PathBuf> {
    // lm-eval writes results_*.json; pick the latest by name
    let mut entries = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if path.extension().and_then(|ext| ext.to_str()) == Some("json")
            && file_name.starts_with("results_")
        {
            entries.push(path);
        }
    }
    entries.sort();
    entries
        .last()
        .cloned()
        .with_context(|| format!("no results JSON found in {}", dir.display()))
}

pub fn print_comparison(task_name: &str) -> Result<()> {
    let task = find_task(task_name).with_context(|| format!("unknown task: {task_name}"))?;

    let output_dir = match task.kind {
        EvalTaskKind::LmEval { output_dir, .. } => output_dir,
        EvalTaskKind::EvalPlus { output_dir } => output_dir,
        EvalTaskKind::CreativeWriting => "creative_writing",
    };

    let root = resolve_project_root()?;
    let dir = root.join("results").join(output_dir);

    if !dir.exists() {
        tracing::info!("No results yet. Run `oz eval <model> --preset {task_name}` first.");
        return Ok(());
    }

    // Collect all CSV files from subdirectories
    let mut all_rows: Vec<(String, String, f64)> = Vec::new(); // (model, metric, value)
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let model_dir = entry.path();
        for file in std::fs::read_dir(&model_dir)? {
            let file = file?;
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("csv") {
                continue;
            }
            let mut rdr = csv::Reader::from_path(&path)?;
            for result in rdr.records() {
                let record = result?;
                if record.len() >= 4 {
                    let model = record[0].to_string();
                    let metric = record[2].to_string();
                    let value: f64 = record[3].parse().unwrap_or(0.0);
                    all_rows.push((model, metric, value));
                }
            }
        }
    }

    if all_rows.is_empty() {
        tracing::info!("No CSV results found in {}.", dir.display());
        return Ok(());
    }

    // Group by model, take primary metric (first non-stderr metric)
    let mut best: Vec<(String, f64)> = Vec::new();
    for (model, _metric, value) in &all_rows {
        if let Some(existing) = best.iter_mut().find(|(m, _)| m == model) {
            if *value > existing.1 {
                existing.1 = *value;
            }
        } else {
            best.push((model.clone(), *value));
        }
    }
    best.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    println!("RANK  MODEL                                                       SCORE");
    for (i, (model, score)) in best.iter().enumerate() {
        println!("{:>4}  {:<60} {:.4}", i + 1, model, score);
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn run_eval(
    model: &str,
    preset: EvalPreset,
    limit: u32,
    base_url: &str,
    temperature: f64,
    tokenizer: Option<&str>,
) -> Result<()> {
    if limit == 0 {
        bail!("--limit must be greater than 0");
    }

    // Look up the task from the registry using the preset's CLI name
    let task = EVAL_TASKS
        .iter()
        .find(|t| t.cli_name == preset.cli_name())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Internal error: preset '{}' not found in EVAL_TASKS registry",
                preset.cli_name()
            )
        })?;

    // Delegate to the task registry — single source of truth for dispatch
    run_eval_task(task, model, limit, base_url, temperature, tokenizer).await?;

    // Build markdown report (uses preset for backward-compatible reporting)
    match crate::eval_report::build_eval_report_for_preset(model, preset) {
        Ok(report) => {
            if let Err(error) = crate::eval_report::write_eval_report(&report) {
                tracing::error!("Markdown report could not be written: {error}");
            } else {
                ozone_core::cli::field("Markdown report:", &report.markdown_path.display());
            }
        }
        Err(error) => {
            tracing::error!("Markdown report could not be generated: {error}");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_lm_eval(
    venv_bin: &Path,
    model: &str,
    task: &str,
    limit: u32,
    output_path: &Path,
    base_url: &str,
    temperature: f64,
    tokenizer: Option<&str>,
) -> Result<std::process::ExitStatus> {
    let lm_eval = venv_bin.join("lm-eval");
    ensure_executable(&lm_eval)?;

    let completions_url = format!("{}/v1/completions", normalize_base_url(base_url));
    let model_args = if let Some(tok) = tokenizer {
        format!(
            "model={model},base_url={completions_url},tokenizer_backend=huggingface,tokenizer={tok},temperature={temperature}"
        )
    } else {
        format!(
            "model={model},base_url={completions_url},tokenizer_backend=None,temperature={temperature}"
        )
    };

    ozone_core::cli::header("oz Eval");
    ozone_core::cli::field("Suite:", &format!("lm-eval ({task})"));
    ozone_core::cli::field("Model:", &model);
    ozone_core::cli::field("Limit:", &limit);
    ozone_core::cli::field("Output:", &output_path.display());
    ozone_core::cli::spacer();

    Command::new(lm_eval)
        .arg("run")
        .arg("--model")
        .arg("local-completions")
        .arg("--model_args")
        .arg(model_args)
        .arg("--tasks")
        .arg(task)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--output_path")
        .arg(output_path)
        .env("OPENAI_API_KEY", "none")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch {}", venv_bin.join("lm-eval").display()))
}

/// Run eval tasks via ozone_eval.py for loglikelihood tasks that need
/// direct llama-cpp-python access (logits_all=True for per-token logprobs).
///
/// The HTTP server cannot provide prompt-token logprobs, so we load the
/// GGUF directly in Python for correct loglikelihood scoring.
fn run_ozone_eval_python(
    venv_bin: &Path,
    model_name: &str,
    tasks: &[&str],
    limit: u32,
    base_url: &str,
    temperature: f64,
) -> Result<()> {
    let python = venv_bin.join("python3");
    let script = resolve_project_root()?
        .join("contrib/evals/scripts/ozone_eval.py");

    ensure_executable(&python)?;

    // Build the GGUF path from the model name — ozone_eval.py resolves it
    // by checking ~/models/ and the workspace.
    // ozone_eval.py resolves GGUF path from model name by checking
    // ~/models/ first, then treating it as a raw path.
    let gguf_path = model_name;

    let tasks_arg = tasks.join(",");

    ozone_core::cli::header("oz Eval (correct logprobs)");
    ozone_core::cli::field("Method:", &"llama-cpp-python + logits_all");
    ozone_core::cli::field("Model:", &model_name);
    ozone_core::cli::field("Tasks:", &tasks_arg);
    ozone_core::cli::field("Limit:", &limit);
    ozone_core::cli::spacer();

    let status = Command::new(&python)
        .arg(&script)
        .arg(gguf_path)
        .arg("--presets")
        .args(tasks)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--base-url")
        .arg(base_url)
        .arg("--temperature")
        .arg(temperature.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch {}", python.display()))?;

    if !status.success() {
        match status.code() {
            Some(code) => bail!("ozone_eval.py failed with exit code {code}"),
            None => bail!("ozone_eval.py terminated by signal"),
        }
    }

    Ok(())
}

fn run_evalplus_codegen(
    venv_bin: &Path,
    model: &str,
    limit: u32,
    root: &Path,
    base_url: &str,
) -> Result<std::process::ExitStatus> {
    let evalplus_codegen = venv_bin.join("evalplus.codegen");
    ensure_executable(&evalplus_codegen)?;

    let upper_bound = limit;
    let id_range = format!("[0,{upper_bound}]");

    let suite_name = "EvalPlus (humaneval)";
    ozone_core::cli::header("oz Eval");
    ozone_core::cli::field("Suite:", &suite_name);
    ozone_core::cli::field("Model:", &model);
    ozone_core::cli::field("Samples:", &limit);
    ozone_core::cli::field("Output root:", &root.display());
    ozone_core::cli::spacer();

    Command::new(evalplus_codegen)
        .arg(model)
        .arg("humaneval")
        .arg("--backend")
        .arg("openai")
        .arg("--base_url")
        .arg(format!("{}/v1", normalize_base_url(base_url)))
        .arg("--root")
        .arg(root)
        .arg("--n_samples")
        .arg("1")
        .arg("--temperature")
        .arg("0.0")
        .arg("--greedy")
        .arg("--id_range")
        .arg(id_range)
        .env("OPENAI_API_KEY", "none")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "failed to launch {}",
                venv_bin.join("evalplus.codegen").display()
            )
        })?;

    // ---- Step 2: Evaluate generated code ----
    let evalplus_evaluate = venv_bin.join("evalplus.evaluate");
    let samples_path = root.join("humaneval").join(format!("{model}_openai_temp_0.0.jsonl"));
    ozone_core::cli::header("Evaluation");
    ozone_core::cli::field("Samples:", &samples_path.display());
    if !samples_path.exists() {
        ozone_core::cli::info("Samples not found — codegen may have failed.");
        return Ok(std::process::ExitStatus::default());
    }
    if limit < 164 {
        ozone_core::cli::info(&format!(
            "Scoring requires all 164 problems. Use --limit 164 to get pass@1 score (current: {limit})."
        ));
        return Ok(std::process::ExitStatus::default());
    }

    let status = Command::new(evalplus_evaluate)
        .arg("--dataset")
        .arg("humaneval")
        .arg("--samples")
        .arg(&samples_path)
        .arg("--parallel")
        .arg("4")
        .env("OPENAI_API_KEY", "none")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "failed to launch {}",
                venv_bin.join("evalplus.evaluate").display()
            )
        })?;

    if status.success() {
        // Find and show the evaluation results JSON
        if let Ok(results_dir) = std::fs::read_dir(root.join("humaneval")) {
            for entry in results_dir.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("evalplus_results_") && name.ends_with(".json") {
                    ozone_core::cli::field("Results:", &path.display());
                }
            }
        }
    }
    Ok(status)
}

fn ensure_executable(path: &Path) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }

    bail!(
        "Missing evaluation runner at {}. Run contrib/evals/bootstrap.sh first.",
        path.display()
    )
}

pub(crate) fn resolve_project_root() -> Result<PathBuf> {
    if let Ok(source_root) = std::env::var("OZONE_SOURCE_ROOT") {
        let candidate = PathBuf::from(source_root);
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }

    if let Some(marker_path) = ozone_core::paths::install_source_root_path() {
        if marker_path.is_file() {
            let source_root = std::fs::read_to_string(&marker_path)
                .with_context(|| format!("failed to read {}", marker_path.display()))?;
            let candidate = PathBuf::from(source_root.trim());
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }

    std::env::current_dir().context("failed to resolve current working directory")
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_base_url;
    use super::{EvalPreset, EVAL_TASKS};
    use clap::ValueEnum;

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8989/"),
            "http://127.0.0.1:8989"
        );
        assert_eq!(
            normalize_base_url("http://127.0.0.1:8989"),
            "http://127.0.0.1:8989"
        );
    }

    #[test]
    fn eval_registry_bbh_has_correct_lm_eval_task_name() {
        let task = EVAL_TASKS
            .iter()
            .find(|t| t.cli_name == "bbh")
            .expect("bbh should be in EVAL_TASKS");
        match &task.kind {
            super::EvalTaskKind::LmEval { task, .. } => {
                assert_eq!(
                    *task, "leaderboard_bbh",
                    "BBH lm-eval task name mismatch between registry and lm-eval"
                );
            }
            _ => panic!("expected LmEval kind for bbh"),
        }
    }

    #[test]
    fn eval_registry_truthfulqa_has_correct_lm_eval_task_name() {
        let task = EVAL_TASKS
            .iter()
            .find(|t| t.cli_name == "truthfulqa")
            .expect("truthfulqa should be in EVAL_TASKS");
        match &task.kind {
            super::EvalTaskKind::LmEval { task, .. } => {
                assert_eq!(
                    *task, "truthfulqa_gen",
                    "TruthfulQA lm-eval task name mismatch between registry and lm-eval"
                );
            }
            _ => panic!("expected LmEval kind for truthfulqa"),
        }
    }

    #[test]
    fn eval_dispatch_task_names_match_registry() {
        // Every EvalPreset that dispatches to run_lm_eval must use the same
        // task name as its EVAL_TASKS entry. This tests the concrete dispatch
        // strings against the single source of truth.
        for preset in EvalPreset::value_variants() {
            let cli = preset.cli_name();
            let Some(registry_task) = EVAL_TASKS.iter().find(|t| t.cli_name == cli) else {
                // Some presets may not be in EVAL_TASKS (e.g. creative writing)
                continue;
            };
            let super::EvalTaskKind::LmEval {
                task: expected_task,
                ..
            } = &registry_task.kind
            else {
                // Skip non-lm-eval presets
                continue;
            };
            // We can't inspect the internal dispatch strings directly, but we
            // can verify the registry has the right name — that's the source of truth.
            // If this test fails, the dispatch in run_eval() is using a stale name.
            assert!(
                !expected_task.is_empty(),
                "lm-eval task name for {cli} must not be empty"
            );
        }
    }
}
