use anyhow::{anyhow, Result};
use ozone_core::paths;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::sleep;

const LLAMACPP_START_TIMEOUT_SECS: u64 = 300;
const LLAMACPP_MANAGED_PORT: u16 = paths::DEFAULT_LLAMACPP_PORT;
const LLAMACPP_LAUNCH_STATE_VERSION: u32 = 1;
const LLAMACPP_GRACEFUL_STOP_TIMEOUT_MILLIS: u64 = 2_000;
const LLAMACPP_PORT_RELEASE_TIMEOUT_MILLIS: u64 = 4_000;

/// Map quant_k / quant_v to llama-server --cache-type-k / --cache-type-v flags.
/// Values: 1 = f16 (default, no flags needed), 2 = q8_0, 3 = q4_0.
/// K and V can differ — e.g. K=q8_0, V=q4_0 saves VRAM while preserving attention quality.
pub fn kv_cache_args(quant_k: u8, quant_v: u8) -> Vec<String> {
    let k_quant = match quant_k {
        2 => Some("q8_0"),
        3 => Some("q4_0"),
        _ => None,
    };
    let v_quant = match quant_v {
        2 => Some("q8_0"),
        3 => Some("q4_0"),
        _ => None,
    };

    let mut args = Vec::new();
    if let Some(q) = k_quant {
        args.push("--cache-type-k".into());
        args.push(q.into());
    }
    if let Some(q) = v_quant {
        args.push("--cache-type-v".into());
        args.push(q.into());
    }
    args
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagedLlamaCppLaunchState {
    version: u32,
    pid: u32,
    port: u16,
    model_id: String,
    profile_name: Option<String>,
    config_fingerprint: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KoboldStartupFailureKind {
    PyInstallerExtraction,
    MissingSharedLibrary,
    RuntimeCrash,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlamaCppStartupFailure {
    /// GGML assertion failure — usually means OOM, unsupported op, or corrupt model
    GgmlAbort,
    /// CUDA out-of-memory — reduce -ngl
    CudaOom,
    /// CUDA runtime error (other than OOM)
    CudaError,
    /// Model file could not be opened or parsed
    ModelLoadFailed,
    /// Missing shared library (.so / .dll)
    MissingSharedLibrary,
    /// Process exited before becoming healthy (generic crash)
    RuntimeCrash { exit_code: Option<i32> },
    /// Health endpoint never responded within timeout
    Timeout,
}

pub async fn is_url_ready(url: &str) -> bool {
    // Build a client with a short timeout. The fallback path also enforces
    // a timeout so that an unreachable server doesn't hang for the OS default
    // (30-120s).
    let client = ozone_core::http::client_with_timeout(2).unwrap_or_else(|_| {
        ozone_core::http::client_with_timeout(5).unwrap_or_else(|_| reqwest::Client::new())
    });
    client
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Query the running llama.cpp server for its configured context length.
pub async fn get_llamacpp_context() -> Option<u32> {
    #[derive(serde::Deserialize)]
    struct PropsResponse {
        default_generation_settings: Option<GenerationSettings>,
    }
    #[derive(serde::Deserialize)]
    struct GenerationSettings {
        n_ctx: Option<u32>,
    }

    let client = ozone_core::http::client_with_timeout(2).ok()?;
    let resp = client
        .get(format!("{}/props", paths::llamacpp_base_url()))
        .send()
        .await
        .ok()?;
    let data: PropsResponse = resp.json().await.ok()?;
    data.default_generation_settings?.n_ctx
}

pub async fn get_llamacpp_model() -> Option<String> {
    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelEntry>,
    }

    #[derive(serde::Deserialize)]
    struct ModelEntry {
        id: String,
    }

    let client = ozone_core::http::client_with_timeout(2).ok()?;
    let resp = client
        .get(format!("{}/v1/models", paths::llamacpp_base_url()))
        .send()
        .await
        .ok()?;
    let data: ModelsResponse = resp.json().await.ok()?;
    let id = data.data.into_iter().next()?.id;
    Some(
        std::path::Path::new(&id)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&id)
            .to_string(),
    )
}

fn llamacpp_config_fingerprint(model_name: &str, args: &[String]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_name.hash(&mut hasher);
    args.hash(&mut hasher);
    format!("{:#016x}", hasher.finish())
}

/// Check whether the model currently loaded by llama-server matches the
/// requested model_name. Handles partial matches (API may return full path).
fn model_name_matches_running(requested: &str, running: Option<String>) -> bool {
    match running {
        Some(id) => id == requested || id.contains(requested),
        None => false,
    }
}

async fn load_llamacpp_launch_state() -> Result<Option<ManagedLlamaCppLaunchState>> {
    let Some(path) = paths::llamacpp_launch_state_path() else {
        return Ok(None);
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn save_llamacpp_launch_state(state: &ManagedLlamaCppLaunchState) -> Result<()> {
    let Some(path) = paths::llamacpp_launch_state_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let contents = serde_json::to_string_pretty(state)?;
    tokio::fs::write(path, format!("{contents}\n")).await?;
    Ok(())
}

async fn clear_llamacpp_launch_state() -> Result<()> {
    let Some(path) = paths::llamacpp_launch_state_path() else {
        return Ok(());
    };
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn pid_is_live(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn signal_pid(pid: u32, signal: i32) -> bool {
    unsafe { libc::kill(pid as i32, signal) == 0 }
}

async fn wait_for_pid_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !pid_is_live(pid) {
            return true;
        }
        sleep(Duration::from_millis(100)).await;
    }
    !pid_is_live(pid)
}

fn is_port_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok()
}

async fn wait_for_port_release(port: u16, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !is_port_listening(port) {
            return true;
        }
        sleep(Duration::from_millis(100)).await;
    }
    !is_port_listening(port)
}

fn strict_llamacpp_pids_on_port(port: u16) -> Result<Vec<u32>> {
    let port_flag = format!("--port {port}");
    let port_equals_flag = format!("--port={port}");
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid=,args="])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut pids = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(pid) = parts[0].trim().parse::<u32>() else {
            continue;
        };
        let args = parts[1];
        if args.contains("llama-server")
            && (args.contains(&port_flag) || args.contains(&port_equals_flag))
        {
            pids.push(pid);
        }
    }
    Ok(pids)
}

async fn stop_tracked_llamacpp_pid(pid: u32) -> bool {
    if !pid_is_live(pid) {
        return true;
    }
    let _ = signal_pid(pid, libc::SIGTERM);
    if wait_for_pid_exit(
        pid,
        Duration::from_millis(LLAMACPP_GRACEFUL_STOP_TIMEOUT_MILLIS),
    )
    .await
    {
        return true;
    }
    let _ = signal_pid(pid, libc::SIGKILL);
    wait_for_pid_exit(
        pid,
        Duration::from_millis(LLAMACPP_GRACEFUL_STOP_TIMEOUT_MILLIS),
    )
    .await
}

pub async fn purge_last_model() -> Result<Vec<u32>> {
    let mut stopped_pids = Vec::new();
    let mut fallback_needed = true;

    if let Some(state) = load_llamacpp_launch_state().await? {
        fallback_needed = !stop_tracked_llamacpp_pid(state.pid).await;
        if !fallback_needed {
            stopped_pids.push(state.pid);
        }
    }

    if fallback_needed {
        for pid in strict_llamacpp_pids_on_port(LLAMACPP_MANAGED_PORT)? {
            if stop_tracked_llamacpp_pid(pid).await {
                stopped_pids.push(pid);
            }
        }
    }

    let _ = wait_for_port_release(
        LLAMACPP_MANAGED_PORT,
        Duration::from_millis(LLAMACPP_PORT_RELEASE_TIMEOUT_MILLIS),
    )
    .await;
    clear_llamacpp_launch_state().await?;
    stopped_pids.sort_unstable();
    stopped_pids.dedup();
    Ok(stopped_pids)
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub llamacpp_running: bool,
    pub llamacpp_model: Option<String>,
}

pub async fn get_service_status() -> ServiceStatus {
    let llama_url = paths::llamacpp_ready_url();
    let llamacpp_ready = is_url_ready(&llama_url).await;
    let llamacpp_model = if llamacpp_ready {
        get_llamacpp_model().await
    } else {
        None
    };
    ServiceStatus {
        llamacpp_running: llamacpp_ready,
        llamacpp_model,
    }
}

pub async fn clear_gpu_backends() -> Result<Vec<String>> {
    let stopped = purge_last_model().await?;
    Ok(stopped
        .into_iter()
        .map(|pid| format!("llama.cpp pid {pid}"))
        .collect())
}

#[cfg(test)]
pub fn resolved_kobold_launcher_path() -> PathBuf {
    paths::launcher_path()
}

pub fn resolved_llamacpp_server_path() -> Result<PathBuf> {
    crate::llama::discover_llama_server_binary()
}

#[tracing::instrument(skip(server_path, args))]
pub async fn start_llamacpp(server_path: &Path, model_name: &str, args: &[String]) -> Result<()> {
    if !server_path.exists() {
        return Err(anyhow!(
            "llama.cpp server binary not found: {}\nSet OZONE_LLAMACPP_SERVER=/path/to/llama-server to use a local install.",
            server_path.display(),
        ));
    }
    if is_url_ready(&paths::llamacpp_ready_url()).await {
        // Verify the running model matches the requested one.
        // If a different model is loaded, kill the old server and proceed.
        let running = get_llamacpp_model().await;
        if model_name_matches_running(model_name, running) {
            return Ok(());
        }
        // Wrong model or couldn't verify — kill and restart
        clear_gpu_backends().await?;
    }

    let log_base = paths::llamacpp_log_path()
        .ok_or_else(|| anyhow!("could not determine ozone data directory"))?;
    // Timestamped log so crashes across restarts are preserved
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let log_path = if let Some(stem) = log_base.file_stem().and_then(|s| s.to_str()) {
        if let Some(parent) = log_base.parent() {
            let ext = log_base
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("log");
            parent.join(format!("{stem}-{timestamp}.{ext}"))
        } else {
            log_base
        }
    } else {
        log_base
    };
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Also create/update a symlink to the latest log for convenience
    let latest_link = paths::llamacpp_log_path();
    if let Some(ref link_path) = latest_link {
        let _ = std::fs::remove_file(link_path);
        let _ = std::os::unix::fs::symlink(&log_path, link_path);
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;
    let log_file2 = log_file.try_clone()?;

    let mut cmd = std::process::Command::new(server_path);
    cmd.arg("--model")
        .arg(model_name)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_file2);

    // Auto-set library path to the binary's directory for CUDA builds
    if let Some(parent) = server_path.parent() {
        // Prepend binary dir + (if it exists) sibling lib/ dir to LD_LIBRARY_PATH
        // so bundled .so files are found (some builds put libs in bin/ alongside
        // the binary, others split them into a lib/ subdirectory).
        let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let lib_dir = parent.join("lib");
        let new_path = if lib_dir.is_dir() {
            format!("{}:{}:{}", parent.display(), lib_dir.display(), existing)
        } else {
            format!("{}:{}", parent.display(), existing)
        };
        cmd.env("LD_LIBRARY_PATH", new_path);
    }

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
    let deadline = std::time::Instant::now() + Duration::from_secs(LLAMACPP_START_TIMEOUT_SECS);
    loop {
        if is_url_ready(&paths::llamacpp_ready_url()).await {
            let state = ManagedLlamaCppLaunchState {
                version: LLAMACPP_LAUNCH_STATE_VERSION,
                pid: child.id(),
                port: LLAMACPP_MANAGED_PORT,
                model_id: model_name.to_string(),
                profile_name: None,
                config_fingerprint: llamacpp_config_fingerprint(model_name, args),
            };
            save_llamacpp_launch_state(&state).await?;
            return Ok(());
        }

        if let Some(status) = child.try_wait()? {
            let tail = tail_file(&log_path, 50).await;
            let failure = classify_llamacpp_startup_failure(&tail, status.code());
            let suggestion = llamacpp_failure_suggestion(&failure);
            return Err(anyhow!(
                "llama-server failed to start ({:?}). {}",
                failure,
                suggestion
            ));
        }

        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let tail = tail_file(&log_path, 50).await;
            let failure = classify_llamacpp_startup_failure(&tail, None);
            let suggestion = llamacpp_failure_suggestion(&failure);
            return Err(anyhow!(
                "llama-server failed to start ({:?}). {}",
                failure,
                suggestion
            ));
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

#[cfg(test)]
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

pub fn classify_llamacpp_startup_failure(
    log_tail: &str,
    exit_status: Option<i32>,
) -> LlamaCppStartupFailure {
    let lower = log_tail.to_lowercase();
    if lower.contains("ggml_abort") {
        LlamaCppStartupFailure::GgmlAbort
    } else if lower.contains("cudaerroroutofmemory")
        || (lower.contains("out of memory") && lower.contains("cuda"))
    {
        LlamaCppStartupFailure::CudaOom
    } else if lower.contains("cuda error") {
        LlamaCppStartupFailure::CudaError
    } else if lower.contains("failed to load model")
        || lower.contains("error loading model")
        || lower.contains("unable to open")
    {
        LlamaCppStartupFailure::ModelLoadFailed
    } else if lower.contains("cannot open shared object")
        || (lower.contains("no such file or directory") && lower.contains(".so"))
    {
        LlamaCppStartupFailure::MissingSharedLibrary
    } else {
        match exit_status {
            Some(code) => LlamaCppStartupFailure::RuntimeCrash {
                exit_code: Some(code),
            },
            None => LlamaCppStartupFailure::Timeout,
        }
    }
}

pub fn llamacpp_failure_suggestion(failure: &LlamaCppStartupFailure) -> &'static str {
    match failure {
        LlamaCppStartupFailure::GgmlAbort => {
            "A GGML assertion failed — this usually means the model ran out of GPU memory or the GGUF file is corrupt. Try reducing -ngl (GPU layers) or verify the model file."
        }
        LlamaCppStartupFailure::CudaOom => {
            "CUDA ran out of GPU memory. Reduce the number of GPU layers (-ngl) or try a smaller quantization."
        }
        LlamaCppStartupFailure::CudaError => {
            "A CUDA runtime error occurred. Check your GPU drivers and CUDA installation."
        }
        LlamaCppStartupFailure::ModelLoadFailed => {
            "llama-server could not open the model file. Check the model path and ensure the file is a valid GGUF."
        }
        LlamaCppStartupFailure::MissingSharedLibrary => {
            "A required shared library is missing. Check that llama-server dependencies are installed (e.g. libcuda.so, libgomp.so)."
        }
        LlamaCppStartupFailure::RuntimeCrash { .. } => {
            "llama-server crashed before becoming ready. Check the log for details."
        }
        LlamaCppStartupFailure::Timeout => {
            "llama-server did not become ready within the timeout. It may still be loading a large model — try again or reduce context size."
        }
    }
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use crate::test_support::env_lock;
    use std::path::PathBuf;

    use super::{
        classify_startup_failure, clear_llamacpp_launch_state, describe_exit_status,
        load_llamacpp_launch_state, resolved_kobold_launcher_path, save_llamacpp_launch_state,
        KoboldStartupFailureKind, ManagedLlamaCppLaunchState, LLAMACPP_LAUNCH_STATE_VERSION,
        LLAMACPP_MANAGED_PORT,
    };

    #[tokio::test]
    async fn managed_launch_state_round_trips_to_canonical_path() {
        let _guard = env_lock();
        let sandbox = StateTestSandbox::new("round-trip");
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root());

        let state = ManagedLlamaCppLaunchState {
            version: LLAMACPP_LAUNCH_STATE_VERSION,
            pid: std::process::id(),
            port: LLAMACPP_MANAGED_PORT,
            model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
            profile_name: Some("balanced".to_string()),
            config_fingerprint: "fingerprint".to_string(),
        };

        save_llamacpp_launch_state(&state)
            .await
            .expect("save managed state");

        let loaded = load_llamacpp_launch_state()
            .await
            .expect("load managed state");

        assert_eq!(loaded, Some(state));
    }

    #[tokio::test]
    async fn clear_managed_launch_state_removes_state_file() {
        let _guard = env_lock();
        let sandbox = StateTestSandbox::new("clear-state");
        let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
        let _home = ScopedEnvVar::set("HOME", sandbox.root());

        let state = ManagedLlamaCppLaunchState {
            version: LLAMACPP_LAUNCH_STATE_VERSION,
            pid: std::process::id(),
            port: LLAMACPP_MANAGED_PORT,
            model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
            profile_name: None,
            config_fingerprint: "fingerprint".to_string(),
        };

        save_llamacpp_launch_state(&state)
            .await
            .expect("save managed state");
        clear_llamacpp_launch_state()
            .await
            .expect("clear managed state");

        assert_eq!(
            load_llamacpp_launch_state()
                .await
                .expect("load managed state after clear"),
            None
        );
    }

    struct StateTestSandbox {
        root: PathBuf,
    }

    impl StateTestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-process-state-tests-{prefix}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create process state sandbox");
            Self { root }
        }

        fn root(&self) -> &PathBuf {
            &self.root
        }

        fn xdg_data_home(&self) -> PathBuf {
            self.root.join("xdg-data")
        }
    }

    impl Drop for StateTestSandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    struct ScopedEnvVar {
        key: &'static str,
        original: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &'static str, value: impl AsRef<std::path::Path>) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, value.as_ref());
            Self { key, original }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            if let Some(original) = self.original.as_ref() {
                std::env::set_var(self.key, original);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn launcher_override_env_wins_when_present() {
        let _guard = env_lock();
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

#[cfg(test)]
mod llamacpp_tests {
    use super::{
        classify_llamacpp_startup_failure, is_url_ready, model_name_matches_running,
        LlamaCppStartupFailure,
    };

    #[test]
    fn ggml_abort_detected() {
        let kind = classify_llamacpp_startup_failure(
            "GGML_ABORT: ggml_abort called — assertion failed at ggml.c:1234",
            Some(134),
        );
        assert_eq!(kind, LlamaCppStartupFailure::GgmlAbort);
    }

    #[test]
    fn cuda_oom_detected() {
        let kind = classify_llamacpp_startup_failure(
            "CUDA error: cudaErrorOutOfMemory — not enough memory on device",
            Some(1),
        );
        assert_eq!(kind, LlamaCppStartupFailure::CudaOom);
    }

    #[test]
    fn model_load_failure_detected() {
        let kind = classify_llamacpp_startup_failure(
            "error: failed to load model from '/models/mymodel.gguf'",
            Some(1),
        );
        assert_eq!(kind, LlamaCppStartupFailure::ModelLoadFailed);
    }

    #[test]
    fn timeout_on_none_exit() {
        let kind = classify_llamacpp_startup_failure("", None);
        assert_eq!(kind, LlamaCppStartupFailure::Timeout);
    }

    #[test]
    fn unknown_on_empty_log() {
        let kind = classify_llamacpp_startup_failure("", Some(1));
        assert_eq!(
            kind,
            LlamaCppStartupFailure::RuntimeCrash { exit_code: Some(1) }
        );
    }

    #[tokio::test]
    async fn is_url_ready_returns_false_for_unreachable_port() {
        // Port 19999 should have nothing listening in any test environment.
        // This test verifies that is_url_ready returns false quickly rather
        // than hanging for the default OS timeout (which can be 30-120s).
        let result = is_url_ready("http://127.0.0.1:19999/health").await;
        assert!(!result, "unreachable port should return false");
    }

    #[test]
    fn model_name_matches_running_detects_exact_match() {
        assert!(model_name_matches_running(
            "my-model.gguf",
            Some("my-model.gguf".into()),
        ));
    }

    #[test]
    fn model_name_matches_running_detects_containment() {
        // llama.cpp API may return the full path or just the filename
        assert!(model_name_matches_running(
            "my-model.gguf",
            Some("/models/my-model.gguf".into()),
        ));
    }

    #[test]
    fn model_name_matches_running_returns_false_for_mismatch() {
        assert!(!model_name_matches_running(
            "model-b.gguf",
            Some("model-a.gguf".into()),
        ));
    }

    #[test]
    fn model_name_matches_running_returns_false_when_no_model_loaded() {
        assert!(!model_name_matches_running("anything.gguf", None));
    }
}

#[cfg(test)]
mod kv_cache_tests {
    use super::kv_cache_args;

    #[test]
    fn kv_cache_args_default_to_empty_for_f16() {
        assert!(
            kv_cache_args(1, 1).is_empty(),
            "quant_k=1 (f16) needs no flags"
        );
        assert!(
            kv_cache_args(0, 0).is_empty(),
            "quant_k=0 should default to no flags"
        );
        assert!(
            kv_cache_args(99, 99).is_empty(),
            "unknown quant should default to no flags"
        );
    }

    #[test]
    fn kv_cache_args_maps_to_q8_0_for_quant_2() {
        let args = kv_cache_args(2, 2);
        assert_eq!(
            args,
            vec!["--cache-type-k", "q8_0", "--cache-type-v", "q8_0"]
        );
    }

    #[test]
    fn kv_cache_args_maps_to_q4_0_for_quant_3() {
        let args = kv_cache_args(3, 3);
        assert_eq!(
            args,
            vec!["--cache-type-k", "q4_0", "--cache-type-v", "q4_0"]
        );
    }

    #[test]
    fn kv_cache_args_allows_asymmetric_k_and_v() {
        // K=q8_0 (2), V=q4_0 (3) — saves VRAM on V while keeping K precise
        let args = kv_cache_args(2, 3);
        assert_eq!(
            args,
            vec!["--cache-type-k", "q8_0", "--cache-type-v", "q4_0"]
        );
    }

    #[test]
    fn kv_cache_args_omits_flags_when_only_k_is_quantized() {
        let args = kv_cache_args(2, 1);
        assert_eq!(args, vec!["--cache-type-k", "q8_0"]);
    }

    #[test]
    fn kv_cache_args_omits_flags_when_only_v_is_quantized() {
        let args = kv_cache_args(1, 3);
        assert_eq!(args, vec!["--cache-type-v", "q4_0"]);
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


#[cfg(test)]
mod managed_launch_state_tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const STALE_PID_VALUE: u32 = i32::MAX as u32;
    const STATE_VERSION: u32 = 1;
    const TEST_PORT: u16 = 8989;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-launch-state-tests-{prefix}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn state_path(&self) -> PathBuf {
            self.root.join("launcher-state.json")
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_state(path: &Path, state: &ManagedLlamaCppLaunchState) {
        let contents = serde_json::to_string_pretty(state).expect("serialize launch state");
        fs::write(path, format!("{contents}\n")).expect("write launch state");
    }

    fn read_state(path: &Path) -> Option<ManagedLlamaCppLaunchState> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    fn pid_is_live(pid: u32) -> bool {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[test]
    fn test_launch_state_round_trips_required_fields() {
        let sandbox = TestSandbox::new("round-trip");
        let state = ManagedLlamaCppLaunchState {
            version: STATE_VERSION,
            pid: std::process::id(),
            port: TEST_PORT,
            model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
            profile_name: Some("balanced".to_string()),
            config_fingerprint: "test-fingerprint".to_string(),
        };

        write_state(&sandbox.state_path(), &state);
        assert_eq!(read_state(&sandbox.state_path()), Some(state));
    }

    #[test]
    fn test_missing_launch_state_file_returns_none() {
        let sandbox = TestSandbox::new("missing-file");
        assert_eq!(read_state(&sandbox.state_path()), None);
    }

    #[test]
    fn test_stale_pid_is_detected_as_not_live() {
        assert!(!pid_is_live(STALE_PID_VALUE));
    }

    #[test]
    fn test_launch_state_version_field_is_present() {
        let sandbox = TestSandbox::new("version");
        let state = ManagedLlamaCppLaunchState {
            version: STATE_VERSION,
            pid: std::process::id(),
            port: TEST_PORT,
            model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
            profile_name: None,
            config_fingerprint: "test-fingerprint".to_string(),
        };

        write_state(&sandbox.state_path(), &state);

        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(sandbox.state_path()).expect("read state file"))
                .expect("valid state json");

        assert_eq!(json["version"], STATE_VERSION);
    }
}
