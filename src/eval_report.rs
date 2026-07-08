use anyhow::{Context, Result};
use crate::eval_result::EvalResult;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::eval::EvalPreset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalMarkdownReport {
    pub title: String,
    pub markdown: String,
    pub source_path: PathBuf,
    pub markdown_path: PathBuf,
}

pub(crate) fn build_eval_report_for_preset(
    model: &str,
    preset: EvalPreset,
) -> Result<EvalMarkdownReport> {
    let root = crate::eval::resolve_project_root()?;
    let artifacts_dir = root.join("results");

    // Look up output directory from the task registry
    let task = crate::eval::EVAL_TASKS
        .iter()
        .find(|t| t.cli_name == preset.cli_name())
        .with_context(|| format!("preset '{}' not in registry", preset.cli_name()))?;

    let output_dir = match task.kind {
        crate::eval::EvalTaskKind::LmEval { output_dir, .. } => output_dir,
        crate::eval::EvalTaskKind::EvalPlus { output_dir } => output_dir,
        crate::eval::EvalTaskKind::CreativeWriting => "creative_writing",
    };

    let title = format!("{} eval report", preset.report_label());

    match task.kind {
        crate::eval::EvalTaskKind::LmEval { .. } => {
            let model_name = std::path::Path::new(model)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let model_dir = crate::eval::find_model_output_dir(&artifacts_dir.join(output_dir), model, model_name);
            build_lm_eval_report(&title, &model_dir)
        }
        crate::eval::EvalTaskKind::EvalPlus { .. } => build_evalplus_report(
            &title,
            &artifacts_dir.join(output_dir).join("humaneval"),
            model,
        ),
        crate::eval::EvalTaskKind::CreativeWriting => {
            // Creative writing reports are generated alongside the CSV by the runner
            let csv_path = artifacts_dir
                .join(output_dir)
                .join(format!("{model}_creative.csv"));
            let markdown = crate::creative_writing::build_creative_report(&csv_path)?;
            let markdown_path = csv_path.with_extension("md");
            Ok(EvalMarkdownReport {
                title,
                markdown,
                source_path: csv_path,
                markdown_path,
            })
        }
    }
}

pub(crate) fn write_eval_report(report: &EvalMarkdownReport) -> Result<()> {
    if let Some(parent) = report.markdown_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&report.markdown_path, &report.markdown)
        .with_context(|| format!("failed to write {}", report.markdown_path.display()))
}

fn build_lm_eval_report(title: &str, model_dir: &Path) -> Result<EvalMarkdownReport> {
    let source_path = latest_json_file(model_dir)?;
    let markdown_path = source_path.with_extension("md");
    let json_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let json: Value = serde_json::from_str(&json_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;

    let markdown = render_lm_eval_markdown(title, &json, &source_path);
    Ok(EvalMarkdownReport {
        title: title.to_string(),
        markdown,
        source_path,
        markdown_path,
    })
}

fn build_evalplus_report(
    title: &str,
    report_dir: &Path,
    model: &str,
) -> Result<EvalMarkdownReport> {
    let source_path = report_dir.join(format!("{model}_openai_temp_0.0.jsonl"));
    let markdown_path = source_path.with_extension("md");
    let jsonl = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;

    let markdown = render_evalplus_markdown(title, &jsonl, &source_path);
    Ok(EvalMarkdownReport {
        title: title.to_string(),
        markdown,
        source_path,
        markdown_path,
    })
}

fn latest_json_file(dir: &Path) -> Result<PathBuf> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
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
        .pop()
        .with_context(|| format!("no lm-eval result JSON found in {}", dir.display()))
}

fn render_lm_eval_markdown(title: &str, json: &Value, source_path: &Path) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {title}\n\n"));
    markdown.push_str(&format!("- Source JSON: `{}`\n", source_path.display()));

    if let Some(model_name) = json.get("model_name").and_then(Value::as_str) {
        markdown.push_str(&format!("- Model: `{model_name}`\n"));
    }
    if let Some(version) = json.get("lm_eval_version").and_then(Value::as_str) {
        markdown.push_str(&format!("- lm-eval version: `{version}`\n"));
    }
    if let Some(total_time) = json
        .get("total_evaluation_time_seconds")
        .and_then(Value::as_str)
    {
        markdown.push_str(&format!("- Total time: `{total_time}` seconds\n"));
    }
    if let Some(config) = json.get("config").and_then(Value::as_object) {
        if let Some(model_args) = config.get("model_args").and_then(Value::as_object) {
            if let Some(base_url) = model_args.get("base_url").and_then(Value::as_str) {
                markdown.push_str(&format!("- Base URL: `{base_url}`\n"));
            }
        }
    }

    if let Some(groups) = json.get("group_subtasks").and_then(Value::as_object) {
        if !groups.is_empty() {
            markdown.push_str("\n## Group structure\n\n");
            for (group_name, subtasks) in groups {
                markdown.push_str(&format!("- **{group_name}**\n"));
                if let Some(subtasks) = subtasks.as_array() {
                    for subtask in subtasks {
                        if let Some(name) = subtask.as_str() {
                            markdown.push_str(&format!("  - {name}\n"));
                        }
                    }
                }
            }
        }
    }

    if let Some(results) = json.get("results").and_then(Value::as_object) {
        let ordered: BTreeMap<&String, &Value> = results.iter().collect();
        markdown.push_str("\n## Metrics\n\n");
        for (task_name, task_value) in ordered {
            let Some(task_object) = task_value.as_object() else {
                continue;
            };

            let metrics = summarize_numeric_fields(task_object);
            if metrics.is_empty() {
                continue;
            }

            markdown.push_str(&format!("### {task_name}\n\n"));
            if let Some(sample_len) = task_object.get("sample_len").and_then(Value::as_u64) {
                markdown.push_str(&format!("- sample_len: `{sample_len}`\n\n"));
            }

            markdown.push_str("| Metric | Value |\n| --- | --- |\n");
            for (metric, value) in metrics {
                markdown.push_str(&format!("| {metric} | {value} |\n"));
            }
            markdown.push('\n');
        }
    }

    markdown.push_str("## Reading the scale\n\n");
    markdown.push_str("- All lm-eval metrics here are normalized fractions from `0.0` to `1.0`.\n");
    markdown.push_str("- Multiply by 100 if you want a percentage-style reading.\n");
    markdown.push_str("- Compare scores within the same suite, not across different suites.\n");

    markdown
}

fn summarize_numeric_fields(object: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    let mut rows = Vec::new();

    for (key, value) in object {
        if key == "name"
            || key == "alias"
            || key == "sample_len"
            || key == "sample_count"
            || key.ends_with("_stderr")
        {
            continue;
        }

        if let Some(number) = value.as_f64() {
            rows.push((key.clone(), format_fraction(number)));
        }
    }

    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows
}

fn render_evalplus_markdown(title: &str, jsonl: &str, source_path: &Path) -> String {
    let mut markdown = String::new();
    let raw_path = source_path.with_file_name(
        source_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.replace(".jsonl", ".raw.jsonl"))
            .unwrap_or_else(|| "raw.jsonl".to_string()),
    );

    markdown.push_str(&format!("# {title}\n\n"));
    markdown.push_str(&format!("- Source JSONL: `{}`\n", source_path.display()));
    markdown.push_str(&format!("- Raw JSONL: `{}`\n", raw_path.display()));
    markdown.push_str("- Status: generation only. Run `evalplus.evaluate` to score pass@k.\n\n");

    let mut sample_count = 0usize;
    markdown.push_str("## Generated samples\n\n");
    for line in jsonl.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                markdown.push_str(&format!(
                    "- Could not parse a sample line from `{}`: `{error}`\n",
                    source_path.display()
                ));
                continue;
            }
        };

        sample_count += 1;
        let task_id = value
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or("<unknown task>");
        let solution = value
            .get("solution")
            .or_else(|| value.get("completion"))
            .and_then(Value::as_str)
            .unwrap_or("<no solution text>");

        markdown.push_str(&format!("### {task_id}\n\n"));
        markdown.push_str("~~~python\n");
        markdown.push_str(solution.trim_end());
        markdown.push_str("\n~~~\n\n");
    }

    markdown.push_str(&format!("- Samples rendered: `{sample_count}`\n\n"));
    markdown.push_str("## Reading the output\n\n");
    markdown.push_str("- This report shows code generation output only.\n");
    markdown
        .push_str("- Run `evalplus.evaluate` on the generated JSONL if you want pass@k scores.\n");

    markdown
}

fn format_fraction(value: f64) -> String {
    format!("{value:.3} ({:.1}%)", value * 100.0)
}


// ---------------------------------------------------------------------------
// Unified report — produces Markdown, JSON, and CSV from EvalResult slices.
// This is additive: existing per-eval-type report paths remain unchanged.
// ---------------------------------------------------------------------------

/// Write a unified report for a set of eval results.
///
/// Creates three files under `results/unified/{model_name}/`:
/// - `report.md` — human-readable summary
/// - `report.json` — machine-readable data
/// - `report.csv` — tabular export
pub fn build_unified_report(results: &[EvalResult], model_name: &str) -> Result<PathBuf> {
    let root = crate::eval::resolve_project_root()?;
    let unified_dir = root.join("results").join("unified").join(model_name);
    fs::create_dir_all(&unified_dir)
        .with_context(|| format!("create unified report dir: {}", unified_dir.display()))?;

    // Write JSON
    let json_path = unified_dir.join("report.json");
    let json = serde_json::to_string_pretty(results)?;
    fs::write(&json_path, &json)
        .with_context(|| format!("write unified JSON: {}", json_path.display()))?;

    // Write Markdown summary
    let md_path = unified_dir.join("report.md");
    let mut md = String::new();
    md.push_str(&format!("# Eval Report — {}\n\n", model_name));
    md.push_str("## Summary\n\n");
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;
    md.push_str(&format!("- **Total tasks**: {}\n", total));
    md.push_str(&format!("- **Passed**: {}\n", passed));
    md.push_str(&format!("- **Failed**: {}\n\n", failed));

    if !results.is_empty() {
        let avg_score: f64 = results.iter().map(|r| r.score).sum::<f64>() / total as f64;
        md.push_str(&format!("- **Average score**: {:.3}\n\n", avg_score));
    }

    md.push_str("## Results\n\n");
    md.push_str("| Task | Suite | Score | Passed | Status | Duration (ms) |\n");
    md.push_str("|------|-------|-------|--------|--------|---------------|\n");
    for r in results {
        let status_str = r.status.as_str();
        md.push_str(&format!(
            "| {} | {} | {:.3} | {} | {} | {} |\n",
            r.task_key, r.suite, r.score, r.passed, status_str, r.duration_ms
        ));
    }
    fs::write(&md_path, &md)
        .with_context(|| format!("write unified Markdown: {}", md_path.display()))?;

    // Write CSV
    let csv_path = unified_dir.join("report.csv");
    let mut wtr = csv::Writer::from_path(&csv_path)
        .with_context(|| format!("create unified CSV: {}", csv_path.display()))?;
    wtr.write_record(["task_key", "suite", "score", "passed", "status", "duration_ms"])
        .with_context(|| "write CSV header")?;
    for r in results {
        wtr.write_record([
            &r.task_key,
            &r.suite,
            &format!("{:.3}", r.score),
            &r.passed.to_string(),
            r.status.as_str(),
            &r.duration_ms.to_string(),
        ])
        .with_context(|| "write CSV row")?;
    }
    wtr.flush()?;

    Ok(unified_dir)
}
#[cfg(test)]
mod tests {
    use super::{render_evalplus_markdown, render_lm_eval_markdown};
    use serde_json::Value;
    use std::fs;
    use std::path::Path;

    #[test]
    fn lm_eval_markdown_includes_metric_values() {
        let json_text = fs::read_to_string("tests/fixtures/eval/lm-eval-gsm8k-success.json")
            .expect("read fixture");
        let json: Value = serde_json::from_str(&json_text).expect("valid fixture json");
        let markdown =
            render_lm_eval_markdown("GSM8K eval report", &json, Path::new("/tmp/results.json"));

        assert!(markdown.contains("GSM8K eval report"));
        assert!(markdown.contains("exact_match,strict-match"));
        assert!(markdown.contains("1.000 (100.0%)"));
    }

    #[test]
    fn evalplus_markdown_renders_code_sample() {
        let jsonl = fs::read_to_string("tests/fixtures/eval/evalplus-humaneval-success.jsonl")
            .expect("read fixture");
        let markdown = render_evalplus_markdown(
            "EvalPlus HumanEval report",
            &jsonl,
            Path::new("/tmp/sample.jsonl"),
        );

        assert!(markdown.contains("EvalPlus HumanEval report"));
        assert!(markdown.contains("HumanEval/0"));
        assert!(markdown.contains("~~~python"));
    }
}
