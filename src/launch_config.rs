use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::prefs::ModelLaunchOverride;

const GGUF_METADATA_LABEL: &str = "GGUF metadata";
#[cfg(not(any(feature = "profiling-ui", feature = "sweep")))]
const SIZE_HEURISTIC_LABEL: &str = "Size heuristic";

const MIB_PER_GIB: f64 = 1024.0;
const VRAM_HEADROOM_RATIO: f64 = 0.9;

// ── VRAM estimation constants ──────────────────────────────────────────
// These are empirically calibrated against llama.cpp server startup
// measurements on NVIDIA RTX 3060 (12GB) and RTX 4090 (24GB) across
// model sizes from 1B to 13B parameters. They are approximations —
// actual VRAM usage varies with backend version, quantization, and
// GPU driver. Formula: overhead + model_weights + kv_cache.

/// Minimum model size (GB) to avoid divide-by-zero in estimation.
const MIN_MODEL_SIZE_GB: f64 = 0.1;
/// Minimum context size used for VRAM/RAM estimation to avoid
/// zero-context edge cases with very small models.
const MIN_CONTEXT_FOR_ESTIMATE: u32 = 1024;
/// Reference context size used as the denominator when scaling
/// context-dependent terms (e.g., KV cache). All context multipliers
/// are relative to 4096 tokens.
const CTX_ESTIMATE_REFERENCE: f64 = 4096.0;

/// Threshold model sizes (GB) for heuristic total-layer estimates.
/// Below SMALL_MODEL_LIMIT_GB, models typically have ≤ SMALL_MODEL_LAYERS.
const SMALL_MODEL_LIMIT_GB: f64 = 8.0;
const SMALL_MODEL_LAYERS: u32 = 32;
/// Between small and medium, models typically have ≤ MEDIUM_MODEL_LAYERS.
const MEDIUM_MODEL_LIMIT_GB: f64 = 12.5;
const MEDIUM_MODEL_LAYERS: u32 = 40;
/// Up to large threshold, models typically have ≤ LARGE_MODEL_LAYERS.
const LARGE_MODEL_LIMIT_GB: f64 = 20.0;
const LARGE_MODEL_LAYERS: u32 = 48;
/// Beyond large, fallback to a conservative estimate.
const FALLBACK_MODEL_LAYERS: u32 = 64;

/// Quantization memory reduction factor per level above q1.
/// q1 = full precision (1.0×), q2 = 0.74×, q3 = 0.59×, etc.
const QUANT_MEMORY_REDUCTION: f64 = 0.35;

/// Base VRAM overhead (MiB) independent of model size or context.
const VRAM_OVERHEAD_BASE_MIB: f64 = 320.0;
/// Additional VRAM overhead per GiB of model size.
const VRAM_OVERHEAD_PER_GIB: f64 = 12.0;
/// Additional VRAM overhead per context multiplier (ctx / 4096).
const VRAM_OVERHEAD_PER_CTX: f64 = 40.0;

/// Base KV cache allocation (MiB) per 4096 tokens of context.
const VRAM_KV_PER_4K_BASE_MIB: f64 = 20.0;
/// Floor KV cache allocation (MiB) regardless of model size.
const VRAM_KV_PER_4K_FLOOR_MIB: f64 = 96.0;
/// KV cache scaling: minimum fraction when GPU layers = 0.
const KV_SCALE_MIN_GPU_FRAC: f64 = 0.25;
/// KV cache scaling: maximum fraction when GPU layers = total.
const KV_SCALE_MAX_GPU_FRAC: f64 = 0.75;

// ── RAM estimation constants ───────────────────────────────────────────
// These model CPU-side memory usage when layers are offloaded to GPU.
// Derived from the same calibration data as the VRAM constants.

/// Base RAM overhead (MiB) independent of model size or context.
const RAM_OVERHEAD_BASE_MIB: f64 = 384.0;
/// Additional RAM overhead per GiB of model size.
const RAM_OVERHEAD_PER_GIB: f64 = 14.0;
/// Additional RAM overhead per context multiplier (ctx / 4096).
const RAM_OVERHEAD_PER_CTX: f64 = 48.0;

/// Minimum CPU fraction for RAM base weight scaling.
const RAM_BASE_MIN_CPU_FRAC: f64 = 0.18;
/// CPU fraction scaling factor added to base RAM weight.
const RAM_BASE_CPU_SCALE: f64 = 1.02;
/// Base RAM KV allocation (MiB) per 4096 tokens of context.
const RAM_KV_PER_4K_BASE_MIB: f64 = 24.0;
/// Floor RAM KV allocation (MiB) regardless of model size.
const RAM_KV_PER_4K_FLOOR_MIB: f64 = 128.0;
/// RAM KV cache scaling: minimum fraction when CPU fraction = 0.
const RAM_KV_MIN_CPU_FRAC: f64 = 0.45;
/// RAM KV cache scaling: maximum fraction when CPU fraction = 1.
const RAM_KV_MAX_CPU_FRAC: f64 = 0.55;
/// Stepped context sizes for the configure hub UI.
///
/// Chosen to match common llama.cpp power-of-two boundaries (4K → 256K)
/// with a non-linear step at the high end. 128K (131072) is skipped in
/// favor of 256K (262144) because most GGUF models in the RC target range
/// (1B–13B) support either 32K or context lengths well above 128K.
/// The 24K and 49K steps provide intermediate headroom for mixed-recall
/// workloads without overshooting VRAM budgets.
pub const CONFIGURE_CONTEXT_STEPS: [u32; 8] =
    [4096, 8192, 16384, 24576, 32768, 49152, 65536, 262144];

/// Default CPU thread count when none is specified by the user or profile.
pub const DEFAULT_THREADS: u32 = 8;

pub use ozone_core::planner::{LaunchPlan, RecommendationMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigureWarningSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureWarning {
    pub severity: ConfigureWarningSeverity,
    pub message: String,
}

/// Estimate the total number of transformer layers from model file size.
///
/// Uses a heuristic of ~0.6 GB per layer, which is typical for 7B–70B
/// parameter models at Q4_K_M quantization.
pub fn estimate_total_layers(size_gb: f64) -> u32 {
    let s = size_gb.max(0.1);
    if s <= SMALL_MODEL_LIMIT_GB {
        SMALL_MODEL_LAYERS
    } else if s <= MEDIUM_MODEL_LIMIT_GB {
        MEDIUM_MODEL_LAYERS
    } else if s <= LARGE_MODEL_LIMIT_GB {
        LARGE_MODEL_LAYERS
    } else {
        FALLBACK_MODEL_LAYERS
    }
}

fn quant_kv_memory_factor(quant_kv: u8) -> f64 {
    let level = quant_kv.max(1) as f64;
    1.0 / (1.0 + (level - 1.0) * QUANT_MEMORY_REDUCTION)
}

/// Average memory factor for asymmetric K/V quantization.
fn asymmetric_kv_factor(quant_k: u8, quant_v: u8) -> f64 {
    let k = quant_kv_memory_factor(quant_k);
    let v = quant_kv_memory_factor(quant_v);
    (k + v) / 2.0
}

fn gpu_layer_fraction(gpu_layers: i32, total_layers: u32) -> f64 {
    if gpu_layers < 0 {
        return 1.0;
    }
    if gpu_layers == 0 {
        return 0.0;
    }
    (gpu_layers as f64 / total_layers as f64).clamp(0.0, 1.0)
}

/// Calculate how many layers remain CPU-resident after GPU offloading.
///
/// A negative `gpu_layers` value means "all layers on GPU".
pub fn estimate_cpu_resident_layers(gpu_layers: i32, total_layers: u32) -> u32 {
    let gpu_layers = if gpu_layers < 0 {
        total_layers
    } else {
        gpu_layers.clamp(0, total_layers as i32) as u32
    };
    total_layers.saturating_sub(gpu_layers)
}

/// Estimate VRAM usage for a given model config.
///
/// Accounts for model weights (at the given quantization), KV cache at the
/// given context length, and a system overhead margin.
pub fn estimate_vram_mb(
    context_size: u32,
    gpu_layers: i32,
    size_gb: f64,
    quant_k: u8,
    quant_v: u8,
    total_layers: u32,
) -> u32 {
    let safe_size = size_gb.max(MIN_MODEL_SIZE_GB);
    let safe_ctx = context_size.max(MIN_CONTEXT_FOR_ESTIMATE) as f64;
    let clamp_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers.min(total_layers as i32)
    };
    let layer_frac = gpu_layer_fraction(clamp_layers, total_layers);
    let ctx_mult = safe_ctx / CTX_ESTIMATE_REFERENCE;
    let overhead_mb = VRAM_OVERHEAD_BASE_MIB + safe_size * VRAM_OVERHEAD_PER_GIB + ctx_mult * VRAM_OVERHEAD_PER_CTX;
    if layer_frac <= 0.0 {
        return overhead_mb.round() as u32;
    }
    let quant_factor = asymmetric_kv_factor(quant_k, quant_v);
    let model_weights_mb = safe_size * MIB_PER_GIB * layer_frac;
    let kv_per_4k_mb = (safe_size * VRAM_KV_PER_4K_BASE_MIB).max(VRAM_KV_PER_4K_FLOOR_MIB);
    let kv_cache_mb = kv_per_4k_mb * ctx_mult * quant_factor * (KV_SCALE_MIN_GPU_FRAC + layer_frac * KV_SCALE_MAX_GPU_FRAC);
    (model_weights_mb + kv_cache_mb + overhead_mb).round() as u32
}

pub fn estimate_ram_mb(
    context_size: u32,
    gpu_layers: i32,
    size_gb: f64,
    quant_k: u8,
    quant_v: u8,
    total_layers: u32,
) -> u32 {
    let safe_size = size_gb.max(MIN_MODEL_SIZE_GB);
    let safe_ctx = context_size.max(MIN_CONTEXT_FOR_ESTIMATE) as f64;
    let quant_factor = asymmetric_kv_factor(quant_k, quant_v);
    let ctx_mult = safe_ctx / CTX_ESTIMATE_REFERENCE;
    let clamp_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers.min(total_layers as i32)
    };
    let gpu_fraction = gpu_layer_fraction(clamp_layers, total_layers);
    let cpu_fraction = 1.0 - gpu_fraction;
    let base_mb = safe_size * MIB_PER_GIB * (RAM_BASE_MIN_CPU_FRAC + cpu_fraction * RAM_BASE_CPU_SCALE);
    let kv_mb =
        (safe_size * RAM_KV_PER_4K_BASE_MIB).max(RAM_KV_PER_4K_FLOOR_MIB) * ctx_mult * quant_factor * (RAM_KV_MIN_CPU_FRAC + cpu_fraction * RAM_KV_MAX_CPU_FRAC);
    let overhead_mb = RAM_OVERHEAD_BASE_MIB + safe_size * RAM_OVERHEAD_PER_GIB + ctx_mult * RAM_OVERHEAD_PER_CTX;
    (base_mb + kv_mb + overhead_mb).round() as u32
}

pub fn fit_gpu_layers_to_budget(
    context_size: u32,
    size_gb: f64,
    quant_k: u8,
    quant_v: u8,
    total_layers: u32,
    budget_mb: u32,
) -> Option<i32> {
    let mut lo = 0i32;
    let mut hi = total_layers as i32;
    let mut best = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let est = estimate_vram_mb(context_size, mid, size_gb, quant_k, quant_v, total_layers);
        if est <= budget_mb {
            best = Some(mid);
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }

    best
}

pub fn classify_mode(gpu_layers: i32, total_layers: u32) -> RecommendationMode {
    if gpu_layers == 0 {
        return RecommendationMode::CpuOnly;
    }
    if gpu_layers < 0 {
        return RecommendationMode::VramFirst;
    }
    if gpu_layers >= total_layers as i32 {
        return RecommendationMode::VramFirst;
    }
    RecommendationMode::MixedMemory
}

pub fn recommend_threads(
    hw: &HardwareProfile,
    mode: &RecommendationMode,
) -> (Option<u32>, Option<u32>) {
    let logical = hw.cpu_logical.max(1) as u32;
    let physical = hw.cpu_physical.max(1) as u32;
    match mode {
        RecommendationMode::CpuOnly => (Some(logical), Some(physical)),
        RecommendationMode::MixedMemory => (Some(physical), Some((physical / 2).max(1))),
        RecommendationMode::VramFirst => (None, None),
    }
}

fn launch_topology(record: &CatalogRecord) -> (u32, String, Option<String>) {
    let fallback_layers = estimate_total_layers(record.model_size_gb.max(0.1));
    inspect_launch_topology(&record.model_path, fallback_layers)
}

#[cfg(any(feature = "profiling-ui", feature = "sweep"))]
fn inspect_launch_topology(
    model_path: &std::path::Path,
    fallback_layers: u32,
) -> (u32, String, Option<String>) {
    let topology = crate::gguf::inspect_model_topology(model_path, fallback_layers);
    (
        topology.total_layers,
        topology.source.label().to_string(),
        topology.note,
    )
}

#[cfg(not(any(feature = "profiling-ui", feature = "sweep")))]
fn inspect_launch_topology(
    _model_path: &std::path::Path,
    fallback_layers: u32,
) -> (u32, String, Option<String>) {
    (
        fallback_layers,
        SIZE_HEURISTIC_LABEL.to_string(),
        Some(
            "Fast launch is using the size-based layer estimate because GGUF topology inspection is unavailable in this build."
                .to_string(),
        ),
    )
}

pub fn plan_launch(record: &CatalogRecord, hw: &HardwareProfile) -> LaunchPlan {
    let (total_layers, layer_source_label, layer_source_note) = launch_topology(record);
    plan_launch_with_layers(
        record,
        hw,
        total_layers,
        layer_source_label,
        layer_source_note,
        false,
    )
}

pub fn step_context_size(current: u32, direction: i32) -> u32 {
    let index = CONFIGURE_CONTEXT_STEPS
        .iter()
        .position(|value| *value == current)
        .unwrap_or_else(|| {
            CONFIGURE_CONTEXT_STEPS
                .iter()
                .position(|value| *value >= current)
                .unwrap_or(CONFIGURE_CONTEXT_STEPS.len().saturating_sub(1))
        }) as i32;
    let next = (index + direction).clamp(0, CONFIGURE_CONTEXT_STEPS.len() as i32 - 1) as usize;
    CONFIGURE_CONTEXT_STEPS[next]
}

pub fn apply_launch_override(
    recommended: &LaunchPlan,
    record: &CatalogRecord,
    hw: &HardwareProfile,
    override_state: &ModelLaunchOverride,
) -> LaunchPlan {
    let recommended_gpu_layers = if recommended.gpu_layers < 0 {
        recommended.total_layers as i32
    } else {
        recommended.gpu_layers
    };
    let context_size = override_state
        .context_size
        .unwrap_or(recommended.context_size)
        .max(1024);
    let gpu_layers = override_state
        .gpu_layers
        .unwrap_or(recommended_gpu_layers)
        .clamp(0, recommended.total_layers as i32);
    let mode = classify_mode(gpu_layers, recommended.total_layers);
    let recommended_threads = recommend_threads(hw, &mode).0;
    let threads = override_state
        .threads
        .or(recommended.threads)
        .or(recommended_threads);
    let quant_k = override_state.quant_k.unwrap_or(recommended.quant_k);
    let quant_v = override_state.quant_v.unwrap_or(recommended.quant_v);
    let blas_threads = override_state.blas_threads.or(recommended.blas_threads);
    let cpu_layers = estimate_cpu_resident_layers(gpu_layers, recommended.total_layers);
    let estimated_vram_mb = estimate_vram_mb(
        context_size,
        gpu_layers,
        record.model_size_gb,
        quant_k,
        quant_v,
        recommended.total_layers,
    );
    let estimated_ram_mb = estimate_ram_mb(
        context_size,
        gpu_layers,
        record.model_size_gb,
        quant_k,
        quant_v,
        recommended.total_layers,
    );

    let customized = context_size != recommended.context_size
        || gpu_layers != recommended.gpu_layers
        || threads != recommended.threads
        || blas_threads != recommended.blas_threads
        || quant_k != recommended.quant_k
        || quant_v != recommended.quant_v;

    let rationale = if customized {
        format!(
            "Configure Hub override: {context_size} ctx, {gpu_layers} GPU layers, \
             {cpu_layers} CPU-resident layers, KV cache K=q{quant_k} V=q{quant_v}."
        )
    } else {
        recommended.rationale.clone()
    };

    LaunchPlan {
        context_size,
        gpu_layers,
        cpu_layers,
        threads,
        blas_threads,
        quant_k,
        quant_v,
        n_parallel: recommended.n_parallel,
        mode,
        rationale,
        estimated: recommended.estimated || customized,
        estimated_vram_mb,
        estimated_ram_mb,
        ..recommended.clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SavedProfileSelection {
    pub context_size: u32,
    pub gpu_layers: i32,
    pub quant_k: u8,
    pub quant_v: u8,
    pub threads: Option<u32>,
}

pub fn apply_saved_profile(
    recommended: &LaunchPlan,
    record: &CatalogRecord,
    hw: &HardwareProfile,
    profile: SavedProfileSelection,
) -> LaunchPlan {
    let mut plan = apply_launch_override(
        recommended,
        record,
        hw,
        &ModelLaunchOverride {
            context_size: Some(profile.context_size),
            gpu_layers: Some(profile.gpu_layers),
            threads: profile.threads,
            blas_threads: None,
            quant_k: None,
            quant_v: None,
        },
    );
    if plan.quant_k != profile.quant_k || plan.quant_v != profile.quant_v {
        plan.quant_k = profile.quant_k.max(1);
        plan.quant_v = profile.quant_v.max(1);
        plan.estimated_vram_mb = estimate_vram_mb(
            plan.context_size,
            plan.gpu_layers,
            record.model_size_gb,
            plan.quant_k,
            plan.quant_v,
            plan.total_layers,
        );
        plan.estimated_ram_mb = estimate_ram_mb(
            plan.context_size,
            plan.gpu_layers,
            record.model_size_gb,
            plan.quant_k,
            plan.quant_v,
            plan.total_layers,
        );
        plan.rationale = format!(
            "Saved profile override: {} ctx, {} GPU layers, {} CPU-resident layers, K=q{} V=q{}.",
            plan.context_size, plan.gpu_layers, plan.cpu_layers, plan.quant_k, plan.quant_v
        );
    }
    plan
}

pub fn build_configure_warnings(plan: &LaunchPlan, hw: &HardwareProfile) -> Vec<ConfigureWarning> {
    let mut warnings = Vec::new();

    if plan.context_size >= 16384 {
        warnings.push(ConfigureWarning {
            severity: ConfigureWarningSeverity::Info,
            message: "High context increases KV-cache usage and startup memory pressure."
                .to_string(),
        });
    }
    if plan.context_size >= 24576 {
        warnings.push(ConfigureWarning {
            severity: ConfigureWarningSeverity::Warning,
            message: "24k-32k context can be slower on smaller GPUs and may lean harder on RAM."
                .to_string(),
        });
    }
    if plan.context_size > 32768 {
        warnings.push(ConfigureWarning {
            severity: if plan.context_size >= 65536 {
                ConfigureWarningSeverity::Critical
            } else {
                ConfigureWarningSeverity::Warning
            },
            message: if plan.context_size >= 65536 {
                "Above 32k is experimental here; 64k+ context can heavily reduce throughput and may force aggressive CPU/RAM fallback."
                    .to_string()
            } else {
                "Above 32k is high-risk; expect noticeably slower generations and much higher KV-cache pressure."
                    .to_string()
            },
        });
    }
    if plan.cpu_layers > 0 {
        warnings.push(ConfigureWarning {
            severity: if plan.cpu_layers > plan.total_layers / 2 {
                ConfigureWarningSeverity::Warning
            } else {
                ConfigureWarningSeverity::Info
            },
            message: format!(
                "{} of {} layers will stay on CPU; this is slower but can make larger context sizes fit.",
                plan.cpu_layers, plan.total_layers
            ),
        });
    }

    if let Some(gpu) = hw.gpu.as_ref() {
        let budget_mb = (gpu.free_mb as f64 * VRAM_HEADROOM_RATIO) as u32;
        if plan.estimated_vram_mb > gpu.total_mb as u32 {
            warnings.push(ConfigureWarning {
                severity: ConfigureWarningSeverity::Critical,
                message: format!(
                    "Estimated VRAM {} MiB exceeds total detected GPU memory {} MiB.",
                    plan.estimated_vram_mb, gpu.total_mb
                ),
            });
        } else if plan.estimated_vram_mb > budget_mb {
            warnings.push(ConfigureWarning {
                severity: ConfigureWarningSeverity::Warning,
                message: format!(
                    "Estimated VRAM {} MiB is above the safe free-memory budget {} MiB.",
                    plan.estimated_vram_mb, budget_mb
                ),
            });
        }
    }

    let safe_ram_budget = (hw.ram_free_mb as f64 * 0.95) as u32;
    if hw.ram_free_mb > 0 && plan.estimated_ram_mb > safe_ram_budget {
        warnings.push(ConfigureWarning {
            severity: if plan.estimated_ram_mb > hw.ram_total_mb as u32 {
                ConfigureWarningSeverity::Critical
            } else {
                ConfigureWarningSeverity::Warning
            },
            message: format!(
                "Estimated RAM {} MiB is close to or above currently free system RAM {} MiB.",
                plan.estimated_ram_mb, hw.ram_free_mb
            ),
        });
    }

    warnings
}

#[cfg(any(feature = "profiling-ui", feature = "bench", feature = "sweep", feature = "analyze"))]
pub fn plan_profiling_launch(record: &CatalogRecord, hw: &HardwareProfile) -> LaunchPlan {
    let (total_layers, layer_source_label, layer_source_note) = launch_topology(record);
    plan_launch_with_layers(
        record,
        hw,
        total_layers,
        layer_source_label,
        layer_source_note,
        true,
    )
}

fn plan_launch_with_layers(
    record: &CatalogRecord,
    hw: &HardwareProfile,
    total_layers: u32,
    layer_source_label: String,
    layer_source_note: Option<String>,
    profiling_mode: bool,
) -> LaunchPlan {
    let rec = &record.recommendation;
    let size_gb = record.model_size_gb.max(0.1);

    let context_size = rec.context_size.max(1024);
    let mut gpu_layers = if rec.gpu_layers < 0 {
        -1i32
    } else {
        rec.gpu_layers.min(total_layers as i32)
    };
    let mut quant_k = rec.quant_k.max(1);
    let mut quant_v = rec.quant_v.max(1);
    let mut rationale = match rec.source {
        crate::catalog::RecSource::Tuned => format!("Using tuned preset: {}", rec.note),
        crate::catalog::RecSource::Benchmarked => {
            format!("Using benchmark-backed recommendation: {}", rec.note)
        }
        crate::catalog::RecSource::Heuristic => format!("Using heuristic fallback: {}", rec.note),
    };
    let mut estimated = matches!(rec.source, crate::catalog::RecSource::Heuristic);
    let should_adapt_to_hardware =
        profiling_mode || matches!(rec.source, crate::catalog::RecSource::Heuristic);

    if should_adapt_to_hardware {
        let ram_need = estimate_ram_mb(
            context_size,
            gpu_layers,
            size_gb,
            quant_k,
            quant_v,
            total_layers,
        );
        if hw.ram_free_mb > 0 && hw.ram_free_mb < (ram_need as f64 * 1.15) as u64 {
            quant_k = quant_k.max(2);
            quant_v = quant_v.max(2);
        }
    }

    let preferred_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers
    };

    let layer_prefix = if layer_source_label == GGUF_METADATA_LABEL {
        format!("GGUF metadata reports {total_layers} layers. ")
    } else {
        format!("oz estimated {total_layers} total layers from model size. ")
    };

    if should_adapt_to_hardware {
        match hw.gpu.as_ref() {
            Some(gpu) => {
                let gpu_budget = (gpu.free_mb as f64 * VRAM_HEADROOM_RATIO) as u32;
                let preferred_vram = estimate_vram_mb(
                    context_size,
                    preferred_layers,
                    size_gb,
                    quant_k,
                    quant_v,
                    total_layers,
                );
                if preferred_vram > gpu_budget {
                    let selected_layers = fit_gpu_layers_to_budget(
                        context_size,
                        size_gb,
                        quant_k,
                        quant_v,
                        total_layers,
                        gpu_budget,
                    )
                    .unwrap_or(0);
                    gpu_layers = selected_layers;
                    estimated = true;
                    let cpu_layers = estimate_cpu_resident_layers(selected_layers, total_layers);
                    rationale = if selected_layers > 0 {
                        format!(
                            "{layer_prefix}Full-GPU VRAM ({preferred_vram} MiB) exceeds budget ({gpu_budget} MiB); start with {selected_layers} GPU layers and {cpu_layers} CPU-resident layers."
                        )
                    } else {
                        format!(
                            "{layer_prefix}Full-GPU VRAM ({preferred_vram} MiB) exceeds budget ({gpu_budget} MiB); start CPU-only with all {total_layers} layers on CPU."
                        )
                    };
                } else if profiling_mode {
                    let cpu_layers = estimate_cpu_resident_layers(preferred_layers, total_layers);
                    rationale = if cpu_layers > 0 {
                        format!(
                            "{layer_prefix}The recommended split fits current GPU budget; start with {} GPU layers and {cpu_layers} CPU-resident layers.",
                            preferred_layers
                        )
                    } else {
                        format!(
                            "{layer_prefix}Estimated VRAM ({preferred_vram} MiB) fits within GPU budget ({gpu_budget} MiB); start with all {total_layers} layers on GPU."
                        )
                    };
                } else {
                    rationale = format!(
                        "{layer_prefix}Estimated VRAM ({preferred_vram} MiB) fits within GPU budget ({gpu_budget} MiB)."
                    );
                }
            }
            None if profiling_mode => {
                gpu_layers = 0;
                estimated = true;
                rationale = format!(
                    "{layer_prefix}No GPU memory data is available; start CPU-only with all {total_layers} layers on CPU."
                );
            }
            None => {}
        }
    }

    let mode = classify_mode(gpu_layers, total_layers);
    let cpu_layers = estimate_cpu_resident_layers(gpu_layers, total_layers);
    let estimated_vram_mb = record
        .benchmark
        .as_ref()
        .map(|b| b.vram_mb)
        .filter(|&v| v > 0)
        .unwrap_or_else(|| {
            estimate_vram_mb(
                context_size,
                gpu_layers,
                size_gb,
                quant_k,
                quant_v,
                total_layers,
            )
        });
    let estimated_ram_mb = estimate_ram_mb(
        context_size,
        gpu_layers,
        size_gb,
        quant_k,
        quant_v,
        total_layers,
    );

    let (threads, blas_threads) = recommend_threads(hw, &mode);

    LaunchPlan {
        model_name: record.model_name.clone(),
        context_size,
        gpu_layers,
        total_layers,
        cpu_layers,
        quant_k,
        quant_v,
        n_parallel: 1,
        threads,
        blas_threads,
        mode,
        rationale,
        estimated,
        estimated_vram_mb,
        estimated_ram_mb,
        source: rec.source.label().to_string(),
        layer_source_label,
        layer_source_note,
    }
}

#[cfg(test)]
#[cfg(feature = "profiling-ui")]
mod tests {
    use super::*;
    use crate::catalog::{RecSource, Recommendation};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_record(path: PathBuf, size_gb: f64) -> CatalogRecord {
        CatalogRecord {
            model_name: "sample.gguf".into(),
            model_path: path,
            model_size_gb: size_gb,
            recommendation: Recommendation {
                context_size: 4096,
                gpu_layers: -1,
                quant_k: 1,
                quant_v: 1,
                note: "sample".into(),
                source: RecSource::Heuristic,
            },
            benchmark: None,
            benchmark_count: 0,
            source_priority: 2,
        }
    }

    fn temp_gguf_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ozone-launch-config-test-{}-{nanos}.gguf",
            std::process::id()
        ))
    }

    fn write_string(buf: &mut Vec<u8>, value: &str) {
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    fn write_metadata_file(path: &std::path::Path) {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&2u64.to_le_bytes());

        write_string(&mut buf, "general.architecture");
        buf.extend_from_slice(&8u32.to_le_bytes());
        write_string(&mut buf, "llama");

        write_string(&mut buf, "llama.block_count");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&40u32.to_le_bytes());

        fs::write(path, buf).expect("write metadata");
    }

    #[test]
    fn fast_launch_uses_metadata_layers() {
        let path = temp_gguf_path();
        write_metadata_file(&path);
        let record = sample_record(path.clone(), 7.0);
        let hw = HardwareProfile {
            gpu: Some(crate::hardware::GpuMemory {
                used_mb: 1000,
                free_mb: 16000,
                total_mb: 17000,
            }),
            ram_total_mb: 32000,
            ram_free_mb: 24000,
            ram_used_mb: 8000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        };

        let plan = plan_launch(&record, &hw);
        assert_eq!(plan.total_layers, 40);
        assert_eq!(plan.cpu_layers, 0);
        assert_eq!(plan.layer_source_label, "GGUF metadata");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn profiling_plan_uses_metadata_layers() {
        let path = temp_gguf_path();
        write_metadata_file(&path);
        let record = sample_record(path.clone(), 7.0);
        let hw = HardwareProfile {
            gpu: Some(crate::hardware::GpuMemory {
                used_mb: 1000,
                free_mb: 16000,
                total_mb: 17000,
            }),
            ram_total_mb: 32000,
            ram_free_mb: 24000,
            ram_used_mb: 8000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        };

        let plan = plan_profiling_launch(&record, &hw);
        assert_eq!(plan.total_layers, 40);
        assert_eq!(plan.cpu_layers, 0);
        assert_eq!(plan.layer_source_label, "GGUF metadata");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn profiling_plan_falls_back_to_cpu_only_without_gpu() {
        let record = sample_record(PathBuf::from("/missing/sample.gguf"), 7.0);
        let hw = HardwareProfile {
            gpu: None,
            ram_total_mb: 32000,
            ram_free_mb: 24000,
            ram_used_mb: 8000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        };

        let plan = plan_profiling_launch(&record, &hw);
        assert_eq!(plan.gpu_layers, 0);
        assert_eq!(plan.cpu_layers, plan.total_layers);
        assert_eq!(plan.mode, RecommendationMode::CpuOnly);
        assert_eq!(plan.layer_source_label, "Size heuristic");
    }
}

#[cfg(test)]
mod configure_tests {
    use super::{
        apply_launch_override, apply_saved_profile, build_configure_warnings, classify_mode,
        step_context_size, ConfigureWarningSeverity, LaunchPlan, RecommendationMode,
        SavedProfileSelection,
    };
    use crate::{
        catalog::{CatalogRecord, RecSource, Recommendation},
        hardware::{GpuMemory, HardwareProfile},
        prefs::ModelLaunchOverride,
    };
    use std::path::PathBuf;

    fn sample_record() -> CatalogRecord {
        CatalogRecord {
            model_name: "sample.gguf".into(),
            model_path: PathBuf::from("/tmp/sample.gguf"),
            model_size_gb: 7.0,
            recommendation: Recommendation {
                context_size: 4096,
                gpu_layers: 24,
                quant_k: 1,
                quant_v: 1,
                note: "sample".into(),
                source: RecSource::Heuristic,
            },
            benchmark: None,
            benchmark_count: 0,
            source_priority: 0,
        }
    }

    fn sample_hw() -> HardwareProfile {
        HardwareProfile {
            gpu: Some(GpuMemory {
                used_mb: 1000,
                free_mb: 9000,
                total_mb: 12000,
            }),
            ram_total_mb: 32000,
            ram_free_mb: 18000,
            ram_used_mb: 14000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        }
    }

    fn sample_plan() -> LaunchPlan {
        LaunchPlan {
            model_name: "sample.gguf".into(),
            context_size: 4096,
            gpu_layers: 24,
            total_layers: 32,
            cpu_layers: 8,
            quant_k: 1,
            quant_v: 1,
            n_parallel: 1,
            threads: Some(4),
            blas_threads: Some(2),
            mode: RecommendationMode::MixedMemory,
            rationale: "sample".into(),
            estimated: false,
            estimated_vram_mb: 4096,
            estimated_ram_mb: 6144,
            source: "heuristic".into(),
            layer_source_label: "Size heuristic".into(),
            layer_source_note: None,
        }
    }

    #[test]
    fn configure_override_recomputes_context_layers_and_mode() {
        let record = sample_record();
        let hw = sample_hw();
        let plan = apply_launch_override(
            &sample_plan(),
            &record,
            &hw,
            &ModelLaunchOverride {
                context_size: Some(16384),
                gpu_layers: Some(8),
                threads: None,
                blas_threads: None,
                quant_k: None,
                quant_v: None,
            },
        );

        assert_eq!(plan.context_size, 16384);
        assert_eq!(plan.gpu_layers, 8);
        assert_eq!(plan.cpu_layers, 24);
        assert_eq!(plan.mode, classify_mode(8, 32));
        assert!(plan.rationale.contains("Configure Hub override"));
    }

    #[test]
    fn configure_warnings_flag_large_context_and_pressure() {
        let warnings = build_configure_warnings(
            &LaunchPlan {
                context_size: 65536,
                estimated_vram_mb: 14000,
                estimated_ram_mb: 24000,
                cpu_layers: 20,
                ..sample_plan()
            },
            &sample_hw(),
        );

        assert!(warnings
            .iter()
            .any(|warning| warning.message.contains("24k-32k context")));
        assert!(warnings
            .iter()
            .any(|warning| warning.message.contains("Above 32k")));
        assert!(warnings
            .iter()
            .any(|warning| warning.severity == ConfigureWarningSeverity::Critical));
    }

    #[test]
    fn saved_profile_recomputes_quant_and_memory_estimates() {
        let record = sample_record();
        let hw = sample_hw();
        let plan = apply_saved_profile(
            &sample_plan(),
            &record,
            &hw,
            SavedProfileSelection {
                context_size: 8192,
                gpu_layers: 12,
                quant_k: 2,
                quant_v: 2,
                threads: Some(6),
            },
        );

        assert_eq!(plan.context_size, 8192);
        assert_eq!(plan.gpu_layers, 12);
        assert_eq!(plan.quant_k, 2);
        assert_eq!(plan.quant_v, 2);
        assert_eq!(plan.threads, Some(6));
        assert!(plan.rationale.contains("Saved profile override"));
        assert!(plan.estimated_vram_mb > 0);
        assert!(plan.estimated_ram_mb > 0);
    }

    #[test]
    fn context_stepper_clamps_at_supported_bounds() {
        assert_eq!(step_context_size(4096, -1), 4096);
        assert_eq!(step_context_size(4096, 1), 8192);
        assert_eq!(step_context_size(32768, 1), 49152);
        assert_eq!(step_context_size(49152, 1), 65536);
        assert_eq!(step_context_size(65536, 1), 262144);
        assert_eq!(step_context_size(262144, 1), 262144);
    }
}
