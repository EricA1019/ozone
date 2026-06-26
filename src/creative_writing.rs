//! Creative Writing Evaluation — diversity-metric-based scoring.
//!
//! This module implements Phase 2 of the eval suite expansion.
//! Scoring uses text-only diversity metrics:
//!   - distinct_2:   ratio of unique bigrams to total bigrams (vocabulary variety)
//!   - self_bleu:     average BLEU score of each sentence vs all others (repetition)
//!   - repetition_ratio: fraction of n-grams appearing more than once
//!   - length:        total token count (short = truncated; long = good)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A single creative writing prompt.
#[derive(Debug, Clone, Deserialize)]
pub struct CreativePrompt {
    pub id: String,
    pub category: String,
    pub prompt: String,
    pub max_tokens: u32,
}

/// The prompt bank file format.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptBank {
    pub prompts: Vec<CreativePrompt>,
}

/// Load the prompt bank from the TOML file.
pub fn load_prompt_bank(root: &Path) -> Result<Vec<CreativePrompt>> {
    let path = root.join("contrib/evals/prompts/creative_writing.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let bank: PromptBank =
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(bank.prompts)
}

/// Diversity scores for a single generation.
#[derive(Debug, Clone)]
pub struct DiversityScores {
    pub distinct_2: f64,
    pub self_bleu: f64,
    pub repetition_ratio: f64,
    pub length: usize,
}

/// Compute diversity scores for generated text.
///
/// - `distinct_2`: Count unique bigrams / count total bigrams. Higher = more varied.
/// - `self_bleu`: Treat each sentence as a reference and compute micro-BLEU-1
///   against all other sentences. Higher = more self-similar (worse).
/// - `repetition_ratio`: Proportion of word unigrams that appear >1 time.
/// - `length`: Total number of whitespace-delimited tokens.
pub fn compute_diversity(text: &str) -> DiversityScores {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let len = tokens.len();

    // distinct-2 (bigram diversity)
    let distinct_2 = if len >= 2 {
        let bigrams: Vec<Vec<&str>> = tokens.windows(2).map(|w| w.to_vec()).collect();
        let total = bigrams.len() as f64;
        let unique_count = bigrams
            .iter()
            .map(|b| format!("{} {}", b[0], b[1]))
            .collect::<HashSet<_>>()
            .len() as f64;
        if total > 0.0 {
            unique_count / total
        } else {
            0.0
        }
    } else {
        0.0
    };

    // self-BLEU-1: treat each sentence as reference
    let sentences: Vec<&str> = text
        .split(['.', '!', '?', '\n'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let self_bleu = if sentences.len() >= 2 {
        let total_precision: f64 = sentences
            .iter()
            .map(|candidate| {
                let cand_tokens: HashSet<&str> = candidate.split_whitespace().collect();
                if cand_tokens.is_empty() {
                    return 0.0;
                }
                let mut ref_counts: HashMap<&str, usize> = HashMap::new();
                let mut total_count = 0usize;
                for ref_sent in &sentences {
                    if ref_sent == candidate {
                        continue;
                    }
                    let ref_tokens: Vec<&str> = ref_sent.split_whitespace().collect();
                    let mut seen = HashSet::new();
                    for t in ref_tokens {
                        if cand_tokens.contains(t) && seen.insert(t) {
                            *ref_counts.entry(t).or_insert(0) += 1;
                        }
                    }
                    total_count += 1;
                }
                if total_count == 0 {
                    return 0.0;
                }
                let clipped: usize = cand_tokens
                    .iter()
                    .map(|t| ref_counts.get(t).copied().unwrap_or(0))
                    .sum();
                clipped as f64 / cand_tokens.len() as f64
            })
            .sum();
        total_precision / sentences.len() as f64
    } else {
        0.0
    };

    // repetition ratio (unigram)
    let repetition_ratio = if len > 0 {
        let unique_unigrams: HashSet<&str> = tokens.iter().copied().collect();
        1.0 - (unique_unigrams.len() as f64 / len as f64)
    } else {
        0.0
    };

    DiversityScores {
        distinct_2,
        self_bleu,
        repetition_ratio,
        length: len,
    }
}

/// Run the creative writing eval for a model.
/// Generates text for each prompt × temperature combination,
/// computes diversity scores, writes CSV, returns the CSV path.
pub async fn run_creative_writing_eval(
    model_name: &str,
    prompts: &[CreativePrompt],
    base_url: &str,
    output_dir: &Path,
) -> Result<PathBuf> {
    let temperatures: &[f64] = &[0.0, 0.4, 0.7, 1.0];
    let csv_path = output_dir.join(format!("{model_name}_creative.csv"));
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    std::fs::create_dir_all(output_dir)?;
    let mut writer = csv::Writer::from_path(&csv_path)?;
    writer.write_record([
        "model",
        "prompt_id",
        "category",
        "temperature",
        "distinct_2",
        "self_bleu",
        "repetition_ratio",
        "length",
        "timestamp",
    ])?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    for prompt in prompts {
        for &temp in temperatures {
            eprintln!("  Generating: prompt='{}', T={}...", prompt.id, temp);
            let text = generate_one(&client, prompt, temp, base_url)
                .await
                .unwrap_or_else(|e| format!("[ERROR: {e}]"));

            let scores = compute_diversity(&text);
            writer.write_record([
                model_name,
                &prompt.id,
                &prompt.category,
                &temp.to_string(),
                &format!("{:.4}", scores.distinct_2),
                &format!("{:.4}", scores.self_bleu),
                &format!("{:.4}", scores.repetition_ratio),
                &scores.length.to_string(),
                &timestamp,
            ])?;
        }
    }

    writer.flush()?;
    Ok(csv_path)
}

/// Generate text for a single prompt × temperature combination via the OpenAI-compatible API.
async fn generate_one(
    client: &reqwest::Client,
    prompt: &CreativePrompt,
    temperature: f64,
    base_url: &str,
) -> Result<String> {
    let url = format!("{}/v1/completions", base_url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "prompt": prompt.prompt,
        "max_tokens": prompt.max_tokens,
        "temperature": temperature,
        "stream": false,
    });

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .with_context(|| {
            format!(
                "API request failed for prompt '{}' at T={}",
                prompt.id, temperature
            )
        })?;

    let body: serde_json::Value = resp
        .json()
        .await
        .with_context(|| format!("failed to parse API response for prompt '{}'", prompt.id))?;

    let text = body["choices"][0]["text"]
        .as_str()
        .or_else(|| body["choices"][0]["message"]["content"].as_str())
        .unwrap_or("[empty response]")
        .to_string();

    Ok(text)
}

/// Build a markdown report from the CSV output.
pub fn build_creative_report(csv_path: &Path) -> Result<String> {
    let mut rdr = csv::Reader::from_path(csv_path)?;
    let mut rows: Vec<HashMap<String, String>> = Vec::new();
    let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();

    for result in rdr.records() {
        let record = result?;
        let mut row = HashMap::new();
        for (i, value) in record.iter().enumerate() {
            if let Some(h) = headers.get(i) {
                row.insert(h.clone(), value.to_string());
            }
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Ok("No creative writing results found.".to_string());
    }

    let model = rows[0]
        .get("model")
        .map(|s| s.as_str())
        .unwrap_or("unknown");
    let mut md = String::new();

    md.push_str(&format!("# Creative Writing Report — {model}\n\n"));
    md.push_str("## Per-Temperature Score Summary\n\n");
    md.push_str(
        "| Temperature | Avg Distinct-2 | Avg Self-BLEU | Avg Repetition Ratio | Avg Length |\n",
    );
    md.push_str(
        "|------------|---------------|---------------|---------------------|------------|\n",
    );

    let mut by_temp: HashMap<String, Vec<&HashMap<String, String>>> = HashMap::new();
    for row in &rows {
        let temp = row.get("temperature").cloned().unwrap_or_default();
        by_temp.entry(temp).or_default().push(row);
    }

    let mut temp_sorted: Vec<&String> = by_temp.keys().collect();
    temp_sorted.sort_by(|a, b| {
        a.parse::<f64>()
            .unwrap_or(0.0)
            .partial_cmp(&b.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for temp_key in &temp_sorted {
        let group = &by_temp[*temp_key];
        let n = group.len() as f64;
        let avg_d2: f64 = group
            .iter()
            .filter_map(|r| r.get("distinct_2").and_then(|v| v.parse::<f64>().ok()))
            .sum::<f64>()
            / n;
        let avg_sb: f64 = group
            .iter()
            .filter_map(|r| r.get("self_bleu").and_then(|v| v.parse::<f64>().ok()))
            .sum::<f64>()
            / n;
        let avg_rr: f64 = group
            .iter()
            .filter_map(|r| {
                r.get("repetition_ratio")
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .sum::<f64>()
            / n;
        let avg_len: f64 = group
            .iter()
            .filter_map(|r| r.get("length").and_then(|v| v.parse::<f64>().ok()))
            .sum::<f64>()
            / n;

        md.push_str(&format!(
            "| T={:<4} | {:.4} | {:.4} | {:.4} | {:.1} |\n",
            temp_key, avg_d2, avg_sb, avg_rr, avg_len,
        ));
    }

    md.push_str("\n## Interpretation\n\n");
    md.push_str("- **Distinct-2** (higher is better): Bigram diversity. A high value means the model uses a varied vocabulary.\n");
    md.push_str("- **Self-BLEU** (lower is better): Self-similarity across sentences. High values indicate repetitive output.\n");
    md.push_str("- **Repetition Ratio** (lower is better): Fraction of repeated words. High values suggest the model is stuck in loops.\n");
    md.push_str("- **Length** (higher is better for open-ended prompts): Generation length in tokens. Very short may mean truncation.\n\n");
    md.push_str("The sweet-spot temperature maximizes distinct-2 while minimizing self-BLEU and repetition.\n");

    Ok(md)
}
