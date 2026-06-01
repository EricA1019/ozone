use ozone_core::hardware::collect_hardware_profile;

#[derive(Debug, Clone, Default)]
pub struct HardwareInfo {
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

impl HardwareInfo {
    pub fn is_available(&self) -> bool {
        self.ram_total_mb > 0
    }

    pub fn status_text(&self) -> String {
        if self.vram_total_mb > 0 {
            format!(
                "VRAM {:.1}/{:.1} GB | RAM {:.1}/{:.1} GB",
                self.vram_used_mb as f64 / 1024.0,
                self.vram_total_mb as f64 / 1024.0,
                self.ram_used_mb as f64 / 1024.0,
                self.ram_total_mb as f64 / 1024.0,
            )
        } else if self.ram_total_mb > 0 {
            format!(
                "RAM {:.1}/{:.1} GB",
                self.ram_used_mb as f64 / 1024.0,
                self.ram_total_mb as f64 / 1024.0,
            )
        } else {
            "Hardware: N/A".to_string()
        }
    }

    pub fn poll() -> Self {
        let hp = collect_hardware_profile();
        let (vram_used_mb, vram_total_mb) = hp
            .gpu
            .as_ref()
            .map(|g| (g.used_mb, g.total_mb))
            .unwrap_or((0, 0));

        Self {
            vram_used_mb,
            vram_total_mb,
            ram_used_mb: hp.ram_used_mb,
            ram_total_mb: hp.ram_total_mb,
        }
    }
}
