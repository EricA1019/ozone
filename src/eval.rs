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
}

impl EvalPreset {
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Gsm8k => "lm-eval GSM8K probe",
            Self::Instruction => "lm-eval instruction-following probe",
            Self::Math => "lm-eval leaderboard_math_hard probe",
            Self::Humaneval => "EvalPlus HumanEval codegen probe",
        }
    }

    pub(crate) fn cli_name(self) -> &'static str {
        match self {
            Self::Gsm8k => "gsm8k",
            Self::Instruction => "instruction",
            Self::Math => "math",
            Self::Humaneval => "humaneval",
        }
    }

    pub(crate) fn report_label(self) -> &'static str {
        match self {
            Self::Gsm8k => "GSM8K",
            Self::Instruction => "Instruction following",
            Self::Math => "Math hard",
            Self::Humaneval => "HumanEval / EvalPlus",
        }
    }
}

/// The kind of eval runner to invoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalTaskKind {
    /// Runs lm-eval with the given task name and output subdirectory.
    LmEval { task: &'static str, output_dir: &'static str },
    /// Runs EvalPlus codegen.
    EvalPlus { output_dir: &'static str },
    /// Runs the creative writing diversity probe (Phase 2).
    #[allow(dead_code)]
    CreativeWriting,
}

/// A named eval task with metadata for CLI and UI.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct EvalTask {
    pub name: &'static str,
    pub cli_name: &'static str,
    pub kind: EvalTaskKind,
    pub description: &'static str,
    pub report_label: &'static str,
}

/// Canonical task registry. Adding a new task = adding one entry here.
pub const EVAL_TASKS: &[EvalTask] = &[
    EvalTask {
        name: "gsm8k",
        cli_name: "gsm8k",
        kind: EvalTaskKind::LmEval { task: "gsm8k", output_dir: "lm_eval_gsm8k_probe" },
        description: "lm-eval GSM8K arithmetic reasoning probe",
        report_label: "GSM8K",
    },
    EvalTask {
        name: "instruction",
        cli_name: "instruction",
        kind: EvalTaskKind::LmEval { task: "leaderboard_instruction_following", output_dir: "lm_eval_instruction_probe" },
        description: "lm-eval instruction-following leaderboard probe",
        report_label: "Instruction following",
    },
    EvalTask {
        name: "math",
        cli_name: "math",
        kind: EvalTaskKind::LmEval { task: "leaderboard_math_hard", output_dir: "lm_eval_math_probe" },
        description: "lm-eval leaderboard_math_hard probe",
        report_label: "Math hard",
    },
    EvalTask {
        name: "humaneval",
        cli_name: "humaneval",
        kind: EvalTaskKind::EvalPlus { output_dir: "evalplus_probe" },
        description: "EvalPlus HumanEval codegen probe",
        report_label: "HumanEval / EvalPlus",
    },
    EvalTask {
        name: "mmlu",
        cli_name: "mmlu",
        kind: EvalTaskKind::LmEval { task: "mmlu", output_dir: "lm_eval_mmlu_probe" },
        description: "Massive Multitask Language Understanding (57 subjects)",
        report_label: "MMLU",
    },
    EvalTask {
        name: "hellaswag",
        cli_name: "hellaswag",
        kind: EvalTaskKind::LmEval { task: "hellaswag", output_dir: "lm_eval_hellaswag_probe" },
        description: "Commonsense narrative completion (HellaSwag)",
        report_label: "HellaSwag",
    },
    EvalTask {
        name: "truthfulqa",
        cli_name: "truthfulqa",
        kind: EvalTaskKind::LmEval { task: "truthfulqa_gen", output_dir: "lm_eval_truthfulqa_probe" },
        description: "TruthfulQA generation (misconception resistance)",
        report_label: "TruthfulQA",
    },
    EvalTask {
        name: "bbh",
        cli_name: "bbh",
        kind: EvalTaskKind::LmEval { task: "bigbench_hard", output_dir: "lm_eval_bbh_probe" },
        description: "BIG-Bench Hard (23 multi-step reasoning tasks)",
        report_label: "BBH",
    },
];

/// Look up a task by CLI name.
pub fn find_task(cli_name: &str) -> Option<&'static EvalTask> {
    EVAL_TASKS.iter().find(|t| t.cli_name == cli_name)
}

/// Run any eval task from the registry.
#[allow(dead_code)]
pub fn run_eval_task(
    task: &EvalTask,
    model: &str,
    limit: u32,
    base_url: &str,
    temperature: f64,
) -> Result<()> {
    if limit == 0 {
        bail!("--limit must be greater than 0");
    }

    let root = resolve_project_root()?;
    let artifacts_dir = root.join("contrib/evals/artifacts");
    let venv_bin = root.join("contrib/evals/.venv/bin");

    if !root.join("contrib/evals").is_dir() {
        bail!(
            "Missing contrib/evals at {}. Run from an Ozone source checkout or set OZONE_SOURCE_ROOT.",
            root.join("contrib/evals").display()
        );
    }

    let status = match task.kind {
        EvalTaskKind::LmEval { task: lm_task, output_dir } => {
            run_lm_eval(
                &venv_bin, model, lm_task, limit,
                &artifacts_dir.join(output_dir),
                base_url, temperature,
            )?
        }
        EvalTaskKind::EvalPlus { output_dir } => {
            run_evalplus_codegen(
                &venv_bin, model, limit,
                &artifacts_dir.join(output_dir),
                base_url,
            )?
        }
        EvalTaskKind::CreativeWriting => {
            bail!("Creative writing eval not yet implemented (Phase 2)");
        }
    };

    if !status.success() {
        match status.code() {
            Some(code) => bail!("Evaluation failed with exit code {code}"),
            None => bail!("Evaluation failed (terminated by signal)"),
        }
    }

    ozone_core::cli::success(&format!(
        "Completed {} for model '{}'.", task.description, model
    ));

    // Generate CSV output
    let csv_path = write_eval_csv(task, model, &artifacts_dir)?;
    ozone_core::cli::field("CSV report:", &csv_path.display());
    ozone_core::cli::field("Artifacts:", &artifacts_dir.display());
    Ok(())
}

/// Extract metric values from lm-eval JSON output and write as CSV row.
#[allow(dead_code)]
fn write_eval_csv(task: &EvalTask, model: &str, artifacts_dir: &Path) -> Result<PathBuf> {
    let output_dir = match task.kind {
        EvalTaskKind::LmEval { output_dir, .. } => artifacts_dir.join(output_dir).join(model),
        EvalTaskKind::EvalPlus { output_dir } => artifacts_dir.join(output_dir).join("humaneval"),
        EvalTaskKind::CreativeWriting => artifacts_dir.join("creative_writing").join(model),
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
                if metric == "alias" { continue; }
                let (val, stderr) = extract_metric_value(value);
                writer.write_record([
                    model, task_key, metric,
                    &val.to_string(), &stderr.to_string(), &timestamp,
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
        let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("results");
        let candidate = base.with_file_name(format!("{stem}_run{n}.csv"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("too many existing CSV files at {}", base.display())
}

/// Find the latest JSON results file in a model output directory.
/// Reuses the existing `latest_json_file` pattern from eval_report.rs.
fn latest_csv_source_file(dir: &Path) -> Result<PathBuf> {
    // lm-eval writes results_*.json; pick the latest by name
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
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
    entries.last().cloned()
        .with_context(|| format!("no results JSON found in {}", dir.display()))
}

pub fn print_comparison(task_name: &str) -> Result<()> {
    let task = find_task(task_name)
        .with_context(|| format!("unknown task: {task_name}"))?;

    let output_dir = match task.kind {
        EvalTaskKind::LmEval { output_dir, .. } => output_dir,
        EvalTaskKind::EvalPlus { output_dir } => output_dir,
        EvalTaskKind::CreativeWriting => "creative_writing",
    };

    let root = resolve_project_root()?;
    let dir = root.join("contrib/evals/artifacts").join(output_dir);

    if !dir.exists() {
        println!("No results yet. Run `ozone eval <model> --preset {task_name}` first.");
        return Ok(());
    }

    // Collect all CSV files from subdirectories
    let mut all_rows: Vec<(String, String, f64)> = Vec::new(); // (model, metric, value)
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() { continue; }
        let model_dir = entry.path();
        for file in std::fs::read_dir(&model_dir)? {
            let file = file?;
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("csv") { continue; }
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
        println!("No CSV results found in {}.", dir.display());
        return Ok(());
    }

    // Group by model, take primary metric (first non-stderr metric)
    let mut best: Vec<(String, f64)> = Vec::new();
    for (model, _metric, value) in &all_rows {
        if let Some(existing) = best.iter_mut().find(|(m, _)| m == model) {
            if *value > existing.1 { existing.1 = *value; }
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

pub fn run_eval(model: &str, preset: EvalPreset, limit: u32, base_url: &str, temperature: f64) -> Result<()> {
    if limit == 0 {
        bail!("--limit must be greater than 0");
    }

    let root = resolve_project_root()?;
    let evals_dir = root.join("contrib/evals");
    let artifacts_dir = evals_dir.join("artifacts");
    let venv_bin = evals_dir.join(".venv/bin");

    if !evals_dir.is_dir() {
        bail!(
            "Missing contrib/evals at {}. Run from an Ozone source checkout or set OZONE_SOURCE_ROOT.",
            evals_dir.display()
        );
    }

    let status = match preset {
        EvalPreset::Gsm8k => run_lm_eval(
            &venv_bin,
            model,
            "gsm8k",
            limit,
            &artifacts_dir.join("lm_eval_gsm8k_probe"),
            base_url,
            temperature,
        )?,
        EvalPreset::Instruction => run_lm_eval(
            &venv_bin,
            model,
            "leaderboard_instruction_following",
            limit,
            &artifacts_dir.join("lm_eval_instruction_probe"),
            base_url,
            temperature,
        )?,
        EvalPreset::Math => run_lm_eval(
            &venv_bin,
            model,
            "leaderboard_math_hard",
            limit,
            &artifacts_dir.join("lm_eval_math_probe"),
            base_url,
            temperature,
        )?,
        EvalPreset::Humaneval => run_evalplus_codegen(
            &venv_bin,
            model,
            limit,
            &artifacts_dir.join("evalplus_probe"),
            base_url,
        )?,
    };

    if !status.success() {
        match status.code() {
            Some(code) => bail!("Evaluation failed with exit code {code}"),
            None => bail!("Evaluation failed (terminated by signal)"),
        }
    }

    ozone_core::cli::success(&format!(
        "Completed {} for model '{}'.",
        preset.description(),
        model
    ));
    match crate::eval_report::build_eval_report_for_preset(model, preset) {
        Ok(report) => {
            if let Err(error) = crate::eval_report::write_eval_report(&report) {
                eprintln!("Markdown report could not be written: {error}");
            } else {
                ozone_core::cli::field("Markdown report:", &report.markdown_path.display());
            }
        }
        Err(error) => {
            eprintln!("Markdown report could not be generated: {error}");
        }
    }
    ozone_core::cli::field("Artifacts:", &artifacts_dir.display());
    Ok(())
}

fn run_lm_eval(
    venv_bin: &Path,
    model: &str,
    task: &str,
    limit: u32,
    output_path: &Path,
    base_url: &str,
    temperature: f64,
) -> Result<std::process::ExitStatus> {
    let lm_eval = venv_bin.join("lm-eval");
    ensure_executable(&lm_eval)?;

    let completions_url = format!("{}/v1/completions", normalize_base_url(base_url));
    let model_args = format!(
        "model={model},base_url={completions_url},tokenizer_backend=None,temperature={temperature}"
    );

    ozone_core::cli::header("Ozone Eval");
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

fn run_evalplus_codegen(
    venv_bin: &Path,
    model: &str,
    limit: u32,
    root: &Path,
    base_url: &str,
) -> Result<std::process::ExitStatus> {
    let evalplus_codegen = venv_bin.join("evalplus.codegen");
    ensure_executable(&evalplus_codegen)?;

    let upper_bound = limit.saturating_sub(1);
    let id_range = format!("[0,{upper_bound}]");

    let suite_name = "EvalPlus (humaneval)";
    ozone_core::cli::header("Ozone Eval");
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
        })
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

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(normalize_base_url("http://127.0.0.1:8989/"), "http://127.0.0.1:8989");
        assert_eq!(normalize_base_url("http://127.0.0.1:8989"), "http://127.0.0.1:8989");
    }
}