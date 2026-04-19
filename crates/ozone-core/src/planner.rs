//! Shared planner data types for the ozone product family.
//!
//! The computation logic lives in `src/planner.rs` (the root `ozone` binary).
//! These types live here so that `ozone-tui` and other crates can reference
//! them without pulling in the full planning dependencies.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationMode {
    VramFirst,
    MixedMemory,
    CpuOnly,
}

impl RecommendationMode {
    pub fn label(&self) -> &'static str {
        match self {
            RecommendationMode::VramFirst => "VRAM",
            RecommendationMode::MixedMemory => "Mixed",
            RecommendationMode::CpuOnly => "CPU",
        }
    }

    pub fn display_label(&self) -> &'static str {
        match self {
            RecommendationMode::VramFirst => "VRAM-first",
            RecommendationMode::MixedMemory => "Mixed",
            RecommendationMode::CpuOnly => "CPU-only",
        }
    }
}

/// A resolved launch configuration for a model.
///
/// Produced by `ozone`'s planner and optionally surfaced in the `ozone+` TUI
/// via the `OZONE__LAUNCH_PLAN` environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub model_name: String,
    pub context_size: u32,
    pub gpu_layers: i32,
    pub total_layers: u32,
    pub cpu_layers: u32,
    pub quant_kv: u8,
    pub threads: Option<u32>,
    pub blas_threads: Option<u32>,
    pub mode: RecommendationMode,
    pub rationale: String,
    pub estimated: bool,
    pub estimated_vram_mb: u32,
    pub estimated_ram_mb: u32,
    pub source: String,
    pub layer_source_label: String,
    pub layer_source_note: Option<String>,
}

impl LaunchPlan {
    pub fn gpu_layers_display(&self) -> u32 {
        if self.gpu_layers < 0 {
            self.total_layers
        } else {
            self.gpu_layers.max(0) as u32
        }
    }
}
