use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sysinfo::System;

const HARDWARE_CACHE_TTL: Duration = Duration::from_secs(30);
const HARDWARE_LIVE_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default)]
pub struct GpuMemory {
    pub used_mb: u64,
    pub free_mb: u64,
    pub total_mb: u64,
}

#[derive(Debug, Clone, Default)]
pub struct HardwareProfile {
    pub gpu: Option<GpuMemory>,
    pub ram_total_mb: u64,
    pub ram_free_mb: u64,
    pub ram_used_mb: u64,
    pub cpu_logical: usize,
    pub cpu_physical: usize,
}

fn query_amd_gpu_memory() -> Option<GpuMemory> {
    // Try JSON format first (rocm-smi >= 5.x)
    if let Ok(out) = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--json"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(card) = json.as_object().and_then(|o| o.values().next()) {
                    let total_b: u64 = card["VRAM Total Memory (B)"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let used_b: u64 = card["VRAM Total Used Memory (B)"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    if total_b > 0 {
                        let total_mb = total_b / 1024 / 1024;
                        let used_mb = used_b / 1024 / 1024;
                        let free_mb = total_mb.saturating_sub(used_mb);
                        return Some(GpuMemory {
                            used_mb,
                            free_mb,
                            total_mb,
                        });
                    }
                }
            }
        }
    }

    // Fallback: legacy text format (rocm-smi < 5.x)
    if let Ok(out) = Command::new("rocm-smi").args(["--showmeminfo", "vram"]).output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut total_b: Option<u64> = None;
            let mut used_b: Option<u64> = None;
            for line in text.lines() {
                if line.contains("VRAM Total Memory (B):") {
                    total_b = line.split(':').nth(1).and_then(|s| s.trim().parse().ok());
                } else if line.contains("VRAM Total Used Memory (B):") {
                    used_b = line.split(':').nth(1).and_then(|s| s.trim().parse().ok());
                }
            }
            if let Some(total_b) = total_b {
                let total_mb = total_b / 1024 / 1024;
                let used_mb = used_b.unwrap_or(0) / 1024 / 1024;
                let free_mb = total_mb.saturating_sub(used_mb);
                return Some(GpuMemory {
                    used_mb,
                    free_mb,
                    total_mb,
                });
            }
        }
    }

    None
}

/// Query GPU memory (NVIDIA `nvidia-smi` first, then AMD `rocm-smi`).
pub fn query_gpu_memory() -> Option<GpuMemory> {
    // Try NVIDIA first
    if let Ok(out) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = text.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let (Ok(used_mb), Ok(free_mb)) = (
                        parts[0].trim().parse::<u64>(),
                        parts[1].trim().parse::<u64>(),
                    ) {
                        return Some(GpuMemory {
                            used_mb,
                            free_mb,
                            total_mb: used_mb + free_mb,
                        });
                    }
                }
            }
        }
    }

    // Fallback to AMD rocm-smi
    query_amd_gpu_memory()
}

static HARDWARE_CACHE: Mutex<Option<(HardwareProfile, Instant)>> = Mutex::new(None);

pub fn collect_hardware_profile() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();

    let ram_total_mb = sys.total_memory() / 1024 / 1024;
    let ram_free_mb = sys.available_memory() / 1024 / 1024;
    let ram_used_mb = ram_total_mb.saturating_sub(ram_free_mb);
    let cpu_logical = sys.cpus().len().max(1);
    let cpu_physical = sys.physical_core_count().unwrap_or(cpu_logical / 2).max(1);

    HardwareProfile {
        gpu: query_gpu_memory(),
        ram_total_mb,
        ram_free_mb,
        ram_used_mb,
        cpu_logical,
        cpu_physical,
    }
}

pub fn load_hardware_live() -> HardwareProfile {
    if let Ok(guard) = HARDWARE_CACHE.lock() {
        if let Some((ref hw, ts)) = *guard {
            if ts.elapsed() < HARDWARE_LIVE_CACHE_TTL {
                return hw.clone();
            }
        }
    }

    let result = collect_hardware_profile();

    if let Ok(mut guard) = HARDWARE_CACHE.lock() {
        *guard = Some((result.clone(), Instant::now()));
    }

    result
}

pub fn load_hardware() -> HardwareProfile {
    if let Ok(guard) = HARDWARE_CACHE.lock() {
        if let Some((ref hw, ts)) = *guard {
            if ts.elapsed() < HARDWARE_CACHE_TTL {
                return hw.clone();
            }
        }
    }

    let result = collect_hardware_profile();

    if let Ok(mut guard) = HARDWARE_CACHE.lock() {
        *guard = Some((result.clone(), Instant::now()));
    }

    result
}
