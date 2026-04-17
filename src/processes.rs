use anyhow::{anyhow, Result};
use ozone_core::paths;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::sleep;

const KOBOLD_START_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KoboldStartupFailureKind {
    PyInstallerExtraction,
    MissingSharedLibrary,
    RuntimeCrash,
    Timeout,
    Unknown,
}

pub async fn is_url_ready(url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => reqwest::Client::new(),
    };
    client
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn get_kobold_model() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let resp = client.get(paths::koboldcpp_ready_url()).send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    data["result"].as_str().map(|s| s.to_string())
}

pub async fn get_kobold_perf() -> Option<f64> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .ok()?;
    let resp = client.get(paths::koboldcpp_perf_url()).send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    let last_ms = data["last_process_time_ms"].as_f64().unwrap_or(0.0);
    let last_tok = data["last_token_count"].as_f64().unwrap_or(0.0);
    if last_ms > 0.0 && last_tok > 0.0 {
        Some(last_tok / (last_ms / 1000.0))
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub kobold_running: bool,
    pub kobold_model: Option<String>,
    pub ollama_running: bool,
    pub st_running: bool,
}

pub async fn get_service_status() -> ServiceStatus {
    let kobold_url = paths::koboldcpp_ready_url();
    let (kobold_ready, ollama_ready, st_ready) = tokio::join!(
        is_url_ready(&kobold_url),
        is_url_ready("http://127.0.0.1:11434/api/tags"),
        is_url_ready("http://127.0.0.1:8000"),
    );
    let kobold_model = if kobold_ready {
        get_kobold_model().await
    } else {
        None
    };
    ServiceStatus {
        kobold_running: kobold_ready,
        kobold_model,
        ollama_running: ollama_ready,
        st_running: st_ready,
    }
}

pub async fn clear_gpu_backends() -> Result<Vec<String>> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid=,args="])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut killed = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim(); // ps pads PIDs with leading spaces
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let pid: u32 = match parts[0].trim().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let args = parts[1];
        if (args.contains("koboldcpp") || (args.contains("ollama") && args.contains("runner")))
            && nix_kill(pid)
        {
            killed.push(args.split('/').next_back().unwrap_or(args).to_string());
        }
    }
    Ok(killed)
}

fn nix_kill(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn resolved_kobold_launcher_path() -> PathBuf {
    paths::launcher_path()
}

pub async fn start_kobold(launcher_path: &Path, model_name: &str, args: &[String]) -> Result<()> {
    if !launcher_path.exists() {
        return Err(anyhow!(
            "KoboldCpp launcher not found: {}\nSet OZONE_KOBOLDCPP_LAUNCHER=/path/to/launch-koboldcpp.sh to use a repaired launcher.",
            launcher_path.display(),
        ));
    }
    if is_url_ready(&paths::koboldcpp_ready_url()).await {
        return Ok(()); // already running
    }

    let log_path = paths::kobold_log_path()
        .ok_or_else(|| anyhow!("could not determine ozone data directory"))?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;
    let log_file2 = log_file.try_clone()?;

    let mut cmd = std::process::Command::new(launcher_path);
    cmd.arg(model_name)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_file2);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let mut child = cmd.spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(KOBOLD_START_TIMEOUT_SECS);
    loop {
        if is_url_ready(&paths::koboldcpp_ready_url()).await {
            return Ok(());
        }

        if let Some(status) = child.try_wait()? {
            let tail = tail_file(&log_path, 40).await;
            return Err(anyhow!(format_startup_failure(
                launcher_path,
                Some(status),
                &tail,
                classify_startup_failure(&tail),
            )));
        }

        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let tail = tail_file(&log_path, 40).await;
            return Err(anyhow!(format_startup_failure(
                launcher_path,
                None,
                &tail,
                KoboldStartupFailureKind::Timeout,
            )));
        }

        sleep(Duration::from_millis(800)).await;
    }
}

async fn tail_file(path: &std::path::Path, n: usize) -> String {
    tokio::fs::read_to_string(path)
        .await
        .map(|text| {
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        })
        .unwrap_or_default()
}

fn classify_startup_failure(log_tail: &str) -> KoboldStartupFailureKind {
    let lower = log_tail.to_lowercase();
    if lower.contains("failed to extract")
        || lower.contains("failed to extract entry")
        || lower.contains("decompression resulted in return code")
    {
        KoboldStartupFailureKind::PyInstallerExtraction
    } else if (lower.contains("cannot open shared object file")
        || lower.contains("error while loading shared libraries")
        || lower.contains("no such file or directory"))
        && lower.contains(".so")
    {
        KoboldStartupFailureKind::MissingSharedLibrary
    } else if lower.contains("segmentation fault")
        || lower.contains("sigsegv")
        || lower.contains("core dumped")
    {
        KoboldStartupFailureKind::RuntimeCrash
    } else if lower.trim().is_empty() {
        KoboldStartupFailureKind::Timeout
    } else {
        KoboldStartupFailureKind::Unknown
    }
}

fn format_startup_failure(
    launcher_path: &Path,
    status: Option<std::process::ExitStatus>,
    log_tail: &str,
    classified: KoboldStartupFailureKind,
) -> String {
    let headline = match classified {
        KoboldStartupFailureKind::PyInstallerExtraction => {
            "KoboldCpp failed during packaged-binary extraction."
        }
        KoboldStartupFailureKind::MissingSharedLibrary => {
            "KoboldCpp is missing a required shared library."
        }
        KoboldStartupFailureKind::RuntimeCrash => "KoboldCpp crashed before its API became ready.",
        KoboldStartupFailureKind::Timeout => {
            "KoboldCpp did not become ready before the startup timeout."
        }
        KoboldStartupFailureKind::Unknown => "KoboldCpp exited before its API became ready.",
    };
    let mut message = String::from(headline);
    if let Some(status) = status {
        message.push_str(&format!(
            "\nProcess status: {}",
            describe_exit_status(status)
        ));
    }
    message.push_str(&format!("\nLauncher: {}", launcher_path.display()));
    for suggestion in remediation_steps(classified, launcher_path) {
        message.push_str("\n- ");
        message.push_str(&suggestion);
    }
    if !log_tail.trim().is_empty() {
        message.push_str("\n\nLauncher log tail:\n");
        message.push_str(log_tail.trim());
    }
    message
}

fn remediation_steps(kind: KoboldStartupFailureKind, launcher_path: &Path) -> Vec<String> {
    let mut steps = match kind {
        KoboldStartupFailureKind::PyInstallerExtraction => vec![
            "The installed packaged KoboldCpp binary looks corrupt or incomplete; replace it or point ozone at a repaired launcher.".to_owned(),
            format!(
                "If you have a working wrapper elsewhere, set {}=/path/to/launch-koboldcpp.sh before launching ozone.",
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
        ],
        KoboldStartupFailureKind::MissingSharedLibrary => vec![
            "The configured KoboldCpp install is missing one of its bundled .so files.".to_owned(),
            format!(
                "Repair the install behind {} or override it with {}.",
                launcher_path.display(),
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
        ],
        KoboldStartupFailureKind::RuntimeCrash => vec![
            "Retry with a repaired launcher or a CPU-safe fallback wrapper before profiling or handing off into ozone+.".to_owned(),
            format!(
                "You can override the launcher path temporarily with {}.",
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
        ],
        KoboldStartupFailureKind::Timeout => vec![
            "Inspect the launcher log for backend startup progress or crashes.".to_owned(),
            "Retry with a smaller context or lower GPU layers if the backend is just slow to load."
                .to_owned(),
        ],
        KoboldStartupFailureKind::Unknown => vec![
            "Run the configured launcher manually once to confirm the backend can start outside ozone."
                .to_owned(),
            format!(
                "If the configured launcher is bad, set {} to a repaired wrapper and retry.",
                "OZONE_KOBOLDCPP_LAUNCHER"
            ),
        ],
    };
    if let Some(log_path) = paths::kobold_log_path() {
        steps.push(format!(
            "Inspect the launcher log at {}.",
            log_path.display()
        ));
    }
    steps
}

fn describe_exit_status(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!("exit code {code}")
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return format!("terminated by signal {signal}");
            }
        }
        "terminated without an exit code".to_owned()
    }
}

pub fn open_browser_app(url: &str) {
    let candidates = [
        "chromium-browser",
        "chromium",
        "google-chrome",
        "google-chrome-stable",
    ];
    for candidate in &candidates {
        if which_exists(candidate) {
            let _ = std::process::Command::new(candidate)
                .arg(format!("--app={url}"))
                .args([
                    "--disable-gpu-compositing",
                    "--disable-extensions",
                    "--window-size=1400,900",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            return;
        }
    }
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use super::{
        classify_startup_failure, describe_exit_status, resolved_kobold_launcher_path,
        KoboldStartupFailureKind,
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn launcher_override_env_wins_when_present() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("OZONE_KOBOLDCPP_LAUNCHER", "/tmp/custom-kobold-launcher.sh");
        let path = resolved_kobold_launcher_path();
        std::env::remove_var("OZONE_KOBOLDCPP_LAUNCHER");
        assert_eq!(path, PathBuf::from("/tmp/custom-kobold-launcher.sh"));
    }

    #[test]
    fn startup_classification_detects_pyinstaller_extract_failure() {
        let kind = classify_startup_failure(
            "[PYI-32814:ERROR] Failed to extract koboldcpp_cublas.so: decompression resulted in return code -3!",
        );
        assert_eq!(kind, KoboldStartupFailureKind::PyInstallerExtraction);
    }

    #[test]
    fn startup_classification_detects_missing_shared_library() {
        let kind = classify_startup_failure(
            "error while loading shared libraries: koboldcpp_default.so: cannot open shared object file: No such file or directory",
        );
        assert_eq!(kind, KoboldStartupFailureKind::MissingSharedLibrary);
    }

    #[test]
    fn startup_classification_detects_runtime_crash() {
        let kind = classify_startup_failure("Segmentation fault (core dumped)");
        assert_eq!(kind, KoboldStartupFailureKind::RuntimeCrash);
    }

    #[test]
    fn exit_status_description_reports_numeric_code() {
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 7"])
            .status()
            .unwrap();
        assert_eq!(describe_exit_status(status), "exit code 7");
    }
}

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

#[derive(Debug, Clone, Default)]
pub struct DiskSnapshot {
    pub sectors_read: u64,
    pub sectors_written: u64,
}

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
