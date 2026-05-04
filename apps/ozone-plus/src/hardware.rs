use std::process::Command;

/// Lightweight GPU VRAM state, shared with the TUI status bar.
#[derive(Debug, Clone, Default)]
pub struct GpuHardware {
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

impl GpuHardware {
    /// Format as a compact status string: "VRAM 4.2/8.0 GB | RAM 12.1/32.0 GB"
    pub fn status_text(&self) -> String {
        if self.vram_total_mb > 0 && self.ram_total_mb > 0 {
            format!(
                "VRAM {:.1}/{:.1} GB | RAM {:.1}/{:.1} GB",
                self.vram_used_mb as f64 / 1024.0,
                self.vram_total_mb as f64 / 1024.0,
                self.ram_used_mb as f64 / 1024.0,
                self.ram_total_mb as f64 / 1024.0,
            )
        } else {
            "Hardware: N/A".to_string()
        }
    }
}

/// Query GPU VRAM from nvidia-smi. Fallback to rocm-smi for AMD.
fn query_gpu_memory() -> Option<(u64, u64)> {
    // Try NVIDIA first
    if let Ok(out) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = text.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let (Ok(used_mb), Ok(total_mb)) = (
                        parts[0].trim().parse::<u64>(),
                        parts[1].trim().parse::<u64>(),
                    ) {
                        return Some((used_mb, total_mb));
                    }
                }
            }
        }
    }

    // Try AMD rocm-smi
    if let Ok(out) = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram"])
        .output()
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
                return Some((used_mb, total_mb));
            }
        }
    }

    None
}

/// Query RAM from sysinfo.
fn query_ram() -> (u64, u64) {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    let ram_total_mb = sys.total_memory() / 1024 / 1024;
    let ram_used_mb = (sys.total_memory() - sys.available_memory()) / 1024 / 1024;
    (ram_used_mb, ram_total_mb)
}

/// Query both GPU and RAM stats in one call.
pub fn query_hardware() -> GpuHardware {
    let (vram_used_mb, vram_total_mb) = query_gpu_memory().unwrap_or((0, 0));
    let (ram_used_mb, ram_total_mb) = query_ram();
    GpuHardware {
        vram_used_mb,
        vram_total_mb,
        ram_used_mb,
        ram_total_mb,
    }
}
