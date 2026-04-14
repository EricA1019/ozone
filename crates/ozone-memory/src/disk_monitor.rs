use serde::{Deserialize, Serialize};
use std::path::Path;

/// Disk space thresholds and check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskMonitorPolicy {
    /// Warn when free space drops below this many bytes
    pub warn_bytes: u64,
    /// Critical when free space drops below this many bytes
    pub critical_bytes: u64,
    /// Emergency when free space drops below this many bytes
    pub emergency_bytes: u64,
}

impl Default for DiskMonitorPolicy {
    fn default() -> Self {
        Self {
            warn_bytes: 500 * 1024 * 1024,     // 500 MiB
            critical_bytes: 100 * 1024 * 1024, // 100 MiB
            emergency_bytes: 20 * 1024 * 1024, // 20 MiB
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskStatus {
    Ok,
    Warn,
    Critical,
    Emergency,
}

impl DiskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Critical => "critical",
            Self::Emergency => "emergency",
        }
    }
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
    pub const fn is_warn_or_worse(self) -> bool {
        !matches!(self, Self::Ok)
    }
    pub const fn should_pause_background_jobs(self) -> bool {
        matches!(self, Self::Emergency)
    }
}

impl std::fmt::Display for DiskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskCheckResult {
    pub free_bytes: u64,
    pub status: DiskStatus,
}

impl DiskCheckResult {
    pub fn new(free_bytes: u64, policy: &DiskMonitorPolicy) -> Self {
        let status = if free_bytes < policy.emergency_bytes {
            DiskStatus::Emergency
        } else if free_bytes < policy.critical_bytes {
            DiskStatus::Critical
        } else if free_bytes < policy.warn_bytes {
            DiskStatus::Warn
        } else {
            DiskStatus::Ok
        };
        Self { free_bytes, status }
    }
}

/// Check free disk space for the filesystem containing `path`.
/// Returns `None` if the check is unsupported on this platform or the path is invalid.
pub fn check_disk_space(path: &Path, policy: &DiskMonitorPolicy) -> Option<DiskCheckResult> {
    free_bytes_for_path(path).map(|free| DiskCheckResult::new(free, policy))
}

#[cfg(unix)]
fn free_bytes_for_path(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // Walk up to find an existing parent if path doesn't exist yet
    let mut check_path = path.to_path_buf();
    while !check_path.exists() {
        if let Some(parent) = check_path.parent() {
            check_path = parent.to_path_buf();
        } else {
            return None;
        }
    }

    let c_path = CString::new(check_path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c_path is null-terminated, stat is properly sized
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: rc == 0, stat is now initialised
    let stat = unsafe { stat.assume_init() };
    Some(stat.f_bavail.saturating_mul(stat.f_frsize))
}

#[cfg(not(unix))]
fn free_bytes_for_path(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_status_thresholds() {
        let policy = DiskMonitorPolicy::default();
        assert_eq!(
            DiskCheckResult::new(1024 * 1024 * 1024, &policy).status,
            DiskStatus::Ok
        );
        assert_eq!(
            DiskCheckResult::new(200 * 1024 * 1024, &policy).status,
            DiskStatus::Warn
        );
        assert_eq!(
            DiskCheckResult::new(50 * 1024 * 1024, &policy).status,
            DiskStatus::Critical
        );
        assert_eq!(
            DiskCheckResult::new(5 * 1024 * 1024, &policy).status,
            DiskStatus::Emergency
        );
    }

    #[test]
    fn test_emergency_pauses_background_jobs() {
        assert!(DiskStatus::Emergency.should_pause_background_jobs());
        assert!(!DiskStatus::Critical.should_pause_background_jobs());
        assert!(!DiskStatus::Ok.should_pause_background_jobs());
    }

    #[test]
    fn test_check_disk_space_returns_something_on_existing_path() {
        // On unix the root always exists, so this should return Some
        let result = check_disk_space(std::path::Path::new("/"), &DiskMonitorPolicy::default());
        #[cfg(unix)]
        assert!(result.is_some());
        #[cfg(not(unix))]
        assert!(result.is_none());
    }
}
