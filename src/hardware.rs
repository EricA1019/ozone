use std::process::Command;
use sysinfo::System;

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

pub fn query_gpu_memory() -> Option<GpuMemory> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 2 {
        return None;
    }
    let used_mb: u64 = parts[0].trim().parse().ok()?;
    let free_mb: u64 = parts[1].trim().parse().ok()?;
    Some(GpuMemory {
        used_mb,
        free_mb,
        total_mb: used_mb + free_mb,
    })
}

pub fn load_hardware() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();

    let ram_total_mb = sys.total_memory() / 1024 / 1024;
    let ram_free_mb = sys.available_memory() / 1024 / 1024;
    let ram_used_mb = ram_total_mb.saturating_sub(ram_free_mb);
    let cpu_logical = sys.cpus().len().max(1);
    let cpu_physical = (cpu_logical / 2).max(1);

    HardwareProfile {
        gpu: query_gpu_memory(),
        ram_total_mb,
        ram_free_mb,
        ram_used_mb,
        cpu_logical,
        cpu_physical,
    }
}
