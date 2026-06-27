use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
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
    pub quant_k: u8,
    pub quant_v: u8,
    pub note: String,
    pub source: RecSource,
}

#[derive(Debug, Clone)]
pub struct BenchmarkRun {
    pub context_size: u32,
    pub gen_speed: f64,
    pub gpu_layers: i32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub vram_mb: u32,
    /// Loaded from DB; reserved for model info display.
    // Reserved for future model-info display surface.
    #[allow(dead_code)]
    pub timestamp_ms: i64,
    /// Loaded from DB; reserved for model info display.
    // Reserved for future model-info display surface.
    #[allow(dead_code)]
    pub model_size_gb: f64,
}

#[derive(Debug, Clone)]
pub struct CatalogRecord {
    pub model_name: String,
    /// Populated during catalog scan; reserved for model management surfaces.
    // Reserved for future model management surface.
    #[allow(dead_code)]
    pub model_path: PathBuf,
    pub model_size_gb: f64,
    pub recommendation: Recommendation,
    pub benchmark: Option<BenchmarkRun>,
    /// Populated during catalog scan; reserved for model management surfaces.
    // Reserved for future model management surface.
    #[allow(dead_code)]
    pub benchmark_count: usize,
    pub source_priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogLoadIssueLevel {
    Warning,
    Error,
}

impl CatalogLoadIssueLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLoadIssue {
    pub level: CatalogLoadIssueLevel,
    pub message: String,
}

impl CatalogLoadIssue {
    fn warning(message: impl Into<String>) -> Self {
        Self {
            level: CatalogLoadIssueLevel::Warning,
            message: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            level: CatalogLoadIssueLevel::Error,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogLoadReport {
    pub records: Vec<CatalogRecord>,
    pub issues: Vec<CatalogLoadIssue>,
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
        if parts.len() < 4 {
            continue;
        }
        let model_name = parts[0].trim();
        if model_name.is_empty() {
            continue;
        }
        let Some(gpu_layers) = parts.get(1).and_then(|s| s.trim().parse().ok()) else {
            continue;
        };
        let Some(context_size) = parts.get(2).and_then(|s| s.trim().parse().ok()) else {
            continue;
        };
        let Some(quant_k) = parts.get(3).and_then(|s| s.trim().parse().ok()) else {
            continue;
        };
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
                quant_k,
                quant_v: quant_k,
                note,
                source: RecSource::Tuned,
            },
        );
    }
    presets
}

fn preset_file_has_entries(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    })
}

fn preset_file_has_invalid_entries(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        let parts: Vec<&str> = trimmed.splitn(5, '|').collect();
        if parts.len() < 4 {
            return true;
        }
        parts[0].trim().is_empty()
            || parts[1].trim().parse::<i32>().is_err()
            || parts[2].trim().parse::<u32>().is_err()
            || parts[3].trim().parse::<u8>().is_err()
    })
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
        let quant_k: u8 = field(section, "Quant KV:")
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        // Also try to parse separate Quant V field (newer format)
        let quant_v: u8 = field(section, "Quant V:")
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(quant_k);

        if context_size > 0 && gen_speed > 0.0 && gen_speed <= 100.0 && vram_mb > 0 {
            runs.push((
                normalize_model_key(&model_name),
                BenchmarkRun {
                    context_size,
                    gen_speed,
                    gpu_layers,
                    quant_k,
                    quant_v,
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
            quant_k: 1,
            quant_v: 1,
            note: "Heuristic MOE profile".into(),
            source: RecSource::Heuristic,
        };
    }
    if size_gb <= 8.0 {
        Recommendation {
            context_size: 16384,
            gpu_layers: -1,
            quant_k: 1,
            quant_v: 1,
            note: "Heuristic small-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else if size_gb <= 12.5 {
        Recommendation {
            context_size: 8192,
            gpu_layers: -1,
            quant_k: 1,
            quant_v: 1,
            note: "Heuristic medium-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else if size_gb <= 14.0 {
        Recommendation {
            context_size: 8192,
            gpu_layers: 32,
            quant_k: 1,
            quant_v: 1,
            note: "Heuristic large-model profile".into(),
            source: RecSource::Heuristic,
        }
    } else {
        Recommendation {
            context_size: 4096,
            gpu_layers: 28,
            quant_k: 1,
            quant_v: 1,
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
                && r.quant_k == rec.quant_k
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
                        quant_k: b.quant_k,
                        quant_v: b.quant_v,
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

async fn scan_models(model_dir: &Path) -> Result<Vec<(String, PathBuf, f64)>> {
    let mut entries = tokio::fs::read_dir(model_dir).await?;
    let mut models = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".gguf") {
            continue;
        }
        let size_gb = match fs::metadata(entry.path()).await {
            Ok(meta) => (meta.len() as f64 / 1_073_741_824.0 * 10.0).round() / 10.0,
            Err(_) => 0.0,
        };
        models.push((name, entry.path(), size_gb));
    }
    Ok(models)
}

async fn read_optional_sidecar(
    path: &Path,
    label: &str,
    missing_message: &str,
    read_failure_message: &str,
    models_present: bool,
    issues: &mut Vec<CatalogLoadIssue>,
) -> Option<String> {
    match fs::read_to_string(path).await {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if models_present {
                issues.push(CatalogLoadIssue::warning(format!(
                    "{label} file {} is missing; {missing_message}",
                    path.display()
                )));
            }
            None
        }
        Err(error) => {
            issues.push(CatalogLoadIssue::error(format!(
                "Failed to read {label} file {}: {error}; {read_failure_message}",
                path.display()
            )));
            None
        }
    }
}

pub async fn load_catalog_report(
    model_dir: &Path,
    preset_file: &Path,
    benchmark_file: &Path,
) -> Result<CatalogLoadReport> {
    let models = scan_models(model_dir).await?;
    let models_present = !models.is_empty();
    let mut issues = Vec::new();

    let preset_text = read_optional_sidecar(
        preset_file,
        "preset",
        "using heuristic recommendations instead",
        "using heuristic recommendations instead",
        models_present,
        &mut issues,
    )
    .await;
    let bench_text = read_optional_sidecar(
        benchmark_file,
        "benchmark",
        "benchmark data will be unavailable",
        "benchmark data will be unavailable",
        models_present,
        &mut issues,
    )
    .await;

    let presets = preset_text
        .as_deref()
        .map(parse_preset_text)
        .unwrap_or_default();
    if let Some(text) = preset_text.as_deref() {
        if preset_file_has_invalid_entries(text) {
            issues.push(CatalogLoadIssue::error(format!(
                "Preset file {} contains invalid entries; using only valid preset lines",
                preset_file.display()
            )));
        }
        if preset_file_has_entries(text) && presets.is_empty() && models_present {
            issues.push(CatalogLoadIssue::error(format!(
                "Preset file {} did not produce any usable preset entries",
                preset_file.display()
            )));
        }
    }

    let benchmarks = bench_text
        .as_deref()
        .map(parse_benchmark_text)
        .unwrap_or_default();
    if let Some(text) = bench_text.as_deref() {
        if !text.trim().is_empty() && benchmarks.is_empty() && models_present {
            issues.push(CatalogLoadIssue::error(format!(
                "Benchmark file {} did not produce any usable benchmark entries",
                benchmark_file.display()
            )));
        }
    }

    Ok(CatalogLoadReport {
        records: build_catalog(models, presets, benchmarks),
        issues,
    })
}

pub async fn catalog_signature(
    model_dir: &Path,
    preset_file: &Path,
    benchmark_file: &Path,
) -> Result<u64> {
    let mut hasher = DefaultHasher::new();

    hash_optional_path_state(&mut hasher, preset_file).await?;
    hash_optional_path_state(&mut hasher, benchmark_file).await?;

    match fs::symlink_metadata(model_dir).await {
        Ok(metadata) => {
            model_dir.hash(&mut hasher);
            metadata.is_dir().hash(&mut hasher);
            metadata.len().hash(&mut hasher);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .hash(&mut hasher);
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            "missing-model-dir".hash(&mut hasher);
            model_dir.hash(&mut hasher);
            return Ok(hasher.finish());
        }
        Err(error) => return Err(error.into()),
    }

    let mut entries = match fs::read_dir(model_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(hasher.finish()),
        Err(error) => return Err(error.into()),
    };

    let mut model_entries = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".gguf") {
            continue;
        }

        let path = entry.path();
        let symlink_meta = fs::symlink_metadata(&path).await.ok();
        let resolved_meta = fs::metadata(&path).await.ok();
        model_entries.push((
            name,
            symlink_meta
                .as_ref()
                .map(|meta| meta.len())
                .unwrap_or_default(),
            symlink_meta
                .as_ref()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .unwrap_or_default(),
            resolved_meta.is_some(),
            resolved_meta
                .as_ref()
                .map(|meta| meta.len())
                .unwrap_or_default(),
            resolved_meta
                .as_ref()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .unwrap_or_default(),
        ));
    }

    model_entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    model_entries.hash(&mut hasher);
    Ok(hasher.finish())
}

async fn hash_optional_path_state(hasher: &mut DefaultHasher, path: &Path) -> Result<()> {
    path.hash(hasher);
    match fs::symlink_metadata(path).await {
        Ok(metadata) => {
            true.hash(hasher);
            metadata.len().hash(hasher);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .hash(hasher);
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            false.hash(hasher);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_catalog_report, CatalogLoadIssueLevel, RecSource};
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-catalog-tests-{prefix}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            if root.exists() {
                std::fs::remove_dir_all(&root).unwrap();
            }
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn model_dir(&self) -> PathBuf {
            self.root.join("models")
        }

        fn preset_file(&self) -> PathBuf {
            self.model_dir().join("koboldcpp-presets.conf")
        }

        fn benchmark_file(&self) -> PathBuf {
            self.model_dir().join("bench-results.txt")
        }

        fn write_model(&self, name: &str) {
            std::fs::create_dir_all(self.model_dir()).unwrap();
            std::fs::write(self.model_dir().join(name), []).unwrap();
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn missing_sidecars_with_models_report_warnings() {
        let sandbox = TestSandbox::new("missing-sidecars");
        sandbox.write_model("alpha.gguf");

        let report = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(load_catalog_report(
                &sandbox.model_dir(),
                &sandbox.preset_file(),
                &sandbox.benchmark_file(),
            ))
            .expect("catalog report should succeed");

        assert_eq!(report.records.len(), 1);
        assert_eq!(report.issues.len(), 2);
        assert!(report
            .issues
            .iter()
            .all(|issue| issue.level == CatalogLoadIssueLevel::Warning));
    }

    #[test]
    fn invalid_preset_entries_report_error_and_fall_back_to_heuristics() {
        let sandbox = TestSandbox::new("invalid-preset");
        sandbox.write_model("alpha.gguf");
        std::fs::write(sandbox.preset_file(), "alpha.gguf|oops|4096|1|bad preset\n").unwrap();

        let report = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(load_catalog_report(
                &sandbox.model_dir(),
                &sandbox.preset_file(),
                &sandbox.benchmark_file(),
            ))
            .expect("catalog report should succeed");

        assert!(report.issues.iter().any(|issue| {
            issue.level == CatalogLoadIssueLevel::Error
                && issue.message.contains("contains invalid entries")
        }));
        assert_eq!(report.records.len(), 1);
        assert_eq!(
            report.records[0].recommendation.source,
            RecSource::Heuristic
        );
    }

    #[test]
    fn invalid_benchmark_file_reports_error() {
        let sandbox = TestSandbox::new("invalid-benchmark");
        sandbox.write_model("alpha.gguf");
        std::fs::write(
            sandbox.benchmark_file(),
            "--- alpha (2026-05-17)\nStatus: ok\nTotally wrong\n",
        )
        .unwrap();

        let report = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(load_catalog_report(
                &sandbox.model_dir(),
                &sandbox.preset_file(),
                &sandbox.benchmark_file(),
            ))
            .expect("catalog report should succeed");

        assert!(report.issues.iter().any(|issue| {
            issue.level == CatalogLoadIssueLevel::Error
                && issue
                    .message
                    .contains("did not produce any usable benchmark entries")
        }));
    }
}
