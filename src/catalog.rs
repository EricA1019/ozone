use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, PartialEq)]
pub enum RecSource {
    Tuned,
    Benchmarked,
    Heuristic,
}

impl RecSource {
    pub fn priority(&self) -> u8 {
        match self {
            RecSource::Tuned => 0,
            RecSource::Benchmarked => 1,
            RecSource::Heuristic => 2,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            RecSource::Tuned => "Tuned",
            RecSource::Benchmarked => "Bench",
            RecSource::Heuristic => "Heur",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub context_size: u32,
    pub gpu_layers: i32,
    pub quant_kv: u8,
    pub note: String,
    pub source: RecSource,
}

#[derive(Debug, Clone)]
pub struct BenchmarkRun {
    pub context_size: u32,
    pub gen_speed: f64,
    pub gpu_layers: i32,
    pub quant_kv: u8,
    pub vram_mb: u32,
    /// Loaded from DB; reserved for model info display.
    #[allow(dead_code)]
    pub timestamp_ms: i64,
    /// Loaded from DB; reserved for model info display.
    #[allow(dead_code)]
    pub model_size_gb: f64,
}

#[derive(Debug, Clone)]
pub struct CatalogRecord {
    pub model_name: String,
    /// Populated during catalog scan; reserved for model management surfaces.
    #[allow(dead_code)]
    pub model_path: PathBuf,
    pub model_size_gb: f64,
    pub recommendation: Recommendation,
    pub benchmark: Option<BenchmarkRun>,
    /// Populated during catalog scan; reserved for model management surfaces.
    #[allow(dead_code)]
    pub benchmark_count: usize,
    pub source_priority: u8,
}

fn normalize_model_key(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    base.to_lowercase()
}

pub fn parse_preset_text(text: &str) -> HashMap<String, Recommendation> {
    let mut presets = HashMap::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = t.splitn(5, '|').collect();
        if parts.is_empty() {
            continue;
        }
        let model_name = parts[0].trim();
        if model_name.is_empty() {
            continue;
        }
        let gpu_layers: i32 = parts
            .get(1)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(-1);
        let context_size: u32 = parts
            .get(2)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let quant_kv: u8 = parts
            .get(3)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1);
        let note = parts
            .get(4)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let key = normalize_model_key(model_name);
        presets.insert(
            key,
            Recommendation {
                context_size,
                gpu_layers,
                quant_kv,
                note,
                source: RecSource::Tuned,
            },
        );
    }
    presets
}

pub fn parse_benchmark_text(text: &str) -> Vec<(String, BenchmarkRun)> {
    let mut runs = Vec::new();
    let sections: Vec<&str> = text.split("\n---").collect();
    for section in &sections {
        let section = section.trim();
        if section.is_empty() {
            continue;
        }
        let header_line = section
            .lines()
            .next()
            .unwrap_or("")
            .trim_start_matches('-')
            .trim();
        let (model_raw, _ts) = if let Some(paren) = header_line.rfind('(') {
            (&header_line[..paren], &header_line[paren..])
        } else {
            (header_line, "")
        };
        let model_name = format!("{}.gguf", model_raw.trim().trim_end_matches("---").trim());

        fn field(section: &str, key: &str) -> Option<String> {
            section
                .lines()
                .find(|l| l.starts_with(key))
                .map(|l| l[key.len()..].trim().to_string())
        }

        let size_gb: f64 = field(section, "Size:")
            .as_deref()
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let gpu_layers: i32 = field(section, "GPU Layers:")
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1);
        let context_size: u32 = field(section, "Max Context:")
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let gen_speed: f64 = field(section, "Gen Speed:")
            .as_deref()
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let vram_mb: u32 = field(section, "VRAM:")
            .as_deref()
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let quant_kv: u8 = field(section, "Quant KV:")
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        if context_size > 0 && gen_speed > 0.0 && gen_speed <= 100.0 && vram_mb > 0 {
            runs.push((
                normalize_model_key(&model_name),
                BenchmarkRun {
                    context_size,
                    gen_speed,
                    gpu_layers,
                    quant_kv,
                    vram_mb,
                    timestamp_ms: 0,
                    model_size_gb: size_gb,
                },
            ));
        }
    }
    runs
}

fn heuristic_recommendation(name: &str, size_gb: f64) -> Recommendation {
    if name.to_lowercase().contains("moe") {
        return Recommendation {
            context_size: 12288,
            gpu_layers: -1,
            quant_kv: 1,
            note: "Heuristic MOE profile".into(),
            source: RecSource::Heuristic,
        };
    }
    if size_gb <= 8.0 {
        Recommendation {
            context_size: 16384,
            gpu_layers: -1,
            quant_kv: 1,
            note: "Heuristic small-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else if size_gb <= 12.5 {
        Recommendation {
            context_size: 8192,
            gpu_layers: -1,
            quant_kv: 1,
            note: "Heuristic medium-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else if size_gb <= 14.0 {
        Recommendation {
            context_size: 8192,
            gpu_layers: 32,
            quant_kv: 1,
            note: "Heuristic large-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else {
        Recommendation {
            context_size: 4096,
            gpu_layers: 28,
            quant_kv: 1,
            note: "Heuristic x-large-model profile".into(),
            source: RecSource::Heuristic,
        }
    }
}

fn select_best_benchmark(
    runs: &[(String, BenchmarkRun)],
    key: &str,
    rec: Option<&Recommendation>,
) -> Option<BenchmarkRun> {
    let matching: Vec<&BenchmarkRun> = runs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, r)| r)
        .collect();
    if matching.is_empty() {
        return None;
    }
    if let Some(rec) = rec {
        if let Some(exact) = matching.iter().find(|r| {
            r.context_size == rec.context_size
                && r.gpu_layers == rec.gpu_layers
                && r.quant_kv == rec.quant_kv
        }) {
            return Some((*exact).clone());
        }
    }
    matching
        .iter()
        .max_by(|a, b| {
            a.gen_speed
                .partial_cmp(&b.gen_speed)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| (*r).clone())
}

pub fn build_catalog(
    models: Vec<(String, PathBuf, f64)>,
    presets: HashMap<String, Recommendation>,
    benchmarks: Vec<(String, BenchmarkRun)>,
) -> Vec<CatalogRecord> {
    let mut records: Vec<CatalogRecord> = models
        .into_iter()
        .map(|(name, path, size_gb)| {
            let key = normalize_model_key(&name);
            let (recommendation, benchmark) = if let Some(preset) = presets.get(&key) {
                let bench = select_best_benchmark(&benchmarks, &key, Some(preset));
                (preset.clone(), bench)
            } else {
                let bench = select_best_benchmark(&benchmarks, &key, None);
                let rec = if let Some(ref b) = bench {
                    Recommendation {
                        context_size: b.context_size,
                        gpu_layers: b.gpu_layers,
                        quant_kv: b.quant_kv,
                        note: "Best known benchmark".into(),
                        source: RecSource::Benchmarked,
                    }
                } else {
                    heuristic_recommendation(&name, size_gb)
                };
                (rec, bench)
            };
            let benchmark_count = benchmarks.iter().filter(|(k, _)| k == &key).count();
            let source_priority = recommendation.source.priority();
            CatalogRecord {
                model_name: name,
                model_path: path,
                model_size_gb: size_gb,
                recommendation,
                benchmark,
                benchmark_count,
                source_priority,
            }
        })
        .collect();
    records.sort_by(|a, b| {
        if a.source_priority != b.source_priority {
            return a.source_priority.cmp(&b.source_priority);
        }
        let a_speed = a.benchmark.as_ref().map(|b| b.gen_speed).unwrap_or(-1.0);
        let b_speed = b.benchmark.as_ref().map(|b| b.gen_speed).unwrap_or(-1.0);
        b_speed
            .partial_cmp(&a_speed)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    records
}

pub async fn load_catalog(
    model_dir: &Path,
    preset_file: &Path,
    benchmark_file: &Path,
) -> Result<Vec<CatalogRecord>> {
    let preset_text = fs::read_to_string(preset_file).await.unwrap_or_default();
    let bench_text = fs::read_to_string(benchmark_file).await.unwrap_or_default();

    let mut entries = tokio::fs::read_dir(model_dir).await?;
    let mut models = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".gguf") {
            continue;
        }
        // Use fs::metadata (follows symlinks); fall back to 0.0 GB for broken symlinks
        let size_gb = match fs::metadata(entry.path()).await {
            Ok(meta) => (meta.len() as f64 / 1_073_741_824.0 * 10.0).round() / 10.0,
            Err(_) => 0.0,
        };
        models.push((name, entry.path(), size_gb));
    }

    let presets = parse_preset_text(&preset_text);
    let benchmarks = parse_benchmark_text(&bench_text);
    Ok(build_catalog(models, presets, benchmarks))
}
