use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

const STALE_PID_VALUE: u32 = i32::MAX as u32;
const STATE_VERSION: u32 = 1;
const TEST_PORT: u16 = 8989;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagedLaunchState {
    version: u32,
    pid: u32,
    port: u16,
    model_id: String,
    profile_name: Option<String>,
}

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
            fs::remove_dir_all(&root).expect("remove stale sandbox");
        }
        fs::create_dir_all(&root).expect("create sandbox");
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

fn write_state(path: &Path, state: &ManagedLaunchState) {
    let contents = serde_json::to_string_pretty(state).expect("serialize launch state");
    fs::write(path, format!("{contents}\n")).expect("write launch state");
}

fn read_state(path: &Path) -> Option<ManagedLaunchState> {
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
fn launch_state_round_trips_required_fields() {
    let sandbox = TestSandbox::new("round-trip");
    let state = ManagedLaunchState {
        version: STATE_VERSION,
        pid: std::process::id(),
        port: TEST_PORT,
        model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
        profile_name: Some("balanced".to_string()),
    };

    write_state(&sandbox.state_path(), &state);

    assert_eq!(read_state(&sandbox.state_path()), Some(state));
}

#[test]
fn missing_launch_state_file_returns_none() {
    let sandbox = TestSandbox::new("missing-file");

    assert_eq!(read_state(&sandbox.state_path()), None);
}

#[test]
fn stale_pid_is_detected_as_not_live() {
    assert!(!pid_is_live(STALE_PID_VALUE));
}

#[test]
fn launch_state_version_field_is_present() {
    let sandbox = TestSandbox::new("version");
    let state = ManagedLaunchState {
        version: STATE_VERSION,
        pid: std::process::id(),
        port: TEST_PORT,
        model_id: "gemma-4-E4B-it-UD-Q8_K_XL.gguf".to_string(),
        profile_name: None,
    };

    write_state(&sandbox.state_path(), &state);

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sandbox.state_path()).expect("read state file"))
            .expect("valid state json");

    assert_eq!(json["version"], STATE_VERSION);
}
