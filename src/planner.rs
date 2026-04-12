use crate::hardware::HardwareProfile;
use crate::catalog::CatalogRecord;

const MIB_PER_GIB: f64 = 1024.0;
const VRAM_HEADROOM_RATIO: f64 = 0.9;

#[derive(Debug, Clone, PartialEq)]
pub enum RecommendationMode { VramFirst, MixedMemory, CpuOnly }

impl RecommendationMode {
    pub fn label(&self) -> &'static str {
        match self {
            RecommendationMode::VramFirst => "VRAM",
            RecommendationMode::MixedMemory => "Mixed",
            RecommendationMode::CpuOnly => "CPU",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub model_name: String,
    pub context_size: u32,
    pub gpu_layers: i32,
    pub quant_kv: u8,
    pub threads: Option<u32>,
    pub blas_threads: Option<u32>,
    pub mode: RecommendationMode,
    pub rationale: String,
    #[allow(dead_code)]
    pub estimated: bool,
    #[allow(dead_code)]
    pub estimated_vram_mb: u32,
    #[allow(dead_code)]
    pub source: String,
}

pub fn estimate_total_layers(size_gb: f64) -> u32 {
    let s = size_gb.max(0.1);
    if s <= 8.0 { 32 }
    else if s <= 12.5 { 40 }
    else if s <= 20.0 { 48 }
    else { 64 }
}

fn quant_kv_memory_factor(quant_kv: u8) -> f64 {
    let level = quant_kv.max(1) as f64;
    1.0 / (1.0 + (level - 1.0) * 0.35)
}

fn gpu_layer_fraction(gpu_layers: i32, total_layers: u32) -> f64 {
    if gpu_layers < 0 { return 1.0; }
    if gpu_layers == 0 { return 0.0; }
    (gpu_layers as f64 / total_layers as f64).clamp(0.0, 1.0)
}

pub fn estimate_vram_mb(context_size: u32, gpu_layers: i32, size_gb: f64, quant_kv: u8, total_layers: u32) -> u32 {
    let safe_size = size_gb.max(0.1);
    let safe_ctx = context_size.max(1024) as f64;
    let clamp_layers = if gpu_layers < 0 { total_layers as i32 } else { gpu_layers.min(total_layers as i32) };
    let layer_frac = gpu_layer_fraction(clamp_layers, total_layers);
    if layer_frac <= 0.0 { return 0; }
    let quant_factor = quant_kv_memory_factor(quant_kv);
    let ctx_mult = safe_ctx / 4096.0;
    let model_weights_mb = safe_size * MIB_PER_GIB * layer_frac;
    let kv_per_4k_mb = (safe_size * 20.0).max(96.0);
    let kv_cache_mb = kv_per_4k_mb * ctx_mult * quant_factor * (0.25 + layer_frac * 0.75);
    let overhead_mb = 320.0 + safe_size * 12.0 + ctx_mult * 40.0;
    (model_weights_mb + kv_cache_mb + overhead_mb).round() as u32
}

fn estimate_ram_need_mb(context_size: u32, size_gb: f64, quant_kv: u8) -> u32 {
    let safe_size = size_gb.max(0.1);
    let safe_ctx = context_size.max(1024) as f64;
    let quant_factor = quant_kv_memory_factor(quant_kv);
    let ctx_mult = safe_ctx / 4096.0;
    let base_mb = safe_size * MIB_PER_GIB * 1.2;
    let kv_mb = (safe_size * 24.0).max(128.0) * ctx_mult * quant_factor;
    (base_mb + kv_mb).round() as u32
}

pub fn classify_mode(gpu_layers: i32, total_layers: u32) -> RecommendationMode {
    if gpu_layers == 0 { return RecommendationMode::CpuOnly; }
    if gpu_layers < 0 { return RecommendationMode::VramFirst; }
    if gpu_layers >= total_layers as i32 { return RecommendationMode::VramFirst; }
    RecommendationMode::MixedMemory
}

pub fn recommend_threads(hw: &HardwareProfile, mode: &RecommendationMode) -> (Option<u32>, Option<u32>) {
    let logical = hw.cpu_logical.max(1) as u32;
    let physical = hw.cpu_physical.max(1) as u32;
    match mode {
        RecommendationMode::CpuOnly => (Some(logical), Some(physical)),
        RecommendationMode::MixedMemory => (Some(physical), Some((physical / 2).max(1))),
        RecommendationMode::VramFirst => (None, None),
    }
}

pub fn plan_launch(record: &CatalogRecord, hw: &HardwareProfile) -> LaunchPlan {
    let rec = &record.recommendation;
    let size_gb = record.model_size_gb.max(0.1);
    let total_layers = estimate_total_layers(size_gb);

    let context_size = rec.context_size.max(1024);
    let mut gpu_layers = if rec.gpu_layers < 0 { -1i32 } else { rec.gpu_layers.min(total_layers as i32) };
    let mut quant_kv = rec.quant_kv.max(1);
    let mut rationale = match rec.source {
        crate::catalog::RecSource::Tuned => format!("Using tuned preset: {}", rec.note),
        crate::catalog::RecSource::Benchmarked => format!("Using benchmark-backed recommendation: {}", rec.note),
        crate::catalog::RecSource::Heuristic => format!("Using heuristic fallback: {}", rec.note),
    };
    let mut estimated = matches!(rec.source, crate::catalog::RecSource::Heuristic);

    let is_heuristic = matches!(rec.source, crate::catalog::RecSource::Heuristic);

    if is_heuristic {
        let ram_need = estimate_ram_need_mb(context_size, size_gb, quant_kv);
        if hw.ram_free_mb > 0 && hw.ram_free_mb < (ram_need as f64 * 1.35) as u64 {
            quant_kv = quant_kv.max(2);
        }
    }

    if is_heuristic {
        if let Some(gpu) = hw.gpu.as_ref() {
            let gpu_budget = (gpu.free_mb as f64 * VRAM_HEADROOM_RATIO) as u32;
            let preferred = if gpu_layers < 0 { total_layers as i32 } else { gpu_layers };
            let preferred_vram = estimate_vram_mb(context_size, preferred, size_gb, quant_kv, total_layers);

            if preferred_vram > gpu_budget {
                let mut selected_layers = 0i32;
                for l in (1..preferred).rev() {
                    let v = estimate_vram_mb(context_size, l, size_gb, quant_kv, total_layers);
                    if v <= gpu_budget {
                        selected_layers = l;
                        break;
                    }
                }
                gpu_layers = selected_layers;
                rationale = if selected_layers > 0 {
                    format!("Full-GPU VRAM ({preferred_vram} MiB) exceeds budget ({gpu_budget} MiB); mixed-memory ({selected_layers}/{total_layers} layers).")
                } else {
                    format!("Full-GPU VRAM ({preferred_vram} MiB) exceeds budget ({gpu_budget} MiB); CPU-only mode.")
                };
                estimated = true;
            } else {
                rationale = format!("Estimated VRAM ({preferred_vram} MiB) fits within GPU budget ({gpu_budget} MiB).");
            }
        }
    }

    let mode = classify_mode(gpu_layers, total_layers);
    let estimated_vram_mb = record.benchmark.as_ref().map(|b| b.vram_mb)
        .filter(|&v| v > 0)
        .unwrap_or_else(|| estimate_vram_mb(context_size, gpu_layers, size_gb, quant_kv, total_layers));

    let (threads, blas_threads) = recommend_threads(hw, &mode);

    LaunchPlan {
        model_name: record.model_name.clone(),
        context_size,
        gpu_layers,
        quant_kv,
        threads,
        blas_threads,
        mode,
        rationale,
        estimated,
        estimated_vram_mb,
        source: rec.source.label().to_string(),
    }
}
