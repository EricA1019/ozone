use crate::catalog::CatalogRecord;
use crate::hardware::HardwareProfile;
use crate::prefs::ModelLaunchOverride;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologySource {
    GgufMetadata,
    SizeHeuristic,
}

impl TopologySource {
    pub fn label(self) -> &'static str {
        match self {
            TopologySource::GgufMetadata => "GGUF metadata",
            TopologySource::SizeHeuristic => "Size heuristic",
        }
    }
}

const MIB_PER_GIB: f64 = 1024.0;
const VRAM_HEADROOM_RATIO: f64 = 0.9;
pub const CONFIGURE_CONTEXT_STEPS: [u32; 7] = [4096, 8192, 16384, 24576, 32768, 49152, 65536];

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

pub fn estimate_total_layers(size_gb: f64) -> u32 {
    let s = size_gb.max(0.1);
    if s <= 8.0 {
        32
    } else if s <= 12.5 {
        40
    } else if s <= 20.0 {
        48
    } else {
        64
    }
}

fn quant_kv_memory_factor(quant_kv: u8) -> f64 {
    let level = quant_kv.max(1) as f64;
    1.0 / (1.0 + (level - 1.0) * 0.35)
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

pub fn estimate_cpu_resident_layers(gpu_layers: i32, total_layers: u32) -> u32 {
    let gpu_layers = if gpu_layers < 0 {
        total_layers
    } else {
        gpu_layers.clamp(0, total_layers as i32) as u32
    };
    total_layers.saturating_sub(gpu_layers)
}

pub fn estimate_vram_mb(
    context_size: u32,
    gpu_layers: i32,
    size_gb: f64,
    quant_kv: u8,
    total_layers: u32,
) -> u32 {
    let safe_size = size_gb.max(0.1);
    let safe_ctx = context_size.max(1024) as f64;
    let clamp_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers.min(total_layers as i32)
    };
    let layer_frac = gpu_layer_fraction(clamp_layers, total_layers);
    if layer_frac <= 0.0 {
        return 0;
    }
    let quant_factor = quant_kv_memory_factor(quant_kv);
    let ctx_mult = safe_ctx / 4096.0;
    let model_weights_mb = safe_size * MIB_PER_GIB * layer_frac;
    let kv_per_4k_mb = (safe_size * 20.0).max(96.0);
    let kv_cache_mb = kv_per_4k_mb * ctx_mult * quant_factor * (0.25 + layer_frac * 0.75);
    let overhead_mb = 320.0 + safe_size * 12.0 + ctx_mult * 40.0;
    (model_weights_mb + kv_cache_mb + overhead_mb).round() as u32
}

pub fn estimate_ram_mb(
    context_size: u32,
    gpu_layers: i32,
    size_gb: f64,
    quant_kv: u8,
    total_layers: u32,
) -> u32 {
    let safe_size = size_gb.max(0.1);
    let safe_ctx = context_size.max(1024) as f64;
    let quant_factor = quant_kv_memory_factor(quant_kv);
    let ctx_mult = safe_ctx / 4096.0;
    let clamp_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers.min(total_layers as i32)
    };
    let gpu_fraction = gpu_layer_fraction(clamp_layers, total_layers);
    let cpu_fraction = 1.0 - gpu_fraction;
    let base_mb = safe_size * MIB_PER_GIB * (0.18 + cpu_fraction * 1.02);
    let kv_mb =
        (safe_size * 24.0).max(128.0) * ctx_mult * quant_factor * (0.45 + cpu_fraction * 0.55);
    let overhead_mb = 384.0 + safe_size * 14.0 + ctx_mult * 48.0;
    (base_mb + kv_mb + overhead_mb).round() as u32
}

pub fn fit_gpu_layers_to_budget(
    context_size: u32,
    size_gb: f64,
    quant_kv: u8,
    total_layers: u32,
    budget_mb: u32,
) -> Option<i32> {
    let mut lo = 0i32;
    let mut hi = total_layers as i32;
    let mut best = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let est = estimate_vram_mb(context_size, mid, size_gb, quant_kv, total_layers);
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

pub fn plan_launch(record: &CatalogRecord, hw: &HardwareProfile) -> LaunchPlan {
    let total_layers = estimate_total_layers(record.model_size_gb.max(0.1));
    let layer_source_label = TopologySource::SizeHeuristic.label().to_string();
    let layer_source_note = Some(
        "Fast launch still uses the size-based layer estimate; the enhanced layer-aware heuristic is currently scoped to profiling."
            .to_string(),
    );
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
    let cpu_layers = estimate_cpu_resident_layers(gpu_layers, recommended.total_layers);
    let estimated_vram_mb = estimate_vram_mb(
        context_size,
        gpu_layers,
        record.model_size_gb,
        recommended.quant_kv,
        recommended.total_layers,
    );
    let estimated_ram_mb = estimate_ram_mb(
        context_size,
        gpu_layers,
        record.model_size_gb,
        recommended.quant_kv,
        recommended.total_layers,
    );

    let customized = context_size != recommended.context_size
        || gpu_layers != recommended.gpu_layers
        || threads != recommended.threads;

    let rationale = if customized {
        format!(
            "Configure Hub override: {context_size} ctx, {gpu_layers} GPU layers, {cpu_layers} CPU-resident layers."
        )
    } else {
        recommended.rationale.clone()
    };

    LaunchPlan {
        context_size,
        gpu_layers,
        cpu_layers,
        threads,
        mode,
        rationale,
        estimated: recommended.estimated || customized,
        estimated_vram_mb,
        estimated_ram_mb,
        ..recommended.clone()
    }
}

pub fn apply_saved_profile(
    recommended: &LaunchPlan,
    record: &CatalogRecord,
    hw: &HardwareProfile,
    context_size: u32,
    gpu_layers: i32,
    quant_kv: u8,
    threads: Option<u32>,
) -> LaunchPlan {
    let mut plan = apply_launch_override(
        recommended,
        record,
        hw,
        &ModelLaunchOverride {
            context_size: Some(context_size),
            gpu_layers: Some(gpu_layers),
            threads,
        },
    );
    if plan.quant_kv != quant_kv {
        plan.quant_kv = quant_kv.max(1);
        plan.estimated_vram_mb = estimate_vram_mb(
            plan.context_size,
            plan.gpu_layers,
            record.model_size_gb,
            plan.quant_kv,
            plan.total_layers,
        );
        plan.estimated_ram_mb = estimate_ram_mb(
            plan.context_size,
            plan.gpu_layers,
            record.model_size_gb,
            plan.quant_kv,
            plan.total_layers,
        );
        plan.rationale = format!(
            "Saved profile override: {} ctx, {} GPU layers, {} CPU-resident layers, qkv {}.",
            plan.context_size, plan.gpu_layers, plan.cpu_layers, plan.quant_kv
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
                "Above 32k is experimental here; 64k context can heavily reduce throughput and may force aggressive CPU/RAM fallback."
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

#[cfg(feature = "profiling-ui")]
pub fn plan_profiling_launch(record: &CatalogRecord, hw: &HardwareProfile) -> LaunchPlan {
    let fallback_layers = estimate_total_layers(record.model_size_gb.max(0.1));
    let topology = crate::gguf::inspect_model_topology(&record.model_path, fallback_layers);
    plan_launch_with_layers(
        record,
        hw,
        topology.total_layers,
        topology.source.label().to_string(),
        topology.note,
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
    let mut quant_kv = rec.quant_kv.max(1);
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
        let ram_need = estimate_ram_mb(context_size, gpu_layers, size_gb, quant_kv, total_layers);
        if hw.ram_free_mb > 0 && hw.ram_free_mb < (ram_need as f64 * 1.15) as u64 {
            quant_kv = quant_kv.max(2);
        }
    }

    let preferred_layers = if gpu_layers < 0 {
        total_layers as i32
    } else {
        gpu_layers
    };

    let layer_prefix = if layer_source_label == TopologySource::GgufMetadata.label() {
        format!("GGUF metadata reports {total_layers} layers. ")
    } else {
        format!("Ozone estimated {total_layers} total layers from model size. ")
    };

    if should_adapt_to_hardware {
        match hw.gpu.as_ref() {
            Some(gpu) => {
                let gpu_budget = (gpu.free_mb as f64 * VRAM_HEADROOM_RATIO) as u32;
                let preferred_vram = estimate_vram_mb(
                    context_size,
                    preferred_layers,
                    size_gb,
                    quant_kv,
                    total_layers,
                );
                if preferred_vram > gpu_budget {
                    let selected_layers = fit_gpu_layers_to_budget(
                        context_size,
                        size_gb,
                        quant_kv,
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
            estimate_vram_mb(context_size, gpu_layers, size_gb, quant_kv, total_layers)
        });
    let estimated_ram_mb =
        estimate_ram_mb(context_size, gpu_layers, size_gb, quant_kv, total_layers);

    let (threads, blas_threads) = recommend_threads(hw, &mode);

    LaunchPlan {
        model_name: record.model_name.clone(),
        context_size,
        gpu_layers,
        total_layers,
        cpu_layers,
        quant_kv,
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
                quant_kv: 1,
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
            "ozone-planner-test-{}-{nanos}.gguf",
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
                quant_kv: 1,
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
        }
    }

    fn sample_plan() -> LaunchPlan {
        LaunchPlan {
            model_name: "sample.gguf".into(),
            context_size: 4096,
            gpu_layers: 24,
            total_layers: 32,
            cpu_layers: 8,
            quant_kv: 1,
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
        let plan = apply_saved_profile(&sample_plan(), &record, &hw, 8192, 12, 2, Some(6));

        assert_eq!(plan.context_size, 8192);
        assert_eq!(plan.gpu_layers, 12);
        assert_eq!(plan.quant_kv, 2);
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
        assert_eq!(step_context_size(65536, 1), 65536);
    }
}
