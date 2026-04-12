use anyhow::{Result, anyhow};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::sleep;

pub async fn is_url_ready(url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => reqwest::Client::new(),
    };
    client.get(url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

pub async fn wait_for_url(url: &str, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if is_url_ready(url).await { return true; }
        sleep(Duration::from_millis(800)).await;
    }
    false
}

pub async fn get_kobold_model() -> Option<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().ok()?;
    let resp = client.get("http://127.0.0.1:5001/api/v1/model").send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    data["result"].as_str().map(|s| s.to_string())
}

pub async fn get_kobold_perf() -> Option<f64> {
    let client = reqwest::Client::builder().timeout(Duration::from_millis(800)).build().ok()?;
    let resp = client.get("http://127.0.0.1:5001/api/extra/perf").send().await.ok()?;
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
    pub st_running: bool,
}

pub async fn get_service_status() -> ServiceStatus {
    let (kobold_ready, st_ready) = tokio::join!(
        is_url_ready("http://127.0.0.1:5001/api/v1/model"),
        is_url_ready("http://127.0.0.1:8000"),
    );
    let kobold_model = if kobold_ready { get_kobold_model().await } else { None };
    ServiceStatus { kobold_running: kobold_ready, kobold_model, st_running: st_ready }
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
        if parts.len() < 2 { continue; }
        let pid: u32 = match parts[0].trim().parse() { Ok(p) => p, Err(_) => continue };
        let args = parts[1];
        if args.contains("koboldcpp") || (args.contains("ollama") && args.contains("runner")) {
            if nix_kill(pid) {
                killed.push(args.split('/').last().unwrap_or(args).to_string());
            }
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

pub async fn start_kobold(launcher_path: &Path, model_name: &str, args: &[String]) -> Result<()> {
    if !launcher_path.exists() {
        return Err(anyhow!("KoboldCpp launcher not found: {}", launcher_path.display()));
    }
    if is_url_ready("http://127.0.0.1:5001/api/v1/model").await {
        return Ok(()); // already running
    }

    let log_path = {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let log_dir = std::path::PathBuf::from(&home).join(".local").join("share").join("ozone");
        std::fs::create_dir_all(&log_dir)?;
        log_dir.join("koboldcpp.log")
    };

    let log_file = std::fs::OpenOptions::new()
        .create(true).write(true).truncate(true)
        .open(&log_path)?;
    let log_file2 = log_file.try_clone()?;

    let mut cmd = std::process::Command::new(launcher_path);
    cmd.arg(model_name).args(args)
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

    cmd.spawn()?;

    if !wait_for_url("http://127.0.0.1:5001/api/v1/model", 120).await {
        let tail = tail_file(&log_path, 40).await;
        return Err(anyhow!("KoboldCpp did not start.\n{tail}"));
    }
    Ok(())
}

async fn tail_file(path: &std::path::Path, n: usize) -> String {
    tokio::fs::read_to_string(path).await
        .map(|text| {
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        })
        .unwrap_or_default()
}

pub fn open_browser_app(url: &str) {
    let candidates = ["chromium-browser", "chromium", "google-chrome", "google-chrome-stable"];
    for candidate in &candidates {
        if which_exists(candidate) {
            let _ = std::process::Command::new(candidate)
                .arg(format!("--app={url}"))
                .args(["--disable-gpu-compositing", "--disable-extensions", "--window-size=1400,900"])
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                .spawn();
            return;
        }
    }
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which").arg(cmd).output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn get_root_disk_name() -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let root_line = mounts.lines().find(|l| l.split_whitespace().nth(1) == Some("/"))?;
    let dev = root_line.split_whitespace().next()?;
    let name = dev.strip_prefix("/dev/")?;
    // NVMe: nvme0n1p1 → nvme0n1
    if name.starts_with("nvme") {
        return name.split('p').next().map(|s| s.to_string());
    }
    // SATA/eMMC: sda1 → sda, mmcblk0p1 → mmcblk0
    Some(name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('p').to_string())
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
            return Some(DiskSnapshot { sectors_read, sectors_written });
        }
    }
    None
}

pub fn compute_disk_delta(prev: &DiskSnapshot, curr: &DiskSnapshot, elapsed_secs: f64) -> (f64, f64) {
    if elapsed_secs <= 0.0 { return (0.0, 0.0); }
    const BYTES_PER_SECTOR: f64 = 512.0;
    const BYTES_PER_MB: f64 = 1_048_576.0;
    let read_sectors = curr.sectors_read.saturating_sub(prev.sectors_read);
    let write_sectors = curr.sectors_written.saturating_sub(prev.sectors_written);
    let read_mb = (read_sectors as f64 * BYTES_PER_SECTOR / BYTES_PER_MB) / elapsed_secs;
    let write_mb = (write_sectors as f64 * BYTES_PER_SECTOR / BYTES_PER_MB) / elapsed_secs;
    (read_mb.max(0.0), write_mb.max(0.0))
}
