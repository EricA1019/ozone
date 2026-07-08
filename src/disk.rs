//! Disk I/O monitoring — reads Linux `/proc/diskstats` for read/write throughput.
//!
//! Used by the launcher UI to display disk activity during model benchmarks.

/// Identify the root block device from `/proc/mounts`.
///
/// Returns the kernel device name (e.g. `"nvme0n1"`, `"sda"`, `"mmcblk0"`)
/// by finding the device mounted at `/` and stripping the partition suffix.
pub fn get_root_disk_name() -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let root_line = mounts
        .lines()
        .find(|l| l.split_whitespace().nth(1) == Some("/"))?;
    let dev = root_line.split_whitespace().next()?;
    let name = dev.strip_prefix("/dev/")?;
    // NVMe: nvme0n1p1 → nvme0n1
    if name.starts_with("nvme") {
        return name.split('p').next().map(|s| s.to_string());
    }
    // SATA/eMMC: sda1 → sda, mmcblk0p1 → mmcblk0
    Some(
        name.trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end_matches('p')
            .to_string(),
    )
}

/// A snapshot of disk sector counters read from `/proc/diskstats`.
#[derive(Debug, Clone, Default)]
pub struct DiskSnapshot {
    pub sectors_read: u64,
    pub sectors_written: u64,
}

/// Read current disk stats for `disk_name` from `/proc/diskstats`.
///
/// Returns `None` if the file is unreadable or the device is not found.
pub fn read_disk_stats(disk_name: &str) -> Option<DiskSnapshot> {
    let text = std::fs::read_to_string("/proc/diskstats").ok()?;
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.get(2) == Some(&disk_name) {
            let sectors_read: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
            let sectors_written: u64 = parts.get(9).and_then(|s| s.parse().ok()).unwrap_or(0);
            return Some(DiskSnapshot {
                sectors_read,
                sectors_written,
            });
        }
    }
    None
}

/// Compute read/write throughput delta between two snapshots.
///
/// Returns `(read_mb_per_sec, write_mb_per_sec)`. Returns `(0.0, 0.0)` when
/// `elapsed_secs` is zero or negative.
pub fn compute_disk_delta(
    prev: &DiskSnapshot,
    curr: &DiskSnapshot,
    elapsed_secs: f64,
) -> (f64, f64) {
    if elapsed_secs <= 0.0 {
        return (0.0, 0.0);
    }
    const BYTES_PER_SECTOR: f64 = 512.0;
    const BYTES_PER_MB: f64 = 1_048_576.0;
    let read_sectors = curr.sectors_read.saturating_sub(prev.sectors_read);
    let write_sectors = curr.sectors_written.saturating_sub(prev.sectors_written);
    let read_mb = (read_sectors as f64 * BYTES_PER_SECTOR / BYTES_PER_MB) / elapsed_secs;
    let write_mb = (write_sectors as f64 * BYTES_PER_SECTOR / BYTES_PER_MB) / elapsed_secs;
    (read_mb.max(0.0), write_mb.max(0.0))
}
