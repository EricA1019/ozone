use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sysinfo::System;

const HARDWARE_CACHE_TTL: Duration = Duration::from_secs(30);
const HARDWARE_LIVE_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuMemory {
    pub used_mb: u64,
    pub free_mb: u64,
    pub total_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub gpu: Option<GpuMemory>,
    /// GPU model name (e.g. "NVIDIA GeForce RTX 3060")
    #[serde(default)]
    pub gpu_name: Option<String>,
    pub ram_total_mb: u64,
    pub ram_free_mb: u64,
    pub ram_used_mb: u64,
    pub cpu_logical: usize,
    pub cpu_physical: usize,
    /// Whether CUDA is available (nvidia-smi present + libcuda found)
    #[serde(default)]
    pub cuda_available: bool,
    /// CUDA driver version string (e.g. "13.0")
    #[serde(default)]
    pub cuda_version: Option<String>,
    /// GPU compute capability (e.g. "8.6")
    #[serde(default)]
    pub compute_capability: Option<String>,
    /// Whether flash attention is supported (compute capability >= 8.0)
    #[serde(default)]
    pub flash_attn_supported: bool,
    /// Unix timestamp (seconds) of when this profile was captured
    #[serde(default)]
    pub captured_at_unix: Option<u64>,
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

/// Query GPU model name via nvidia-smi.
fn query_gpu_name() -> Option<String> {
    Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Query CUDA driver version via nvidia-smi.
fn query_cuda_driver_version() -> Option<String> {
    Command::new("nvidia-smi")
        .args(["--query-gpu=driver_version", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Query GPU compute capability via nvidia-smi.
fn query_compute_capability() -> Option<String> {
    Command::new("nvidia-smi")
        .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Check if CUDA is available: nvidia-smi works + libcuda is findable.
fn check_cuda_available() -> bool {
    // Quick check: does nvidia-smi exist and work?
    let smi_ok = Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !smi_ok {
        return false;
    }
    // Confirm libcuda is on the linker path
    Command::new("ldconfig")
        .args(["-p"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains("libcuda.so"))
        .unwrap_or(false)
}

/// Derive flash attention support from compute capability.
fn compute_flash_attn_supported(cap: Option<&str>) -> bool {
    cap.and_then(|c| c.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .map(|major| major >= 8)
        .unwrap_or(false)
}

pub fn collect_hardware_profile() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();

    let ram_total_mb = sys.total_memory() / 1024 / 1024;
    let ram_free_mb = sys.available_memory() / 1024 / 1024;
    let ram_used_mb = ram_total_mb.saturating_sub(ram_free_mb);
    let cpu_logical = sys.cpus().len().max(1);
    let cpu_physical = sys.physical_core_count().unwrap_or(cpu_logical / 2).max(1);

    let gpu = query_gpu_memory();
    let gpu_name = query_gpu_name();
    let cuda_available = check_cuda_available();
    let cuda_version = if cuda_available {
        query_cuda_driver_version()
    } else {
        None
    };
    let compute_capability = if cuda_available {
        query_compute_capability()
    } else {
        None
    };
    let flash_attn_supported = compute_flash_attn_supported(compute_capability.as_deref());

    HardwareProfile {
        gpu,
        gpu_name,
        ram_total_mb,
        ram_free_mb,
        ram_used_mb,
        cpu_logical,
        cpu_physical,
        cuda_available,
        cuda_version,
        compute_capability,
        flash_attn_supported,
        captured_at_unix: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        ),
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

/// Cached hardware is saved to disk so we can skip polling on every startup.
pub fn load_cached_hardware() -> Option<HardwareProfile> {
    let path = crate::paths::data_dir()?.join("hardware-cache.json");
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_hardware_profile(profile: &HardwareProfile) {
    let Some(data_dir) = crate::paths::data_dir() else {
        return;
    };
    let path = data_dir.join("hardware-cache.json");
    if let Ok(text) = serde_json::to_string_pretty(profile) {
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::write(&path, text);
    }
}

pub fn load_hardware() -> HardwareProfile {
    // Check in-memory cache first (30s TTL)
    if let Ok(guard) = HARDWARE_CACHE.lock() {
        if let Some((ref hw, ts)) = *guard {
            if ts.elapsed() < HARDWARE_CACHE_TTL {
                return hw.clone();
            }
        }
    }

    // Check persistent system profile cache (24h TTL)
    const SYSTEM_PROFILE_TTL_SECS: u64 = 86400; // 24 hours
    if let Some(cached) = load_system_profile() {
        let age_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(cached.captured_at_unix.unwrap_or(0));
        if age_secs < SYSTEM_PROFILE_TTL_SECS && cached.gpu.is_some() {
            // Cache is fresh — skip polling, use it
            if let Ok(mut guard) = HARDWARE_CACHE.lock() {
                *guard = Some((cached.clone(), Instant::now()));
            }
            return cached;
        }
    }

    let result = collect_hardware_profile();
    save_system_profile(&result);

    if let Ok(mut guard) = HARDWARE_CACHE.lock() {
        *guard = Some((result.clone(), Instant::now()));
    }

    result
}

/// Load the persistent system profile from disk.
pub fn load_system_profile() -> Option<HardwareProfile> {
    let path = crate::paths::data_dir()?.join("system-profile.json");
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Save a hardware profile to disk as the system profile.
pub fn save_system_profile(profile: &HardwareProfile) {
    let Some(data_dir) = crate::paths::data_dir() else {
        return;
    };
    let path = data_dir.join("system-profile.json");
    if let Ok(text) = serde_json::to_string_pretty(profile) {
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::write(&path, text);
    }
}

/// Force a fresh hardware capture and save it as the system profile.
/// Call this when the user explicitly chooses "Import Specs".
pub fn import_system_specs() -> HardwareProfile {
    let profile = collect_hardware_profile();
    save_system_profile(&profile);

    // Refresh in-memory cache
    if let Ok(mut guard) = HARDWARE_CACHE.lock() {
        *guard = Some((profile.clone(), Instant::now()));
    }

    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_memory_defaults_are_zero() {
        let mem = GpuMemory::default();
        assert_eq!(mem.used_mb, 0);
        assert_eq!(mem.free_mb, 0);
        assert_eq!(mem.total_mb, 0);
    }

    #[test]
    fn hardware_profile_defaults_are_sane() {
        let profile = HardwareProfile::default();
        assert!(profile.gpu.is_none());
        assert!(profile.gpu_name.is_none());
        assert_eq!(profile.ram_total_mb, 0);
        assert_eq!(profile.cpu_logical, 0);
        assert_eq!(profile.cpu_physical, 0);
        assert!(!profile.cuda_available);
        assert!(profile.cuda_version.is_none());
        assert!(profile.compute_capability.is_none());
        assert!(!profile.flash_attn_supported);
    }

    #[test]
    fn gpu_memory_json_roundtrip() {
        let mem = GpuMemory {
            used_mb: 2048,
            free_mb: 8192,
            total_mb: 10240,
        };
        let json = serde_json::to_string(&mem).expect("serialize");
        assert_eq!(json, r#"{"usedMb":2048,"freeMb":8192,"totalMb":10240}"#);
        let deserialized: GpuMemory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.used_mb, 2048);
        assert_eq!(deserialized.free_mb, 8192);
        assert_eq!(deserialized.total_mb, 10240);
    }

    #[test]
    fn hardware_profile_json_roundtrip() {
        let profile = HardwareProfile {
            gpu: Some(GpuMemory { used_mb: 512, free_mb: 11776, total_mb: 12288 }),
            gpu_name: Some("NVIDIA GeForce RTX 3060".into()),
            ram_total_mb: 32000,
            ram_free_mb: 16000,
            ram_used_mb: 16000,
            cpu_logical: 12,
            cpu_physical: 6,
            cuda_available: true,
            cuda_version: Some("12.4".into()),
            compute_capability: Some("8.6".into()),
            flash_attn_supported: true,
            captured_at_unix: Some(1_700_000_000),
        };
        let json = serde_json::to_string(&profile).expect("serialize");
        let deserialized: HardwareProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.gpu.unwrap().total_mb, 12288);
        assert_eq!(deserialized.gpu_name.unwrap(), "NVIDIA GeForce RTX 3060");
        assert_eq!(deserialized.ram_total_mb, 32000);
        assert_eq!(deserialized.cpu_logical, 12);
        assert!(deserialized.cuda_available);
        assert_eq!(deserialized.cuda_version.unwrap(), "12.4");
        assert_eq!(deserialized.compute_capability.unwrap(), "8.6");
        assert!(deserialized.flash_attn_supported);
        assert_eq!(deserialized.captured_at_unix.unwrap(), 1_700_000_000);
    }

    #[test]
    fn hardware_profile_unknown_fields_ignored() {
        // Forward compatibility: extra fields should not break deserialization.
        let json = r#"{"ramTotalMb":16000,"ramFreeMb":8000,"ramUsedMb":8000,"cpuLogical":8,"cpuPhysical":4,"unknownField":"ignored"}"#;
        let profile: HardwareProfile = serde_json::from_str(json).expect("deserialize with unknown field");
        assert_eq!(profile.ram_total_mb, 16000);
        assert_eq!(profile.cpu_logical, 8);
    }

    #[test]
    fn compute_flash_attn_supported_various_capabilities() {
        // None → false (no GPU info)
        assert!(!compute_flash_attn_supported(None));
        // Empty string → false
        assert!(!compute_flash_attn_supported(Some("")));
        // Below 8.0 → false
        assert!(!compute_flash_attn_supported(Some("7.5")));
        assert!(!compute_flash_attn_supported(Some("6.0")));
        assert!(!compute_flash_attn_supported(Some("3.5")));
        // At 8.0 → true
        assert!(compute_flash_attn_supported(Some("8.0")));
        // Above 8.0 → true
        assert!(compute_flash_attn_supported(Some("8.6")));
        assert!(compute_flash_attn_supported(Some("9.0")));
        assert!(compute_flash_attn_supported(Some("10.0")));
        // Invalid format → false
        assert!(!compute_flash_attn_supported(Some("invalid")));
        assert!(!compute_flash_attn_supported(Some("eight-point-six")));
    }

    #[test]
    fn hardware_profile_serde_rename_all_camel_case() {
        // Verify serde field naming convention.
        let profile = HardwareProfile {
            ram_total_mb: 16000,
            ram_free_mb: 8000,
            ram_used_mb: 8000,
            cpu_logical: 8,
            cpu_physical: 4,
            ..Default::default()
        };
        let json = serde_json::to_string(&profile).expect("serialize");
        // Field names should use camelCase
        assert!(json.contains("ramTotalMb"));
        assert!(json.contains("ramFreeMb"));
        assert!(json.contains("ramUsedMb"));
        assert!(json.contains("cpuLogical"));
        assert!(json.contains("cpuPhysical"));
        // Should NOT contain snake_case names
        assert!(!json.contains("ram_total_mb"));
    }
}
